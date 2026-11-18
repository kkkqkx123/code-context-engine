//! Operations commands: service health, metrics, server configuration, the
//! retry queue and project dead letters.
//!
//! These panels observe the server rather than a project's content, so most
//! commands take no project scope; the dead-letter commands do, because dead
//! letters are recorded per project.

use cce_api::models::{
    AdmissionStatsResponse, Bm25HealthResponse, ConfigInfoResponse, ConfigReloadQuery,
    ConfigReloadResponse, ConfigValidateResponse, DeadLetterActionResponse, DeadLetterListResponse,
    DeadLetterRetryRequest, EmbeddingHealthResponse, HealthStatus, MetricsCleanupQuery,
    MetricsCleanupResponse, MetricsHistoryQuery, QdrantHealthResponse, RetryQueueDeadResponse,
    RetryQueueQuery, RetryQueueStatusResponse,
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

pub fn metrics_history(
    cx: &App,
    client: Client,
    hours: u64,
    metric: Option<String>,
) -> Task<Result<Vec<cce_api::models::AggregatedMetric>, ClientError>> {
    let (from, to) = history_window(hours);
    runtime::spawn(cx, async move {
        let query = MetricsHistoryQuery {
            from,
            to,
            metric,
            project_id: None,
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
