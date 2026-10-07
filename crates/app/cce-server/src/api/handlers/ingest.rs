//! Remote ingest entries for gateway-driven indexing.
//!
//! The gateway pushes manifests and raw file payloads; the server stages
//! them under the registered project root and then runs the existing full
//! and incremental pipelines. Chunk staging lives under the system temp
//! directory, keyed by project and manifest version. These routes only
//! exist in admission-enabled builds and always sit behind the admission
//! layer.

pub mod admission;
pub mod batch;
pub mod commit;
pub mod event;
pub mod manifest;
pub mod paths;
pub mod staging;

use axum::Router;
use axum::routing::{get, post};

use crate::api::state::AppState;

pub use admission::{AdmissionStatsResponse, TokenAuditEntry, handle_admission_stats};
pub use batch::handle_ingest_batch;
pub use commit::handle_ingest_commit;
pub use event::handle_ingest_event;
pub use manifest::handle_ingest_manifest;
pub use staging::{INGEST_STAGING_RETENTION, sweep_expired_staging};

/// Routes served only when the admission feature is enabled.
pub fn ingest_routes(metrics: std::sync::Arc<cce_admission::AdmissionMetrics>) -> Router<AppState> {
    Router::new()
        .route(
            "/api/project/{id}/ingest/manifest",
            post(handle_ingest_manifest),
        )
        .route("/api/project/{id}/ingest/batch", post(handle_ingest_batch))
        .route(
            "/api/project/{id}/ingest/commit",
            post(handle_ingest_commit),
        )
        .route("/api/project/{id}/ingest/event", post(handle_ingest_event))
        .route("/api/admission/stats", get(handle_admission_stats))
        .layer(axum::Extension(metrics))
}
