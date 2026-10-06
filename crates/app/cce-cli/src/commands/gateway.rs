//! Local file gateway for remote hosting.
//!
//! The gateway runs on the machine holding the source files. It enumerates
//! locally, pushes manifests and contents to the remote ingest entries, and
//! polls for incremental changes. It links only the scanner and the shared
//! utilities: no vector store, embedder, or parser is involved, and the
//! remote side never pulls from this machine.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use anyhow::{Context, Result};
use base64::Engine as _;

use cce_api::models::{
    IngestBatchRequest, IngestBatchResponse, IngestCommitResponse, IngestEvent, IngestEventKind,
    IngestEventRequest, IngestEventResponse, IngestFileMeta, IngestManifestRequest,
    IngestManifestResponse, IngestedFile, MAX_INGEST_BATCH_FILES, MAX_INGEST_FILE_BYTES,
};
use cce_scanner::{FSScanner, ScanOptions};

use crate::cli::GatewayCommands;
use crate::client::ApiClient;
use crate::output::{print_error, print_success};

/// Raw byte budget per upload batch, kept below the server body bound.
const BATCH_RAW_BYTES: u64 = 4 * 1024 * 1024;

/// Changes above this count fall back to a full sync pass.
const WATCH_FULL_SYNC_THRESHOLD: usize = 100;

/// Execute the gateway subcommand.
pub async fn execute(cmd: &GatewayCommands, server: &str, verbose: bool) -> Result<()> {
    let client = ApiClient::new(server)?;
    match cmd {
        GatewayCommands::Sync {
            project_id,
            path,
            extensions,
            exclude,
            gitignore,
            no_commit,
        } => {
            let params = SyncParams {
                project_id: *project_id,
                path,
                extensions,
                exclude,
                gitignore: *gitignore,
                commit: !no_commit,
            };
            sync_once(&client, &params, verbose).await
        }
        GatewayCommands::Watch {
            project_id,
            path,
            extensions,
            exclude,
            gitignore,
            interval_secs,
        } => {
            let params = SyncParams {
                project_id: *project_id,
                path,
                extensions,
                exclude,
                gitignore: *gitignore,
                commit: true,
            };
            watch_loop(&client, &params, *interval_secs, verbose).await
        }
    }
}

/// Shared sync parameters for one-shot and watch passes.
struct SyncParams<'a> {
    project_id: i64,
    path: &'a str,
    extensions: &'a str,
    exclude: &'a str,
    gitignore: bool,
    commit: bool,
}

/// Scan the local directory and return usable entries keyed by identity.
async fn scan_local(params: &SyncParams<'_>) -> Result<ScanSnapshot> {
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
        root_path: params.path.to_string(),
        include_patterns: Vec::new(),
        exclude_patterns,
        follow_symlinks: false,
        respect_gitignore: params.gitignore,
        gitignore_patterns: Vec::new(),
        gitignore_path: None,
        max_content_size: None,
        max_file_size: None,
    };
    let mut scanner = FSScanner::new();
    let report = scanner
        .scan_report(&options)
        .context("gateway failed to scan the local directory")?;
    if !report.failures.is_empty() {
        eprintln!(
            "warning: {} paths could not be scanned and are skipped",
            report.failures.len()
        );
    }
    let mut files: HashMap<String, ScannedFile> = HashMap::new();
    let mut skipped_oversize = 0usize;
    let mut skipped_binary = 0usize;
    for entry in report.entries {
        if entry.size > MAX_INGEST_FILE_BYTES {
            skipped_oversize += 1;
            continue;
        }
        if !entry.is_text() {
            skipped_binary += 1;
            continue;
        }
        if !wanted.is_empty() {
            let matches = entry
                .relative_path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| wanted.contains(&ext.to_lowercase()))
                .unwrap_or(false);
            if !matches {
                continue;
            }
        }
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
    Ok(ScanSnapshot {
        files,
        skipped_oversize,
        skipped_binary,
    })
}

/// Usable scan result for gateway pushes.
struct ScanSnapshot {
    files: HashMap<String, ScannedFile>,
    skipped_oversize: usize,
    skipped_binary: usize,
}

/// One scannable file with its fingerprint.
struct ScannedFile {
    absolute: PathBuf,
    size: u64,
    modified_secs: i64,
    content_hash: Option<String>,
}

/// Push one full sync pass: manifest, missing contents, then commit.
async fn sync_once(client: &ApiClient, params: &SyncParams<'_>, verbose: bool) -> Result<()> {
    let snapshot = scan_local(params).await?;
    if verbose {
        println!(
            "scanned {} files (skipped {} oversize, {} binary)",
            snapshot.files.len(),
            snapshot.skipped_oversize,
            snapshot.skipped_binary
        );
    }
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
    };
    let manifest_url = format!("/api/project/{}/ingest/manifest", params.project_id);
    let manifest_response: IngestManifestResponse = client.post(&manifest_url, &manifest).await?;
    if verbose {
        println!(
            "manifest: {} unchanged, {} to upload",
            manifest_response.unchanged,
            manifest_response.upload.len()
        );
    }
    let (staged, errors) =
        upload_paths(client, params, &snapshot, &manifest_response.upload).await?;
    if !errors.is_empty() {
        for error in &errors {
            print_error(error);
        }
        anyhow::bail!("gateway staged {staged} files with {} errors", errors.len());
    }
    if params.commit {
        let commit_url = format!("/api/project/{}/ingest/commit", params.project_id);
        let commit: IngestCommitResponse = client.post(&commit_url, &serde_json::json!({})).await?;
        print_success(&format!(
            "gateway sync complete: {} files indexed, {} entities",
            commit.indexed_files, commit.total_entities
        ));
    } else {
        print_success(&format!("gateway staged {staged} files without commit"));
    }
    Ok(())
}

/// Upload the requested paths in bounded batches.
async fn upload_paths(
    client: &ApiClient,
    params: &SyncParams<'_>,
    snapshot: &ScanSnapshot,
    upload: &[String],
) -> Result<(usize, Vec<String>)> {
    let mut batch: Vec<IngestedFile> = Vec::new();
    let mut batch_bytes = 0u64;
    let mut staged = 0usize;
    let mut errors = Vec::new();
    for identity in upload {
        let Some(file) = snapshot.files.get(identity) else {
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
        batch_bytes += bytes.len() as u64;
        batch.push(IngestedFile {
            relative_path: identity.clone(),
            content_hash: file.content_hash.clone(),
            content_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
        });
        if batch.len() >= MAX_INGEST_BATCH_FILES || batch_bytes >= BATCH_RAW_BYTES {
            flush_batch(
                client,
                params.project_id,
                &mut batch,
                &mut batch_bytes,
                &mut staged,
                &mut errors,
            )
            .await;
        }
    }
    if !batch.is_empty() {
        flush_batch(
            client,
            params.project_id,
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
    client: &ApiClient,
    project_id: i64,
    batch: &mut Vec<IngestedFile>,
    batch_bytes: &mut u64,
    staged: &mut usize,
    errors: &mut Vec<String>,
) {
    let request = IngestBatchRequest {
        files: std::mem::take(batch),
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

/// Poll for local changes and push them as ingest events.
async fn watch_loop(
    client: &ApiClient,
    params: &SyncParams<'_>,
    interval_secs: u64,
    verbose: bool,
) -> Result<()> {
    sync_once(client, params, verbose).await?;
    let mut baseline: BTreeMap<String, Option<String>> = scan_local(params)
        .await?
        .files
        .into_iter()
        .map(|(identity, file)| (identity, file.content_hash))
        .collect();
    println!(
        "gateway watching {} (poll every {interval_secs}s)",
        params.path
    );
    let interval_secs = interval_secs.max(1);
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
        let snapshot = match scan_local(params).await {
            Ok(snapshot) => snapshot,
            Err(e) => {
                eprintln!("warning: rescan failed: {e:#}");
                continue;
            }
        };
        let current: BTreeMap<String, Option<String>> = snapshot
            .files
            .iter()
            .map(|(identity, file)| (identity.clone(), file.content_hash.clone()))
            .collect();
        let mut events = Vec::new();
        for (identity, hash) in &current {
            match baseline.get(identity) {
                None => {
                    if let Some(event) = read_event(&snapshot, identity, IngestEventKind::Created) {
                        events.push(event);
                    }
                }
                Some(previous) if previous != hash => {
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
        if events.len() > WATCH_FULL_SYNC_THRESHOLD {
            if let Err(e) = sync_once(client, params, verbose).await {
                eprintln!("warning: fallback sync failed: {e:#}");
                continue;
            }
        } else {
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
        }
        baseline = current;
    }
}

/// Read one changed file into an ingest event.
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
    Some(IngestEvent {
        relative_path: identity.to_string(),
        kind,
        content_hash: file.content_hash.clone(),
        content_base64: Some(base64::engine::general_purpose::STANDARD.encode(&bytes)),
    })
}
