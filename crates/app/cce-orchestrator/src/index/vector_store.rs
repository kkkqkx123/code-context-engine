//! Backend-enum vector storage dispatch.
//!
//! The assembly layer selects one backend per configuration and uses the same
//! instance for indexing and retrieval. No trait objects are used.

use std::sync::Arc;

use cce_storage_common::{DenseSearchQuery, ScoredPoint, VectorPoint, VectorStorage};
use cce_storage_local::LocalVectorStore;
use cce_storage_qdrant::QdrantClient;
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
