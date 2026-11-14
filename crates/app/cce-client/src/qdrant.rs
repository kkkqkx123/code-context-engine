//! Vector backend process lifecycle.

use cce_api::models::{QdrantActionResponse, QdrantProcessStatusResponse};
use cce_api::paths;

use crate::error::ClientError;
use crate::transport::{Client, INTERACTIVE_BUDGET};

impl Client {
    /// Current lifecycle state of the managed backend process.
    pub async fn qdrant_process_status(&self) -> Result<QdrantProcessStatusResponse, ClientError> {
        self.get_json(paths::QDRANT_PROCESS_STATUS.to_string(), INTERACTIVE_BUDGET)
            .await
    }

    /// Start the managed backend process.
    pub async fn qdrant_process_start(&self) -> Result<QdrantActionResponse, ClientError> {
        self.post_json_empty(
            paths::QDRANT_PROCESS_START.to_string(),
            self.long_task_budget(),
        )
        .await
    }

    /// Stop the managed backend process.
    pub async fn qdrant_process_stop(&self) -> Result<QdrantActionResponse, ClientError> {
        self.post_json_empty(
            paths::QDRANT_PROCESS_STOP.to_string(),
            self.long_task_budget(),
        )
        .await
    }

    /// Restart the managed backend process.
    pub async fn qdrant_process_restart(&self) -> Result<QdrantActionResponse, ClientError> {
        self.post_json_empty(
            paths::QDRANT_PROCESS_RESTART.to_string(),
            self.long_task_budget(),
        )
        .await
    }
}
