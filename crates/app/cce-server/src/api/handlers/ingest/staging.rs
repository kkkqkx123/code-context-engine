//! Shared staging foundation for gateway-driven ingest.
//!
//! Covers manifest-version chunk staging, chunk decoding and verification,
//! and staging reclamation shared by the manifest, batch, commit and event
//! entries. Project path resolution lives in the sibling paths module.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, SystemTime};

use base64::Engine as _;
use cce_api::models::{
    INGEST_CHUNK_BYTES, IngestedFile, MAX_INGEST_FILE_BYTES, total_chunks_for_size,
};

use super::paths::{safe_join, storage_path};

/// Retention for manifest-version chunk staging.
///
/// Days-scale so an interrupted push can resume across a long outage,
/// short enough that abandoned versions do not pile up on the remote host.
pub const INGEST_STAGING_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Chunks staged for one manifest version before further batch requests
/// for that version are refused.
pub(crate) const MAX_STAGING_CHUNKS_PER_VERSION: u64 = 1_048_576;

/// Bytes staged for one manifest version before further batch requests
/// for that version are refused.
pub(crate) const MAX_STAGING_BYTES_PER_VERSION: u64 = 64 * 1024 * 1024 * 1024;

/// Base directory holding received chunks for resume, outside the project
/// mirror so the scanner never indexes staging artifacts.
pub(crate) fn ingest_staging_base() -> PathBuf {
    std::env::temp_dir().join("cce-ingest")
}

/// Version staging directory for one project under an explicit base.
pub(crate) fn staging_root_in(base: &Path, project_id: i64, manifest_version: u64) -> PathBuf {
    base.join(format!("project-{project_id}"))
        .join(format!("manifest-{manifest_version}"))
}

/// Stable directory name for one relative path inside the staging root.
pub(crate) fn staging_file_dir(root: &Path, relative: &str) -> PathBuf {
    let digest = cce_utils::hash::calculate_hash(relative.as_bytes());
    root.join(digest)
}

/// Chunk usage recorded for one manifest version.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VersionStagingUsage {
    /// Chunk files on disk.
    pub(crate) chunks: u64,
    /// Raw bytes held by those chunk files.
    pub(crate) bytes: u64,
}

/// Process-local chunk usage per manifest version.
///
/// The version directory is walked the first time this process stages into
/// it; afterwards batch admission adjusts the counters in memory so the
/// check stays proportional to the request instead of the staged tree.
static STAGING_USAGE: LazyLock<Mutex<HashMap<(i64, u64), VersionStagingUsage>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Chunk usage counters for one version, walking staging on first touch.
pub(crate) fn staging_usage_snapshot(
    project_id: i64,
    manifest_version: u64,
) -> VersionStagingUsage {
    let mut registry = STAGING_USAGE.lock().unwrap_or_else(|p| p.into_inner());
    *registry
        .entry((project_id, manifest_version))
        .or_insert_with(|| {
            walk_version_staging_in(&ingest_staging_base(), project_id, manifest_version)
        })
}

/// Credit successfully staged chunks to a version's usage counters.
pub(crate) fn credit_staging_usage(
    project_id: i64,
    manifest_version: u64,
    pending: VersionStagingUsage,
) {
    if pending.chunks == 0 && pending.bytes == 0 {
        return;
    }
    let mut registry = STAGING_USAGE.lock().unwrap_or_else(|p| p.into_inner());
    let used = registry
        .entry((project_id, manifest_version))
        .or_insert_with(|| {
            walk_version_staging_in(&ingest_staging_base(), project_id, manifest_version)
        });
    used.chunks = used.chunks.saturating_add(pending.chunks);
    used.bytes = used.bytes.saturating_add(pending.bytes);
}

/// Drop the usage record for one version, e.g. after its directory went away.
pub(crate) fn forget_usage(project_id: i64, manifest_version: u64) {
    STAGING_USAGE
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(&(project_id, manifest_version));
}

/// Drop every usage record belonging to one project.
pub(crate) fn forget_project_usage(project_id: i64) {
    STAGING_USAGE
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .retain(|(project, _), _| *project != project_id);
}

/// Count the chunk files already staged for one version under an explicit base.
pub(crate) fn walk_version_staging_in(
    base: &Path,
    project_id: i64,
    manifest_version: u64,
) -> VersionStagingUsage {
    let root = staging_root_in(base, project_id, manifest_version);
    let mut usage = VersionStagingUsage::default();
    let Ok(file_dirs) = std::fs::read_dir(&root) else {
        return usage;
    };
    for file_dir in file_dirs.flatten() {
        let Ok(chunks) = std::fs::read_dir(file_dir.path()) else {
            continue;
        };
        for chunk in chunks.flatten() {
            let Ok(metadata) = chunk.metadata() else {
                continue;
            };
            usage.chunks = usage.chunks.saturating_add(1);
            usage.bytes = usage.bytes.saturating_add(metadata.len());
        }
    }
    usage
}

/// Whether staged plus pending usage stays inside the per-version bounds.
pub(crate) fn within_staging_bounds(
    used: VersionStagingUsage,
    pending: VersionStagingUsage,
) -> bool {
    used.chunks.saturating_add(pending.chunks) <= MAX_STAGING_CHUNKS_PER_VERSION
        && used.bytes.saturating_add(pending.bytes) <= MAX_STAGING_BYTES_PER_VERSION
}

/// Drop manifest-version staging directories older than `retention`.
///
/// Directory modification time decides expiry. Removal failures are only
/// logged, so reclaiming orphans never blocks the ingest flow.
pub fn sweep_expired_staging(retention: Duration) {
    sweep_expired_staging_in(&ingest_staging_base(), retention);
}

/// Sweep every project's staging base for expired manifest versions.
pub(crate) fn sweep_expired_staging_in(base: &Path, retention: Duration) {
    let Ok(projects) = std::fs::read_dir(base) else {
        return;
    };
    for project in projects.flatten() {
        let file_name = project.file_name();
        let Some(name) = file_name.to_str().and_then(|n| n.strip_prefix("project-")) else {
            continue;
        };
        let Ok(project_id) = name.parse::<i64>() else {
            continue;
        };
        sweep_project_staging_in(base, project_id, retention);
    }
}

/// Sweep one project's expired manifest versions before a commit indexes.
pub(crate) fn sweep_project_staging(project_id: i64, retention: Duration) {
    sweep_project_staging_in(&ingest_staging_base(), project_id, retention);
}

/// Remove a project's manifest directories whose age reaches `retention`.
pub(crate) fn sweep_project_staging_in(base: &Path, project_id: i64, retention: Duration) {
    let root = base.join(format!("project-{project_id}"));
    let Ok(versions) = std::fs::read_dir(&root) else {
        return;
    };
    for version in versions.flatten() {
        let file_name = version.file_name();
        let Some(name) = file_name.to_str().and_then(|n| n.strip_prefix("manifest-")) else {
            continue;
        };
        let Ok(manifest_version) = name.parse::<u64>() else {
            continue;
        };
        let Ok(metadata) = version.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        let Ok(age) = SystemTime::now().duration_since(modified) else {
            continue;
        };
        if age < retention {
            continue;
        }
        let removed = match std::fs::remove_dir_all(version.path()) {
            Ok(()) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
            Err(e) => {
                tracing::warn!(
                    project_id,
                    manifest_version,
                    error = %e,
                    "Failed to remove expired ingest staging"
                );
                false
            }
        };
        if removed {
            forget_usage(project_id, manifest_version);
        }
    }
}

/// Chunks and bytes one decoded request would add to a version's staging.
///
/// Pieces already on disk are resumed rather than counted again, so
/// re-pushing a version stays idempotent for admission as well as storage.
pub(crate) fn pending_pieces_in(
    base: &Path,
    project_id: i64,
    manifest_version: u64,
    file: &DecodedFile<'_>,
) -> VersionStagingUsage {
    let path = storage_path(file.relative);
    let received: HashSet<u32> =
        received_chunk_indices_in(base, project_id, manifest_version, &path)
            .into_iter()
            .collect();
    let mut pending = VersionStagingUsage::default();
    for (index, bytes) in &file.pieces {
        if !received.contains(index) {
            pending.chunks = pending.chunks.saturating_add(1);
            pending.bytes = pending.bytes.saturating_add(bytes.len() as u64);
        }
    }
    pending
}

/// Indices already stored for one file in the given manifest version under
/// an explicit staging base.
pub(crate) fn received_chunk_indices_in(
    base: &Path,
    project_id: i64,
    manifest_version: u64,
    relative: &str,
) -> Vec<u32> {
    if manifest_version == 0 {
        return Vec::new();
    }
    let dir = staging_file_dir(
        &staging_root_in(base, project_id, manifest_version),
        relative,
    );
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str()
            && let Some(index) = name.strip_prefix("chunk_")
            && let Ok(index) = index.parse::<u32>()
        {
            out.push(index);
        }
    }
    out.sort_unstable();
    out
}

/// Indices already stored for one file in the given manifest version.
pub(crate) fn received_chunk_indices(
    project_id: i64,
    manifest_version: u64,
    relative: &str,
) -> Vec<u32> {
    received_chunk_indices_in(
        &ingest_staging_base(),
        project_id,
        manifest_version,
        relative,
    )
}

/// Decode one chunk payload, decompressing when negotiated, and verify the
/// per-chunk hash of the uncompressed bytes. Corrupted pieces are rejected
/// without touching previously stored chunks.
pub(crate) fn decode_chunk_payload(
    relative: &str,
    encoded: &str,
    compressed: bool,
    expected_chunk_hash: Option<&str>,
) -> Result<Vec<u8>, String> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| format!("{relative}: content is not valid base64: {e}"))?;
    let bytes = cce_supply::decompress_chunk(raw.as_slice(), compressed)
        .map_err(|e| format!("{relative}: {e}"))?;
    if bytes.len() > INGEST_CHUNK_BYTES + 1024 {
        return Err(format!(
            "{relative}: chunk exceeds the {INGEST_CHUNK_BYTES} byte bound"
        ));
    }
    if let Some(expected) = expected_chunk_hash {
        let actual = cce_supply::chunk_hash(&bytes);
        if actual != expected {
            return Err(format!(
                "{relative}: chunk hash mismatch; a fresh upload of the piece is required"
            ));
        }
    }
    Ok(bytes)
}

/// Stage one decoded payload under the project mirror.
pub(crate) async fn stage_bytes(
    root: &Path,
    relative: &str,
    bytes: &[u8],
    expected_hash: Option<&str>,
) -> Result<(), String> {
    if bytes.len() as u64 > MAX_INGEST_FILE_BYTES {
        return Err(format!(
            "{relative} exceeds the {MAX_INGEST_FILE_BYTES} byte ingest bound"
        ));
    }
    if let Some(expected) = expected_hash {
        let actual = cce_supply::chunk_hash(bytes);
        if actual != expected {
            return Err(format!(
                "{relative} changed between manifest and upload; a fresh manifest is required"
            ));
        }
    }
    let dest = safe_join(root, relative)?;
    if let Some(parent) = dest.parent()
        && let Err(e) = tokio::fs::create_dir_all(parent).await
    {
        return Err(format!("failed to create parent of {relative}: {e}"));
    }
    tokio::fs::write(&dest, bytes)
        .await
        .map_err(|e| format!("failed to stage {relative}: {e}"))
}

/// Upper bound on chunks for one file, derived from the 1 MiB file cap.
pub(crate) fn max_file_chunks() -> u32 {
    total_chunks_for_size(MAX_INGEST_FILE_BYTES)
}

/// One file's chunks decoded and verified, ready to be staged.
#[derive(Debug)]
pub(crate) struct DecodedFile<'a> {
    /// Gateway-relative path of the file.
    pub(crate) relative: &'a str,
    /// Declared chunk count of the file.
    pub(crate) total: u32,
    /// Whole-file hash the reassembled bytes must match.
    pub(crate) content_hash: Option<&'a str>,
    /// Decoded pieces keyed by chunk index.
    pub(crate) pieces: BTreeMap<u32, Vec<u8>>,
    /// Chunks this request would add to version staging.
    pub(crate) pending_chunks: u64,
    /// Bytes this request would add to version staging.
    pub(crate) pending_bytes: u64,
}

/// Decode and verify every chunk of one file inside a batch.
///
/// Group validation is identical for every manifest version: the declared
/// total must be positive and within the file bound, all pieces of a file
/// must agree on the total and whole-file hash, indices must stay in range
/// and unique, and each payload must pass per-chunk verification.
pub(crate) fn decode_group<'a>(
    relative: &'a str,
    entries: &[&'a IngestedFile],
) -> Result<DecodedFile<'a>, String> {
    let Some(first) = entries.first() else {
        return Err(format!("{relative}: chunk group is empty"));
    };
    let total = first.total_chunks;
    if total == 0 {
        return Err(format!("{relative}: total chunks must be positive"));
    }
    if total > max_file_chunks() {
        return Err(format!(
            "{relative}: declares {total} chunks, more than the {} bound",
            max_file_chunks()
        ));
    }
    let content_hash = first.content_hash.as_deref();
    let mut pieces: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
    for entry in entries {
        if entry.total_chunks != total {
            return Err(format!(
                "{relative}: chunk {} declares {} total chunks while the group started with {total}",
                entry.chunk_index, entry.total_chunks
            ));
        }
        if entry.content_hash.as_deref() != content_hash {
            return Err(format!(
                "{relative}: chunk {} carries a different whole-file hash",
                entry.chunk_index
            ));
        }
        if entry.chunk_index >= total {
            return Err(format!(
                "{relative}: chunk {} exceeds {total}",
                entry.chunk_index
            ));
        }
        if pieces.contains_key(&entry.chunk_index) {
            return Err(format!(
                "{relative}: chunk {} appears more than once in the batch",
                entry.chunk_index
            ));
        }
        let bytes = decode_chunk_payload(
            relative,
            &entry.content_base64,
            entry.compressed,
            entry.chunk_hash.as_deref(),
        )?;
        pieces.insert(entry.chunk_index, bytes);
    }
    Ok(DecodedFile {
        relative,
        total,
        content_hash,
        pieces,
        pending_chunks: 0,
        pending_bytes: 0,
    })
}

/// Parameters for staging one decoded file.
pub(crate) struct FileStageParams<'a> {
    /// Project mirror root.
    pub(crate) root: &'a Path,
    /// Staging base directory holding manifest versions.
    pub(crate) staging_base: &'a Path,
    /// Project the file belongs to.
    pub(crate) project_id: i64,
    /// Manifest version of the batch.
    pub(crate) manifest_version: u64,
    /// Decoded file to stage.
    pub(crate) file: &'a DecodedFile<'a>,
}

/// Store one decoded file into the mirror for its manifest version.
///
/// Chunk bytes are already individually verified by the group decoder;
/// the reassembled file is verified against the whole-file hash by
/// `stage_bytes` before it reaches the mirror, so a corrupted piece can
/// never poison a completed file. Version zero keeps no resume bookkeeping
/// and therefore requires every piece inside this batch; versioned files
/// persist their pieces first and reassemble from staging, tolerating
/// pieces from earlier batches. Returns whether the whole file reached the
/// mirror.
pub(crate) async fn stage_file(params: FileStageParams<'_>) -> Result<bool, String> {
    let FileStageParams {
        root,
        staging_base,
        project_id,
        manifest_version,
        file,
    } = params;
    let relative = file.relative;
    let path = storage_path(relative);
    if manifest_version == 0 {
        if file.pieces.len() as u32 != file.total {
            return Err(format!(
                "{relative}: manifest version zero carries no resume bookkeeping; the whole file must arrive in one batch (received {} of {} chunks)",
                file.pieces.len(),
                file.total
            ));
        }
        let assembled: Vec<u8> = file
            .pieces
            .values()
            .flat_map(|piece| piece.iter().copied())
            .collect();
        stage_bytes(root, &path, &assembled, file.content_hash).await?;
        return Ok(true);
    }
    let file_dir = staging_file_dir(
        &staging_root_in(staging_base, project_id, manifest_version),
        &path,
    );
    std::fs::create_dir_all(&file_dir)
        .map_err(|e| format!("{relative}: failed to stage chunk: {e}"))?;
    for (index, bytes) in &file.pieces {
        let chunk_path = file_dir.join(format!("chunk_{index}"));
        std::fs::write(&chunk_path, bytes)
            .map_err(|e| format!("{relative}: failed to stage chunk: {e}"))?;
    }
    let Some(assembled) = read_assembled_pieces(&file_dir, file.total, relative)? else {
        return Ok(false);
    };
    stage_bytes(root, &path, &assembled, file.content_hash).await?;
    Ok(true)
}

/// Read every staged chunk of one file back in index order.
///
/// Returns `None` while any piece is still missing, so the caller reports
/// a partial file instead of a failure.
pub(crate) fn read_assembled_pieces(
    file_dir: &Path,
    total_chunks: u32,
    relative: &str,
) -> Result<Option<Vec<u8>>, String> {
    let mut assembled = Vec::new();
    for index in 0..total_chunks {
        let candidate = file_dir.join(format!("chunk_{index}"));
        match std::fs::read(&candidate) {
            Ok(piece) => assembled.extend_from_slice(&piece),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("{relative}: failed to reassemble: {e}")),
        }
    }
    Ok(Some(assembled))
}

/// Remove chunk staging for a project after a successful commit.
pub(crate) fn clear_project_staging(project_id: i64) {
    clear_project_staging_in(&ingest_staging_base(), project_id);
}

/// Drop a project's whole staging tree under an explicit base.
pub(crate) fn clear_project_staging_in(base: &Path, project_id: i64) {
    let root = base.join(format!("project-{project_id}"));
    if let Err(e) = std::fs::remove_dir_all(&root)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!("Failed to clear ingest staging for project {project_id}: {e}");
    }
    forget_project_usage(project_id);
}
