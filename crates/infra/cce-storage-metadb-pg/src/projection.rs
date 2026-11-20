//! Durable file-state projection operations for the PostgreSQL branch.

use cce_storage_common::metadb::IndexStateProjection;
use cce_types::StorageError;

use crate::error::classify_pg;
use crate::rows::projection_from_row;

use super::PostgresClient;

impl PostgresClient {
    pub(crate) async fn projection_load(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Vec<IndexStateProjection>, StorageError> {
        let operation_id = operation_id.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let rows = client
                .query(
                    "SELECT project_id, operation_id, file_path, version, state_json, updated_at \
                     FROM index_state_projection \
                     WHERE project_id = $1 AND operation_id = $2 ORDER BY file_path",
                    &[&project_id, &operation_id],
                )
                .await
                .map_err(classify_pg)?;
            rows.iter().map(projection_from_row).collect()
        })
        .await
    }

    pub(crate) async fn projection_upsert(
        &self,
        row: &IndexStateProjection,
    ) -> Result<(), StorageError> {
        let row = row.clone();
        self.run(async move {
            let client = self.pooled().await?;
            upsert_projection(&client, &row).await
        })
        .await
    }

    pub(crate) async fn projection_upsert_batch(
        &self,
        rows: &[IndexStateProjection],
    ) -> Result<(), StorageError> {
        let rows = rows.to_vec();
        if rows.is_empty() {
            return Ok(());
        }
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            for row in &rows {
                upsert_projection(&tx, row).await?;
            }
            tx.commit().await.map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn projection_delete(
        &self,
        project_id: i64,
        operation_id: &str,
        file_path: &str,
    ) -> Result<usize, StorageError> {
        let operation_id = operation_id.to_string();
        let file_path = file_path.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let changed = client
                .execute(
                    "DELETE FROM index_state_projection \
                     WHERE project_id = $1 AND operation_id = $2 AND file_path = $3",
                    &[&project_id, &operation_id, &file_path],
                )
                .await
                .map_err(classify_pg)?;
            Ok(changed as usize)
        })
        .await
    }

    pub(crate) async fn projection_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        let operation_id = operation_id.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let changed = client
                .execute(
                    "DELETE FROM index_state_projection \
                     WHERE project_id = $1 AND operation_id = $2",
                    &[&project_id, &operation_id],
                )
                .await
                .map_err(classify_pg)?;
            Ok(changed as usize)
        })
        .await
    }

    pub(crate) async fn projection_delete_by_project(
        &self,
        project_id: i64,
    ) -> Result<usize, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let changed = client
                .execute(
                    "DELETE FROM index_state_projection WHERE project_id = $1",
                    &[&project_id],
                )
                .await
                .map_err(classify_pg)?;
            Ok(changed as usize)
        })
        .await
    }
}

/// Upsert a projection row without letting an older version overwrite a newer one.
async fn upsert_projection(
    executor: &impl deadpool_postgres::GenericClient,
    row: &IndexStateProjection,
) -> Result<(), StorageError> {
    executor
        .execute(
            "INSERT INTO index_state_projection \
             (project_id, operation_id, file_path, version, state_json, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (project_id, operation_id, file_path) DO UPDATE SET \
                version = EXCLUDED.version, state_json = EXCLUDED.state_json, \
                updated_at = EXCLUDED.updated_at \
             WHERE EXCLUDED.version >= index_state_projection.version",
            &[
                &row.project_id,
                &row.operation_id,
                &row.file_path,
                &row.version,
                &row.state_json,
                &row.updated_at,
            ],
        )
        .await
        .map_err(classify_pg)?;
    Ok(())
}
