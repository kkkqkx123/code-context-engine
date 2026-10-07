//! Incremental poll loop: detect local changes and push them as ingest events.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anyhow::Result;
use base64::Engine as _;

use cce_api::models::{
    IngestEvent, IngestEventKind, IngestEventRequest, IngestEventResponse, MAX_INGEST_FILE_BYTES,
};

use super::client::GatewayClient;
use super::health::write_heartbeat;
use super::params::SyncParams;
use super::scan::{
    BaselineFingerprint, ScanSnapshot, fingerprints_match, save_cached_entries, scan_local,
};
use super::sync::sync_once;

/// Forced full scans happen at least this often in poll rounds.
const FULL_SCAN_EVERY_POLLS: usize = 60;

/// Forced full scans happen at least this often in wall-clock time.
const FULL_SCAN_INTERVAL: Duration = Duration::from_secs(300);

/// Whether a poll must ignore hash reuse and rescan everything.
///
/// The forced scan bounds the mtime-granularity staleness window of hash
/// reuse: whichever leg trips first, fixed rounds or wall-clock time,
/// forces one full rescan so neither fast nor slow polling leaves a file
/// unverified indefinitely.
fn should_force_full(polls_since_full: usize, last_full_scan: Instant, now: Instant) -> bool {
    polls_since_full >= FULL_SCAN_EVERY_POLLS
        || now.saturating_duration_since(last_full_scan) >= FULL_SCAN_INTERVAL
}

/// Build the fingerprint baseline from one scan's usable files.
fn fingerprint_map(snapshot: &ScanSnapshot) -> BTreeMap<String, BaselineFingerprint> {
    snapshot
        .files
        .iter()
        .map(|(identity, file)| (identity.clone(), BaselineFingerprint::from(file)))
        .collect()
}

/// Poll for local changes and push them as ingest events.
///
/// The baseline lives in memory with an optional on-disk scan cache for
/// cross-restart hash reuse; losing both rebuilds through one full sync
/// pass, which is also the crash-recovery path for the resident daemon.
/// Each poll rescans incrementally against the previous round's entries so
/// unchanged files skip re-hashing, and a forced full scan runs every fixed
/// number of rounds or within a fixed wall-clock window, whichever comes
/// first, so reuse cannot hide a change indefinitely. Change detection
/// itself always compares the size, modification time and content hash
/// triple of the baseline; the reuse decision never takes part in it. The
/// wait between polls adapts to the baseline size when enabled.
pub async fn watch_loop(client: &GatewayClient, params: &SyncParams, verbose: bool) -> Result<()> {
    let first = sync_once(client, params, verbose).await?;
    let mut baseline = fingerprint_map(&first.snapshot);
    let mut previous_entries = first.entries;
    let mut polls_since_full: usize = 0;
    let mut last_full_scan = Instant::now();
    let mut last_full_scan_at = Some(chrono::Utc::now().to_rfc3339());
    let started_at = chrono::Utc::now().to_rfc3339();
    let mut interval_secs = params.effective_interval(baseline.len());
    if params.json_progress {
        println!(
            "{}",
            serde_json::json!({
                "kind": "watching",
                "path": params.path,
                "interval_secs": interval_secs,
                "baseline_files": baseline.len(),
            })
        );
    } else {
        println!(
            "gateway watching {} (poll every {interval_secs}s)",
            params.path
        );
    }
    write_heartbeat(
        params,
        baseline.len(),
        &started_at,
        last_full_scan_at.clone(),
    );
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
        let now = Instant::now();
        let force_full = should_force_full(polls_since_full, last_full_scan, now);
        let outcome = match scan_local(
            params,
            if force_full {
                None
            } else {
                Some(&previous_entries)
            },
        )
        .await
        {
            Ok(outcome) => outcome,
            Err(e) => {
                eprintln!("warning: rescan failed: {e:#}");
                continue;
            }
        };
        if force_full {
            polls_since_full = 0;
            last_full_scan = now;
            last_full_scan_at = Some(chrono::Utc::now().to_rfc3339());
        } else {
            polls_since_full += 1;
        }
        previous_entries = outcome.entries;
        if let Some(cache_file) = params.cache_file.as_deref()
            && let Err(e) = save_cached_entries(cache_file, &previous_entries)
        {
            eprintln!("warning: scan cache write failed: {e:#}");
        }
        let snapshot = outcome.snapshot;
        let current = fingerprint_map(&snapshot);
        let mut events = Vec::new();
        for identity in current.keys() {
            let Some(file) = snapshot.files.get(identity) else {
                continue;
            };
            match baseline.get(identity) {
                None => {
                    if let Some(event) = read_event(&snapshot, identity, IngestEventKind::Created) {
                        events.push(event);
                    }
                }
                Some(previous) if !fingerprints_match(previous, file) => {
                    if let Some(event) = read_event(&snapshot, identity, IngestEventKind::Modified)
                    {
                        events.push(event);
                    }
                }
                Some(_) => {}
            }
        }
        for identity in baseline.keys() {
            if !current.contains_key(identity) {
                events.push(IngestEvent {
                    relative_path: identity.clone(),
                    kind: IngestEventKind::Deleted,
                    content_hash: None,
                    content_base64: None,
                });
            }
        }
        if events.is_empty() {
            interval_secs = params.effective_interval(current.len());
            baseline = current;
            continue;
        }
        if params.json_progress {
            println!(
                "{}",
                serde_json::json!({
                    "kind": "pushing",
                    "changes": events.len(),
                })
            );
        } else if verbose {
            println!("pushing {} change(s)", events.len());
        }
        if events.len() > params.full_sync_threshold {
            // Too many changes for the event path: fall back to one full
            // sync pass, which also counts as this round's forced full scan.
            match sync_once(client, params, verbose).await {
                Ok(outcome) => {
                    previous_entries = outcome.entries;
                    baseline = fingerprint_map(&outcome.snapshot);
                    interval_secs = params.effective_interval(baseline.len());
                    polls_since_full = 0;
                    last_full_scan = Instant::now();
                    last_full_scan_at = Some(chrono::Utc::now().to_rfc3339());
                    write_heartbeat(
                        params,
                        baseline.len(),
                        &started_at,
                        last_full_scan_at.clone(),
                    );
                }
                Err(e) => {
                    eprintln!("warning: fallback sync failed: {e:#}");
                    continue;
                }
            }
            continue;
        }
        let request = IngestEventRequest { events };
        let url = format!("/api/project/{}/ingest/event", params.project_id);
        match client.post::<_, IngestEventResponse>(&url, &request).await {
            Ok(response) => {
                if !response.errors.is_empty() {
                    for error in &response.errors {
                        eprintln!("warning: {error}");
                    }
                }
            }
            Err(e) => {
                eprintln!("warning: event push failed: {e:#}");
                continue;
            }
        }
        baseline = current;
        interval_secs = params.effective_interval(baseline.len());
        write_heartbeat(
            params,
            baseline.len(),
            &started_at,
            last_full_scan_at.clone(),
        );
    }
}

/// Read one changed file into an ingest event.
///
/// Events always carry raw bytes without compression. Bytes are verified
/// against the scan hash before assembly; drifted reads are dropped so the
/// next poll can reconverge instead of pushing stale content.
fn read_event(
    snapshot: &ScanSnapshot,
    identity: &str,
    kind: IngestEventKind,
) -> Option<IngestEvent> {
    let file = snapshot.files.get(identity)?;
    let bytes = std::fs::read(&file.absolute).ok()?;
    if bytes.len() as u64 > MAX_INGEST_FILE_BYTES {
        eprintln!("warning: {identity} exceeds the ingest bound and is skipped");
        return None;
    }
    if let Some(expected) = file.content_hash.as_deref()
        && cce_utils::hash::calculate_hash(&bytes) != expected
    {
        eprintln!("warning: {identity} changed after scanning and is skipped");
        return None;
    }
    Some(IngestEvent {
        relative_path: identity.to_string(),
        kind,
        content_hash: file.content_hash.clone(),
        content_base64: Some(base64::engine::general_purpose::STANDARD.encode(&bytes)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forced_full_scan_trips_on_rounds_or_time_whichever_comes_first() {
        let base = Instant::now();
        assert!(!should_force_full(0, base, base));
        assert!(!should_force_full(
            FULL_SCAN_EVERY_POLLS - 1,
            base,
            base + Duration::from_secs(1)
        ));
        assert!(should_force_full(FULL_SCAN_EVERY_POLLS, base, base));
        assert!(!should_force_full(0, base, base + Duration::from_secs(299)));
        assert!(should_force_full(0, base, base + FULL_SCAN_INTERVAL));
    }
}
