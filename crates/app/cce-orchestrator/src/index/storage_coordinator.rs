//! Storage coordination for indexing
//!
//! This module coordinates storage operations across multiple backends:
//! - Qdrant for vector storage
//! - BM25 for full-text search
//! - SQLite for entity mappings and metadata
//!
//! # Batch Processing
//!
//! All storage operations support batch processing to control memory usage
//! and avoid API rate limits. Use `store_vectors_batched` for large datasets.
//!
//! # Structure
//!
//! `StorageCoordinator` owns the shared project/epoch state and delegates the
//! work to responsibility-scoped submodules:
//! - `mapping`: pure record-shape mappings
//! - `generation`: manifest lifecycle, generation compaction/copying and GC
//! - `candidate`: hot-update candidate preparation and per-file cleanup
//! - `vector`/`bm25`/`summary`/`entities`: per-backend write paths
//! - `file_ops`: cross-backend file removal and hot updates
//! - `checkpoint`: checkpoint queries

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use crate::CheckpointManager;
use cce_llm_client::OpenAICompatibleProvider;
use cce_metrics::IndexQualityMetrics;
use cce_storage_bm25::Bm25Client;
use cce_storage_relation_sqlite::SqliteClient;
use cce_storage_vector_qdrant::QdrantClient;

use super::super::error::OrchestratorError;
use super::vector_store::{FulltextStore, RelationStore, VectorStore};

pub(crate) mod bm25;
pub(crate) mod candidate;
pub(crate) mod checkpoint;
pub(crate) mod entities;
pub(crate) mod file_ops;
pub(crate) mod generation;
pub(crate) mod mapping;
pub(crate) mod summary;
pub(crate) mod vector;

pub use mapping::build_bm25_documents;

/// Storage coordinator managing multiple storage backends
pub struct StorageCoordinator {
    vector: Option<VectorStore>,
    bm25: Option<Arc<tokio::sync::Mutex<Bm25Client>>>,
    embedder: Option<Arc<OpenAICompatibleProvider>>,
    metadata_store: Option<Arc<SqliteClient>>,
    project_group_id: String,
    project_id: i64,
    /// Current epoch for version-aware storage
    epoch: Arc<AtomicI64>,
    /// Current batch_id for per-epoch version tracking
    batch_id: Arc<AtomicI64>,
    /// Whether file summary vectors are embedded into Qdrant.
    ///
    /// When disabled, summary text is still generated and persisted to SQLite
    /// and BM25 (so NL document export keeps working) but the summary vector
    /// embedding step is skipped. This lets the pipeline serve export-only
    /// workloads without populating the vector store.
    embed_summaries: Arc<AtomicBool>,
    /// Checkpoint manager for work-unit-level progress tracking
    checkpoint_manager: Option<Arc<CheckpointManager>>,
    /// Operation ID for the current indexing operation
    operation_id: Option<String>,
    /// Operation ID of the currently prepared hot-update candidate.
    candidate_operation: Arc<StdMutex<Option<String>>>,
    /// Relation epoch produced by the current hot-update candidate.
    candidate_relation_epoch: Arc<AtomicI64>,
    /// Files whose candidate generation has already been cleared for this operation.
    prepared_files: Arc<StdMutex<HashSet<String>>>,
    /// Index-quality counters for silent-loss surfaces that do not fail the batch.
    quality_metrics: Option<Arc<IndexQualityMetrics>>,
    /// Wall-clock deadline for one `store_vectors_batched` pass, in seconds.
    /// 0 means derive: microbatch count x per-request retry budget factor.
    embedding_stage_timeout_secs: u64,
}

impl StorageCoordinator {
    /// Create a new storage coordinator with a required project ID
    ///
    /// `with_project_group_id()` must be called before any storage operations.
    pub fn new(project_id: i64) -> Result<Self, cce_types::error::ConfigError> {
        if project_id <= 0 {
            return Err(cce_types::error::ConfigError::invalid_project_id(
                project_id,
            ));
        }
        Ok(Self {
            vector: None,
            bm25: None,
            embedder: None,
            metadata_store: None,
            project_group_id: String::new(),
            project_id,
            epoch: Arc::new(AtomicI64::new(0)),
            batch_id: Arc::new(AtomicI64::new(0)),
            embed_summaries: Arc::new(AtomicBool::new(true)),
            checkpoint_manager: None,
            operation_id: None,
            candidate_operation: Arc::new(StdMutex::new(None)),
            candidate_relation_epoch: Arc::new(AtomicI64::new(0)),
            prepared_files: Arc::new(StdMutex::new(HashSet::new())),
            quality_metrics: None,
            embedding_stage_timeout_secs: 0,
        })
    }

    /// Set index-quality metrics
    pub fn with_quality_metrics(mut self, metrics: Arc<IndexQualityMetrics>) -> Self {
        self.quality_metrics = Some(metrics);
        self
    }

    /// Set vector backend directly (local or Qdrant enum dispatch).
    pub fn with_vector(mut self, store: VectorStore) -> Self {
        self.vector = Some(store);
        self
    }

    /// Set Qdrant client (wraps into the vector backend enum).
    pub fn with_qdrant(mut self, client: Arc<QdrantClient>) -> Self {
        self.vector = Some(VectorStore::Qdrant(client));
        self
    }

    /// Set embedded local vector store.
    pub fn with_local(mut self, store: Arc<cce_storage_vector_local::LocalVectorStore>) -> Self {
        self.vector = Some(VectorStore::Local(store));
        self
    }

    /// Set BM25 client
    pub fn with_bm25(mut self, client: Arc<tokio::sync::Mutex<Bm25Client>>) -> Self {
        self.bm25 = Some(client);
        self
    }

    /// Set fulltext backend via enum dispatch.
    ///
    /// Only the local branch is wired into the write path. A remote branch
    /// is rejected loudly (error log, client left unset) instead of being
    /// silently dropped, so a misconfiguration surfaces instead of
    /// degrading to index-without-fulltext.
    pub fn with_fulltext_store(mut self, store: FulltextStore) -> Self {
        match store {
            FulltextStore::Local(client) => {
                self.bm25 = Some(client);
            }
            FulltextStore::Remote(_) => {
                tracing::error!(
                    "remote fulltext branch is not wired into the write path; pass the local branch"
                );
            }
        }
        self
    }

    /// Fulltext backend name for logging.
    pub fn fulltext_backend_name(&self) -> &'static str {
        match &self.bm25 {
            Some(_) => "local",
            None => "none",
        }
    }

    /// Set embedder
    pub fn with_embedder(mut self, embedder: Arc<OpenAICompatibleProvider>) -> Self {
        self.embedder = Some(embedder);
        self
    }

    /// Set the embedding stage wall-clock deadline in seconds (0 = no deadline).
    pub fn set_embedding_stage_timeout(&mut self, secs: u64) {
        self.embedding_stage_timeout_secs = secs;
    }

    /// Set metadata store
    pub fn with_metadata_store(mut self, store: Arc<SqliteClient>) -> Self {
        self.metadata_store = Some(store);
        self
    }

    /// Set relation backend via enum dispatch.
    ///
    /// Only the local branch is wired into the write path. A remote branch
    /// is rejected loudly (error log, store left unset) instead of being
    /// silently dropped, so a misconfiguration surfaces instead of
    /// degrading to index-without-metadata.
    pub fn with_relation_store(mut self, store: RelationStore) -> Self {
        match store {
            RelationStore::Local(client) => {
                self.metadata_store = Some(client);
            }
            RelationStore::Remote(_) => {
                tracing::error!(
                    "remote relation branch is not wired into the write path; pass the local branch"
                );
            }
        }
        self
    }

    /// Relation backend name for logging.
    pub fn relation_backend_name(&self) -> &'static str {
        match &self.metadata_store {
            Some(_) => "local",
            None => "none",
        }
    }

    /// Set project group ID used for payload isolation in Qdrant.
    pub fn with_project_group_id(mut self, project_group_id: impl Into<String>) -> Self {
        self.project_group_id = project_group_id.into();
        self
    }

    /// Set the current epoch for version-aware storage
    pub fn with_epoch(self, epoch: i64) -> Self {
        self.epoch.store(epoch, Ordering::Release);
        self
    }

    /// Set the current batch_id for per-epoch version tracking
    pub fn with_batch_id(self, batch_id: i64) -> Self {
        self.batch_id.store(batch_id, Ordering::Release);
        self
    }

    /// Set checkpoint manager for work-unit-level progress tracking
    pub fn with_checkpoint_manager(
        mut self,
        cm: Arc<CheckpointManager>,
        operation_id: String,
    ) -> Self {
        self.checkpoint_manager = Some(cm);
        self.operation_id = Some(operation_id);
        self
    }

    /// Set or update checkpoint context after storage is created.
    /// Used when operation_id is not known at construction time.
    pub fn set_checkpoint_context(
        &mut self,
        cm: Option<Arc<CheckpointManager>>,
        operation_id: Option<String>,
    ) {
        self.checkpoint_manager = cm;
        self.operation_id = operation_id;
    }

    /// Get the epoch the coordinator is currently writing into.
    pub fn epoch(&self) -> i64 {
        self.epoch.load(Ordering::Acquire)
    }

    /// Get the current batch_id
    pub fn batch_id(&self) -> i64 {
        self.batch_id.load(Ordering::Acquire)
    }

    /// Resolve the published (active) data epoch.
    ///
    /// Unlike [`Self::epoch`] this ignores any in-flight candidate: during a
    /// hot update the candidate epoch has no physical rows for unchanged
    /// files (inheritance is a manifest link, not a copy), so regeneration
    /// sweeps must target the active generation. Returns `None` when the
    /// project was never indexed.
    pub(crate) fn active_data_epoch(&self) -> Result<Option<i64>, OrchestratorError> {
        let Some(client) = self.metadata_store.clone() else {
            return Ok(None);
        };
        cce_storage_relation_sqlite::cache::FileHashCache::new(client, self.project_id)
            .active_epoch()
            .map_err(OrchestratorError::Storage)
    }

    /// Align the write epoch to the active (published) data generation.
    ///
    /// Recovery writes that must land in the generation queries read —
    /// without a surrounding candidate-epoch switch — call this before
    /// storing. Returns the aligned epoch, or `None` when the project was
    /// never indexed.
    pub(crate) fn align_epoch_to_active_generation(
        &mut self,
    ) -> Result<Option<i64>, OrchestratorError> {
        let epoch = self.active_data_epoch()?;
        if let Some(epoch) = epoch {
            self.epoch.store(epoch, Ordering::Release);
        }
        Ok(epoch)
    }

    /// Check if storage is configured
    pub fn is_configured(&self) -> bool {
        self.vector.is_some() || self.bm25.is_some()
    }

    /// Check if any vector backend (local or Qdrant) is configured.
    pub fn has_vector(&self) -> bool {
        self.vector.is_some()
    }

    /// Check if Qdrant vector storage is configured (remote branch only).
    pub fn has_qdrant(&self) -> bool {
        matches!(self.vector.as_ref(), Some(VectorStore::Qdrant(_)))
    }

    /// Get the configured vector backend, if any.
    pub fn vector(&self) -> Option<&VectorStore> {
        self.vector.as_ref()
    }

    /// Get the configured Qdrant client, if the remote branch is active.
    pub fn qdrant(&self) -> Option<&Arc<QdrantClient>> {
        match self.vector.as_ref() {
            Some(VectorStore::Qdrant(client)) => Some(client),
            _ => None,
        }
    }

    pub(crate) fn ensure_project_group_id(&self) -> Result<(), OrchestratorError> {
        if self.project_group_id.trim().is_empty() {
            return Err(OrchestratorError::index(
                "project_context",
                "project_group_id must be configured before vector operations",
            ));
        }
        Ok(())
    }

    /// Ensure the vector collection exists (create if not)
    ///
    /// Must be called before any upsert operations to ensure the target
    /// collection has been created.
    pub async fn initialize_vector(&self) -> Result<(), OrchestratorError> {
        if let Some(ref vector) = self.vector {
            use cce_storage_common::VectorStorage;
            vector.ensure_collection().await?;
            tracing::info!(
                backend = vector.backend_name(),
                "Vector collection initialized"
            );
        }
        Ok(())
    }

    /// Ensure the vector collection exists (legacy Qdrant-named entry).
    pub async fn initialize_qdrant(&self) -> Result<(), OrchestratorError> {
        self.initialize_vector().await
    }

    /// Check if BM25 full-text search is configured
    pub fn has_bm25(&self) -> bool {
        self.bm25.is_some()
    }

    /// Get the configured embedder, if any.
    pub fn embedder(&self) -> Option<&Arc<OpenAICompatibleProvider>> {
        self.embedder.as_ref()
    }

    /// Whether summary vectors are embedded into Qdrant.
    pub fn embed_summaries(&self) -> bool {
        self.embed_summaries.load(Ordering::Acquire)
    }

    /// Enable or disable summary vector embedding.
    pub fn set_embed_summaries(&self, enabled: bool) {
        self.embed_summaries.store(enabled, Ordering::Release);
    }

    /// Get the configured SQLite metadata store, if any.
    pub fn metadata_client(&self) -> Option<&Arc<SqliteClient>> {
        self.metadata_store.as_ref()
    }

    /// Get the relation backend enum wrapping the configured store.
    pub fn relation_store(&self) -> Option<RelationStore> {
        self.metadata_store
            .as_ref()
            .map(|store| RelationStore::local(store.clone()))
    }

    /// Get the fulltext backend enum wrapping the configured client.
    pub fn fulltext_store(&self) -> Option<FulltextStore> {
        self.bm25
            .as_ref()
            .map(|client| FulltextStore::local(client.clone()))
    }

    /// Get the project ID this coordinator writes for.
    pub fn project_id(&self) -> i64 {
        self.project_id
    }
}
