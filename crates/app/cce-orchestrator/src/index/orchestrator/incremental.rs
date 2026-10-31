//! Incremental, single-file index operations.
//!
//! Single-file operations parse or evict one path from the document and
//! vector backends only. Relation graph changes always flow through the
//! hot-update incremental path (delta publish over the base cache), never
//! through direct builder writes: the full-index builder is round-local and
//! retired at publish, so any direct mutation would corrupt the shared
//! snapshot.

use cce_scanner::FileEntry;
use cce_types::ParsedFile;

use super::IndexOrchestrator;
use crate::error::OrchestratorError;

impl IndexOrchestrator {
    /// Index a single file (for incremental indexing).
    ///
    /// Parses the file without touching any relation graph. Callers that need
    /// relation coverage must route through the hot-update coordinator's
    /// incremental path.
    pub async fn index_file(
        &mut self,
        file_path: &std::path::Path,
    ) -> Result<ParsedFile, OrchestratorError> {
        let language_info =
            cce_types::language::LanguageInfo::detect_from_path(&file_path.to_string_lossy());

        let file_entry = FileEntry::new(
            file_path.to_path_buf(),
            file_path.to_path_buf(),
            0,
            chrono::Utc::now(),
        )
        .with_language_info(language_info);

        let (parsed, _) = self.file_processor.process_file(&file_entry).await?;

        Ok(parsed)
    }

    /// Remove a file from the index.
    ///
    /// Best-effort across backends: every stage runs even when an earlier
    /// one failed, and failures are aggregated instead of short-circuiting.
    /// A retry therefore converges (completed stages are idempotent) instead
    /// of leaving residue behind the failure point while skipping the rest.
    ///
    /// The generic storage removal already covers summary data: the vector
    /// delete is untyped (all `PointKind`s including legacy summary points)
    /// and the BM25 delete removes every document of the file.
    /// No separate summary call is made.
    ///
    /// Relation state is intentionally untouched here; deletions flow through
    /// the hot-update delta path.
    pub async fn remove_file(&self, file_path: &std::path::Path) -> Result<(), OrchestratorError> {
        if let Err(error) = self.storage.remove_file(file_path).await {
            // State tracking is in-memory and infallible: always clean it so a
            // retry observes no stale state for this file.
            self.state_tracker.remove_state(file_path).await;
            return Err(OrchestratorError::index(
                "remove_file",
                format!(
                    "partial failure removing {} (completed stages are safe to retry): storage: {error}",
                    file_path.display(),
                ),
            ));
        }

        // State tracking is in-memory and infallible: always clean it so a
        // retry observes no stale state for this file.
        self.state_tracker.remove_state(file_path).await;
        Ok(())
    }
}
