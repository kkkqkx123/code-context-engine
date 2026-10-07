//! Remote vector backend implementing the shared contract.
//!
//! The existing process management, breaker, retry and filter translation
//! stay untouched; this only adapts `QdrantClient` to `VectorStorage` so the
//! assembly layer can dispatch by backend enum.

use cce_storage_common::{DenseSearchQuery, ScoredPoint, VectorPoint, VectorStorage};
use cce_types::{PointKind, StorageError};

use crate::client::QdrantClient;
use crate::retrieval::QdrantRetrieval;

#[async_trait::async_trait]
impl VectorStorage for QdrantClient {
    fn backend_name(&self) -> &'static str {
        "qdrant"
    }

    async fn ensure_collection(&self) -> Result<bool, StorageError> {
        self.initialize().await.map_err(StorageError::from)
    }

    async fn collection_exists(&self) -> Result<bool, StorageError> {
        self.collection_exists().await.map_err(StorageError::from)
    }

    async fn delete_collection(&self) -> Result<(), StorageError> {
        self.delete_collection().await.map_err(StorageError::from)
    }

    async fn clear_collection(&self) -> Result<(), StorageError> {
        self.clear_collection().await.map_err(StorageError::from)
    }

    async fn upsert_points(&self, points: &[VectorPoint]) -> Result<(), StorageError> {
        self.upsert_points(points).await.map_err(StorageError::from)
    }

    async fn search_dense(
        &self,
        query: DenseSearchQuery,
    ) -> Result<Vec<ScoredPoint>, StorageError> {
        let retrieval = QdrantRetrieval::new(
            self.http_client().clone(),
            self.base_url().to_string(),
            self.collection_name().to_string(),
        );
        retrieval.search_dense(query).await
    }

    async fn delete_by_file_path_scoped(
        &self,
        file_path: &str,
        group_id: &str,
        point_type: Option<PointKind>,
    ) -> Result<(), StorageError> {
        self.delete_by_file_path_scoped(file_path, group_id, point_type)
            .await
            .map_err(StorageError::from)
    }

    async fn delete_by_file_path_scoped_epoch(
        &self,
        file_path: &str,
        group_id: &str,
        epoch: i64,
    ) -> Result<(), StorageError> {
        self.delete_by_file_path_scoped_epoch(file_path, group_id, epoch)
            .await
            .map_err(StorageError::from)
    }

    async fn delete_by_group_epoch(&self, group_id: &str, epoch: i64) -> Result<(), StorageError> {
        self.delete_by_group_epoch(group_id, epoch)
            .await
            .map_err(StorageError::from)
    }

    async fn delete_by_group(&self, group_id: &str) -> Result<(), StorageError> {
        self.delete_by_group(group_id)
            .await
            .map_err(StorageError::from)
    }

    async fn delete_by_file_paths_scoped(
        &self,
        file_paths: &[&str],
        group_id: &str,
        point_type: Option<PointKind>,
    ) -> Result<(), StorageError> {
        self.delete_by_file_paths_scoped(file_paths, group_id, point_type)
            .await
            .map_err(StorageError::from)
    }

    async fn scroll_all_points(&self) -> Result<Vec<VectorPoint>, StorageError> {
        self.scroll_all_points().await.map_err(StorageError::from)
    }

    async fn count_points_by_group(&self, group_id: &str) -> Result<usize, StorageError> {
        self.count_points_by_group(group_id)
            .await
            .map_err(StorageError::from)
    }

    async fn count_all_points(&self) -> Result<usize, StorageError> {
        self.count_all_points().await.map_err(StorageError::from)
    }

    async fn health(&self) -> Result<bool, StorageError> {
        self.health().await.map_err(StorageError::from)
    }
}
