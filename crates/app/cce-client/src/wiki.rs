//! Wiki context export.

use cce_api::models::{WikiContextQuery, WikiContextResponse, WikiDiffRequest, WikiDiffResponse};
use cce_api::paths;

use crate::error::ClientError;
use crate::transport::{Client, INTERACTIVE_BUDGET};

impl Client {
    /// Full Wiki Context Package export of a project.
    pub async fn wiki_context(
        &self,
        project_id: i64,
        query: &WikiContextQuery,
    ) -> Result<WikiContextResponse, ClientError> {
        self.get_json_query(
            paths::project_scoped(paths::WIKI_CONTEXT_TEMPLATE, project_id),
            query,
            self.long_task_budget(),
        )
        .await
    }

    /// Incremental invalidation diff against baseline hashes.
    pub async fn wiki_diff(
        &self,
        project_id: i64,
        request: &WikiDiffRequest,
    ) -> Result<WikiDiffResponse, ClientError> {
        self.post_json(
            paths::project_scoped(paths::WIKI_DIFF_TEMPLATE, project_id),
            request,
            INTERACTIVE_BUDGET,
        )
        .await
    }
}
