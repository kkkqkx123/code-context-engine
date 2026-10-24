//! Full sync pass: manifest, missing contents, and commit.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use base64::Engine as _;

use cce_api::models::{
    INGEST_CHUNK_BYTES, IngestBatchRequest, IngestCommitResponse, IngestFileMeta,
    IngestManifestRequest, IngestManifestResponse, IngestedFile, MAX_INGEST_BATCH_FILES,
    MAX_INGEST_CHUNKS_PER_BATCH, MAX_INGEST_FILE_BYTES, MissingChunk, total_chunks_for_size,
};
use cce_api::paths::GATEWAY_BATCH_WIRE_BYTES;

use super::client::GatewayClient;
use super::params::SyncParams;
use super::scan::{
    ScanOutcome, ScanSnapshot, load_cached_entries, manifest_version_for_snapshot,
    save_cached_entries, scan_local,
};

/// Push one full sync pass: manifest, missing contents, then commit.
///
/// Upload batches account base64 wire length against the shared API wire
/// budget so a batch never exceeds the admission body limit after growth.
/// When a scan cache file is configured, the pass loads the previous entries
/// and scans incrementally so unchanged files skip re-hashing; without a
/// cache the pass scans everything. The manifest still covers the whole tree
/// and change detection still compares fingerprints, so reuse only saves
/// reads. The returned outcome seeds the watch loop's baseline and its next
/// incremental pass.
pub async fn sync_once(
    client: &GatewayClient,
    params: &SyncParams,
    verbose: bool,
) -> Result<ScanOutcome> {
    let cached = params
        .cache_file
        .as_deref()
        .map(load_cached_entries)
        .unwrap_or_default();
    let previous = if cached.is_empty() {
        None
    } else {
        Some(cached)
    };
    let outcome = scan_local(params, previous.as_ref()).await?;
    if let Some(cache_file) = params.cache_file.as_deref()
        && let Err(e) = save_cached_entries(cache_file, &outcome.entries)
    {
        eprintln!("warning: scan cache write failed: {e:#}");
    }
    let snapshot = &outcome.snapshot;
    if params.json_progress {
        println!(
            "{}",
            serde_json::json!({
                "kind": "scan",
                "files": snapshot.files.len(),
                "skipped_oversize": snapshot.skipped_oversize,
                "skipped_binary": snapshot.skipped_binary,
            })
        );
    } else if verbose {
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
    let manifest_response: IngestManifestResponse =
        client.post_manifest(params.project_id, &manifest).await?;
    if params.json_progress {
        println!(
            "{}",
            serde_json::json!({
                "kind": "manifest",
                "manifest_version": manifest_response.manifest_version,
                "unchanged": manifest_response.unchanged,
                "upload_files": manifest_response.upload.len(),
                "missing_chunks": manifest_response.missing_chunks.len(),
                "compressed": params.compress,
            })
        );
    } else if verbose {
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
        let commit: IngestCommitResponse = client
            .post_commit(params.project_id, &serde_json::json!({}))
            .await?;
        if params.json_progress {
            println!(
                "{}",
                serde_json::json!({
                    "kind": "sync_complete",
                    "indexed_files": commit.indexed_files,
                    "total_entities": commit.total_entities,
                })
            );
        } else {
            println!(
                "gateway sync complete: {} files indexed, {} entities",
                commit.indexed_files, commit.total_entities
            );
        }
    } else if params.json_progress {
        println!(
            "{}",
            serde_json::json!({
                "kind": "staged",
                "staged": staged,
                "commit": false,
            })
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
            && cce_supply::chunk_hash(&bytes) != expected
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
                || batch_bytes >= GATEWAY_BATCH_WIRE_BYTES
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
    match client.post_batch(project_id, &request).await {
        Ok(response) => {
            *staged += response.staged;
            errors.extend(response.errors);
        }
        Err(e) => errors.push(format!("batch upload failed: {e:#}")),
    }
}

/// Hash one chunk of raw bytes for per-chunk verification.
pub fn chunk_hash(bytes: &[u8]) -> String {
    cce_supply::chunk_hash(bytes)
}

/// Optionally compress chunk bytes. Compression stays off unless the
/// gateway explicitly enables it; the server decompresses before hashing.
pub fn maybe_compress(bytes: &[u8], compress: bool) -> (Vec<u8>, bool) {
    cce_supply::maybe_compress(bytes, compress)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supply_wrappers_delegate_to_shared_kernel() {
        assert_eq!(chunk_hash(b"hello"), cce_supply::chunk_hash(b"hello"));
        let repetitive = vec![b'a'; 64];
        let (plain, compressed) = maybe_compress(&repetitive, false);
        assert!(!compressed);
        assert_eq!(plain.len(), repetitive.len());
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
    fn compression_keeps_incompressible_and_empty_bytes_raw() {
        let mut randomish = Vec::with_capacity(2048);
        let mut state: u64 = 0x1234_5678_9abc_def0;
        for _ in 0..2048 {
            state = state
                .wrapping_mul(0x1000_0000_01b3)
                .wrapping_add(0xcbf2_9ce4_8422_2325);
            randomish.push((state >> 33) as u8);
        }
        let (kept, compressed) = maybe_compress(&randomish, true);
        assert!(!compressed);
        assert_eq!(kept, randomish);
        let (empty, compressed) = maybe_compress(&[], true);
        assert!(!compressed);
        assert!(empty.is_empty());
    }
}
