//! Storage inspection and removal of indexed content.

use cce_api::models::{
    BatchDeleteRequest, BatchDeleteResponse, ClearIndexRequest, ClearIndexResponse,
    DeleteEntityResponse, DeleteFileResponse, IndexStatsResponse, StorageQuery,
    StorageStatusResponse,
};
use cce_api::paths;

use crate::error::ClientError;
use crate::transport::{Client, INTERACTIVE_BUDGET, file_scoped, id_scoped};

impl Client {
    /// Report backend connectivity, item counts and disk usage.
    pub async fn storage_status(&self) -> Result<StorageStatusResponse, ClientError> {
        self.get_json(paths::STORAGE_STATUS.to_string(), INTERACTIVE_BUDGET)
            .await
    }

    /// Report the indexed statistics of one project.
    pub async fn index_stats(&self, project_id: i64) -> Result<IndexStatsResponse, ClientError> {
        self.get_json_query(
            paths::INDEX_STATS.to_string(),
            &StorageQuery { project_id },
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Drop selected storage components of a project.
    pub async fn clear_index(
        &self,
        request: &ClearIndexRequest,
    ) -> Result<ClearIndexResponse, ClientError> {
        self.delete_json_body(paths::INDEX.to_string(), request, self.long_task_budget())
            .await
    }

    /// Remove one file from every index.
    pub async fn delete_file(
        &self,
        project_id: i64,
        file_path: &str,
    ) -> Result<DeleteFileResponse, ClientError> {
        self.delete_json_with_query(
            file_scoped(paths::INDEX_FILE_TEMPLATE, file_path),
            &StorageQuery { project_id },
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Remove one entity from every index.
    pub async fn delete_entity(
        &self,
        project_id: i64,
        entity_id: u64,
    ) -> Result<DeleteEntityResponse, ClientError> {
        self.delete_json_with_query(
            id_scoped(
                paths::INDEX_ENTITY_TEMPLATE,
                project_id,
                &entity_id.to_string(),
            ),
            &StorageQuery { project_id },
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Remove several files and entities in one request.
    pub async fn batch_delete(
        &self,
        project_id: i64,
        request: &BatchDeleteRequest,
    ) -> Result<BatchDeleteResponse, ClientError> {
        self.delete_json_body_with_query(
            paths::INDEX_BATCH.to_string(),
            &StorageQuery { project_id },
            request,
            self.long_task_budget(),
        )
        .await
    }
}
