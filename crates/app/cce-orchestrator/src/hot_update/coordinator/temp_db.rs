use crate::hot_update::error::{HotUpdateError, Result};
use crate::index::vector_store::RelationStore;

/// Create a temporary metadata store for the change detector.
///
/// Prefers an in-memory store. The error is propagated instead of panicking
/// when none can be created.
pub(crate) fn create_temp_db() -> Result<RelationStore> {
    RelationStore::in_memory().map_err(|e| {
        HotUpdateError::config(format!(
            "Cannot create metadata store for change detector: {e}"
        ))
    })
}
