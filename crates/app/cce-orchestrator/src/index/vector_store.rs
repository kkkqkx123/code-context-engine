//! Backend-enum vector storage dispatch.
//!
//! The assembly layer selects one backend per configuration and uses the same
//! instance for indexing and retrieval. No trait objects are used.

use std::sync::Arc;

use cce_storage_bm25::{Bm25Client, ElasticsearchClient};
use cce_storage_common::{DenseSearchQuery, ScoredPoint, VectorPoint, VectorStorage};
use cce_storage_relation_pg::PostgresClient;
use cce_storage_relation_sqlite::SqliteClient;
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

#[async_trait::async_trait]
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
/// Local is the embedded Tantivy branch; Remote is the Elasticsearch
/// branch. The enum mirrors [`VectorStore`] so assembly selects branches
/// from configuration instead of threading concrete client types through
/// every caller.
#[derive(Clone)]
pub enum FulltextStore {
    /// Embedded Tantivy index.
    Local(Arc<tokio::sync::Mutex<Bm25Client>>),
    /// External Elasticsearch service.
    Remote(Arc<ElasticsearchClient>),
}

impl FulltextStore {
    /// Wrap a local BM25 client.
    pub fn local(client: Arc<tokio::sync::Mutex<Bm25Client>>) -> Self {
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
    pub fn as_local(&self) -> Option<&Arc<tokio::sync::Mutex<Bm25Client>>> {
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
    pub fn into_local(self) -> Option<Arc<tokio::sync::Mutex<Bm25Client>>> {
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
    pub fn from_database_config(
        database: &cce_config::global::DatabaseConfig,
    ) -> Result<Self, StorageError> {
        match database.fulltext_backend {
            cce_config::modules::FulltextBackend::Local => {
                let client = Bm25Client::new(database.bm25.clone());
                Ok(Self::Local(Arc::new(tokio::sync::Mutex::new(client))))
            }
            cce_config::modules::FulltextBackend::Remote => {
                let client = ElasticsearchClient::from_database_config(database).map_err(|e| {
                    StorageError::query(format!("failed to create remote fulltext client: {e}"))
                })?;
                Ok(Self::Remote(Arc::new(client)))
            }
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

/// Relation backend holding one concrete implementation.
///
/// Local is the embedded SQLite branch (per-project database files);
/// Remote is the PostgreSQL branch (per-row project filtering). Path
/// rules, cache eviction, capacity stats, and the project-delete
/// two-step semantics stay inside the local client.
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
            cce_config::modules::RelationBackend::Remote => {
                let client = PostgresClient::from_database_config(database)?;
                Ok(Self::Remote(Arc::new(client)))
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fulltext_store_local_branch_dispatch() {
        let database = cce_config::global::DatabaseConfig::default();
        let store =
            FulltextStore::from_database_config(&database).expect("local branch must build");
        assert!(store.is_local());
        assert_eq!(store.backend_name(), "local");
    }

    #[test]
    fn fulltext_store_remote_branch_dispatch() {
        let mut database = cce_config::global::DatabaseConfig {
            fulltext_backend: cce_config::modules::FulltextBackend::Remote,
            ..cce_config::global::DatabaseConfig::default()
        };
        assert!(FulltextStore::from_database_config(&database).is_err());
        database.fulltext_remote.url = Some("http://localhost:9200".to_string());
        let store =
            FulltextStore::from_database_config(&database).expect("remote branch must build");
        assert!(store.is_remote());
        assert_eq!(store.backend_name(), "remote");
        assert!(store.as_remote().is_some());
        assert!(store.as_local().is_none());
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
    fn relation_store_remote_branch_dispatch() {
        let mut database = cce_config::global::DatabaseConfig {
            relation_backend: cce_config::modules::RelationBackend::Remote,
            ..cce_config::global::DatabaseConfig::default()
        };
        assert!(RelationStore::from_database_config(&database).is_err());
        database.relation_remote.url = Some("postgres://localhost:5432/cce".to_string());
        let store =
            RelationStore::from_database_config(&database).expect("remote branch must build");
        assert!(store.is_remote());
        assert_eq!(store.backend_name(), "remote");
        assert!(store.as_remote().is_some());
        assert!(store.as_local().is_none());
        let scoped = store.for_project(1).expect("remote scoping clones");
        assert!(scoped.is_remote());
    }
}
