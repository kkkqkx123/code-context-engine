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
use std::sync::atomic::{AtomicI64, Ordering};

use crate::CheckpointManager;
use cce_llm::Embedder;
use cce_storage_bm25::Bm25Client;
use cce_storage_qdrant::QdrantClient;
use cce_storage_sqlite::ProjectIndexManifestRepository;
use cce_storage_sqlite::SqliteClient;

use super::super::error::OrchestratorError;

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
    qdrant: Option<Arc<QdrantClient>>,
    bm25: Option<Arc<tokio::sync::Mutex<Bm25Client>>>,
    embedder: Option<Arc<dyn Embedder>>,
    metadata_store: Option<Arc<SqliteClient>>,
    project_group_id: String,
    project_id: i64,
    /// Current epoch for version-aware storage
    epoch: Arc<AtomicI64>,
    /// Current batch_id for per-epoch version tracking
    batch_id: Arc<AtomicI64>,
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
            qdrant: None,
            bm25: None,
            embedder: None,
            metadata_store: None,
            project_group_id: String::new(),
            project_id,
            epoch: Arc::new(AtomicI64::new(0)),
            batch_id: Arc::new(AtomicI64::new(0)),
            checkpoint_manager: None,
            operation_id: None,
            candidate_operation: Arc::new(StdMutex::new(None)),
            candidate_relation_epoch: Arc::new(AtomicI64::new(0)),
            prepared_files: Arc::new(StdMutex::new(HashSet::new())),
        })
    }

    /// Set Qdrant client
    pub fn with_qdrant(mut self, client: Arc<QdrantClient>) -> Self {
        self.qdrant = Some(client);
        self
    }

    /// Set BM25 client
    pub fn with_bm25(mut self, client: Arc<tokio::sync::Mutex<Bm25Client>>) -> Self {
        self.bm25 = Some(client);
        self
    }

    /// Set embedder
    pub fn with_embedder(mut self, embedder: Arc<dyn Embedder>) -> Self {
        self.embedder = Some(embedder);
        self
    }

    /// Set metadata store
    pub fn with_metadata_store(mut self, store: Arc<SqliteClient>) -> Self {
        self.metadata_store = Some(store);
        self
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
        let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) else {
            return Ok(None);
        };
        let conn = client
            .read_connection()
            .map_err(OrchestratorError::Storage)?;
        if let Some(manifest) = ProjectIndexManifestRepository::get_active(&conn, self.project_id)
            .map_err(OrchestratorError::Storage)?
        {
            return Ok(Some(manifest.data_epoch));
        }
        client
            .project_meta_get_int_optional(self.project_id, "active_epoch")
            .map_err(OrchestratorError::Storage)
    }

    /// Check if storage is configured
    pub fn is_configured(&self) -> bool {
        self.qdrant.is_some() || self.bm25.is_some()
    }

    /// Check if Qdrant vector storage is configured
    pub fn has_qdrant(&self) -> bool {
        self.qdrant.is_some()
    }

    /// Get the configured Qdrant client, if any.
    pub fn qdrant(&self) -> Option<&Arc<QdrantClient>> {
        self.qdrant.as_ref()
    }

    pub(crate) fn ensure_project_group_id(&self) -> Result<(), OrchestratorError> {
        if self.project_group_id.trim().is_empty() {
            return Err(OrchestratorError::index(
                "project_context",
                "project_group_id must be configured before Qdrant operations",
            ));
        }
        Ok(())
    }

    /// Ensure the Qdrant collection exists (create if not)
    ///
    /// Must be called before any upsert operations to ensure the target
    /// collection has been created.
    pub async fn initialize_qdrant(&self) -> Result<(), OrchestratorError> {
        if let Some(ref qdrant) = self.qdrant {
            qdrant.initialize().await?;
            tracing::info!("Qdrant collection initialized");
        }
        Ok(())
    }

    /// Check if BM25 full-text search is configured
    pub fn has_bm25(&self) -> bool {
        self.bm25.is_some()
    }

    /// Get the configured embedder, if any.
    pub fn embedder(&self) -> Option<&Arc<dyn Embedder>> {
        self.embedder.as_ref()
    }

    /// Get the configured SQLite metadata store, if any.
    pub fn metadata_client(&self) -> Option<&Arc<SqliteClient>> {
        self.metadata_store.as_ref()
    }

    /// Get the project ID this coordinator writes for.
    pub fn project_id(&self) -> i64 {
        self.project_id
    }
}
