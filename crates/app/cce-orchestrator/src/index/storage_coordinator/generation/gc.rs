//! Retention-based garbage collection of stale generations.
//!
//! SQLite manifests are the source of truth for retention; external stores
//! are cleaned before the durable manifest rows are removed so a backend
//! failure leaves the plan available for a later retry.

use std::collections::HashSet;

use cce_storage_sqlite::ProjectIndexManifestRepository;

use crate::error::OrchestratorError;

use super::StorageCoordinator;

impl StorageCoordinator {
    /// Remove stale data and relation generations after a successful or
    /// failed publication.
    ///
    /// SQLite manifests are the source of truth for retention. External
    /// stores are cleaned before the SQLite rows are removed, so a backend
    /// failure leaves the durable manifest available for a later retry.
    pub async fn gc_generations(
        &self,
        keep_active_generations: usize,
        stale_before: i64,
    ) -> Result<(), OrchestratorError> {
        let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) else {
            return Ok(());
        };
        let plan = {
            let conn = client
                .read_connection()
                .map_err(OrchestratorError::Storage)?;
            ProjectIndexManifestRepository::generation_gc_plan(
                &conn,
                self.project_id,
                keep_active_generations,
                stale_before,
            )
            .map_err(OrchestratorError::Storage)?
        };

        if let Some(qdrant) = &self.qdrant {
            self.ensure_project_group_id()?;
            let epochs: HashSet<i64> = qdrant
                .scroll_all_points()
                .await?
                .into_iter()
                .filter(|point| {
                    point.payload.group_id.as_deref() == Some(self.project_group_id.as_str())
                })
                .filter_map(|point| point.payload.epoch)
                .filter(|epoch| !plan.protected_data_epochs.contains(epoch))
                .collect();
            for epoch in epochs {
                qdrant
                    .delete_by_group_epoch(&self.project_group_id, epoch)
                    .await?;
            }
        }

        if let Some(bm25) = &self.bm25 {
            let mut client = bm25.lock().await;
            let epochs = client.epochs_by_project(self.project_id).await?;
            for epoch in epochs {
                if !plan.protected_data_epochs.contains(&epoch) {
                    client
                        .delete_by_project_epoch("default", self.project_id, epoch)
                        .await?;
                }
            }
        }

        client
            .with_transaction(|tx| {
                ProjectIndexManifestRepository::apply_generation_gc(tx, self.project_id, &plan)
            })
            .map_err(OrchestratorError::Storage)
    }

    /// Run generation GC with the default retention policy.
    pub async fn gc_stale_generations(&self) -> Result<(), OrchestratorError> {
        const KEEP_ACTIVE_GENERATIONS: usize = 2;
        const STALE_AFTER_SECS: i64 = 60 * 60;
        self.gc_generations(
            KEEP_ACTIVE_GENERATIONS,
            chrono::Utc::now().timestamp() - STALE_AFTER_SECS,
        )
        .await
    }
}
