//! Lightweight file supply gateway for remote hosting.
//!
//! The gateway runs where the source files live. It enumerates locally,
//! pushes manifests and contents to the remote ingest entries, and polls
//! for incremental changes. It links only the scanner and shared contract:
//! no vector store, embedder, parser, or orchestrator is involved, and the
//! remote side never pulls from this machine.
//!
//! Polling reuses the previous round's hashes for unchanged files as a pure
//! read optimization, with a periodic forced full scan bounding the mtime
//! staleness window; change detection always compares fingerprints.
//! `SyncParams` carries every business default, so the CLI branch and the
//! daemon only translate their explicit arguments over it.
//!
//! The same core backs the one-shot CLI branch and the standalone resident
//! daemon; both share this crate so behavior cannot drift.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use base64::Engine as _;

use cce_api::models::{
    INGEST_CHUNK_BYTES, IngestBatchRequest, IngestBatchResponse, IngestCommitResponse, IngestEvent,
    IngestEventKind, IngestEventRequest, IngestEventResponse, IngestFileMeta,
    IngestManifestRequest, IngestManifestResponse, IngestedFile, MAX_INGEST_BATCH_FILES,
    MAX_INGEST_CHUNKS_PER_BATCH, MAX_INGEST_FILE_BYTES, MissingChunk, total_chunks_for_size,
};
use cce_scanner::{FSScanner, FileEntry, ScanOptions};

/// Wire byte budget per upload batch, kept below the server body bound.
///
/// Accounting uses base64 wire length so a batch never exceeds the admission
/// body limit after transport growth.
const BATCH_WIRE_BYTES: u64 = 4 * 1024 * 1024;

/// Forced full scans happen at least this often in poll rounds.
const FULL_SCAN_EVERY_POLLS: usize = 60;

/// Forced full scans happen at least this often in wall-clock time.
const FULL_SCAN_INTERVAL: Duration = Duration::from_secs(300);

/// Minimal HTTP client carrying the admission token from the environment.
#[derive(Debug, Clone)]
pub struct GatewayClient {
    client: reqwest::Client,
    base_url: String,
    token: Option<String>,
}

impl GatewayClient {
    /// Build a client for the given server base URL.
    pub fn new(base_url: &str) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .context("Failed to create gateway HTTP client")?;
        let token = cce_api::gateway_token_from_env();
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
        })
    }

    /// POST a JSON body and decode the JSON response.
    pub async fn post<T: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self.client.post(&url).json(body);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .await
            .context(format!("Failed to POST {url}"))?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Request failed with status {status}: {text}");
        }
        response
            .json()
            .await
            .context("Failed to parse response JSON")
    }
}

/// Shared sync parameters for one-shot and watch passes.
///
/// This structure is the single source of business defaults: the CLI branch
/// and the standalone daemon only translate their explicit arguments over
/// `with_defaults`, so the three entry points cannot drift apart. Explicit
/// command-line values outrank the environment, which outranks these
/// defaults.
#[derive(Debug, Clone)]
pub struct SyncParams {
    /// Remote project id.
    pub project_id: i64,
    /// Local directory to supply.
    pub path: String,
    /// File extensions to include, comma separated, empty means all text.
    pub extensions: String,
    /// Directories to exclude, comma separated.
    pub exclude: String,
    /// Whether to respect gitignore files.
    pub gitignore: bool,
    /// Whether to run the index commit after staging.
    pub commit: bool,
    /// Whether to compress batch chunks before upload, off by default.
    ///
    /// Compression only applies to the batch path; incremental events always
    /// carry raw bytes so the event entry stays a single whole-file shape.
    /// The default picks up `CCE_GATEWAY_COMPRESS`.
    pub compress: bool,
    /// Poll interval in seconds for watch mode.
    pub interval_secs: u64,
    /// Lower bound applied to the poll interval.
    pub min_interval_secs: u64,
    /// Change count above which a watch pass falls back to a full sync.
    pub full_sync_threshold: usize,
    /// Heartbeat file for supervisors and health probes, if any.
    pub health_file: Option<PathBuf>,
}

impl SyncParams {
    /// Parameters for one project with the shared business defaults.
    ///
    /// Every entry point starts here and overrides only what its caller
    /// stated explicitly.
    pub fn with_defaults(project_id: i64, path: impl Into<String>) -> Self {
        Self {
            project_id,
            path: path.into(),
            extensions: String::new(),
            exclude: "node_modules,target,.git,vendor".to_string(),
            gitignore: true,
            commit: true,
            compress: compression_enabled(),
            interval_secs: 5,
            min_interval_secs: 1,
            full_sync_threshold: 100,
            health_file: None,
        }
    }
}

/// Whether chunk compression is enabled. Optional negotiation, off by
/// default; the remote decompresses before hash verification.
pub fn compression_enabled() -> bool {
    std::env::var("CCE_GATEWAY_COMPRESS")
        .map(|v| {
            let v = v.trim().to_lowercase();
            v == "1" || v == "true" || v == "yes" || v == "on"
        })
        .unwrap_or(false)
}

/// Usable scan result for gateway pushes.
#[derive(Debug)]
pub struct ScanSnapshot {
    /// Usable files keyed by normalized identity.
    pub files: HashMap<String, ScannedFile>,
    /// Files skipped for exceeding the ingest bound.
    pub skipped_oversize: usize,
    /// Files skipped as binary.
    pub skipped_binary: usize,
}

/// One scannable file with its fingerprint.
#[derive(Debug, Clone)]
pub struct ScannedFile {
    /// Local absolute path.
    pub absolute: PathBuf,
    /// File size in bytes.
    pub size: u64,
    /// Modification time as seconds since the unix epoch.
    pub modified_secs: i64,
    /// Full-content hash when known.
    pub content_hash: Option<String>,
}

/// Baseline fingerprint for incremental comparison.
///
/// The full-content hash stays authoritative. Size and modification time make
/// hash-less entries comparable and keep the idempotence key explicit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaselineFingerprint {
    /// File size in bytes at baseline time.
    pub size: u64,
    /// Modification time at baseline time.
    pub modified_secs: i64,
    /// Full-content hash when known.
    pub content_hash: Option<String>,
}

impl From<&ScannedFile> for BaselineFingerprint {
    fn from(file: &ScannedFile) -> Self {
        Self {
            size: file.size,
            modified_secs: file.modified_secs,
            content_hash: file.content_hash.clone(),
        }
    }
}

/// Whether the current file matches the baseline fingerprint.
///
/// Hashed entries compare by hash only. Hash-less entries fall back to the
/// size plus modification time comparison.
pub fn fingerprints_match(previous: &BaselineFingerprint, current: &ScannedFile) -> bool {
    if previous.content_hash.is_some() || current.content_hash.is_some() {
        return previous.content_hash == current.content_hash;
    }
    previous.size == current.size && previous.modified_secs == current.modified_secs
}

/// Stable manifest version for a snapshot. The same file set yields the
/// same version so an interrupted push can resume without retransmitting
/// received chunks; any content change yields a new version.
pub fn manifest_version_for_snapshot(snapshot: &ScanSnapshot) -> u64 {
    let mut identities: Vec<&String> = snapshot.files.keys().collect();
    identities.sort();
    let mut hash: u64 = 0xcbf29ce484222325;
    for identity in identities {
        if let Some(file) = snapshot.files.get(identity) {
            for byte in identity.as_bytes() {
                hash ^= *byte as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
            for byte in file.size.to_le_bytes() {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
            if let Some(content_hash) = file.content_hash.as_deref() {
                for byte in content_hash.as_bytes() {
                    hash ^= *byte as u64;
                    hash = hash.wrapping_mul(0x100000001b3);
                }
            }
        }
    }
    hash.max(1)
}

/// Hash one chunk of raw bytes for per-chunk verification.
///
/// Delegates to the shared hash domain so gateway and server verify the same
/// digest for the same bytes.
pub fn chunk_hash(bytes: &[u8]) -> String {
    cce_utils::hash::calculate_hash(bytes)
}

/// Optionally compress chunk bytes. Compression stays off unless the
/// gateway explicitly enables it; the server decompresses before hashing.
pub fn maybe_compress(bytes: &[u8], compress: bool) -> (Vec<u8>, bool) {
    if !compress || bytes.is_empty() {
        return (bytes.to_vec(), false);
    }
    match zstd::encode_all(bytes, 3) {
        Ok(encoded) if (encoded.len() as u64) < (bytes.len() as u64) => (encoded, true),
        _ => (bytes.to_vec(), false),
    }
}

/// One scan pass: usable files plus the entries the next pass reuses.
#[derive(Debug)]
pub struct ScanOutcome {
    /// Usable files keyed by normalized identity.
    pub snapshot: ScanSnapshot,
    /// Scanned entries keyed by relative path. Passing this back into
    /// `scan_local` lets the scanner reuse hashes for unchanged files.
    pub entries: HashMap<PathBuf, FileEntry>,
}

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

/// Scan the local directory and return usable entries keyed by identity.
///
/// With `previous` set, the scanner reuses each unchanged file's hash
/// instead of re-reading it; reuse is a pure read optimization and never
/// takes part in change detection, which stays fingerprint-based. New and
/// deleted files are unaffected by reuse.
///
/// The scan uses the same traversal and ignore semantics as local indexing.
/// The ingest bound is applied as the scanner size limit so oversized files
/// are marked before hashing. The extension filter is an explicit user-side
/// supply pre-filter; the remote project configuration stays authoritative
/// for parsing. Plugin file filters are intentionally not applied here;
/// oversupplied files are filtered again by the remote pipeline.
pub async fn scan_local(
    params: &SyncParams,
    previous: Option<&HashMap<PathBuf, FileEntry>>,
) -> Result<ScanOutcome> {
    let exclude_patterns: Vec<String> = params
        .exclude
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let wanted: Vec<String> = params
        .extensions
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_start_matches('.').to_lowercase())
        .collect();
    let options = ScanOptions {
        root_path: params.path.clone(),
        include_patterns: Vec::new(),
        exclude_patterns,
        follow_symlinks: false,
        respect_gitignore: params.gitignore,
        gitignore_patterns: Vec::new(),
        gitignore_path: None,
        max_content_size: Some(MAX_INGEST_FILE_BYTES),
        max_file_size: Some(MAX_INGEST_FILE_BYTES),
    };
    let mut scanner = FSScanner::new();
    let report = match previous {
        Some(previous) => scanner.scan_incremental_report(&options, previous),
        None => scanner.scan_report(&options),
    }
    .context("gateway failed to scan the local directory")?;
    if !report.failures.is_empty() {
        eprintln!(
            "warning: {} paths could not be scanned and are skipped",
            report.failures.len()
        );
    }
    let mut files: HashMap<String, ScannedFile> = HashMap::new();
    let mut entries: HashMap<PathBuf, FileEntry> = HashMap::with_capacity(report.entries.len());
    let mut skipped_oversize = 0usize;
    let mut skipped_binary = 0usize;
    for entry in report.entries {
        if entry.size > MAX_INGEST_FILE_BYTES {
            skipped_oversize += 1;
        } else if !entry.is_text() {
            skipped_binary += 1;
        } else {
            let matches = wanted.is_empty()
                || entry
                    .relative_path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| wanted.contains(&ext.to_lowercase()))
                    .unwrap_or(false);
            if matches {
                let identity = cce_types::path::normalize_project_path(&entry.identity_key());
                files.insert(
                    identity,
                    ScannedFile {
                        absolute: entry.path.clone(),
                        size: entry.size,
                        modified_secs: entry.modified.timestamp(),
                        content_hash: entry.content_hash.clone(),
                    },
                );
            }
        }
        entries.insert(entry.relative_path.clone(), entry);
    }
    Ok(ScanOutcome {
        snapshot: ScanSnapshot {
            files,
            skipped_oversize,
            skipped_binary,
        },
        entries,
    })
}

/// Push one full sync pass: manifest, missing contents, then commit.
///
/// The pass always scans without hash reuse: the manifest covers the whole
/// tree, so reuse would only hide a locally drifted file. The returned
/// outcome seeds the watch loop's baseline and its next incremental pass.
pub async fn sync_once(
    client: &GatewayClient,
    params: &SyncParams,
    verbose: bool,
) -> Result<ScanOutcome> {
    let outcome = scan_local(params, None).await?;
    let snapshot = &outcome.snapshot;
    if verbose {
        println!(
            "scanned {} files (skipped {} oversize, {} binary)",
            snapshot.files.len(),
            snapshot.skipped_oversize,
            snapshot.skipped_binary
        );
    }
    let manifest_version = manifest_version_for_snapshot(snapshot);
    let manifest = IngestManifestRequest {
        files: snapshot
            .files
            .iter()
            .map(|(identity, file)| IngestFileMeta {
                relative_path: identity.clone(),
                size: file.size,
                modified_secs: file.modified_secs,
                content_hash: file.content_hash.clone(),
            })
            .collect(),
        manifest_version,
    };
    let manifest_url = format!("/api/project/{}/ingest/manifest", params.project_id);
    let manifest_response: IngestManifestResponse = client.post(&manifest_url, &manifest).await?;
    if verbose {
        println!(
            "manifest v{}: {} unchanged, {} files and {} chunks to upload{}",
            manifest_response.manifest_version,
            manifest_response.unchanged,
            manifest_response.upload.len(),
            manifest_response.missing_chunks.len(),
            if params.compress { " (compressed)" } else { "" },
        );
    }
    let (staged, errors) = upload_paths(
        client,
        params,
        snapshot,
        &manifest_response.upload,
        &manifest_response.missing_chunks,
        manifest_response.manifest_version.max(manifest_version),
    )
    .await?;
    if !errors.is_empty() {
        for error in &errors {
            eprintln!("error: {error}");
        }
        anyhow::bail!("gateway staged {staged} files with {} errors", errors.len());
    }
    if params.commit {
        let commit_url = format!("/api/project/{}/ingest/commit", params.project_id);
        let commit: IngestCommitResponse = client.post(&commit_url, &serde_json::json!({})).await?;
        println!(
            "gateway sync complete: {} files indexed, {} entities",
            commit.indexed_files, commit.total_entities
        );
    } else {
        println!("gateway staged {staged} files without commit");
    }
    Ok(outcome)
}

/// Upload the requested paths in bounded chunk batches.
pub async fn upload_paths(
    client: &GatewayClient,
    params: &SyncParams,
    snapshot: &ScanSnapshot,
    upload: &[String],
    missing_chunks: &[MissingChunk],
    manifest_version: u64,
) -> Result<(usize, Vec<String>)> {
    use std::collections::{HashMap, HashSet};
    let mut wanted: HashMap<String, HashSet<u32>> = HashMap::new();
    for identity in upload {
        let Some(file) = snapshot.files.get(identity) else {
            continue;
        };
        let total = total_chunks_for_size(file.size.max(1));
        wanted.insert(identity.clone(), (0..total).collect());
    }
    for missing in missing_chunks {
        wanted
            .entry(missing.relative_path.clone())
            .or_default()
            .insert(missing.chunk_index);
    }
    let mut batch: Vec<IngestedFile> = Vec::new();
    let mut batch_bytes = 0u64;
    let mut staged = 0usize;
    let mut errors = Vec::new();
    let mut identities: Vec<String> = wanted.keys().cloned().collect();
    identities.sort();
    for identity in identities {
        let wanted_indices = wanted.get(&identity).cloned().unwrap_or_default();
        let Some(file) = snapshot.files.get(&identity) else {
            errors.push(format!("{identity}: disappeared after the manifest scan"));
            continue;
        };
        let bytes = match tokio::fs::read(&file.absolute).await {
            Ok(bytes) => bytes,
            Err(e) => {
                errors.push(format!("{identity}: failed to read local file: {e}"));
                continue;
            }
        };
        if bytes.len() as u64 > MAX_INGEST_FILE_BYTES {
            errors.push(format!(
                "{identity}: grew past the ingest bound after scanning"
            ));
            continue;
        }
        if let Some(expected) = file.content_hash.as_deref()
            && cce_utils::hash::calculate_hash(&bytes) != expected
        {
            errors.push(format!(
                "{identity}: changed between manifest and upload; a fresh manifest is required"
            ));
            continue;
        }
        let total = total_chunks_for_size(bytes.len().max(1) as u64);
        let mut indices: Vec<u32> = wanted_indices.into_iter().collect();
        indices.sort_unstable();
        for chunk_index in indices {
            if chunk_index >= total {
                continue;
            }
            let start = (chunk_index as usize) * INGEST_CHUNK_BYTES;
            let end = (start + INGEST_CHUNK_BYTES).min(bytes.len());
            let chunk_bytes = &bytes[start..end];
            let hash = chunk_hash(chunk_bytes);
            let (payload_bytes, compressed) = maybe_compress(chunk_bytes, params.compress);
            let content_base64 = base64::engine::general_purpose::STANDARD.encode(&payload_bytes);
            batch_bytes += content_base64.len() as u64;
            batch.push(IngestedFile {
                relative_path: identity.clone(),
                content_hash: file.content_hash.clone(),
                content_base64,
                chunk_index,
                total_chunks: total,
                chunk_hash: Some(hash),
                compressed,
            });
            if batch.len() >= MAX_INGEST_CHUNKS_PER_BATCH
                || batch.len() >= MAX_INGEST_BATCH_FILES
                || batch_bytes >= BATCH_WIRE_BYTES
            {
                flush_batch(
                    client,
                    params.project_id,
                    manifest_version,
                    &mut batch,
                    &mut batch_bytes,
                    &mut staged,
                    &mut errors,
                )
                .await;
            }
        }
    }
    for identity in upload {
        if !snapshot.files.contains_key(identity) {
            errors.push(format!("{identity}: disappeared after the manifest scan"));
        }
    }
    if !batch.is_empty() {
        flush_batch(
            client,
            params.project_id,
            manifest_version,
            &mut batch,
            &mut batch_bytes,
            &mut staged,
            &mut errors,
        )
        .await;
    }
    Ok((staged, errors))
}

/// Send one upload batch to the remote ingest entry.
async fn flush_batch(
    client: &GatewayClient,
    project_id: i64,
    manifest_version: u64,
    batch: &mut Vec<IngestedFile>,
    batch_bytes: &mut u64,
    staged: &mut usize,
    errors: &mut Vec<String>,
) {
    let request = IngestBatchRequest {
        files: std::mem::take(batch),
        manifest_version,
    };
    *batch_bytes = 0;
    let url = format!("/api/project/{project_id}/ingest/batch");
    match client.post::<_, IngestBatchResponse>(&url, &request).await {
        Ok(response) => {
            *staged += response.staged;
            errors.extend(response.errors);
        }
        Err(e) => errors.push(format!("batch upload failed: {e:#}")),
    }
}

/// Schema version of the heartbeat record written by this build.
const GATEWAY_HEALTH_VERSION: u32 = 1;

/// Resident health state written as a heartbeat file.
///
/// The format only ever gains fields: supervisors parse leniently, treating
/// an absent `version` as the earliest shape.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayHealth {
    /// Heartbeat schema version; absent in records written before
    /// versioning was introduced.
    #[serde(default)]
    pub version: u32,
    /// Local path being supplied.
    pub path: String,
    /// Remote project id.
    pub project_id: i64,
    /// Last successful sync in RFC3339, if any.
    pub last_sync: Option<String>,
    /// Files in the last baseline.
    pub baseline_files: usize,
    /// Daemon start time in RFC3339.
    pub started_at: String,
    /// Last forced full scan in RFC3339, if any.
    #[serde(default)]
    pub last_full_scan: Option<String>,
}

/// Build one heartbeat record for the current watch state.
fn health_record(
    params: &SyncParams,
    baseline_files: usize,
    started_at: &str,
    last_full_scan: Option<String>,
) -> GatewayHealth {
    GatewayHealth {
        version: GATEWAY_HEALTH_VERSION,
        path: params.path.clone(),
        project_id: params.project_id,
        last_sync: Some(chrono::Utc::now().to_rfc3339()),
        baseline_files,
        started_at: started_at.to_string(),
        last_full_scan,
    }
}

/// Write the heartbeat file for process supervisors and health probes.
pub fn write_health_file(path: &std::path::Path, health: &GatewayHealth) -> Result<()> {
    let content = serde_json::to_string_pretty(health).context("health must serialize")?;
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).context("health parent must be creatable")?;
    }
    std::fs::write(path, content).context("health file must be writable")?;
    Ok(())
}

/// Persist the heartbeat when one is configured, warning otherwise.
fn write_heartbeat(
    params: &SyncParams,
    baseline_files: usize,
    started_at: &str,
    last_full_scan: Option<String>,
) {
    let Some(path) = params.health_file.as_deref() else {
        return;
    };
    let health = health_record(params, baseline_files, started_at, last_full_scan);
    if let Err(e) = write_health_file(path, &health) {
        eprintln!("warning: health write failed: {e:#}");
    }
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
/// The baseline lives in memory only; losing it rebuilds through one full
/// sync pass, which is also the crash-recovery path for the resident daemon.
/// Each poll rescans incrementally against the previous round's entries so
/// unchanged files skip re-hashing, and a forced full scan runs every fixed
/// number of rounds or within a fixed wall-clock window, whichever comes
/// first, so reuse cannot hide a change indefinitely. Change detection
/// itself always compares the size, modification time and content hash
/// triple of the baseline; the reuse decision never takes part in it.
pub async fn watch_loop(client: &GatewayClient, params: &SyncParams, verbose: bool) -> Result<()> {
    let first = sync_once(client, params, verbose).await?;
    let mut baseline = fingerprint_map(&first.snapshot);
    let mut previous_entries = first.entries;
    let mut polls_since_full: usize = 0;
    let mut last_full_scan = Instant::now();
    let mut last_full_scan_at = Some(chrono::Utc::now().to_rfc3339());
    let started_at = chrono::Utc::now().to_rfc3339();
    let interval_secs = params.interval_secs.max(params.min_interval_secs);
    println!(
        "gateway watching {} (poll every {interval_secs}s)",
        params.path
    );
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
            continue;
        }
        if verbose {
            println!("pushing {} change(s)", events.len());
        }
        if events.len() > params.full_sync_threshold {
            // Too many changes for the event path: fall back to one full
            // sync pass, which also counts as this round's forced full scan.
            match sync_once(client, params, verbose).await {
                Ok(outcome) => {
                    previous_entries = outcome.entries;
                    baseline = fingerprint_map(&outcome.snapshot);
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
    fn manifest_version_is_stable_for_same_snapshot() {
        let mut files = HashMap::new();
        files.insert(
            "src/main.rs".to_string(),
            ScannedFile {
                absolute: PathBuf::from("/tmp/src/main.rs"),
                size: 12,
                modified_secs: 0,
                content_hash: Some("abc".to_string()),
            },
        );
        let first = ScanSnapshot {
            files,
            skipped_oversize: 0,
            skipped_binary: 0,
        };
        let version = manifest_version_for_snapshot(&first);
        assert!(version >= 1);
    }

    #[test]
    fn chunk_hash_is_stable() {
        assert_eq!(chunk_hash(b"hello"), chunk_hash(b"hello"));
        assert!(chunk_hash(b"hello") != chunk_hash(b"world"));
    }

    #[test]
    fn chunk_hash_shares_common_hash_domain() {
        assert_eq!(
            chunk_hash(b"hello"),
            cce_utils::hash::calculate_hash(b"hello")
        );
    }

    #[test]
    fn baseline_matches_by_hash_when_present() {
        let file = ScannedFile {
            absolute: PathBuf::from("/tmp/a.rs"),
            size: 10,
            modified_secs: 7,
            content_hash: Some("hash-a".to_string()),
        };
        let same = BaselineFingerprint {
            size: 99,
            modified_secs: 99,
            content_hash: Some("hash-a".to_string()),
        };
        assert!(fingerprints_match(&same, &file));
        let changed = BaselineFingerprint {
            size: 10,
            modified_secs: 7,
            content_hash: Some("hash-b".to_string()),
        };
        assert!(!fingerprints_match(&changed, &file));
    }

    #[test]
    fn baseline_falls_back_to_size_and_mtime_without_hash() {
        let file = ScannedFile {
            absolute: PathBuf::from("/tmp/a.rs"),
            size: 10,
            modified_secs: 7,
            content_hash: None,
        };
        let same = BaselineFingerprint {
            size: 10,
            modified_secs: 7,
            content_hash: None,
        };
        assert!(fingerprints_match(&same, &file));
        let changed = BaselineFingerprint {
            size: 11,
            modified_secs: 7,
            content_hash: None,
        };
        assert!(!fingerprints_match(&changed, &file));
    }

    #[test]
    fn compression_defaults_off_and_shrinks_repetitive_bytes() {
        let repetitive = vec![b'a'; 4096];
        let (plain, compressed) = maybe_compress(&repetitive, false);
        assert!(!compressed);
        assert_eq!(plain.len(), repetitive.len());
        let (encoded, compressed) = maybe_compress(&repetitive, true);
        assert!(compressed);
        assert!(encoded.len() < repetitive.len());
    }

    #[test]
    fn defaults_are_the_shared_contract_of_every_entry_point() {
        let params = SyncParams::with_defaults(3, "/srv/repo");
        assert_eq!(params.project_id, 3);
        assert_eq!(params.path, "/srv/repo");
        assert_eq!(params.extensions, "");
        assert_eq!(params.exclude, "node_modules,target,.git,vendor");
        assert!(params.gitignore);
        assert!(params.commit);
        assert_eq!(params.compress, compression_enabled());
        assert_eq!(params.interval_secs, 5);
        assert_eq!(params.min_interval_secs, 1);
        assert_eq!(params.full_sync_threshold, 100);
        assert!(params.health_file.is_none());
    }

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

    #[test]
    fn heartbeat_is_versioned_and_parses_legacy_records_leniently() {
        let params = SyncParams::with_defaults(9, "/srv/repo");
        let record = health_record(&params, 12, "2026-01-01T00:00:00Z", None);
        assert_eq!(record.version, GATEWAY_HEALTH_VERSION);
        assert_eq!(
            record.version,
            serde_json::to_value(&record)
                .expect("health serializes")
                .get("version")
                .and_then(|v| v.as_u64())
                .expect("version field is written") as u32
        );
        let legacy = serde_json::json!({
            "path": "/srv/repo",
            "project_id": 9,
            "last_sync": null,
            "baseline_files": 3,
            "started_at": "2025-01-01T00:00:00Z"
        });
        let parsed: GatewayHealth =
            serde_json::from_value(legacy).expect("legacy heartbeat parses leniently");
        assert_eq!(parsed.version, 0);
        assert!(parsed.last_full_scan.is_none());
    }

    #[tokio::test]
    async fn incremental_rescan_keeps_hashes_and_detects_tail_rewrite() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("a.rs"), "fn a() {}").expect("write a.rs");
        let params = SyncParams::with_defaults(7, dir.path().to_string_lossy().to_string());
        let first = scan_local(&params, None).await.expect("first scan");
        let hash_before = first
            .snapshot
            .files
            .get("a.rs")
            .expect("a.rs is usable")
            .content_hash
            .clone()
            .expect("a.rs carries a content hash");

        std::fs::write(dir.path().join("a.rs"), "fn a() {}\nfn extra() {}").expect("rewrite a.rs");
        std::fs::write(dir.path().join("b.rs"), "fn b() {}").expect("write b.rs");
        let second = scan_local(&params, Some(&first.entries))
            .await
            .expect("incremental scan");
        let rewritten = second
            .snapshot
            .files
            .get("a.rs")
            .expect("a.rs still usable")
            .content_hash
            .clone()
            .expect("rewritten a.rs carries a hash");
        assert_ne!(
            rewritten, hash_before,
            "a tail rewrite must surface as a new hash"
        );
        assert!(
            second.snapshot.files.contains_key("b.rs"),
            "new files are unaffected by reuse"
        );

        let third = scan_local(&params, Some(&second.entries))
            .await
            .expect("second incremental scan");
        assert_eq!(
            third
                .snapshot
                .files
                .get("a.rs")
                .and_then(|f| f.content_hash.as_deref()),
            Some(rewritten.as_str()),
            "an unchanged file keeps its hash across incremental passes"
        );
        std::fs::remove_file(dir.path().join("b.rs")).expect("remove b.rs");
        let fourth = scan_local(&params, Some(&third.entries))
            .await
            .expect("third incremental scan");
        assert!(
            !fourth.snapshot.files.contains_key("b.rs"),
            "deleted files disappear from the snapshot"
        );
    }
}
