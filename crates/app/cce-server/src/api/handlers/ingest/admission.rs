//! Admission observability entry.

use axum::extract::{Extension, State};

use crate::api::state::AppState;
use cce_api::models::{AdmissionGlobalStats, AdmissionStatsResponse, TokenAuditEntry};

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
    let snapshot = metrics.snapshot();
    let global = AdmissionGlobalStats {
        admitted: snapshot.admitted,
        auth_rejections: snapshot.auth_rejections,
        scope_rejections: snapshot.scope_rejections,
        rate_rejections: snapshot.rate_rejections,
        body_rejections: snapshot.body_rejections,
        quota_rejections: snapshot.quota_rejections,
    };
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
