use crate::hot_update::error::{HotUpdateError, Result};
use crate::index::vector_store::{MetadataStore, in_memory_metadata_store};

/// Create a temporary metadata store for the change detector.
///
/// Prefers an in-memory store. The error is propagated instead of panicking
/// when none can be created.
pub(crate) fn create_temp_db() -> Result<MetadataStore> {
    in_memory_metadata_store().map_err(|e| {
        HotUpdateError::config(format!(
            "Cannot create metadata store for change detector: {e}"
        ))
    })
}
