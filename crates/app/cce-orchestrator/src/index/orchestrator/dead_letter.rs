//! Dead-letter truncate-retry executor.
//!
//! Deterministic embedding failures (input over the provider token budget)
//! always fail again under plain retry, so those files pile up in the dead
//! letter queue. This executor performs a lossy self-heal pass: it re-chunks
//! each dead-lettered file from its hash-verified on-disk content, truncates
//! embedding-path chunks over the embedder input budget, and re-stores them
//! into the ACTIVE data generation. The tracker `truncated` marker is only
//! consumed once the repair is fully recorded (store + success mark), so an
//! infrastructure blip mid-pass leaves the file queued for an idempotent
//! replay instead of burning its single lossy attempt.

use std::path::Path;

use cce_parser::ast_to_nl::chunker::{ChunkPath, ChunkedResult};
use cce_storage_sqlite::FileRepository;
use cce_types::OutputMode;
use cce_utils::token_estimation::{estimate_tokens, truncate_to_token_budget};

use crate::error::OrchestratorError;
use crate::index_state::ModuleType;

use super::IndexOrchestrator;

/// Outcome of one dead-letter truncate-retry pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeadLetterRetryReport {
    /// Files re-chunked from disk and re-stored.
    pub retried: usize,
    /// Files whose Embedding module succeeded after the truncate retry.
    pub succeeded: usize,
    /// Files retried that failed again; they stay in the dead letter state.
    pub still_failed: usize,
    /// Embedding chunks cut to the input token budget by this pass.
    pub truncated_chunks: usize,
}

/// Resolve the root-relative path stored in the `files` table from the
/// tracker's recorded path, which may be absolute (full index) or relative.
fn project_relative_path(root: &Path, recorded: &str) -> Option<String> {
    let path = Path::new(recorded);
    if path.is_absolute() {
        path.strip_prefix(root)
            .ok()
            .map(|rel| rel.to_string_lossy().into_owned())
    } else {
        Some(recorded.to_owned())
    }
}

impl IndexOrchestrator {
    /// Run one truncate-retry pass over the Embedding dead letter queue.
    ///
    /// Only the Embedding module is eligible: BM25 has no provider token
    /// budget, so truncation there would only lose recall. Files that are
    /// missing on disk or whose content drifted outside change tracking are
    /// skipped (the regular change flow owns them); one-file errors are
    /// recorded and the pass continues so a single bad file never interrupts
    /// the whole queue.
    pub async fn retry_dead_letter_with_truncation(
        &mut self,
    ) -> Result<DeadLetterRetryReport, OrchestratorError> {
        self.retry_dead_letters_for_files(None).await
    }

    /// Run the truncate-retry pass restricted to specific files.
    ///
    /// `files = None` processes every candidate (same as
    /// [`Self::retry_dead_letter_with_truncation`]); an explicit list keeps
    /// only the candidates whose recorded file path matches, enabling
    /// single-file manual retries.
    pub async fn retry_dead_letters_for_files(
        &mut self,
        files: Option<&[String]>,
    ) -> Result<DeadLetterRetryReport, OrchestratorError> {
        let mut candidates: Vec<_> = self.state_tracker.get_truncate_retry_candidates().await;
        if let Some(files) = files {
            candidates.retain(|state| files.iter().any(|f| f == &state.file_path));
        }
        if candidates.is_empty() {
            return Ok(DeadLetterRetryReport::default());
        }

        let client = self
            .storage
            .metadata_client()
            .ok_or_else(|| {
                OrchestratorError::index(
                    "dead_letter_retry",
                    "SQLite metadata store is not configured",
                )
            })?
            .clone();
        if self.storage.embedder().is_none() {
            return Err(OrchestratorError::index(
                "dead_letter_retry",
                "embedder is not configured",
            ));
        }

        // Recovery writes must land in the generation queries read; a
        // restored orchestrator otherwise still sits at epoch 0.
        if self.storage.align_epoch_to_active_generation()?.is_none() {
            return Err(OrchestratorError::index(
                "dead_letter_retry",
                "project has no active index generation to repair",
            ));
        }
        // Detach the previous operation's checkpoint context so recovery
        // writes never record work-unit checkpoints under a finished
        // operation; every operation re-arms it before storing.
        self.storage.set_checkpoint_context(None, None);

        let root = {
            let conn = client
                .read_connection()
                .map_err(OrchestratorError::Storage)?;
            cce_storage_sqlite::source_reader::resolve_project_root(&conn, self.project_id)
                .ok_or_else(|| {
                    OrchestratorError::index(
                        "dead_letter_retry",
                        format!("project {} has no registered root path", self.project_id),
                    )
                })?
        };

        let mut report = DeadLetterRetryReport::default();
        for state in candidates {
            let tracker_path = Path::new(&state.file_path);
            let file = state.file_path.as_str();

            let Some(relative_path) = project_relative_path(&root, file) else {
                tracing::warn!(
                    file,
                    "dead-letter retry: recorded path is outside the project root; skipping"
                );
                continue;
            };

            let content_hash = {
                let read_result = client.read_connection().map_err(OrchestratorError::Storage);
                match read_result {
                    Ok(conn) => {
                        match FileRepository::get_content_hash_by_path(
                            &conn,
                            &relative_path,
                            self.project_id,
                        ) {
                            Ok(hash) => hash,
                            Err(error) => {
                                // Infrastructure fault: keep the file queued so
                                // a later pass can retry the lookup.
                                tracing::error!(
                                    file,
                                    %error,
                                    "dead-letter retry: content hash lookup failed; file stays queued"
                                );
                                continue;
                            }
                        }
                    }
                    Err(error) => {
                        tracing::error!(
                            file,
                            %error,
                            "dead-letter retry: database connection failed; file stays queued"
                        );
                        continue;
                    }
                }
            };
            let Some(content_hash) = content_hash.filter(|hash| !hash.is_empty()) else {
                tracing::warn!(
                    file,
                    "dead-letter retry: no indexed content hash for the recorded path; skipping"
                );
                continue;
            };

            let read_path = root.join(&relative_path);
            let mut chunks = match self
                .file_processor
                .rechunk_file_from_disk(
                    &read_path,
                    &relative_path,
                    &content_hash,
                    OutputMode::Embedding,
                )
                .await
            {
                Ok(Some(chunks)) => chunks,
                Ok(None) => {
                    tracing::warn!(
                        file,
                        "dead-letter retry: on-disk content missing or drifted; skipping"
                    );
                    continue;
                }
                Err(error) => {
                    // One file's re-chunk error must not abort the queue; the
                    // truncate marker is not consumed, so a later pass replays.
                    tracing::error!(
                        file,
                        %error,
                        "dead-letter retry: re-chunk failed; file stays queued for the next pass"
                    );
                    continue;
                }
            };

            let truncated_chunks = self.apply_embed_budget(file, &mut chunks);
            report.truncated_chunks += truncated_chunks;
            if !chunks.iter().any(|c| c.path == ChunkPath::Embedding) {
                tracing::warn!(
                    file,
                    "dead-letter retry: file produced no embedding chunks; skipping"
                );
                continue;
            }

            report.retried += 1;
            match self
                .storage
                .store_vectors_batched(&chunks, self.batch_config.embedding_batch_size, 0)
                .await
            {
                Ok(_) => {
                    if let Err(error) = self
                        .state_tracker
                        .mark_success(tracker_path, ModuleType::Embedding)
                        .await
                    {
                        // The vectors are stored; leaving the truncate marker
                        // unconsumed lets the next pass replay idempotently
                        // and confirm the success state.
                        tracing::error!(
                            file,
                            %error,
                            "dead-letter retry stored chunks but could not mark Embedding success"
                        );
                    } else if let Err(error) = self
                        .state_tracker
                        .set_module_truncated(tracker_path, ModuleType::Embedding)
                        .await
                    {
                        tracing::error!(
                            file,
                            %error,
                            "dead-letter retry succeeded but the truncate marker could not be recorded"
                        );
                    }
                    report.succeeded += 1;
                    tracing::info!(
                        file,
                        truncated_chunks,
                        "dead-letter truncate-retry succeeded"
                    );
                }
                Err(error) => {
                    if let Err(state_error) = self
                        .state_tracker
                        .mark_failed(
                            tracker_path,
                            ModuleType::Embedding,
                            error.as_module_failure(),
                        )
                        .await
                    {
                        tracing::error!(
                            file,
                            %state_error,
                            "dead-letter retry could not mark Embedding failure"
                        );
                    }
                    report.still_failed += 1;
                    tracing::warn!(
                        file,
                        truncated_chunks,
                        %error,
                        "dead-letter truncate-retry failed; module stays in dead letter"
                    );
                }
            }
        }
        if let Some(metrics) = &self.quality_metrics {
            metrics.record_dead_letter_truncated(report.truncated_chunks);
        }
        Ok(report)
    }

    /// Truncate embedding-path chunks that exceed the embedder input budget,
    /// returning how many chunks were cut.
    fn apply_embed_budget(&self, file: &str, chunks: &mut [ChunkedResult]) -> usize {
        let mut truncated = 0;
        for chunk in chunks.iter_mut() {
            if chunk.path != ChunkPath::Embedding {
                continue;
            }
            let result = truncate_to_token_budget(&chunk.text, self.embed_input_token_limit);
            if !result.truncated {
                continue;
            }
            tracing::info!(
                file,
                chunk_id = %chunk.chunk_id,
                limit = self.embed_input_token_limit,
                from_tokens = result.original_estimate,
                to_tokens = estimate_tokens(&result.text),
                from_bytes = result.original_len,
                to_bytes = result.final_len,
                "dead-letter retry: truncated over-budget embedding chunk"
            );
            chunk.text = result.text;
            chunk.truncated = true;
            chunk.token_count = estimate_tokens(&chunk.text);
            truncated += 1;
        }
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::super::IndexOrchestrator;

    /// An empty queue is a no-op that never touches storage configuration.
    #[tokio::test]
    async fn truncate_retry_without_candidates_returns_zero_report() {
        let mut orchestrator = IndexOrchestrator::new(1).expect("valid project");
        let report = orchestrator
            .retry_dead_letter_with_truncation()
            .await
            .expect("pass should run");
        assert_eq!(report.retried, 0);
        assert_eq!(report.succeeded, 0);
        assert_eq!(report.still_failed, 0);
    }
}
