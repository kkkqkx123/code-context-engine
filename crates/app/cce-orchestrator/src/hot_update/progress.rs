//! Per-file module progress markers for recovery skipping.
//!
//! Each storage-backed module records a fingerprint of the inputs that
//! produced its stored data for a file. On recovery, a module skips a file
//! when its recorded fingerprint still matches the current inputs, meaning the
//! module's work for that file already completed against a durable candidate.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::hot_update::error::{HotUpdateError, Result};
use crate::operation::{CheckpointManager, OperationContext};

/// Progress marker key for the embedding module.
pub const MODULE_EMBEDDING: &str = "embedding";
/// Progress marker key for the BM25 module.
pub const MODULE_BM25: &str = "bm25";
/// Progress marker key for the summary module.
pub const MODULE_SUMMARY: &str = "summary";

/// Parse the persisted module progress JSON into a map.
pub fn read_module_progress(json: Option<&str>) -> HashMap<String, String> {
    match json {
        Some(raw) => match serde_json::from_str(raw) {
            Ok(progress) => progress,
            Err(error) => {
                tracing::warn!(error = %error, "Corrupt module progress; resetting to empty");
                HashMap::new()
            }
        },
        None => HashMap::new(),
    }
}

/// Strict variant that surfaces corruption instead of resetting.
pub fn read_module_progress_strict(
    json: Option<&str>,
) -> std::result::Result<HashMap<String, String>, serde_json::Error> {
    match json {
        Some(raw) => serde_json::from_str(raw),
        None => Ok(HashMap::new()),
    }
}

/// Serialize the module progress map to JSON.
pub fn write_module_progress(progress: &HashMap<String, String>) -> String {
    serde_json::to_string(progress).unwrap_or_else(|error| {
        tracing::warn!(error = %error, "Failed to serialize module progress; storing empty map");
        "{}".to_string()
    })
}

/// Compute the module input fingerprint persisted in a progress marker.
///
/// Covers every input that determines the module's stored data for a file: the
/// module/configuration fingerprint plus the content hash. Recovery skips a
/// file only when this recomputed fingerprint still matches the recorded one,
/// so a configuration change or a content change between a crash and its
/// resume forces the module to redo the file.
pub fn module_input_fingerprint(config_fingerprint: &str, content_hash: &str) -> String {
    let mut buf = Vec::with_capacity(config_fingerprint.len() + content_hash.len() + 1);
    buf.extend_from_slice(config_fingerprint.as_bytes());
    buf.push(b'\x1f');
    buf.extend_from_slice(content_hash.as_bytes());
    cce_utils::hash::calculate_hash(&buf)
}

/// Record a completed module for a file on its checkpoint, merging with any
/// existing progress. Called after the module's data for the file was durably
/// written into the candidate generation.
pub async fn persist_module_progress(
    checkpoint_manager: &Option<Arc<CheckpointManager>>,
    ctx: &OperationContext,
    file_path: &Path,
    module: &str,
    fingerprint: &str,
) -> Result<()> {
    let Some(cm) = checkpoint_manager else {
        return Ok(());
    };
    let path = file_path.to_string_lossy().to_string();
    let mut progress = cm
        .get_file_checkpoint(&ctx.operation_id, &path)
        .await
        .map_err(|e| HotUpdateError::hot_update(e.to_string()))?
        .map(|record| read_module_progress(record.module_progress.as_deref()))
        .unwrap_or_default();
    progress.insert(module.to_string(), fingerprint.to_string());

    let mut record = cm
        .create_file_checkpoint(&ctx.operation_id, 0, &path)
        .await
        .map_err(|e| HotUpdateError::hot_update(e.to_string()))?;
    record.module_progress = Some(write_module_progress(&progress));
    cm.save_file_checkpoint(&record)
        .await
        .map_err(|e| HotUpdateError::hot_update(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_progress_reads_as_empty() {
        assert!(read_module_progress(None).is_empty());
        assert!(read_module_progress_strict(None).is_ok_and(|map| map.is_empty()));
    }

    #[test]
    fn progress_roundtrips_through_json() {
        let mut progress = HashMap::new();
        progress.insert(MODULE_EMBEDDING.to_string(), "fp-emb".to_string());
        progress.insert(MODULE_BM25.to_string(), "fp-bm25".to_string());

        let encoded = write_module_progress(&progress);
        assert_eq!(read_module_progress(Some(&encoded)), progress);
    }

    /// Corruption resets the marker map for the lenient reader, so the module
    /// redoes the file instead of skipping work that may never have landed;
    /// the strict reader surfaces it for callers that must not guess.
    #[test]
    fn corrupt_progress_resets_or_reports() {
        assert!(read_module_progress(Some("{not json")).is_empty());
        assert!(read_module_progress_strict(Some("{not json")).is_err());
    }

    /// The fingerprint must react to every input that changes the stored data:
    /// a configuration change or a content change each produce a new value,
    /// and the separator prevents concatenation collisions.
    #[test]
    fn fingerprint_covers_configuration_and_content() {
        let base = module_input_fingerprint("cfg-a", "content-a");
        assert_eq!(base, module_input_fingerprint("cfg-a", "content-a"));
        assert_ne!(base, module_input_fingerprint("cfg-b", "content-a"));
        assert_ne!(base, module_input_fingerprint("cfg-a", "content-b"));
        assert_ne!(
            module_input_fingerprint("ab", "c"),
            module_input_fingerprint("a", "bc"),
            "the separator must keep the two inputs distinguishable"
        );
    }
}
