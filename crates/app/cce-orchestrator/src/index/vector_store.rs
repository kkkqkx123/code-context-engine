//! Backend-enum vector storage dispatch.
//!
//! The assembly layer selects one backend per configuration and uses the same
//! instance for indexing and retrieval. No trait objects are used.

use std::sync::Arc;

use cce_storage_bm25::{Bm25Client, ElasticsearchClient};
use cce_storage_common::metadb::{
    AdmissionAuditRecord, CheckpointRecord, CheckpointStatus, ChunkRecord, EntityDetailMapping,
    EntityRecord, FileCheckpointRecord, FileRecord, GenerationOverride, ProjectIndexManifest,
    ProjectRecord, RelationStorage, WorkUnitCheckpointRecord, WorkUnitStatus,
};
use cce_storage_common::{
    DenseSearchQuery, FulltextDocument, FulltextError, FulltextHit, FulltextSearchOptions,
    FulltextStorage, ScoredPoint, VectorPoint, VectorStorage,
};
use cce_storage_metadb_pg::PostgresClient;
use cce_storage_metadb_sqlite::SqliteClient;
use cce_storage_vector_local::LocalVectorStore;
use cce_storage_vector_qdrant::QdrantClient;
use cce_types::{PointKind, StorageError};

/// Vector backend holding one concrete implementation.
///
/// Local is the default embedded engine; Qdrant keeps the remote service for
/// large-scale deployments. Switching backends requires a full reindex.
#[derive(Clone)]
pub enum VectorStore {
    /// Embedded local engine (mmap plus WAL plus HNSW).
    Local(Arc<LocalVectorStore>),
    /// External Qdrant service.
    Qdrant(Arc<QdrantClient>),
}

impl VectorStore {
    /// Wrap a local store.
    pub fn local(store: Arc<LocalVectorStore>) -> Self {
        Self::Local(store)
    }

    /// Wrap a Qdrant client.
    pub fn qdrant(client: Arc<QdrantClient>) -> Self {
        Self::Qdrant(client)
    }

    /// Whether this is the embedded backend.
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }

    /// Whether this is the remote backend.
    pub fn is_qdrant(&self) -> bool {
        matches!(self, Self::Qdrant(_))
    }

    /// Backend name for logging (`local` or `qdrant`).
    pub fn backend_name(&self) -> &'static str {
        match self {
            Self::Local(_) => "local",
            Self::Qdrant(_) => "qdrant",
        }
    }

    /// Borrow the Qdrant client when this is the remote branch.
    ///
    /// Reserved for the Qdrant lifecycle task, the only caller that needs
    /// the concrete client (process management). Status and health paths
    /// must use the backend-neutral diagnostics below instead.
    pub fn as_qdrant(&self) -> Option<&Arc<QdrantClient>> {
        match self {
            Self::Qdrant(client) => Some(client),
            _ => None,
        }
    }

    /// Borrow the local store when this is the embedded branch.
    ///
    /// Reserved for the Qdrant lifecycle task, the only caller that needs
    /// the concrete client (process management). Status and health paths
    /// must use the backend-neutral diagnostics below instead.
    pub fn as_local(&self) -> Option<&Arc<LocalVectorStore>> {
        match self {
            Self::Local(store) => Some(store),
            _ => None,
        }
    }
}

/// Backend-neutral vector diagnostics snapshot.
///
/// Lets HTTP handlers report health without downcasting to a concrete
/// client: the local branch synthesizes the snapshot from its own
/// health/count probes, the Qdrant branch delegates to its diagnostic call.
#[derive(Debug, Clone)]
pub struct VectorDiagnostics {
    /// Whether the backend is reachable and healthy.
    pub reachable: bool,
    /// Backend version, when the backend exposes one.
    pub version: Option<String>,
    /// Whether the shared collection exists.
    pub collection_exists: bool,
    /// Number of points in the collection.
    pub points_count: u64,
    /// Human-readable failure detail, when unhealthy.
    pub error: Option<String>,
}

impl VectorStore {
    /// Circuit-breaker summary without touching the concrete client.
    ///
    /// The embedded branch has no breaker (in-process calls, no transient
    /// network failures), so it reports a fixed marker string.
    pub fn circuit_breaker_summary(&self) -> String {
        match self {
            Self::Local(_) => "n/a (local backend)".to_string(),
            Self::Qdrant(client) => client.circuit_breaker_state(),
        }
    }

    /// Whether the backend manages its own server process.
    ///
    /// Returns the Qdrant `auto_start` flag on the remote branch and `None`
    /// on the embedded branch, which has no subprocess to manage.
    pub fn managed_process(&self) -> Option<bool> {
        match self {
            Self::Local(_) => None,
            Self::Qdrant(client) => Some(client.config().auto_start),
        }
    }

    /// Backend-neutral diagnostics snapshot for status endpoints.
    ///
    /// Never fails: probe errors are folded into the snapshot so handlers
    /// stay branch-free.
    pub async fn diagnose_summary(&self) -> VectorDiagnostics {
        match self {
            Self::Qdrant(client) => match client.diagnose().await {
                Ok(diag) => VectorDiagnostics {
                    reachable: diag.reachable,
                    version: diag.version,
                    collection_exists: diag.collection_exists,
                    points_count: diag.points_count,
                    error: diag.error,
                },
                Err(e) => VectorDiagnostics {
                    reachable: false,
                    version: None,
                    collection_exists: false,
                    points_count: 0,
                    error: Some(format!("Diagnostic failed: {e}")),
                },
            },
            Self::Local(store) => {
                let healthy = VectorStorage::health(store.as_ref()).await.unwrap_or(false);
                let (collection_exists, points_count) = match (
                    VectorStorage::collection_exists(store.as_ref()).await,
                    VectorStorage::count_all_points(store.as_ref()).await,
                ) {
                    (Ok(exists), Ok(count)) => (exists, count as u64),
                    (Ok(exists), Err(_)) => (exists, 0),
                    _ => (false, 0),
                };
                VectorDiagnostics {
                    reachable: healthy,
                    version: None,
                    collection_exists,
                    points_count,
                    error: if healthy {
                        None
                    } else {
                        Some("Local vector store reported unhealthy".to_string())
                    },
                }
            }
        }
    }

    /// Build a store from the resolved database configuration.
    ///
    /// Local resolves its data dir against the SQLite path; Qdrant uses the
    /// configured URL. The workspace path only affects Qdrant collection
    /// naming compatibility (both branches share one collection name).
    pub fn from_database_config(
        database: &cce_config::global::DatabaseConfig,
    ) -> Result<Self, StorageError> {
        match database.vector_backend {
            cce_config::modules::VectorBackend::Local => {
                let store = LocalVectorStore::open(&database.vector_local, &database.sqlite.path)
                    .map_err(|e| {
                    StorageError::query(format!("failed to open local vector store: {e}"))
                })?;
                Ok(Self::Local(Arc::new(store)))
            }
            cce_config::modules::VectorBackend::Qdrant => {
                let client = QdrantClient::new(database.qdrant.clone(), ".").map_err(|e| {
                    StorageError::query(format!("failed to create qdrant client: {e}"))
                })?;
                Ok(Self::Qdrant(Arc::new(client)))
            }
        }
    }
}

impl std::fmt::Debug for VectorStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local(_) => write!(f, "VectorStore::Local(..)"),
            Self::Qdrant(_) => write!(f, "VectorStore::Qdrant(..)"),
        }
    }
}

impl VectorStorage for VectorStore {
    fn backend_name(&self) -> &'static str {
        match self {
            Self::Local(store) => VectorStorage::backend_name(store.as_ref()),
            Self::Qdrant(client) => VectorStorage::backend_name(client.as_ref()),
        }
    }

    async fn ensure_collection(&self) -> Result<bool, StorageError> {
        match self {
            Self::Local(store) => VectorStorage::ensure_collection(store.as_ref()).await,
            Self::Qdrant(client) => VectorStorage::ensure_collection(client.as_ref()).await,
        }
    }

    async fn collection_exists(&self) -> Result<bool, StorageError> {
        match self {
            Self::Local(store) => VectorStorage::collection_exists(store.as_ref()).await,
            Self::Qdrant(client) => VectorStorage::collection_exists(client.as_ref()).await,
        }
    }

    async fn delete_collection(&self) -> Result<(), StorageError> {
        match self {
            Self::Local(store) => VectorStorage::delete_collection(store.as_ref()).await,
            Self::Qdrant(client) => VectorStorage::delete_collection(client.as_ref()).await,
        }
    }

    async fn clear_collection(&self) -> Result<(), StorageError> {
        match self {
            Self::Local(store) => VectorStorage::clear_collection(store.as_ref()).await,
            Self::Qdrant(client) => VectorStorage::clear_collection(client.as_ref()).await,
        }
    }

    async fn upsert_points(&self, points: &[VectorPoint]) -> Result<(), StorageError> {
        match self {
            Self::Local(store) => VectorStorage::upsert_points(store.as_ref(), points).await,
            Self::Qdrant(client) => VectorStorage::upsert_points(client.as_ref(), points).await,
        }
    }

    async fn search_dense(
        &self,
        query: DenseSearchQuery,
    ) -> Result<Vec<ScoredPoint>, StorageError> {
        match self {
            Self::Local(store) => VectorStorage::search_dense(store.as_ref(), query).await,
            Self::Qdrant(client) => VectorStorage::search_dense(client.as_ref(), query).await,
        }
    }

    async fn delete_by_file_path_scoped(
        &self,
        file_path: &str,
        group_id: &str,
        point_type: Option<PointKind>,
    ) -> Result<(), StorageError> {
        match self {
            Self::Local(store) => {
                VectorStorage::delete_by_file_path_scoped(
                    store.as_ref(),
                    file_path,
                    group_id,
                    point_type,
                )
                .await
            }
            Self::Qdrant(client) => {
                VectorStorage::delete_by_file_path_scoped(
                    client.as_ref(),
                    file_path,
                    group_id,
                    point_type,
                )
                .await
            }
        }
    }

    async fn delete_by_file_path_scoped_epoch(
        &self,
        file_path: &str,
        group_id: &str,
        epoch: i64,
    ) -> Result<(), StorageError> {
        match self {
            Self::Local(store) => {
                VectorStorage::delete_by_file_path_scoped_epoch(
                    store.as_ref(),
                    file_path,
                    group_id,
                    epoch,
                )
                .await
            }
            Self::Qdrant(client) => {
                VectorStorage::delete_by_file_path_scoped_epoch(
                    client.as_ref(),
                    file_path,
                    group_id,
                    epoch,
                )
                .await
            }
        }
    }

    async fn delete_by_group_epoch(&self, group_id: &str, epoch: i64) -> Result<(), StorageError> {
        match self {
            Self::Local(store) => {
                VectorStorage::delete_by_group_epoch(store.as_ref(), group_id, epoch).await
            }
            Self::Qdrant(client) => {
                VectorStorage::delete_by_group_epoch(client.as_ref(), group_id, epoch).await
            }
        }
    }

    async fn delete_by_group(&self, group_id: &str) -> Result<(), StorageError> {
        match self {
            Self::Local(store) => VectorStorage::delete_by_group(store.as_ref(), group_id).await,
            Self::Qdrant(client) => VectorStorage::delete_by_group(client.as_ref(), group_id).await,
        }
    }

    async fn scroll_all_points(&self) -> Result<Vec<VectorPoint>, StorageError> {
        match self {
            Self::Local(store) => VectorStorage::scroll_all_points(store.as_ref()).await,
            Self::Qdrant(client) => VectorStorage::scroll_all_points(client.as_ref()).await,
        }
    }

    async fn count_points_by_group(&self, group_id: &str) -> Result<usize, StorageError> {
        match self {
            Self::Local(store) => {
                VectorStorage::count_points_by_group(store.as_ref(), group_id).await
            }
            Self::Qdrant(client) => {
                VectorStorage::count_points_by_group(client.as_ref(), group_id).await
            }
        }
    }

    async fn count_all_points(&self) -> Result<usize, StorageError> {
        match self {
            Self::Local(store) => VectorStorage::count_all_points(store.as_ref()).await,
            Self::Qdrant(client) => VectorStorage::count_all_points(client.as_ref()).await,
        }
    }

    async fn health(&self) -> Result<bool, StorageError> {
        match self {
            Self::Local(store) => VectorStorage::health(store.as_ref()).await,
            Self::Qdrant(client) => VectorStorage::health(client.as_ref()).await,
        }
    }
}

/// Fulltext backend holding one concrete implementation.
///
/// Local is the embedded Tantivy branch and the only supported branch; the
/// remote Elasticsearch client is forward scaffolding and assembly rejects
/// it at startup until the remote read and write paths are wired. The enum
/// mirrors [`VectorStore`] so assembly selects branches from configuration
/// instead of threading concrete client types through every caller.
#[derive(Clone)]
pub enum FulltextStore {
    /// Embedded Tantivy index.
    Local(Arc<Bm25Client>),
    /// External Elasticsearch service.
    Remote(Arc<ElasticsearchClient>),
}

impl FulltextStore {
    /// Wrap a local BM25 client.
    pub fn local(client: Arc<Bm25Client>) -> Self {
        Self::Local(client)
    }

    /// Wrap a remote Elasticsearch client.
    pub fn remote(client: Arc<ElasticsearchClient>) -> Self {
        Self::Remote(client)
    }

    /// Whether this is the embedded backend.
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }

    /// Whether this is the remote backend.
    pub fn is_remote(&self) -> bool {
        matches!(self, Self::Remote(_))
    }

    /// Backend name for logging (`local` or `remote`).
    pub fn backend_name(&self) -> &'static str {
        match self {
            Self::Local(_) => "local",
            Self::Remote(_) => "remote",
        }
    }

    /// Borrow the local client when this is the embedded branch.
    pub fn as_local(&self) -> Option<&Arc<Bm25Client>> {
        match self {
            Self::Local(client) => Some(client),
            _ => None,
        }
    }

    /// Borrow the remote client when this is the remote branch.
    pub fn as_remote(&self) -> Option<&Arc<ElasticsearchClient>> {
        match self {
            Self::Remote(client) => Some(client),
            _ => None,
        }
    }

    /// Unwrap into the local handle when this is the embedded branch.
    pub fn into_local(self) -> Option<Arc<Bm25Client>> {
        match self {
            Self::Local(client) => Some(client),
            _ => None,
        }
    }

    /// Unwrap into the remote handle when this is the remote branch.
    pub fn into_remote(self) -> Option<Arc<ElasticsearchClient>> {
        match self {
            Self::Remote(client) => Some(client),
            _ => None,
        }
    }

    /// Build the store from the resolved database configuration.
    ///
    /// The remote branch is rejected at startup: only the local backend is
    /// supported, and refusing here keeps a remote selection from failing
    /// halfway through indexing or search.
    pub fn from_database_config(
        database: &cce_config::global::DatabaseConfig,
    ) -> Result<Self, StorageError> {
        match database.fulltext_backend {
            cce_config::modules::FulltextBackend::Local => {
                let client = Bm25Client::new(database.bm25.clone());
                Ok(Self::Local(Arc::new(client)))
            }
            cce_config::modules::FulltextBackend::Remote => Err(StorageError::validation(
                "remote fulltext backend is not supported yet: the supported matrix is local-only; remote branches are forward scaffolding",
            )),
        }
    }
}

impl std::fmt::Debug for FulltextStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local(_) => write!(f, "FulltextStore::Local(..)"),
            Self::Remote(_) => write!(f, "FulltextStore::Remote(..)"),
        }
    }
}

impl FulltextStorage for FulltextStore {
    async fn batch_index(
        &self,
        index_name: &str,
        documents: &[FulltextDocument],
    ) -> Result<usize, FulltextError> {
        match self {
            Self::Local(client) => {
                FulltextStorage::batch_index(client.as_ref(), index_name, documents).await
            }
            Self::Remote(client) => {
                FulltextStorage::batch_index(client.as_ref(), index_name, documents).await
            }
        }
    }

    async fn search(
        &self,
        query: &str,
        options: &FulltextSearchOptions,
    ) -> Result<Vec<FulltextHit>, FulltextError> {
        match self {
            Self::Local(client) => FulltextStorage::search(client.as_ref(), query, options).await,
            Self::Remote(client) => FulltextStorage::search(client.as_ref(), query, options).await,
        }
    }

    async fn delete_by_file_path_scoped(
        &self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
    ) -> Result<usize, FulltextError> {
        match self {
            Self::Local(client) => {
                FulltextStorage::delete_by_file_path_scoped(
                    client.as_ref(),
                    index_name,
                    file_path,
                    project_id,
                )
                .await
            }
            Self::Remote(client) => {
                FulltextStorage::delete_by_file_path_scoped(
                    client.as_ref(),
                    index_name,
                    file_path,
                    project_id,
                )
                .await
            }
        }
    }

    async fn delete_by_file_path_scoped_epoch(
        &self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, FulltextError> {
        match self {
            Self::Local(client) => {
                FulltextStorage::delete_by_file_path_scoped_epoch(
                    client.as_ref(),
                    index_name,
                    file_path,
                    project_id,
                    epoch,
                )
                .await
            }
            Self::Remote(client) => {
                FulltextStorage::delete_by_file_path_scoped_epoch(
                    client.as_ref(),
                    index_name,
                    file_path,
                    project_id,
                    epoch,
                )
                .await
            }
        }
    }

    async fn delete_by_project_epoch(
        &self,
        index_name: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, FulltextError> {
        match self {
            Self::Local(client) => {
                FulltextStorage::delete_by_project_epoch(
                    client.as_ref(),
                    index_name,
                    project_id,
                    epoch,
                )
                .await
            }
            Self::Remote(client) => {
                FulltextStorage::delete_by_project_epoch(
                    client.as_ref(),
                    index_name,
                    project_id,
                    epoch,
                )
                .await
            }
        }
    }

    async fn delete_all_project_docs(
        &self,
        index_name: &str,
        project_id: i64,
    ) -> Result<usize, FulltextError> {
        match self {
            Self::Local(client) => {
                FulltextStorage::delete_all_project_docs(client.as_ref(), index_name, project_id)
                    .await
            }
            Self::Remote(client) => {
                FulltextStorage::delete_all_project_docs(client.as_ref(), index_name, project_id)
                    .await
            }
        }
    }

    async fn snapshot_documents(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<FulltextDocument>, FulltextError> {
        match self {
            Self::Local(client) => {
                FulltextStorage::snapshot_documents(client.as_ref(), project_id, epoch).await
            }
            Self::Remote(client) => {
                FulltextStorage::snapshot_documents(client.as_ref(), project_id, epoch).await
            }
        }
    }

    async fn document_count(&self) -> Result<usize, FulltextError> {
        match self {
            Self::Local(client) => FulltextStorage::document_count(client.as_ref()).await,
            Self::Remote(client) => FulltextStorage::document_count(client.as_ref()).await,
        }
    }

    async fn document_count_by_project(&self, project_id: i64) -> Result<usize, FulltextError> {
        match self {
            Self::Local(client) => {
                FulltextStorage::document_count_by_project(client.as_ref(), project_id).await
            }
            Self::Remote(client) => {
                FulltextStorage::document_count_by_project(client.as_ref(), project_id).await
            }
        }
    }

    async fn epochs_by_project(&self, project_id: i64) -> Result<Vec<i64>, FulltextError> {
        match self {
            Self::Local(client) => {
                FulltextStorage::epochs_by_project(client.as_ref(), project_id).await
            }
            Self::Remote(client) => {
                FulltextStorage::epochs_by_project(client.as_ref(), project_id).await
            }
        }
    }

    async fn clear_index(&self, index_name: &str) -> Result<usize, FulltextError> {
        match self {
            Self::Local(client) => FulltextStorage::clear_index(client.as_ref(), index_name).await,
            Self::Remote(client) => FulltextStorage::clear_index(client.as_ref(), index_name).await,
        }
    }

    async fn flush(&self) -> Result<(), FulltextError> {
        match self {
            Self::Local(client) => FulltextStorage::flush(client.as_ref()).await,
            Self::Remote(client) => FulltextStorage::flush(client.as_ref()).await,
        }
    }

    fn is_enabled(&self) -> bool {
        match self {
            Self::Local(client) => FulltextStorage::is_enabled(client.as_ref()),
            Self::Remote(client) => FulltextStorage::is_enabled(client.as_ref()),
        }
    }

    fn backend_name(&self) -> &'static str {
        match self {
            Self::Local(client) => FulltextStorage::backend_name(client.as_ref()),
            Self::Remote(client) => FulltextStorage::backend_name(client.as_ref()),
        }
    }
}

/// Backend-neutral fulltext diagnostics snapshot.
///
/// Mirrors [`VectorDiagnostics`]: the local branch probes its document
/// count, the remote branch probes the search service over HTTP. Probe
/// failures fold into the snapshot so handlers stay branch-free.
#[derive(Debug, Clone)]
pub struct FulltextDiagnostics {
    /// Whether the backend is reachable and serving.
    pub reachable: bool,
    /// Backend version, when the backend exposes one.
    pub version: Option<String>,
    /// Whether the index exists and answers reads.
    pub index_exists: bool,
    /// Number of documents in the index.
    pub documents_count: u64,
    /// Human-readable failure detail, when unhealthy.
    pub error: Option<String>,
}

impl FulltextStore {
    /// Configured index name of the active branch.
    ///
    /// Both branches validate the per-call index name against their own
    /// configuration, so write and delete paths must use this name instead
    /// of a hardcoded literal.
    pub fn configured_index_name(&self) -> String {
        match self {
            Self::Local(client) => client.config().index_name.clone(),
            Self::Remote(client) => client.config().index_name.clone(),
        }
    }

    /// Backend-neutral diagnostics snapshot for status endpoints.
    ///
    /// Never fails: probe errors are folded into the snapshot so handlers
    /// stay branch-free.
    pub async fn diagnose_summary(&self) -> FulltextDiagnostics {
        match FulltextStorage::document_count(self).await {
            Ok(count) => FulltextDiagnostics {
                reachable: FulltextStorage::is_enabled(self),
                version: None,
                index_exists: true,
                documents_count: count as u64,
                error: if FulltextStorage::is_enabled(self) {
                    None
                } else {
                    Some("Fulltext backend is disabled".to_string())
                },
            },
            Err(e) => FulltextDiagnostics {
                reachable: false,
                version: None,
                index_exists: false,
                documents_count: 0,
                error: Some(format!("Diagnostic failed: {e}")),
            },
        }
    }
}

/// Relation backend holding one concrete implementation.
///
/// Local is the embedded SQLite branch (per-project database files) and the
/// only supported branch; the remote PostgreSQL client is forward
/// scaffolding and assembly rejects it at startup until the remote paths
/// are wired. Path rules, cache eviction, capacity stats, and the
/// project-delete two-step semantics stay inside the local client.
#[derive(Clone)]
pub enum RelationStore {
    /// Embedded SQLite repositories.
    Local(Arc<SqliteClient>),
    /// External PostgreSQL service.
    Remote(Arc<PostgresClient>),
}

impl RelationStore {
    /// Wrap a local SQLite client.
    pub fn local(client: Arc<SqliteClient>) -> Self {
        Self::Local(client)
    }

    /// Wrap a remote PostgreSQL client.
    pub fn remote(client: Arc<PostgresClient>) -> Self {
        Self::Remote(client)
    }

    /// Whether this is the embedded backend.
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }

    /// Whether this is the remote backend.
    pub fn is_remote(&self) -> bool {
        matches!(self, Self::Remote(_))
    }

    /// Backend name for logging (`local` or `remote`).
    pub fn backend_name(&self) -> &'static str {
        match self {
            Self::Local(_) => "local",
            Self::Remote(_) => "remote",
        }
    }

    /// Borrow the local client when this is the embedded branch.
    pub fn as_local(&self) -> Option<&Arc<SqliteClient>> {
        match self {
            Self::Local(client) => Some(client),
            _ => None,
        }
    }

    /// Borrow the remote client when this is the remote branch.
    pub fn as_remote(&self) -> Option<&Arc<PostgresClient>> {
        match self {
            Self::Remote(client) => Some(client),
            _ => None,
        }
    }

    /// Unwrap into the local handle when this is the embedded branch.
    pub fn into_local(self) -> Option<Arc<SqliteClient>> {
        match self {
            Self::Local(client) => Some(client),
            _ => None,
        }
    }

    /// Unwrap into the remote handle when this is the remote branch.
    pub fn into_remote(self) -> Option<Arc<PostgresClient>> {
        match self {
            Self::Remote(client) => Some(client),
            _ => None,
        }
    }

    /// Open the per-project scoped handle.
    ///
    /// The local branch opens the per-project database file; the remote
    /// branch carries project filtering per query, so scoping is a clone.
    pub fn for_project(&self, project_id: i64) -> Result<Self, StorageError> {
        match self {
            Self::Local(client) => Ok(Self::Local(client.for_project(project_id)?)),
            Self::Remote(client) => Ok(Self::Remote(client.clone())),
        }
    }

    /// Build the store from the resolved database configuration.
    ///
    /// The remote branch is rejected at startup: only the local backend is
    /// supported, and refusing here keeps a remote selection from failing
    /// halfway through indexing or search.
    pub fn from_database_config(
        database: &cce_config::global::DatabaseConfig,
    ) -> Result<Self, StorageError> {
        match database.relation_backend {
            cce_config::modules::RelationBackend::Local => {
                let client = SqliteClient::new(database.sqlite.clone()).map_err(|e| {
                    StorageError::query(format!("failed to open relation store: {e}"))
                })?;
                Ok(Self::Local(Arc::new(client)))
            }
            cce_config::modules::RelationBackend::Remote => Err(StorageError::validation(
                "remote relation backend is not supported yet: the supported matrix is local-only; remote branches are forward scaffolding",
            )),
        }
    }
}

impl std::fmt::Debug for RelationStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local(_) => write!(f, "RelationStore::Local(..)"),
            Self::Remote(_) => write!(f, "RelationStore::Remote(..)"),
        }
    }
}

macro_rules! dispatch_relation {
    ($self:expr, $method:ident ($($arg:expr),*)) => {
        match $self {
            Self::Local(client) => RelationStorage::$method(client.as_ref(), $($arg),*).await,
            Self::Remote(client) => RelationStorage::$method(client.as_ref(), $($arg),*).await,
        }
    };
}

impl RelationStorage for RelationStore {
    async fn ensure_project(&self, project_id: i64, root_path: &str) -> Result<(), StorageError> {
        dispatch_relation!(self, ensure_project(project_id, root_path))
    }

    async fn project_record(&self, project_id: i64) -> Result<Option<ProjectRecord>, StorageError> {
        dispatch_relation!(self, project_record(project_id))
    }

    async fn project_meta_get_int(&self, project_id: i64, key: &str) -> Result<i64, StorageError> {
        dispatch_relation!(self, project_meta_get_int(project_id, key))
    }

    async fn project_meta_set_int(
        &self,
        project_id: i64,
        key: &str,
        value: i64,
    ) -> Result<(), StorageError> {
        dispatch_relation!(self, project_meta_set_int(project_id, key, value))
    }

    async fn manifest_begin_building(
        &self,
        project_id: i64,
        data_epoch: i64,
        operation_id: &str,
        input_fingerprint: Option<&str>,
    ) -> Result<ProjectIndexManifest, StorageError> {
        dispatch_relation!(
            self,
            manifest_begin_building(project_id, data_epoch, operation_id, input_fingerprint)
        )
    }

    async fn manifest_mark_candidate_ready(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<(), StorageError> {
        dispatch_relation!(
            self,
            manifest_mark_candidate_ready(project_id, operation_id)
        )
    }

    async fn manifest_activate(
        &self,
        project_id: i64,
        data_epoch: i64,
        relation_epoch: i64,
        operation_id: &str,
        input_fingerprint: Option<&str>,
    ) -> Result<ProjectIndexManifest, StorageError> {
        dispatch_relation!(
            self,
            manifest_activate(
                project_id,
                data_epoch,
                relation_epoch,
                operation_id,
                input_fingerprint
            )
        )
    }

    async fn manifest_mark_failed(
        &self,
        project_id: i64,
        operation_id: &str,
        reason: &str,
    ) -> Result<(), StorageError> {
        dispatch_relation!(self, manifest_mark_failed(project_id, operation_id, reason))
    }

    async fn manifest_active(
        &self,
        project_id: i64,
    ) -> Result<Option<ProjectIndexManifest>, StorageError> {
        dispatch_relation!(self, manifest_active(project_id))
    }

    async fn manifest_recycle_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, manifest_recycle_epoch(project_id, epoch))
    }

    async fn overrides_replace(
        &self,
        project_id: i64,
        epoch: i64,
        overrides: &[GenerationOverride],
    ) -> Result<(), StorageError> {
        dispatch_relation!(self, overrides_replace(project_id, epoch, overrides))
    }

    async fn overrides_for_generation(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<GenerationOverride>, StorageError> {
        dispatch_relation!(self, overrides_for_generation(project_id, epoch))
    }

    async fn files_upsert(
        &self,
        project_id: i64,
        epoch: i64,
        files: &[FileRecord],
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, files_upsert(project_id, epoch, files))
    }

    async fn files_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, files_delete_by_project_epoch(project_id, epoch))
    }

    async fn files_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        dispatch_relation!(self, files_delete_by_project(project_id))
    }

    async fn entities_upsert(&self, entities: &[EntityRecord]) -> Result<usize, StorageError> {
        dispatch_relation!(self, entities_upsert(entities))
    }

    async fn entities_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, entities_delete_by_project_epoch(project_id, epoch))
    }

    async fn entities_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        dispatch_relation!(self, entities_delete_by_project(project_id))
    }

    async fn entities_count(&self, project_id: i64, epoch: i64) -> Result<i64, StorageError> {
        dispatch_relation!(self, entities_count(project_id, epoch))
    }

    async fn chunks_upsert(&self, chunks: &[ChunkRecord]) -> Result<usize, StorageError> {
        dispatch_relation!(self, chunks_upsert(chunks))
    }

    async fn chunks_by_ids(
        &self,
        project_id: i64,
        chunk_ids: &[String],
        epochs: &[i64],
    ) -> Result<Vec<ChunkRecord>, StorageError> {
        dispatch_relation!(self, chunks_by_ids(project_id, chunk_ids, epochs))
    }

    async fn chunks_delete_by_file(
        &self,
        project_id: i64,
        file_path: &str,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, chunks_delete_by_file(project_id, file_path))
    }

    async fn chunks_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, chunks_delete_by_project_epoch(project_id, epoch))
    }

    async fn chunks_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        dispatch_relation!(self, chunks_delete_by_project(project_id))
    }

    async fn chunks_count(&self, project_id: i64, epoch: i64) -> Result<i64, StorageError> {
        dispatch_relation!(self, chunks_count(project_id, epoch))
    }

    async fn mappings_upsert(
        &self,
        mappings: &[EntityDetailMapping],
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, mappings_upsert(mappings))
    }

    async fn mappings_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, mappings_delete_by_project_epoch(project_id, epoch))
    }

    async fn mappings_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        dispatch_relation!(self, mappings_delete_by_project(project_id))
    }

    async fn summary_upsert(
        &self,
        file_id: i64,
        epoch: i64,
        summary_json: &str,
    ) -> Result<(), StorageError> {
        dispatch_relation!(self, summary_upsert(file_id, epoch, summary_json))
    }

    async fn summary_at_epoch(
        &self,
        file_id: i64,
        epoch: i64,
    ) -> Result<Option<String>, StorageError> {
        dispatch_relation!(self, summary_at_epoch(file_id, epoch))
    }

    async fn summaries_by_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<(String, String, i64)>, StorageError> {
        dispatch_relation!(self, summaries_by_epoch(project_id, epoch))
    }

    async fn summaries_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, summaries_delete_by_project_epoch(project_id, epoch))
    }

    async fn summaries_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        dispatch_relation!(self, summaries_delete_by_project(project_id))
    }

    async fn checkpoint_create(
        &self,
        project_id: i64,
        checkpoint: &CheckpointRecord,
    ) -> Result<i64, StorageError> {
        dispatch_relation!(self, checkpoint_create(project_id, checkpoint))
    }

    async fn checkpoint_get(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Option<CheckpointRecord>, StorageError> {
        dispatch_relation!(self, checkpoint_get(project_id, operation_id))
    }

    async fn checkpoint_set_status(
        &self,
        project_id: i64,
        operation_id: &str,
        status: CheckpointStatus,
    ) -> Result<(), StorageError> {
        dispatch_relation!(
            self,
            checkpoint_set_status(project_id, operation_id, status)
        )
    }

    async fn checkpoint_update(
        &self,
        project_id: i64,
        checkpoint: &CheckpointRecord,
    ) -> Result<(), StorageError> {
        dispatch_relation!(self, checkpoint_update(project_id, checkpoint))
    }

    async fn file_checkpoint_upsert(
        &self,
        project_id: i64,
        file: &FileCheckpointRecord,
    ) -> Result<(), StorageError> {
        dispatch_relation!(self, file_checkpoint_upsert(project_id, file))
    }

    async fn file_checkpoint_get(
        &self,
        project_id: i64,
        operation_id: &str,
        file_path: &str,
    ) -> Result<Option<FileCheckpointRecord>, StorageError> {
        dispatch_relation!(
            self,
            file_checkpoint_get(project_id, operation_id, file_path)
        )
    }

    async fn checkpoint_files_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(
            self,
            checkpoint_files_delete_by_operation(project_id, operation_id)
        )
    }

    async fn work_unit_insert(
        &self,
        record: &WorkUnitCheckpointRecord,
    ) -> Result<i64, StorageError> {
        dispatch_relation!(self, work_unit_insert(record))
    }

    async fn work_unit_set_status(
        &self,
        project_id: i64,
        operation_id: &str,
        stage: &str,
        work_unit_hash: &str,
        status: WorkUnitStatus,
    ) -> Result<(), StorageError> {
        dispatch_relation!(
            self,
            work_unit_set_status(project_id, operation_id, stage, work_unit_hash, status)
        )
    }

    async fn work_units_list(
        &self,
        project_id: i64,
        operation_id: &str,
        stage: &str,
    ) -> Result<Vec<WorkUnitCheckpointRecord>, StorageError> {
        dispatch_relation!(self, work_units_list(project_id, operation_id, stage))
    }

    async fn work_unit_by_hash(
        &self,
        project_id: i64,
        operation_id: &str,
        stage: &str,
        work_unit_hash: &str,
    ) -> Result<Option<WorkUnitCheckpointRecord>, StorageError> {
        dispatch_relation!(
            self,
            work_unit_by_hash(project_id, operation_id, stage, work_unit_hash)
        )
    }

    async fn snapshot_allocate(
        &self,
        project_id: i64,
        operation_id: &str,
        config_fingerprint: &str,
    ) -> Result<i64, StorageError> {
        dispatch_relation!(
            self,
            snapshot_allocate(project_id, operation_id, config_fingerprint)
        )
    }

    async fn snapshot_write_ready(
        &self,
        project_id: i64,
        epoch: i64,
        snapshot: &cce_types::CanonicalRelationSnapshot,
        input_fingerprint: &str,
        snapshot_fingerprint: &str,
    ) -> Result<(), StorageError> {
        dispatch_relation!(
            self,
            snapshot_write_ready(
                project_id,
                epoch,
                snapshot,
                input_fingerprint,
                snapshot_fingerprint
            )
        )
    }

    async fn snapshot_read(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<cce_types::CanonicalRelationSnapshot, StorageError> {
        dispatch_relation!(self, snapshot_read(project_id, epoch))
    }

    async fn snapshot_manifest(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<cce_types::RelationSnapshotManifest>, StorageError> {
        dispatch_relation!(self, snapshot_manifest(project_id, epoch))
    }

    async fn snapshot_delta_chain(
        &self,
        project_id: i64,
        after_epoch: i64,
        up_to_epoch: i64,
    ) -> Result<Vec<cce_types::SnapshotDelta>, StorageError> {
        dispatch_relation!(
            self,
            snapshot_delta_chain(project_id, after_epoch, up_to_epoch)
        )
    }

    async fn snapshot_find_base(
        &self,
        project_id: i64,
        delta_epoch: i64,
    ) -> Result<Option<i64>, StorageError> {
        dispatch_relation!(self, snapshot_find_base(project_id, delta_epoch))
    }

    async fn snapshot_mark_failed(
        &self,
        project_id: i64,
        epoch: i64,
        reason: &str,
    ) -> Result<(), StorageError> {
        dispatch_relation!(self, snapshot_mark_failed(project_id, epoch, reason))
    }

    async fn snapshot_delete_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, snapshot_delete_epoch(project_id, epoch))
    }

    async fn snapshot_delete_project(&self, project_id: i64) -> Result<usize, StorageError> {
        dispatch_relation!(self, snapshot_delete_project(project_id))
    }

    async fn admission_record_admitted(
        &self,
        fingerprint: &str,
        projects: &[i64],
        quota_bytes: Option<u64>,
        bytes: u64,
    ) -> Result<(), StorageError> {
        dispatch_relation!(
            self,
            admission_record_admitted(fingerprint, projects, quota_bytes, bytes)
        )
    }

    async fn admission_record_rejection(
        &self,
        fingerprint: &str,
        projects: &[i64],
        quota_bytes: Option<u64>,
        reason: &str,
    ) -> Result<(), StorageError> {
        dispatch_relation!(
            self,
            admission_record_rejection(fingerprint, projects, quota_bytes, reason)
        )
    }

    async fn admission_get(
        &self,
        fingerprint: &str,
    ) -> Result<Option<AdmissionAuditRecord>, StorageError> {
        dispatch_relation!(self, admission_get(fingerprint))
    }

    async fn admission_list(&self) -> Result<Vec<AdmissionAuditRecord>, StorageError> {
        dispatch_relation!(self, admission_list())
    }

    async fn db_size(&self) -> Result<u64, StorageError> {
        dispatch_relation!(self, db_size())
    }

    async fn delete_project_db(&self, project_id: i64) -> Result<usize, StorageError> {
        dispatch_relation!(self, delete_project_db(project_id))
    }

    fn backend_name(&self) -> &'static str {
        match self {
            Self::Local(client) => RelationStorage::backend_name(client.as_ref()),
            Self::Remote(client) => RelationStorage::backend_name(client.as_ref()),
        }
    }

    fn is_per_project_db(&self) -> bool {
        match self {
            Self::Local(client) => RelationStorage::is_per_project_db(client.as_ref()),
            Self::Remote(client) => RelationStorage::is_per_project_db(client.as_ref()),
        }
    }
}

/// Backend-neutral relation diagnostics snapshot.
///
/// Mirrors [`VectorDiagnostics`]: both branches probe their database size.
/// Probe failures fold into the snapshot so handlers stay branch-free.
#[derive(Debug, Clone)]
pub struct RelationDiagnostics {
    /// Whether the backend is reachable and serving.
    pub reachable: bool,
    /// Backend version, when the backend exposes one.
    pub version: Option<String>,
    /// Aggregate on-disk size in bytes, when the probe succeeds.
    pub size_bytes: u64,
    /// Human-readable failure detail, when unhealthy.
    pub error: Option<String>,
}

impl RelationStore {
    /// Backend-neutral diagnostics snapshot for status endpoints.
    ///
    /// Never fails: probe errors are folded into the snapshot so handlers
    /// stay branch-free.
    pub async fn diagnose_summary(&self) -> RelationDiagnostics {
        match RelationStorage::db_size(self).await {
            Ok(size) => RelationDiagnostics {
                reachable: true,
                version: None,
                size_bytes: size,
                error: None,
            },
            Err(e) => RelationDiagnostics {
                reachable: false,
                version: None,
                size_bytes: 0,
                error: Some(format!("Diagnostic failed: {e}")),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fulltext_store_satisfies_fulltext_contract() {
        cce_storage_common::assert_fulltext_storage::<FulltextStore>();
        let database = cce_config::global::DatabaseConfig::default();
        let store =
            FulltextStore::from_database_config(&database).expect("local branch must build");
        assert_eq!(FulltextStorage::backend_name(&store), "local");
    }

    #[test]
    fn relation_store_satisfies_relation_contract() {
        cce_storage_common::assert_relation_storage::<RelationStore>();
        let root = Arc::new(SqliteClient::in_memory().expect("in-memory client"));
        let store = RelationStore::local(root);
        assert_eq!(RelationStorage::backend_name(&store), "local");
        assert!(RelationStorage::is_per_project_db(&store));
    }

    #[tokio::test]
    async fn fulltext_diagnose_folds_probe_failure() {
        let database = cce_config::global::DatabaseConfig::default();
        let store =
            FulltextStore::from_database_config(&database).expect("local branch must build");
        let diag = store.diagnose_summary().await;
        assert!(!diag.reachable);
        assert!(!diag.index_exists);
        assert_eq!(diag.documents_count, 0);
        assert!(diag.error.is_some());
    }

    #[tokio::test]
    async fn relation_diagnose_reports_local_size() {
        let root = Arc::new(SqliteClient::in_memory().expect("in-memory client"));
        let store = RelationStore::local(root);
        let diag = store.diagnose_summary().await;
        assert!(diag.reachable);
        assert!(diag.error.is_none());
    }

    #[test]
    fn fulltext_store_remote_branch_rejected_at_startup() {
        let mut database = cce_config::global::DatabaseConfig {
            fulltext_backend: cce_config::modules::FulltextBackend::Remote,
            ..cce_config::global::DatabaseConfig::default()
        };
        assert!(FulltextStore::from_database_config(&database).is_err());
        database.fulltext_remote.url = Some("http://localhost:9200".to_string());
        database.fulltext_remote.index_name = Some("code_index".to_string());
        let err = FulltextStore::from_database_config(&database)
            .expect_err("remote branch must fail at startup");
        assert!(
            err.to_string().contains("not supported yet"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn relation_store_local_branch_dispatch() {
        let root = Arc::new(SqliteClient::in_memory().expect("in-memory client"));
        let store = RelationStore::local(root);
        assert!(store.is_local());
        assert_eq!(store.backend_name(), "local");
        let scoped = store.for_project(1).expect("project scoping must work");
        assert!(scoped.is_local());
    }

    #[test]
    fn relation_store_remote_branch_rejected_at_startup() {
        let mut database = cce_config::global::DatabaseConfig {
            relation_backend: cce_config::modules::RelationBackend::Remote,
            ..cce_config::global::DatabaseConfig::default()
        };
        assert!(RelationStore::from_database_config(&database).is_err());
        database.relation_remote.url = Some("postgres://localhost:5432/cce".to_string());
        let err = RelationStore::from_database_config(&database)
            .expect_err("remote branch must fail at startup");
        assert!(
            err.to_string().contains("not supported yet"),
            "unexpected error: {err}"
        );
    }
}
