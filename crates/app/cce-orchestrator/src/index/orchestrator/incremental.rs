//! Incremental, single-file index operations.
//!
//! Hot updates index or remove one file at a time: `index_file` parses a single
//! path, `remove_file` removes it from all backends, and the relation
//! maintenance methods (`clear_relations*`) plus the relation builder accessors
//! support incremental graph repair.

use cce_relation::index::{FileLevelOps, RelationQueryOps};
use cce_scanner::FileEntry;
use cce_types::ParsedFile;

use super::IndexOrchestrator;
use crate::error::OrchestratorError;

impl IndexOrchestrator {
    /// Index a single file (for incremental indexing)
    pub async fn index_file(
        &mut self,
        file_path: &std::path::Path,
    ) -> Result<ParsedFile, OrchestratorError> {
        let language_info =
            cce_types::language::LanguageInfo::detect_from_path(file_path.to_str().unwrap_or(""));

        let file_entry = FileEntry::new(
            file_path.to_path_buf(),
            file_path.to_path_buf(),
            0,
            chrono::Utc::now(),
        )
        .with_language_info(language_info);

        let (parsed, _) = self.file_processor.process_file(&file_entry).await?;

        // Add to relation builder if present
        if let Some(ref builder) = self.relation_builder {
            builder.add_parsed_file(&parsed);
        }

        Ok(parsed)
    }

    /// Remove a file from the index
    pub async fn remove_file(&self, file_path: &std::path::Path) -> Result<(), OrchestratorError> {
        // Remove from relation index first
        if let Some(ref builder) = self.relation_builder {
            let file_id = file_path.to_string_lossy().to_string();
            builder.index().remove_file(&file_id);
        }

        // Remove from storage backends
        self.storage.remove_file(file_path).await?;

        // Remove from summary index
        self.storage.remove_file_from_summary(file_path).await?;

        // Remove from state tracker
        self.state_tracker.remove_state(file_path).await;

        Ok(())
    }
}

impl IndexOrchestrator {
    /// Get the relation builder (immutable reference)
    pub fn get_relation_builder(&self) -> Option<&cce_relation::IndexBuilder> {
        self.relation_builder.as_ref()
    }

    /// Get the relation builder (mutable reference)
    pub fn get_relation_builder_mut(&mut self) -> Option<&mut cce_relation::IndexBuilder> {
        self.relation_builder.as_mut()
    }

    /// Clear all relations from the index
    pub fn clear_relations(&mut self) -> usize {
        if let Some(ref builder) = self.relation_builder {
            let count = builder.index().resolved_relation_count();
            builder.clear();
            tracing::info!("Cleared {} relations from index", count);
            count
        } else {
            0
        }
    }

    /// Clear relations for a specific file from the index
    pub fn clear_relations_for_file(&mut self, file_path: &str) -> usize {
        if let Some(ref builder) = self.relation_builder {
            // Count relations before removal
            let relation_count_before = builder.index().resolved_relation_count();

            // Remove file from relation index (this removes entities and their relations)
            builder.index().remove_file(file_path);

            let relation_count_after = builder.index().resolved_relation_count();

            relation_count_before.saturating_sub(relation_count_after)
        } else {
            0
        }
    }
}
