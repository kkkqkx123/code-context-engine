//! Admission observability entry.

use axum::extract::{Extension, State};

use crate::api::state::AppState;

/// Per-token audit entry for persistent admission observability.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct TokenAuditEntry {
    /// Log-safe token fingerprint.
    pub fingerprint: String,
    /// Bound projects as comma separated ids.
    pub projects: String,
    /// Configured byte quota, if any.
    pub quota_bytes: Option<i64>,
    /// Stored bytes accepted.
    pub bytes_used: i64,
    /// Admitted batches.
    pub admitted: i64,
    /// Rejections by cause.
    pub auth_rejections: i64,
    /// Rejections by cause.
    pub scope_rejections: i64,
    /// Rejections by cause.
    pub rate_rejections: i64,
    /// Rejections by cause.
    pub body_rejections: i64,
    /// Rejections for exceeding the byte quota.
    pub quota_rejections: i64,
    /// Last use as seconds since the unix epoch, if any.
    pub last_used: Option<i64>,
    /// Last rejection reason, if any.
    pub last_reject_reason: Option<String>,
}

/// Admission counters with per-token persistent audit.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct AdmissionStatsResponse {
    /// Global in-memory counters.
    #[serde(flatten)]
    pub global: cce_admission::AdmissionStats,
    /// Per-token durable audit rows.
    #[serde(default)]
    pub tokens: Vec<TokenAuditEntry>,
}

/// Render admission counters with per-token durable audit for rejection
/// analysis. Usage and last-use time survive restarts through the metadata
/// store; in-memory counters cover the current process.
#[utoipa::path(
    get, path = "/api/admission/stats", tag = "Admission",
    responses(
        (status = 200, body = AdmissionStatsResponse, description = "Success")
    )
)]
pub async fn handle_admission_stats(
    State(state): State<AppState>,
    Extension(metrics): Extension<std::sync::Arc<cce_admission::AdmissionMetrics>>,
) -> axum::Json<AdmissionStatsResponse> {
    let global = metrics.snapshot();
    let mut tokens = Vec::new();
    if let Some(store) = state.engine.metadata_store()
        && let Ok(records) = store.as_ref().with_transaction(|tx| {
            cce_storage_metadb_sqlite::AdmissionAuditRepository::list_all(tx)
        })
    {
        for record in records {
            tokens.push(TokenAuditEntry {
                fingerprint: record.token_fingerprint,
                projects: record.projects,
                quota_bytes: record.quota_bytes,
                bytes_used: record.bytes_used,
                admitted: record.admitted,
                auth_rejections: record.auth_rejections,
                scope_rejections: record.scope_rejections,
                rate_rejections: record.rate_rejections,
                body_rejections: record.body_rejections,
                quota_rejections: record.quota_rejections,
                last_used: record.last_used,
                last_reject_reason: record.last_reject_reason,
            });
        }
    }
    axum::Json(AdmissionStatsResponse { global, tokens })
}
