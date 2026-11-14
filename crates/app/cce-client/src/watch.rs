//! Server-side directory watch.

use cce_api::models::{
    StartWatchRequest, StartWatchResponse, StopWatchResponse, WatchStatusResponse,
};
use cce_api::paths;

use crate::error::ClientError;
use crate::transport::{Client, INTERACTIVE_BUDGET};

impl Client {
    /// Start watching a directory the server can read.
    pub async fn watch_start(
        &self,
        project_id: i64,
        request: &StartWatchRequest,
    ) -> Result<StartWatchResponse, ClientError> {
        self.post_json(
            paths::project_scoped(paths::WATCH_START_TEMPLATE, project_id),
            request,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Stop the project's directory watch.
    pub async fn watch_stop(&self, project_id: i64) -> Result<StopWatchResponse, ClientError> {
        self.post_json_empty(
            paths::project_scoped(paths::WATCH_STOP_TEMPLATE, project_id),
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Current watch state of the project.
    pub async fn watch_status(&self, project_id: i64) -> Result<WatchStatusResponse, ClientError> {
        self.get_json(
            paths::project_scoped(paths::WATCH_STATUS_TEMPLATE, project_id),
            INTERACTIVE_BUDGET,
        )
        .await
    }
}
