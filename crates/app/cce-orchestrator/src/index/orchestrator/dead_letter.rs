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
        let candidates = self.state_tracker.get_truncate_retry_candidates().await;
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
    use std::path::Path;
    use std::sync::Arc;

    use cce_config::{AstToNlConfig, NestProcessorConfig};
    use cce_llm::{Embedder, EmbeddingResult, LlmError};

    use super::super::IndexOrchestrator;
    use crate::hot_update::FileChangeType;
    use crate::index_state::{ModuleType, ModuleUpdateState, TrackerFailure};
    use cce_storage_sqlite::{
        NewProjectRecord, ProjectIndexManifestRepository, ProjectRepository, SqliteClient,
    };

    struct StubEmbedder;

    #[async_trait::async_trait]
    impl Embedder for StubEmbedder {
        async fn embed(&self, texts: &[&str]) -> Result<EmbeddingResult, LlmError> {
            Ok(EmbeddingResult {
                embeddings: texts.iter().map(|_| vec![0.5_f32, 0.5_f32]).collect(),
                prompt_tokens: 0,
                total_tokens: 0,
            })
        }

        async fn embed_one(&self, text: &str) -> Result<Vec<f32>, LlmError> {
            self.embed(&[text]).await.map(|r| r.embeddings[0].clone())
        }

        async fn embed_vectors(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, LlmError> {
            self.embed(texts).await.map(|r| r.embeddings)
        }

        fn dimension(&self) -> usize {
            2
        }

        fn model_name(&self) -> &str {
            "stub-embedder"
        }

        fn is_healthy(&self) -> bool {
            true
        }
    }

    /// End-to-end executor pass: a dead-lettered file is re-chunked from
    /// disk, its over-budget embedding chunks are truncated, records land in
    /// the active generation with the `truncated` marker, and the module
    /// leaves the dead letter queue.
    #[tokio::test]
    async fn truncate_retry_repairs_dead_letter_embedding() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("src/lib.rs");
        std::fs::create_dir(dir.path().join("src")).expect("create src dir");
        let content = "pub fn alpha() -> u32 { let mut v: Vec<u32> = Vec::new(); \
             for i in 0..64 { v.push(i * i + 7); } v.iter().sum() }\n\
             pub fn beta(text: &str) -> usize { text.chars().filter(|c| c.is_alphabetic()).count() }\n";
        std::fs::write(&file_path, content).expect("write source file");
        let hash = cce_utils::hash::calculate_hash(content.as_bytes());

        let database = Arc::new(SqliteClient::in_memory().expect("in-memory database"));
        database
            .with_transaction(|tx| {
                ProjectRepository::insert(
                    tx,
                    &NewProjectRecord::new("test".to_string(), dir.path().display().to_string()),
                )?;
                ProjectIndexManifestRepository::activate(tx, 1, 1, 0, "initial", None)?;
                tx.execute(
                    "INSERT INTO files
                        (path, language, last_modified, created_at, project_id, content_hash, epoch, batch_id)
                     VALUES ('src/lib.rs', 'Rust', 1, 1, 1, ?1, 1, 0)",
                    rusqlite::params![hash],
                )
                .map(|_| ())
                .map_err(|error| cce_types::StorageError::insert("files", error.to_string()))
            })
            .expect("initial generation should be created");

        let mut orchestrator = IndexOrchestrator::new(1)
            .expect("valid project")
            .with_metadata_store(database.clone())
            .with_embedder(Arc::new(StubEmbedder))
            .with_file_processor_configs(
                NestProcessorConfig::default(),
                &AstToNlConfig::both(),
                &cce_config::LicenseHeaderConfig::default(),
            )
            .with_dead_letter_config(true, 8);

        // Drive Embedding into the dead letter queue for the recorded file
        // with the token-limit classification that makes it a truncate target.
        let recorded = file_path.to_string_lossy().to_string();
        {
            let tracker = orchestrator.state_tracker();
            tracker
                .create_update(Path::new(&recorded), FileChangeType::Modified)
                .await;
            tracker
                .mark_failed(
                    Path::new(&recorded),
                    ModuleType::Embedding,
                    TrackerFailure {
                        message: "Token limit exceeded: 9000 > 8192".to_string(),
                        code: Some(crate::index_state::TOKEN_LIMIT_ERROR_CODE.to_string()),
                        retryable: false,
                    },
                )
                .await
                .expect("state exists");
            assert_eq!(tracker.get_truncate_retry_candidates().await.len(), 1);
        }

        let report = orchestrator
            .retry_dead_letter_with_truncation()
            .await
            .expect("retry pass should run");
        assert_eq!(report.retried, 1);
        assert_eq!(report.succeeded, 1);
        assert_eq!(report.still_failed, 0);
        assert!(
            report.truncated_chunks > 0,
            "over-budget chunks should be counted in the report"
        );

        // The module left the dead letter queue and consumed its one attempt.
        let tracker = orchestrator.state_tracker();
        let state = tracker
            .get_state(Path::new(&recorded))
            .await
            .expect("state exists");
        let record = state.get_module_state(ModuleType::Embedding);
        assert!(matches!(record.state, ModuleUpdateState::Success));
        assert!(record.truncated);
        assert!(tracker.get_truncate_retry_candidates().await.is_empty());

        // Stored active-generation records carry the truncate marker.
        let conn = database.read_connection().expect("read connection");
        let (total, truncated): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(truncated), 0) FROM chunks
                 WHERE project_id = 1 AND epoch = 1 AND path = 'emb'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("chunk counts should be queryable");
        assert!(total > 0, "embedding chunk records should be stored");
        assert!(
            truncated > 0,
            "over-budget chunks should be stored with the truncated marker"
        );
    }

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
