//! Backend-enum vector storage dispatch.
//!
//! The assembly layer selects one backend per configuration and uses the same
//! instance for indexing and retrieval. No trait objects are used.

use std::sync::Arc;

use cce_storage_bm25::Bm25Client;
use cce_storage_common::{DenseSearchQuery, ScoredPoint, VectorPoint, VectorStorage};
use cce_storage_local::LocalVectorStore;
use cce_storage_qdrant::QdrantClient;
use cce_storage_sqlite::SqliteClient;
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
    pub fn as_qdrant(&self) -> Option<&Arc<QdrantClient>> {
        match self {
            Self::Qdrant(client) => Some(client),
            _ => None,
        }
    }

    /// Borrow the local store when this is the embedded branch.
    pub fn as_local(&self) -> Option<&Arc<LocalVectorStore>> {
        match self {
            Self::Local(store) => Some(store),
            _ => None,
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
/// Local is the only phase-2 branch (embedded Tantivy). The enum mirrors
/// [`VectorStore`] so assembly selects branches from configuration instead
/// of threading concrete client types through every caller.
#[derive(Clone)]
pub enum FulltextStore {
    /// Embedded Tantivy index.
    Local(Arc<tokio::sync::Mutex<Bm25Client>>),
}

impl FulltextStore {
    /// Wrap a local BM25 client.
    pub fn local(client: Arc<tokio::sync::Mutex<Bm25Client>>) -> Self {
        Self::Local(client)
    }

    /// Whether this is the embedded backend.
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }

    /// Backend name for logging (`local`).
    pub fn backend_name(&self) -> &'static str {
        match self {
            Self::Local(_) => "local",
        }
    }

    /// Borrow the local client.
    pub fn as_local(&self) -> &Arc<tokio::sync::Mutex<Bm25Client>> {
        match self {
            Self::Local(client) => client,
        }
    }

    /// Unwrap into the concrete client handle.
    pub fn into_local(self) -> Arc<tokio::sync::Mutex<Bm25Client>> {
        match self {
            Self::Local(client) => client,
        }
    }

    /// Build the store from the resolved database configuration.
    ///
    /// Phase 2 only supports the local branch; a remote selection is
    /// rejected here (structural validation already rejects it earlier).
    pub fn from_database_config(
        database: &cce_config::global::DatabaseConfig,
    ) -> Result<Self, StorageError> {
        match database.fulltext_backend {
            cce_config::modules::FulltextBackend::Local => {
                let client = Bm25Client::new(database.bm25.clone());
                Ok(Self::Local(Arc::new(tokio::sync::Mutex::new(client))))
            }
            cce_config::modules::FulltextBackend::Remote => Err(StorageError::query(
                "remote fulltext backend is reserved and not enabled in this phase",
            )),
        }
    }
}

impl std::fmt::Debug for FulltextStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local(_) => write!(f, "FulltextStore::Local(..)"),
        }
    }
}

/// Relation backend holding one concrete implementation.
///
/// Local is the only phase-2 branch (embedded SQLite with per-project
/// database files). Path rules, cache eviction, capacity stats, and the
/// project-delete two-step semantics stay inside the local client; this
/// enum only fixes the dispatch shape for the future remote branch.
#[derive(Clone)]
pub enum RelationStore {
    /// Embedded SQLite repositories.
    Local(Arc<SqliteClient>),
}

impl RelationStore {
    /// Wrap a local SQLite client.
    pub fn local(client: Arc<SqliteClient>) -> Self {
        Self::Local(client)
    }

    /// Whether this is the embedded backend.
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }

    /// Backend name for logging (`local`).
    pub fn backend_name(&self) -> &'static str {
        match self {
            Self::Local(_) => "local",
        }
    }

    /// Borrow the local client.
    pub fn as_local(&self) -> &Arc<SqliteClient> {
        match self {
            Self::Local(client) => client,
        }
    }

    /// Unwrap into the concrete client handle.
    pub fn into_local(self) -> Arc<SqliteClient> {
        match self {
            Self::Local(client) => client,
        }
    }

    /// Open the per-project scoped handle, preserving the local branch
    /// per-project-database semantics.
    pub fn for_project(&self, project_id: i64) -> Result<Self, StorageError> {
        match self {
            Self::Local(client) => Ok(Self::Local(client.for_project(project_id)?)),
        }
    }

    /// Build the store from the resolved database configuration.
    ///
    /// Phase 2 only supports the local branch; a remote selection is
    /// rejected here (structural validation already rejects it earlier).
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
            cce_config::modules::RelationBackend::Remote => Err(StorageError::query(
                "remote relation backend is reserved and not enabled in this phase",
            )),
        }
    }
}

impl std::fmt::Debug for RelationStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local(_) => write!(f, "RelationStore::Local(..)"),
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
    fn fulltext_store_rejects_remote_branch() {
        let database = cce_config::global::DatabaseConfig {
            fulltext_backend: cce_config::modules::FulltextBackend::Remote,
            ..cce_config::global::DatabaseConfig::default()
        };
        assert!(FulltextStore::from_database_config(&database).is_err());
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
    fn relation_store_rejects_remote_branch() {
        let database = cce_config::global::DatabaseConfig {
            relation_backend: cce_config::modules::RelationBackend::Remote,
            ..cce_config::global::DatabaseConfig::default()
        };
        assert!(RelationStore::from_database_config(&database).is_err());
    }
}
