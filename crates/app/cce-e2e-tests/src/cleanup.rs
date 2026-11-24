//! Test context and resource cleanup for E2E tests

/// Initialize minimal test logging.
///
/// Quiet by default (WARN for third-party crates, INFO for cce crates) so
/// test output stays readable. Set `RUST_LOG` to opt into verbose logging,
/// e.g. `RUST_LOG=cce_storage_bm25=debug cargo test ...`.
pub fn init_minimal_logging() {
    use tracing_subscriber::{Layer, layer::SubscriberExt, util::SubscriberInitExt};

    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        "warn,cce_types=info,cce_parser=info,cce_storage_bm25=info,cce_storage_vector_qdrant=info,cce_storage_metadb_sqlite=info,cce_scanner=info,cce_llm_client=info,cce_orchestrator=info".into()
    });

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_level(true)
                .with_filter(filter),
        )
        .try_init()
        .ok();
}
