//! Dead-letter list, retry and acknowledgement.

use cce_api::models::{
    DeadLetterAcknowledgeRequest, DeadLetterActionResponse, DeadLetterListResponse,
    DeadLetterRetryRequest,
};
use cce_api::paths;

use crate::error::ClientError;
use crate::transport::{Client, INTERACTIVE_BUDGET};

impl Client {
    /// Files that currently sit in the dead-letter set.
    pub async fn dead_letters(
        &self,
        project_id: i64,
    ) -> Result<DeadLetterListResponse, ClientError> {
        self.get_json(
            paths::project_scoped(paths::PROJECT_DEAD_LETTERS_TEMPLATE, project_id),
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Re-run indexing over dead-letter files, or over all of them when the
    /// request carries no file list.
    pub async fn retry_dead_letters(
        &self,
        project_id: i64,
        request: &DeadLetterRetryRequest,
    ) -> Result<DeadLetterActionResponse, ClientError> {
        self.post_json(
            paths::project_scoped(paths::PROJECT_DEAD_LETTERS_RETRY_TEMPLATE, project_id),
            request,
            self.long_task_budget(),
        )
        .await
    }

    /// Take one dead-letter file out of the retry flow.
    pub async fn acknowledge_dead_letter(
        &self,
        project_id: i64,
        request: &DeadLetterAcknowledgeRequest,
    ) -> Result<DeadLetterActionResponse, ClientError> {
        self.post_json(
            paths::project_scoped(paths::PROJECT_DEAD_LETTERS_ACK_TEMPLATE, project_id),
            request,
            INTERACTIVE_BUDGET,
        )
        .await
    }
}
