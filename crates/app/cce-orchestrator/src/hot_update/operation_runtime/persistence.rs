//! Hot-update operation runtime.
//!
//! The background worker previously held the entire
//! `Arc<Mutex<HotUpdateCoordinator>>` while running a complete operation
//! (parse + embed + publish), blocking watch-status/stop-watch/mode-switch
//! APIs for the whole operation duration. The operation-critical mutable
//! state lives in this sub-component with its own mutex: the worker locks
//! only the runtime while an operation runs, leaving the coordinator free
//! for event accumulation, storm detection and configuration handling
//! (those paths already run on cloned components and never contend here).

use std::path::{Path, PathBuf};

use cce_scanner::FileEntry;
use cce_storage_sqlite::repo::{ChunkRepository, ProjectIndexManifestRepository};
use rusqlite::OptionalExtension;

use crate::hot_update::change::{BatchChangeResult, FileChangeType};
use crate::hot_update::error::{HotUpdateError, Result};
use crate::hot_update::file_processor::FileProcessor;
use crate::operation::checkpoint::{
    ParsedCheckpointEnvelope, ParsedCheckpointPayload, SummaryCheckpointPayload,
};
use crate::operation::{
    AggregatedMetrics, ModuleFailure, OperationContext, OperationResult, OperationStatus,
    OperationSummary,
};

/// Mutable state shared by every hot-update operation.
///
/// Guarded by its own mutex so the background worker can run a long
/// operation while other coordinator APIs (watch status, stop watch, mode
/// checks) keep working under the coordinator lock.
use super::HotUpdateOperationRuntime;
impl HotUpdateOperationRuntime {
    /// Create a hot-update checkpoint with the changed file list
    pub(crate) async fn persist_hot_parse_checkpoints(
        &self,
        ctx: &OperationContext,
        batch_result: &BatchChangeResult,
        cm: &crate::operation::CheckpointManager,
    ) -> Result<()> {
        for parse_result in &batch_result.parse_results {
            let path = parse_result.file_path.to_string_lossy().to_string();
            let mut record = cm
                .create_file_checkpoint(&ctx.operation_id, 0, &path)
                .await
                .map_err(|error| HotUpdateError::hot_update(error.to_string()))?;
            let envelope = ParsedCheckpointEnvelope::new(
                parse_result.file_change_type,
                parse_result.parsed_file.clone(),
            );
            record.language = Some(parse_result.parsed_file.language.to_string());
            record.file_size = Some(parse_result.parsed_file.source.len() as i64);
            record.content_hash = Some(cce_utils::hash::calculate_hash(
                parse_result.parsed_file.source.as_bytes(),
            ));
            record.parsed_data = Some(
                crate::operation::checkpoint::encode_parsed_checkpoint(
                    &ParsedCheckpointPayload::Parsed(Box::new(envelope)),
                )
                .map_err(|error| HotUpdateError::hot_update(error.to_string()))?,
            );
            // Carry through a summary restored from a previous run's
            // checkpoint so a crash between the parse and summary phases
            // keeps it durable; live runs write it after the summary phase.
            if let Some(summary) = &parse_result.file_summary {
                let payload = SummaryCheckpointPayload::new(
                    summary.clone(),
                    crate::operation::checkpoint::plugin_fingerprint_for(
                        parse_result.parsed_file.language,
                    ),
                    parse_result.summary_fingerprint.clone(),
                );
                record.summary_data = Some(
                    crate::operation::checkpoint::encode_summary_checkpoint(&payload)
                        .map_err(|error| HotUpdateError::hot_update(error.to_string()))?,
                );
            }
            cm.save_file_checkpoint(&record)
                .await
                .map_err(|error| HotUpdateError::hot_update(error.to_string()))?;
        }

        for change in &batch_result.file_changes {
            if change.change_type != FileChangeType::Deleted {
                continue;
            }
            let path = change.path.to_string_lossy().to_string();
            let mut record = cm
                .create_file_checkpoint(&ctx.operation_id, 0, &path)
                .await
                .map_err(|error| HotUpdateError::hot_update(error.to_string()))?;
            record.content_hash = Some(change.content_hash.clone());
            record.file_size = Some(change.size as i64);
            record.parsed_data = Some(
                crate::operation::checkpoint::encode_parsed_checkpoint(
                    &ParsedCheckpointPayload::Deleted,
                )
                .map_err(|error| HotUpdateError::hot_update(error.to_string()))?,
            );
            cm.save_file_checkpoint(&record)
                .await
                .map_err(|error| HotUpdateError::hot_update(error.to_string()))?;
        }
        Ok(())
    }

    /// Refresh parse checkpoints with the file summaries produced by the
    /// summary processor.
    pub(crate) async fn persist_summaries_to_checkpoints(
        &self,
        ctx: &OperationContext,
        batch_result: &BatchChangeResult,
        cm: &crate::operation::CheckpointManager,
    ) -> Result<()> {
        let entries: Vec<(String, cce_parser::summary::FileSummary, Option<String>)> = batch_result
            .parse_results
            .iter()
            .filter_map(|parse_result| {
                let path = parse_result.file_path.to_string_lossy().to_string();
                let plugin_fingerprint = crate::operation::checkpoint::plugin_fingerprint_for(
                    parse_result.parsed_file.language,
                );
                parse_result
                    .file_summary
                    .as_ref()
                    .map(|summary| (path, summary.clone(), plugin_fingerprint))
            })
            .collect();
        let summary_config_fingerprint = batch_result
            .parse_results
            .iter()
            .find_map(|parse_result| parse_result.summary_fingerprint.clone());
        crate::operation::checkpoint::persist_summaries_to_checkpoints(
            cm,
            &ctx.operation_id,
            0,
            &entries,
            summary_config_fingerprint,
        )
        .await
        .map_err(|error| HotUpdateError::hot_update(error.to_string()))
    }
    /// Helper: Finalize operation and compile result
    pub(crate) async fn finalize_operation(
        &self,
        ctx: &OperationContext,
        batch_result: &BatchChangeResult,
        failures: Vec<ModuleFailure>,
    ) -> Result<OperationResult> {
        let total_files = batch_result.processed_count();
        let failed_files = failures.len();

        let status = if failed_files == 0 {
            OperationStatus::Completed
        } else {
            OperationStatus::PartiallyCompleted {
                failed_count: failed_files,
            }
        };

        Ok(OperationResult {
            operation_id: ctx.operation_id.clone(),
            status,
            summary: OperationSummary {
                total_files_processed: total_files,
                total_files_failed: failed_files,
                total_modules_retried: 0,
                total_duration_ms: ctx.elapsed_ms(),
                can_resume: !failures.is_empty(),
            },
            failed_modules: failures,
            metrics: AggregatedMetrics {
                total_llm_tokens: 0,
                total_llm_cost_usd: 0.0,
                avg_file_duration_ms: if total_files > 0 {
                    ctx.elapsed_ms() as f64 / total_files as f64
                } else {
                    0.0
                },
                estimated_cost_per_file: 0.0,
            },
        })
    }

    // ==================== File processing ====================

    /// Create a file processor whose parser seeds the raw entity ID counter.
    pub(crate) fn new_file_processor(&self) -> FileProcessor {
        let processor = FileProcessor::with_entity_id_seed(self.entity_id_seed());
        match &self.parse_probe {
            Some(probe) => processor.with_parse_counter(probe.clone()),
            None => processor,
        }
    }

    /// Compute the parser entity-ID seed for a hot-update batch.
    pub(crate) fn entity_id_seed(&self) -> u64 {
        let Some(store) = &self.metadata_store else {
            return 0;
        };
        let Ok(conn) = store.read_connection() else {
            return 0;
        };
        let project_id = self.project_id;
        let active_epoch = match ProjectIndexManifestRepository::get_active(&conn, project_id) {
            Ok(Some(manifest)) => manifest.data_epoch,
            Ok(None) => cce_storage_sqlite::ProjectRepository::meta_get_int_optional(
                &conn,
                project_id,
                "active_epoch",
            )
            .map(|value| value.unwrap_or(0))
            .unwrap_or(0),
            Err(_) => return 0,
        };
        let candidate_epoch =
            ProjectIndexManifestRepository::get_building_max_epoch(&conn, project_id)
                .ok()
                .flatten()
                .unwrap_or(0);
        let active_max = ChunkRepository::max_entity_id_for_epoch(&conn, project_id, active_epoch)
            .ok()
            .flatten()
            .unwrap_or(0);
        let candidate_max =
            ChunkRepository::max_entity_id_for_epoch(&conn, project_id, candidate_epoch)
                .ok()
                .flatten()
                .unwrap_or(0);
        active_max.max(candidate_max) + 1
    }

    // ==================== Hash publication ====================

    pub(crate) async fn commit_file_hashes(
        &self,
        paths: &[PathBuf],
        deleted_paths: &[PathBuf],
    ) -> Result<()> {
        // 1. Remove hash records for deleted files so the next scan does not
        //    re-discover them as stale entries.
        if !deleted_paths.is_empty() {
            let db = self.change_detector.db();
            let conn = db.write_connection().map_err(|e| {
                HotUpdateError::hot_update(format!("Failed to get connection: {}", e))
            })?;
            let project_id = self.change_detector.project_id();
            let manifest =
                cce_storage_sqlite::ProjectIndexManifestRepository::get_active(&conn, project_id)
                    .map_err(|error| {
                    HotUpdateError::hot_update(format!(
                        "Failed to read active project manifest: {error}"
                    ))
                })?;
            let active_epoch = match manifest {
                Some(manifest) => manifest.data_epoch,
                // No manifest means the data generation was never published; a
                // missing legacy meta row is the legitimate default 0, while
                // real DB failures are propagated instead of silently
                // deleting hashes from the wrong epoch.
                None => cce_storage_sqlite::ProjectRepository::meta_get_int_optional(
                    &conn,
                    project_id,
                    "active_epoch",
                )
                .map_err(|error| {
                    HotUpdateError::hot_update(format!("Failed to read active_epoch meta: {error}"))
                })?
                .unwrap_or(0),
            };
            let tx = conn.unchecked_transaction().map_err(|e| {
                HotUpdateError::hot_update(format!("Failed to start transaction: {}", e))
            })?;
            for path in deleted_paths {
                cce_storage_sqlite::repo::file_repo::FileRepository::delete_by_path_at_epoch(
                    &tx,
                    &path.to_string_lossy(),
                    project_id,
                    active_epoch,
                )
                .map_err(|e| HotUpdateError::hot_update(format!("Failed to delete hash: {}", e)))?;
            }
            tx.commit().map_err(|e| {
                HotUpdateError::hot_update(format!("Failed to commit deletion: {}", e))
            })?;
        }

        if paths.is_empty() {
            return Ok(());
        }

        // Hash only the changed paths instead of rescanning the whole project.
        // The old `FSScanner::scan` walked and hashed every file in the
        // project on every hot update, making this commit scale with the total
        // project size instead of the change size. A path that vanished
        // between detection and commit is skipped (its stale hash is then
        // cleaned up by the next `scan_and_detect`).
        let root = Path::new(&self.change_detector.scan_options().root_path);
        let file_processor = cce_scanner::FileProcessor::new();
        let mut entries_to_publish = Vec::with_capacity(paths.len());
        for path in paths {
            let absolute = self.resolve_scan_path(path);
            // Mirror the walker's oversized-file rule: entries beyond the
            // limit carry no hash, so `update_cache_with_hashes` leaves the
            // previous hash in place.
            let entry = match std::fs::metadata(&absolute) {
                Ok(metadata)
                    if self
                        .change_detector
                        .scan_options()
                        .max_file_size
                        .is_some_and(|limit| metadata.len() > limit) =>
                {
                    FileEntry {
                        path: absolute.clone(),
                        relative_path: path.clone(),
                        size: metadata.len(),
                        modified: metadata.modified().unwrap_or(std::time::UNIX_EPOCH).into(),
                        content_hash: None,
                        language_info: None,
                    }
                }
                Ok(_) => match file_processor.process_file(&absolute, root) {
                    Ok(entry) => entry,
                    Err(error) => {
                        tracing::warn!(
                            path = %absolute.display(),
                            %error,
                            "Failed to hash a published file; leaving its previous hash in place"
                        );
                        continue;
                    }
                },
                Err(error) => {
                    tracing::warn!(
                        path = %absolute.display(),
                        %error,
                        "Published file vanished before hash commit; skipping"
                    );
                    continue;
                }
            };
            entries_to_publish.push(entry);
        }
        self.change_detector
            .update_cache_with_hashes(&entries_to_publish)
            .await
    }

    /// Check the durable publication state for a recovered operation.
    pub(crate) fn is_manifest_active(&self, operation_id: &str) -> Result<bool> {
        let Some(store) = &self.metadata_store else {
            return Ok(false);
        };
        let client = store.as_ref();
        let conn = client
            .read_connection()
            .map_err(|error| HotUpdateError::hot_update(error.to_string()))?;
        let state = conn
            .query_row(
                "SELECT state FROM project_index_manifests
                 WHERE project_id = ?1 AND operation_id = ?2",
                rusqlite::params![self.project_id, operation_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| HotUpdateError::hot_update(error.to_string()))?;
        Ok(state.as_deref() == Some("active"))
    }
}
