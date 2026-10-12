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
#[cfg(test)]
mod staging_tests;

use axum::Router;
use axum::routing::{get, post};

use crate::api::state::AppState;

pub use admission::handle_admission_stats;
pub use batch::handle_ingest_batch;
pub use commit::handle_ingest_commit;
pub use event::handle_ingest_event;
pub use manifest::handle_ingest_manifest;
pub use staging::{INGEST_STAGING_RETENTION, sweep_expired_staging};

/// Routes served only when the admission feature is enabled.
pub fn ingest_routes(metrics: std::sync::Arc<cce_admission::AdmissionMetrics>) -> Router<AppState> {
    Router::new()
        .route(
            cce_api::paths::INGEST_MANIFEST_TEMPLATE,
            post(handle_ingest_manifest),
        )
        .route(
            cce_api::paths::INGEST_BATCH_TEMPLATE,
            post(handle_ingest_batch),
        )
        .route(
            cce_api::paths::INGEST_COMMIT_TEMPLATE,
            post(handle_ingest_commit),
        )
        .route(
            cce_api::paths::INGEST_EVENT_TEMPLATE,
            post(handle_ingest_event),
        )
        .route(cce_api::paths::ADMISSION_STATS, get(handle_admission_stats))
        .layer(axum::Extension(metrics))
}
