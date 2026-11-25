//! Reconstruction of a hot-update input batch from its checkpoints.
//!
//! A resumed operation must rebuild the exact change set its previous run had
//! already parsed, then continue from the first stage that did not finish.
//! This module owns that reconstruction: it turns persisted per-file
//! checkpoints back into a [`BatchChangeResult`], reusing every envelope that
//! is still valid and falling back to a fresh parse only where the durable
//! state cannot be trusted (missing or incompatible payload, drifted disk
//! content).
//!
//! Keeping the reconstruction separate from the orchestration in
//! `resume.rs` lets the durable-state mapping be exercised without a full
//! coordinator: the callers only need a checkpoint manager, a scan root and a
//! metadata store.

use std::path::Path;

use crate::hot_update::change::{
    BatchChangeResult, FileChange, FileChangeType, ParseResultWithChanges,
};
use crate::hot_update::error::{HotUpdateError, Result};
use crate::operation::checkpoint::ParsedCheckpointPayload;

/// Batch index every hot-update checkpoint is written under.
///
/// A hot update persists a single batch (`operation_runtime.rs` creates the
/// batch checkpoint with index 0 and one file checkpoint per changed file),
/// so recovery reads that batch only. Multi-batch recovery belongs to the
/// full-index path, which tracks `current_batch_index` separately.
const RECOVERY_BATCH_INDEX: u32 = 0;

/// Mutable state shared by every hot-update operation.
///
/// Guarded by its own mutex so the background worker can run a long
/// operation while other coordinator APIs (watch status, stop watch, mode
/// checks) keep working under the coordinator lock.
use super::HotUpdateOperationRuntime;

impl HotUpdateOperationRuntime {
    /// Rebuild the batch a previous run had prepared, from its checkpoints.
    ///
    /// Per checkpoint:
    /// - a deletion tombstone becomes a `Deleted` file change;
    /// - a compatible envelope whose on-disk content still matches its
    ///   recorded hash is reused as-is (summary, render fingerprint, module
    ///   progress and export marker included);
    /// - an envelope whose file vanished from disk becomes a `Deleted` file
    ///   change, because the source no longer exists to process;
    /// - anything else (missing or incompatible payload, drifted content) is
    ///   re-parsed from disk.
    pub(crate) async fn reconstruct_batch_from_checkpoints(
        &self,
        operation_id: &str,
    ) -> Result<BatchChangeResult> {
        let Some(checkpoint_manager) = self.checkpoint_manager.as_ref() else {
            return Err(HotUpdateError::hot_update(
                "cannot resume a hot operation without its checkpoint manager",
            ));
        };

        let file_checkpoints = checkpoint_manager
            .get_batch_files(operation_id, RECOVERY_BATCH_INDEX)
            .await
            .map_err(|error| HotUpdateError::hot_update(error.to_string()))?;

        let mut result = BatchChangeResult::new();
        let mut file_processor = self.new_file_processor().await;
        for checkpoint in file_checkpoints {
            // Restore the project-relative path identity used by the
            // change-detection cache, `files` rows and storage removal,
            // exactly like `process_watch_paths` does for live events.
            let identity = self.relativize_scan_path(Path::new(&checkpoint.file_path));
            let payload = checkpoint
                .parsed_data
                .as_deref()
                .and_then(crate::operation::checkpoint::decode_parsed_checkpoint)
                .filter(ParsedCheckpointPayload::is_compatible);

            match payload {
                Some(ParsedCheckpointPayload::Deleted) => {
                    result.add_file_change(FileChange::new(
                        identity,
                        FileChangeType::Deleted,
                        checkpoint.content_hash.unwrap_or_default(),
                        checkpoint.file_size.unwrap_or_default().max(0) as u64,
                        chrono::Utc::now(),
                    ));
                    continue;
                }
                Some(ParsedCheckpointPayload::Parsed(envelope)) => {
                    let read_path = self.resolve_scan_path(&identity);
                    let disk_matches = match checkpoint.content_hash.as_deref() {
                        Some(expected) => {
                            cce_scanner::file_matches_scan_hash(&read_path, expected).await
                        }
                        None => false,
                    };
                    if disk_matches {
                        // A file whose NL document was already exported in a
                        // previous (interrupted) run is a resume candidate.
                        //
                        // The skip is validated against the persisted render
                        // fingerprint in the export processor: the document is
                        // only skipped when the current rendering inputs still
                        // match the ones captured at export time. Configuration
                        // changes between the crash and the resume therefore
                        // force a re-export instead of leaving a stale document.
                        let already_exported =
                            checkpoint
                                .export_path
                                .as_deref()
                                .is_some_and(|export_path| {
                                    self.watch_root
                                        .as_deref()
                                        .map(|root| root.join(export_path).exists())
                                        .unwrap_or(false)
                                });
                        let mut parse_result = ParseResultWithChanges::new(
                            identity,
                            envelope.parsed_file,
                            envelope.change_type,
                            envelope.change_type == FileChangeType::Added,
                        );
                        // Restore the pre-generated summary from its record
                        // column so the summary/export processors reuse it.
                        // Entity groups are not persisted: the export
                        // processor derives them from the parsed file at
                        // render time.
                        if let Some(bytes) = checkpoint.summary_data.as_deref()
                            && let Some(summary_payload) =
                                crate::operation::checkpoint::decode_summary_checkpoint(bytes)
                        {
                            parse_result.file_summary = Some(summary_payload.file_summary);
                            parse_result.stored_summary_fingerprint =
                                summary_payload.summary_config_fingerprint;
                        }
                        parse_result.stored_render_fingerprint = checkpoint.render_fingerprint;
                        parse_result.stored_content_hash = checkpoint.content_hash.clone();
                        parse_result.module_progress =
                            crate::hot_update::progress::read_module_progress(
                                checkpoint.module_progress.as_deref(),
                            );
                        let parse_result = if already_exported {
                            parse_result.with_already_exported()
                        } else {
                            parse_result
                        };
                        result.add_parse_result(parse_result);
                        continue;
                    }
                }
                // Missing/incompatible payload: fall through to a fresh parse.
                None => {}
            }

            let read_path = self.resolve_scan_path(&identity);
            if read_path.exists() {
                // The source is still on disk but no trustworthy envelope
                // survives, so this file is processed as a modification of
                // its current content.
                let parse_path = identity.to_string_lossy().into_owned();
                result.add_parse_result(
                    file_processor
                        .process_file_change_at(
                            &read_path,
                            &parse_path,
                            FileChangeType::Modified,
                            &self.metadata_store,
                            self.project_id,
                        )
                        .await?,
                );
            } else {
                result.add_file_change(FileChange::new(
                    identity,
                    FileChangeType::Deleted,
                    String::new(),
                    0,
                    chrono::Utc::now(),
                ));
            }
        }
        Ok(result)
    }
}
