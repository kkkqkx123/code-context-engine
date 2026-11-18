//! Operations commands: service health, metrics, server configuration, the
//! retry queue and project dead letters.
//!
//! These panels observe the server rather than a project's content, so most
//! commands take no project scope; the dead-letter commands do, because dead
//! letters are recorded per project.

use cce_api::models::{
    AdmissionStatsResponse, Bm25HealthResponse, ConfigInfoResponse, ConfigReloadQuery,
    ConfigReloadResponse, ConfigValidateResponse, DeadLetterActionResponse, DeadLetterListResponse,
    DeadLetterRetryRequest, EmbeddingHealthResponse, HealthStatus, LlmTestRequest, LlmTestResponse,
    MetricsCleanupQuery, MetricsCleanupResponse, MetricsHistoryQuery, ProjectConfigGetResponse,
    ProjectConfigUpdateRequest, ProjectConfigUpdateResponse, ProjectLlmOverviewResponse,
    ProjectLlmUpdateRequest, ProjectLlmUpdateResponse, QdrantHealthResponse,
    RetryQueueDeadResponse, RetryQueueQuery, RetryQueueStatusResponse,
};
use cce_client::{Client, ClientError};
use gpui_kit::{App, Task};

use crate::runtime;
use crate::services::time;

pub fn health(cx: &App, client: Client) -> Task<Result<HealthStatus, ClientError>> {
    runtime::spawn(cx, async move { client.health().await })
}

pub fn health_qdrant(cx: &App, client: Client) -> Task<Result<QdrantHealthResponse, ClientError>> {
    runtime::spawn(cx, async move { client.health_qdrant().await })
}

pub fn health_embedding(
    cx: &App,
    client: Client,
) -> Task<Result<EmbeddingHealthResponse, ClientError>> {
    runtime::spawn(cx, async move { client.health_embedding().await })
}

pub fn health_bm25(cx: &App, client: Client) -> Task<Result<Bm25HealthResponse, ClientError>> {
    runtime::spawn(cx, async move { client.health_bm25().await })
}

pub fn admission_stats(
    cx: &App,
    client: Client,
) -> Task<Result<AdmissionStatsResponse, ClientError>> {
    runtime::spawn(cx, async move { client.admission_stats().await })
}

pub fn metrics_text(cx: &App, client: Client) -> Task<Result<String, ClientError>> {
    runtime::spawn(cx, async move { client.metrics_prometheus().await })
}

pub fn metrics_json(cx: &App, client: Client) -> Task<Result<serde_json::Value, ClientError>> {
    runtime::spawn(cx, async move { client.metrics_json().await })
}

/// One history query: a window plus the optional scope narrowing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryInput {
    /// Window width used when `from`/`to` are omitted.
    pub hours: u64,
    /// Explicit window start, RFC 3339; wins over `hours` together with `to`.
    pub from: Option<String>,
    pub to: Option<String>,
    pub metric: Option<String>,
    /// Restrict to one project, as the web console can.
    pub project_id: Option<i64>,
}

impl HistoryInput {
    /// Resolve the window: explicit bounds win, otherwise the last `hours`.
    fn window(&self) -> (String, String) {
        match (self.from.clone(), self.to.clone()) {
            (Some(from), Some(to)) => (from, to),
            _ => history_window(self.hours),
        }
    }
}

pub fn metrics_history(
    cx: &App,
    client: Client,
    input: HistoryInput,
) -> Task<Result<Vec<cce_api::models::AggregatedMetric>, ClientError>> {
    let (from, to) = input.window();
    runtime::spawn(cx, async move {
        let query = MetricsHistoryQuery {
            from,
            to,
            metric: input.metric,
            project_id: input.project_id,
            operation_type: None,
        };
        client.metrics_history(&query).await
    })
}

pub fn cleanup_metrics(
    cx: &App,
    client: Client,
    before: Option<String>,
) -> Task<Result<MetricsCleanupResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let query = MetricsCleanupQuery {
            all: before.is_none(),
            before,
        };
        client.cleanup_metrics(&query).await
    })
}

pub fn config_info(cx: &App, client: Client) -> Task<Result<ConfigInfoResponse, ClientError>> {
    runtime::spawn(cx, async move { client.config_info().await })
}

pub fn config_validate(
    cx: &App,
    client: Client,
) -> Task<Result<ConfigValidateResponse, ClientError>> {
    runtime::spawn(cx, async move { client.config_validate().await })
}

/// Reload server configuration for one project, or for every project when the
/// panel has no scope selected.
pub fn config_reload(
    cx: &App,
    client: Client,
    project_id: Option<i64>,
) -> Task<Result<ConfigReloadResponse, ClientError>> {
    runtime::spawn(cx, async move {
        client
            .config_reload(&ConfigReloadQuery { project_id })
            .await
    })
}

/// Read the stored project profile document.
pub fn project_config(
    cx: &App,
    client: Client,
    project_id: i64,
) -> Task<Result<ProjectConfigGetResponse, ClientError>> {
    runtime::spawn(cx, async move { client.project_config(project_id).await })
}

/// Replace the stored project profile document.
pub fn update_project_config(
    cx: &App,
    client: Client,
    project_id: i64,
    request: ProjectConfigUpdateRequest,
) -> Task<Result<ProjectConfigUpdateResponse, ClientError>> {
    runtime::spawn(cx, async move {
        client.update_project_config(project_id, &request).await
    })
}

/// Masked LLM settings of one project.
pub fn llm_overview(
    cx: &App,
    client: Client,
    project_id: i64,
) -> Task<Result<ProjectLlmOverviewResponse, ClientError>> {
    runtime::spawn(
        cx,
        async move { client.project_llm_overview(project_id).await },
    )
}

/// Patch the project-effective chat and rerank settings.
pub fn update_llm(
    cx: &App,
    client: Client,
    project_id: i64,
    request: ProjectLlmUpdateRequest,
) -> Task<Result<ProjectLlmUpdateResponse, ClientError>> {
    runtime::spawn(cx, async move {
        client.update_project_llm_config(project_id, &request).await
    })
}

/// Probe one configured model without persisting anything.
pub fn test_llm(
    cx: &App,
    client: Client,
    project_id: i64,
    request: LlmTestRequest,
) -> Task<Result<LlmTestResponse, ClientError>> {
    runtime::spawn(cx, async move {
        client.test_project_llm(project_id, &request).await
    })
}

pub fn retry_queue_status(
    cx: &App,
    client: Client,
    project_id: Option<i64>,
) -> Task<Result<RetryQueueStatusResponse, ClientError>> {
    runtime::spawn(cx, async move {
        client
            .retry_queue_status(&RetryQueueQuery { project_id })
            .await
    })
}

pub fn retry_queue_dead(
    cx: &App,
    client: Client,
    project_id: Option<i64>,
) -> Task<Result<RetryQueueDeadResponse, ClientError>> {
    runtime::spawn(cx, async move {
        client
            .retry_queue_dead(&RetryQueueQuery { project_id })
            .await
    })
}

/// Run one retry-queue processing pass.
pub fn retry_queue_process(
    cx: &App,
    client: Client,
    project_id: Option<i64>,
) -> Task<Result<cce_api::models::RetryQueueProcessResponse, ClientError>> {
    runtime::spawn(cx, async move {
        client
            .retry_queue_process(&RetryQueueQuery { project_id })
            .await
    })
}

/// Discard every queued retry.
pub fn retry_queue_clear(
    cx: &App,
    client: Client,
    project_id: Option<i64>,
) -> Task<Result<cce_api::models::RetryQueueClearResponse, ClientError>> {
    runtime::spawn(cx, async move {
        client
            .retry_queue_clear(&RetryQueueQuery { project_id })
            .await
    })
}

/// Discard the dead retry-queue entries.
pub fn retry_queue_dead_clear(
    cx: &App,
    client: Client,
    project_id: Option<i64>,
) -> Task<Result<cce_api::models::RetryQueueDeadClearResponse, ClientError>> {
    runtime::spawn(cx, async move {
        client
            .retry_queue_dead_clear(&RetryQueueQuery { project_id })
            .await
    })
}

pub fn dead_letters(
    cx: &App,
    client: Client,
    project_id: i64,
) -> Task<Result<DeadLetterListResponse, ClientError>> {
    runtime::spawn(cx, async move { client.dead_letters(project_id).await })
}

pub fn retry_dead_letters(
    cx: &App,
    client: Client,
    project_id: i64,
    files: Vec<String>,
) -> Task<Result<DeadLetterActionResponse, ClientError>> {
    runtime::spawn(cx, async move {
        client
            .retry_dead_letters(project_id, &DeadLetterRetryRequest { files })
            .await
    })
}

pub fn acknowledge_dead_letter(
    cx: &App,
    client: Client,
    project_id: i64,
    file_path: String,
) -> Task<Result<DeadLetterActionResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let request = cce_api::models::DeadLetterAcknowledgeRequest {
            file_path,
            module: None,
        };
        client.acknowledge_dead_letter(project_id, &request).await
    })
}

/// Time window for a metrics history query: the last `hours` hours up to now.
/// Acknowledge several dead-letter files in one pass.
///
/// The wire takes one file per call, so the batch is a sequential loop here
/// rather than a second endpoint. It stops at the first failure and reports
/// how many files were acknowledged before it, because a partial result the
/// caller cannot see is worse than a slow one.
pub fn acknowledge_dead_letters(
    cx: &App,
    client: Client,
    project_id: i64,
    files: Vec<String>,
) -> Task<Result<usize, ClientError>> {
    runtime::spawn(cx, async move {
        let mut done = 0;
        for file_path in files {
            let request = cce_api::models::DeadLetterAcknowledgeRequest {
                file_path,
                module: None,
            };
            client.acknowledge_dead_letter(project_id, &request).await?;
            done += 1;
        }
        Ok(done)
    })
}

pub fn history_window(hours: u64) -> (String, String) {
    let now = time::unix_seconds();
    let span = hours.saturating_mul(3600);
    (time::rfc3339(now.saturating_sub(span)), time::rfc3339(now))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_orders_from_before_to() {
        let (from, to) = history_window(2);
        assert!(from < to, "{from} should sort before {to}");
        assert!(to.ends_with('Z'));
        assert_eq!(from.len(), to.len());
    }
}
