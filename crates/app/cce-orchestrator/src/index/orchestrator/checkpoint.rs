//! Checkpoint recovery and rehydration for full index runs.
//!
//! A resumed full index must reconstruct its exact prior state before
//! continuing: recover the deterministic file indexer, move the batch boundary
//! backwards when content hashes changed, replay relation inputs from the
//! operation-local spool, and accumulate NL export chunks of already-completed
//! batches. Keeping these phases here keeps `execute()` focused on dispatch.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::CheckpointManager;
use crate::hot_update::FileChangeType;
use crate::operation::checkpoint::{
    ParsedCheckpointEnvelope, ParsedCheckpointPayload, decode_summary_checkpoint,
};
use cce_scanner::{FileEntry, ScanOptions};
use cce_types::{OutputMode, ParsedFile};

use super::{FullIndexContext, IndexOrchestrator};
use crate::error::OrchestratorError;
use crate::index::file_indexer::FileIndexer;
use crate::index::options::IndexOptions;
use crate::index::relation_build_spool::RelationBuildSpool;

impl IndexOrchestrator {
    /// Recover the file indexer from the latest checkpoint, or initialize a
    /// fresh one when recovery is not possible.
    ///
    /// Returns `(indexer, recovered_start_batch, operation_id)`.
    pub(super) async fn recover_file_indexer(
        &self,
        root_dir: &Path,
        batch_size: usize,
        scan_options: &ScanOptions,
    ) -> Result<(FileIndexer, usize, String), OrchestratorError> {
        let checkpoint_manager = self.checkpoint_manager.clone();

        let (file_indexer, recovered_start_batch, operation_id) = if checkpoint_manager.is_some() {
            match self
                .storage
                .get_latest_checkpoint("full_index", &root_dir.to_string_lossy())
                .await
            {
                Ok(Some(checkpoint)) => {
                    match FileIndexer::recover(
                        root_dir,
                        batch_size,
                        scan_options,
                        checkpoint.clone(),
                        self.scanner_metrics.clone(),
                    ) {
                        Ok(indexer) => {
                            let next_batch = (indexer.checkpoint().current_batch_index) as usize;
                            let next_batch = if next_batch <= indexer.total_batches() {
                                tracing::info!(
                                    "Recovered checkpoint: operation_id={}, last_completed_batch={}, resuming from batch {}/{}",
                                    indexer.operation_id(),
                                    indexer.checkpoint().current_batch_index,
                                    next_batch,
                                    indexer.total_batches()
                                );
                                next_batch
                            } else {
                                tracing::warn!(
                                    "Checkpoint batch_index {} is at or beyond total_batches {}, starting fresh",
                                    indexer.checkpoint().current_batch_index,
                                    indexer.total_batches()
                                );
                                0
                            };
                            // Validate the persisted batch boundary of the
                            // resume-start batch (and, as fallback, its
                            // predecessor) against the current deterministic
                            // batching. A mismatch means the file set changed
                            // while keeping the same count; resume from the
                            // last verified boundary or start fresh.
                            let mut verified_start = next_batch;
                            if let Some(cm) = checkpoint_manager.as_ref() {
                                let mut boundaries = Vec::new();
                                for probe in
                                    [next_batch as u32, (next_batch as u32).saturating_sub(1)]
                                {
                                    if let Ok(Some(record)) =
                                        cm.get_batch_checkpoint(indexer.operation_id(), probe).await
                                    {
                                        boundaries.push(record);
                                    }
                                }
                                if let Err(error) =
                                    indexer.validate_checkpoint(&checkpoint, &boundaries)
                                {
                                    tracing::warn!(
                                        "Checkpoint boundary validation failed: {error}, starting fresh"
                                    );
                                    verified_start = 0;
                                }
                            }
                            let op_id = indexer.operation_id().to_string();
                            (indexer, verified_start, op_id)
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Checkpoint recovery failed: {}, creating new operation",
                                e.reason()
                            );
                            let indexer = FileIndexer::initialize(
                                root_dir,
                                batch_size,
                                scan_options,
                                checkpoint_manager.clone(),
                                self.scanner_metrics.clone(),
                                self.scanner_plugin_registry.clone(),
                            )
                            .await
                            .map_err(|e| {
                                OrchestratorError::index(
                                    "file_indexing",
                                    format!("Failed to initialize FileIndexer: {}", e),
                                )
                            })?;
                            let op_id = indexer.operation_id().to_string();
                            (indexer, 0, op_id)
                        }
                    }
                }
                _ => {
                    tracing::debug!("No checkpoint found, creating new operation");
                    let indexer = FileIndexer::initialize(
                        root_dir,
                        batch_size,
                        scan_options,
                        checkpoint_manager.clone(),
                        self.scanner_metrics.clone(),
                        self.scanner_plugin_registry.clone(),
                    )
                    .await
                    .map_err(|e| {
                        OrchestratorError::index(
                            "file_indexing",
                            format!("Failed to initialize FileIndexer: {}", e),
                        )
                    })?;
                    let op_id = indexer.operation_id().to_string();
                    (indexer, 0, op_id)
                }
            }
        } else {
            tracing::warn!("CheckpointManager not configured, starting fresh without recovery");
            let indexer = FileIndexer::initialize(
                root_dir,
                batch_size,
                scan_options,
                None,
                self.scanner_metrics.clone(),
                self.scanner_plugin_registry.clone(),
            )
            .await
            .map_err(|e| {
                OrchestratorError::index(
                    "file_indexing",
                    format!("Failed to initialize FileIndexer: {}", e),
                )
            })?;
            let op_id = indexer.operation_id().to_string();
            (indexer, 0, op_id)
        };

        Ok((file_indexer, recovered_start_batch, operation_id))
    }

    /// Validate content hashes for all completed batches.
    ///
    /// When a content hash mismatches, the recovery boundary is moved backwards
    /// so those batches get re-processed.
    pub(super) async fn validate_recovered_hashes(
        &self,
        file_indexer: &FileIndexer,
        operation_id: &str,
        options: &IndexOptions,
        start_batch: usize,
    ) -> Result<usize, OrchestratorError> {
        let mut start_batch = start_batch;
        if start_batch > 0 {
            if let Some(cm) = self.checkpoint_manager.as_ref() {
                'completed_batches: for batch_idx in 0..start_batch {
                    let checkpoint_files = cm
                        .get_batch_files(operation_id, batch_idx as u32)
                        .await
                        .map_err(|error| {
                            OrchestratorError::index(
                                "checkpoint_recovery",
                                format!("Failed to load file checkpoints: {error}"),
                            )
                        })?;
                    let checkpoints: HashMap<_, _> = checkpoint_files
                        .into_iter()
                        .map(|record| (record.file_path.clone(), record))
                        .collect();
                    let batch = file_indexer
                        .get_batch(batch_idx)
                        .map_err(|error| OrchestratorError::index("checkpoint_recovery", error))?;

                    for entry in batch {
                        let is_code = is_relation_code_file(entry);

                        let path = entry.path.to_string_lossy();
                        let Some(record) = checkpoints.get(path.as_ref()) else {
                            start_batch = batch_idx;
                            break 'completed_batches;
                        };
                        if record.content_hash != entry.content_hash {
                            start_batch = batch_idx;
                            break 'completed_batches;
                        }

                        // Only recover parsed data for code files when relation building is needed
                        if options.build_relations && is_code {
                            let Some(parsed_data) = record.parsed_data.as_deref() else {
                                start_batch = batch_idx;
                                break 'completed_batches;
                            };
                            match crate::operation::checkpoint::decode_parsed_checkpoint(
                                parsed_data,
                            ) {
                                Some(payload) => {
                                    if !payload.is_compatible() {
                                        tracing::warn!(file = %path, "Incompatible parsed checkpoint version, re-parsing");
                                        start_batch = batch_idx;
                                        break 'completed_batches;
                                    }
                                    if matches!(payload, ParsedCheckpointPayload::Deleted) {
                                        // Full-index checkpoints never write
                                        // tombstones; treat one as drift and
                                        // re-process the batch.
                                        start_batch = batch_idx;
                                        break 'completed_batches;
                                    }
                                }
                                None => {
                                    tracing::warn!(file = %path, "Invalid parsed checkpoint");
                                    start_batch = batch_idx;
                                    break 'completed_batches;
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(start_batch)
    }

    /// Rehydrate relation inputs for batches completed in a previous run.
    ///
    /// The completed portion of a resumed operation must take part in the final
    /// graph, but must not recreate the old all-files in-memory cache.
    pub(super) async fn replay_recovered_relations(
        &self,
        file_indexer: &FileIndexer,
        operation_id: &str,
        start_batch: usize,
        relation_spool: Option<&mut RelationBuildSpool>,
    ) -> Result<(), OrchestratorError> {
        let checkpoint_manager = self.checkpoint_manager.as_ref().ok_or_else(|| {
            OrchestratorError::index(
                "checkpoint_recovery",
                "relation recovery requires the checkpoint manager that supplied the recovered operation",
            )
        })?;
        let builder = self.relation_builder.as_ref().ok_or_else(|| {
            OrchestratorError::index("relation_build", "relation builder is unavailable")
        })?;
        let spool = relation_spool.ok_or_else(|| {
            OrchestratorError::index("relation_build_spool", "relation spool is unavailable")
        })?;

        for batch_idx in 0..start_batch {
            let checkpoint_files = checkpoint_manager
                .get_batch_files(operation_id, batch_idx as u32)
                .await
                .map_err(|error| {
                    OrchestratorError::index(
                        "checkpoint_recovery",
                        format!("Failed to reload parsed checkpoint data: {error}"),
                    )
                })?;
            let checkpoints: HashMap<_, _> = checkpoint_files
                .into_iter()
                .map(|record| (record.file_path.clone(), record))
                .collect();
            let batch = file_indexer
                .get_batch(batch_idx)
                .map_err(|error| OrchestratorError::index("checkpoint_recovery", error))?;

            for entry in batch.iter().filter(|entry| is_relation_code_file(entry)) {
                let path = entry.path.to_string_lossy();
                let record = checkpoints.get(path.as_ref()).ok_or_else(|| {
                    OrchestratorError::index(
                        "checkpoint_recovery",
                        format!("Missing relation checkpoint for {}", entry.path.display()),
                    )
                })?;
                let parsed_data = record.parsed_data.as_deref().ok_or_else(|| {
                    OrchestratorError::index(
                        "checkpoint_recovery",
                        format!("Missing parsed relation data for {}", entry.path.display()),
                    )
                })?;
                let payload = crate::operation::checkpoint::decode_parsed_checkpoint(parsed_data)
                    .ok_or(OrchestratorError::index(
                    "checkpoint_recovery",
                    format!(
                        "Invalid parsed relation checkpoint for {}",
                        entry.path.display()
                    ),
                ))?;
                if !payload.is_compatible() {
                    return Err(OrchestratorError::index(
                        "checkpoint_recovery",
                        format!(
                            "Incompatible parsed relation checkpoint for {}",
                            entry.path.display()
                        ),
                    ));
                }
                let ParsedCheckpointPayload::Parsed(envelope) = payload else {
                    return Err(OrchestratorError::index(
                        "checkpoint_recovery",
                        format!("Missing parsed relation data for {}", entry.path.display()),
                    ));
                };
                spool.append(&envelope.parsed_file).map_err(|error| {
                    OrchestratorError::index(
                        "relation_build_spool",
                        format!("Failed to store recovered relation input: {error}"),
                    )
                })?;
                builder.add_file_symbols(&envelope.parsed_file, spool.project_symbols());
            }
        }
        Ok(())
    }

    /// Accumulate chunks and summaries of batches completed in a previous run.
    ///
    /// Their documents must be exported once at the end because export runs
    /// after relation finalize.
    pub(super) async fn accumulate_recovered_export(
        &self,
        ctx: &mut FullIndexContext,
    ) -> Result<(), OrchestratorError> {
        let checkpoint_manager = self.checkpoint_manager.as_ref().ok_or_else(|| {
            OrchestratorError::index(
                "checkpoint_recovery",
                "export accumulation requires the checkpoint manager",
            )
        })?;

        for batch_idx in 0..ctx.start_batch {
            let checkpoint_files = checkpoint_manager
                .get_batch_files(&ctx.operation_id, batch_idx as u32)
                .await
                .map_err(|error| {
                    OrchestratorError::index(
                        "checkpoint_recovery",
                        format!("Failed to reload export checkpoints: {error}"),
                    )
                })?;
            for record in checkpoint_files {
                if record.content_hash.is_none() {
                    continue;
                }
                let envelope = match record.parsed_data.as_deref() {
                    Some(bytes) => {
                        match crate::operation::checkpoint::decode_parsed_checkpoint(bytes) {
                            Some(payload) if payload.is_compatible() => {
                                let ParsedCheckpointPayload::Parsed(envelope) = payload else {
                                    continue;
                                };
                                envelope
                            }
                            _ => continue,
                        }
                    }
                    None => continue,
                };
                let path_str = envelope.parsed_file.path.clone();
                // Reuse the summary persisted for a completed batch so the
                // final export of recovered files stays consistent with the
                // freshly processed batches. Summaries live in the record's
                // own column, written by the summary phase.
                if let Some(bytes) = record.summary_data.as_deref() {
                    if let Some(summary_payload) = decode_summary_checkpoint(bytes) {
                        ctx.export_summaries_by_file
                            .entry(path_str.clone())
                            .or_insert_with(|| summary_payload.file_summary);
                    }
                }
                let mut processor = self.file_processor.clone();
                let content_route = cce_types::ContentRoute::detect_from_path(&path_str);
                // Export recovery has no output-mode context, but the final
                // NL export always needs embedding text, so Both is the safe
                // fallback for this low-frequency path.
                match processor
                    .process_parsed_file_complete(
                        &envelope.parsed_file,
                        content_route,
                        OutputMode::Both,
                    )
                    .await
                {
                    Ok(complete) => {
                        let Some(spool) = ctx.export_spool.as_mut() else {
                            return Err(OrchestratorError::index(
                                "export_spool",
                                "export spool missing for recovered export accumulation",
                            ));
                        };
                        spool.append(&path_str, complete.chunks).map_err(|error| {
                            OrchestratorError::index(
                                "export_spool",
                                format!(
                                    "Failed to spool recovered export chunks for {path_str}: {error}"
                                ),
                            )
                        })?;
                    }
                    Err(error) => {
                        tracing::warn!(
                            file = %path_str,
                            error = %error,
                            "Failed to rebuild chunks for recovered file export"
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

/// Whether a scanned file participates in relation construction.
fn is_relation_code_file(entry: &FileEntry) -> bool {
    entry
        .language_info
        .as_ref()
        .is_some_and(|info| !info.is_document_like())
}

/// Persist one parsed file into the operation checkpoint so a resumed run can
/// reuse it instead of re-parsing.
pub(super) async fn persist_parsed_checkpoint(
    checkpoint_manager: &Option<Arc<CheckpointManager>>,
    operation_id: &str,
    batch_index: u32,
    file_entry: &FileEntry,
    parsed: &ParsedFile,
) -> Result<(), OrchestratorError> {
    let Some(cm) = checkpoint_manager else {
        return Ok(());
    };
    let mut checkpoint = cm
        .create_file_checkpoint(operation_id, batch_index, &parsed.path)
        .await
        .map_err(OrchestratorError::Storage)?;
    checkpoint.content_hash = file_entry.content_hash.clone();
    checkpoint.file_size = Some(file_entry.size as i64);
    checkpoint.language = file_entry
        .language_info
        .as_ref()
        .map(|info| info.language.to_string());
    let payload = ParsedCheckpointPayload::Parsed(Box::new(ParsedCheckpointEnvelope::new(
        FileChangeType::Modified,
        parsed.clone(),
    )));
    checkpoint.parsed_data = Some(
        crate::operation::checkpoint::encode_parsed_checkpoint(&payload)
            .map_err(OrchestratorError::Storage)?,
    );
    checkpoint.updated_at = chrono::Utc::now().to_rfc3339();
    cm.save_file_checkpoint(&checkpoint)
        .await
        .map_err(OrchestratorError::Storage)
}
