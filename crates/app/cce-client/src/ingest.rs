//! Remote file supply: manifest negotiation, chunk upload and commit.

use cce_api::models::{
    IngestBatchRequest, IngestBatchResponse, IngestCommitResponse, IngestEventRequest,
    IngestEventResponse, IngestManifestRequest, IngestManifestResponse,
};
use cce_api::paths;

use crate::error::ClientError;
use crate::transport::{Client, INTERACTIVE_BUDGET};

impl Client {
    /// Announce the local snapshot and learn which files and chunks the server
    /// still needs.
    pub async fn ingest_manifest(
        &self,
        project_id: i64,
        request: &IngestManifestRequest,
    ) -> Result<IngestManifestResponse, ClientError> {
        self.post_json(
            paths::project_scoped(paths::INGEST_MANIFEST_TEMPLATE, project_id),
            request,
            self.long_task_budget(),
        )
        .await
    }

    /// Upload one batch of files.
    pub async fn ingest_batch(
        &self,
        project_id: i64,
        request: &IngestBatchRequest,
    ) -> Result<IngestBatchResponse, ClientError> {
        self.post_json(
            paths::project_scoped(paths::INGEST_BATCH_TEMPLATE, project_id),
            request,
            self.long_task_budget(),
        )
        .await
    }

    /// Run the index over every staged file.
    pub async fn ingest_commit(
        &self,
        project_id: i64,
    ) -> Result<IngestCommitResponse, ClientError> {
        self.post_json_empty(
            paths::project_scoped(paths::INGEST_COMMIT_TEMPLATE, project_id),
            self.long_task_budget(),
        )
        .await
    }

    /// Push incremental change events.
    pub async fn ingest_event(
        &self,
        project_id: i64,
        request: &IngestEventRequest,
    ) -> Result<IngestEventResponse, ClientError> {
        self.post_json(
            paths::project_scoped(paths::INGEST_EVENT_TEMPLATE, project_id),
            request,
            INTERACTIVE_BUDGET,
        )
        .await
    }
}
