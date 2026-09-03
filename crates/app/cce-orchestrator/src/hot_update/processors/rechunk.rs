//! Shared helpers for regeneration sweeps driven by configuration drift.
//!
//! Sweeps refresh derived data (vectors, BM25 documents) by re-running the
//! local parse pipeline from on-disk content under the new configuration, so
//! unchanged source files never wait for an LLM round-trip to catch up.

use cce_storage_sqlite::SqliteClient;
use cce_types::StorageError;

/// Drift detection outcome for a persisted string fingerprint.
pub(crate) enum FingerprintDrift {
    /// Stored value matches; nothing to do.
    Current,
    /// No stored value yet (first run / fresh database). The current value is
    /// recorded as baseline without triggering a sweep.
    BaselineWritten,
    /// Stored value differs from the current configuration; a sweep is due.
    Drifted,
}

/// Compare `current` against the stored fingerprint under `key`.
pub(crate) fn detect_fingerprint_drift(
    client: &SqliteClient,
    project_id: i64,
    key: &str,
    current: &str,
) -> Result<FingerprintDrift, StorageError> {
    match client.project_meta_get_string_optional(project_id, key)? {
        Some(stored) if stored == current => Ok(FingerprintDrift::Current),
        None => {
            persist_fingerprint(client, project_id, key, current)?;
            Ok(FingerprintDrift::BaselineWritten)
        }
        Some(_) => Ok(FingerprintDrift::Drifted),
    }
}

/// Persist the fingerprint after a successful sweep.
pub(crate) fn persist_fingerprint(
    client: &SqliteClient,
    project_id: i64,
    key: &str,
    value: &str,
) -> Result<(), StorageError> {
    client.project_meta_set_string(project_id, key, value)
}

/// Resolve the project root directory used to read swept files from disk.
pub(crate) fn resolve_project_root(
    client: &SqliteClient,
    project_id: i64,
) -> Result<std::path::PathBuf, StorageError> {
    let conn = client
        .read_connection()
        .map_err(|e| StorageError::query(format!("Failed to open read connection: {e}")))?;
    let root: String = conn
        .query_row(
            "SELECT root_path FROM projects WHERE id = ?1",
            [project_id],
            |row| row.get(0),
        )
        .map_err(|e| StorageError::query(format!("Failed to query project root: {e}")))?;
    Ok(std::path::PathBuf::from(root))
}

/// Collect the project-relative paths the current operation will process
/// through the normal change flow.
///
/// Chunking-drift sweeps skip these: sweeping them would rewrite stale
/// content from the ancestor generation that the normal flow immediately
/// replaces with fresh work anyway.
pub(crate) fn operation_changed_paths(
    batch_result: &crate::hot_update::BatchChangeResult,
) -> std::collections::HashSet<String> {
    batch_result
        .parse_results
        .iter()
        .map(|result| result.file_path.to_string_lossy().to_string())
        .chain(
            batch_result
                .file_changes
                .iter()
                .map(|change| change.path.to_string_lossy().to_string()),
        )
        .collect()
}
