//! Operations commands: service health, metrics, server configuration, the
//! retry queue and project dead letters.
//!
//! These panels observe the server rather than a project's content, so most
//! commands take no project scope; the dead-letter commands do, because dead
//! letters are recorded per project.

use std::time::{SystemTime, UNIX_EPOCH};

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
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    let span = hours.saturating_mul(3600);
    (rfc3339(now.saturating_sub(span)), rfc3339(now))
}

/// RFC 3339 timestamp in UTC for a unix second count.
fn rfc3339(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Civil date from days since the unix epoch (proleptic Gregorian).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_formats_as_rfc3339() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(1_700_000_000), "2023-11-14T22:13:20Z");
    }

    #[test]
    fn leap_day_formats() {
        assert_eq!(rfc3339(1_709_164_800), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn window_orders_from_before_to() {
        let (from, to) = history_window(2);
        assert!(from < to, "{from} should sort before {to}");
        assert!(to.ends_with('Z'));
        assert_eq!(from.len(), to.len());
    }
}
