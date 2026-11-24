//! Rerank workflow tests
//!
//! Verifies that reranking actually executes once the config layer enables it
//! (handler available) and that the merged results carry `rerank_score`
//! metadata; also verifies the per-request override can force reranking off.

use std::sync::Arc;

use cce_e2e_tests::mock_chat_server::MockChatServer;
use cce_llm_client::{
    GenerativeRerankRequestHandler, ProductionRerankHandler, build_generative_rerank_provider,
};
use cce_orchestrator::{SearchConfig, SearchSources};

use crate::helper::{EmptyFixture, QueryWorkflowTest, init_minimal_logging, mock_embedding};

/// Build a generative rerank handler backed by the mock chat server.
///
/// The provider is assembled through the current factory from an `AppConfig`
/// pointing at the mock endpoint, so the test exercises the production
/// wiring (config resolution → llm-suite provider → request handler) with
/// no external LLM service.
fn rerank_handler(base_url: &str) -> Arc<ProductionRerankHandler> {
    use cce_config::modules::{ProviderConfig, RerankModelConfig};

    let mut config = cce_config::AppConfig::default();
    config.llm.providers.insert(
        "mock-provider".to_string(),
        ProviderConfig {
            id: "mock-provider".to_string(),
            name: "Mock".to_string(),
            base_url: base_url.to_string(),
            api_keys: vec!["test-key".to_string()],
            ..ProviderConfig::default()
        },
    );
    config.llm.rerank_models.insert(
        "mock-rerank".to_string(),
        RerankModelConfig {
            provider_id: "mock-provider".to_string(),
            model: "mock-rerank-model".to_string(),
            ..Default::default()
        },
    );
    let provider = Arc::new(
        build_generative_rerank_provider(&config, "mock-rerank")
            .expect("rerank provider must build"),
    );
    Arc::new(ProductionRerankHandler::Generative(Arc::new(
        GenerativeRerankRequestHandler::new(provider),
    )))
}

/// Config that mirrors `[rerank] enabled = true` plus permissive candidate
/// selection (BM25 scores do not reach the default 0.3 min score).
fn rerank_enabled_config() -> SearchConfig {
    let mut config = SearchConfig::default();
    config.rerank.enabled = true;
    config.rerank.min_score = 0.0;
    config.rerank.score_drop_threshold = 1e9;
    config.rerank.min_candidates = 1;
    config.rerank.return_reasoning = true;
    config
}

fn fixture_with_functions() -> EmptyFixture {
    let fixture = EmptyFixture::new().expect("Failed to create fixture");
    fixture
        .add_file(
            "src/lib.rs",
            r#"
/// Process user data
pub fn process_user(name: String) -> String {
    name
}

/// Validate user input
pub fn validate_user(name: &str) -> bool {
    !name.is_empty()
}

/// Persist the user record
pub fn persist_user(name: &str) -> String {
    format!("saved {}", name)
}
"#,
        )
        .expect("Failed to add file");
    fixture
}

/// With the config layer enabling rerank (handler wired), a
/// search response must carry `rerank_score` metadata.
#[tokio::test]
async fn test_rerank_runs_when_config_enabled() {
    init_minimal_logging();

    let mock_chat = MockChatServer::start_rerank(true).await;
    let handler = rerank_handler(&mock_chat.base_url);

    let mut query_test = QueryWorkflowTest::new(
        fixture_with_functions().into_test_fixture(),
        mock_embedding(),
    )
    .with_sources(SearchSources::none().with_bm25())
    .with_config(rerank_enabled_config())
    .with_rerank_handler(handler);

    let index_result = query_test.index().await.expect("Index failed");
    assert!(
        index_result.total_entities >= 3,
        "Expected at least 3 entities"
    );

    let query_result = query_test
        .search_bm25("process user", 10)
        .await
        .expect("BM25 query failed");

    assert!(query_result.total > 0, "Expected at least one result");
    let with_rerank_score = query_result
        .items
        .iter()
        .filter(|r| r.metadata.contains_key("rerank_score"))
        .count();
    assert!(
        with_rerank_score > 0,
        "rerank must write rerank_score metadata when config enables it; items: {:?}",
        query_result
            .items
            .iter()
            .map(|r| (&r.id, r.metadata.keys().collect::<Vec<_>>()))
            .collect::<Vec<_>>()
    );

    let with_reasoning = query_result
        .items
        .iter()
        .filter(|r| r.metadata.contains_key("rerank_reasoning"))
        .count();
    assert_eq!(
        with_reasoning, with_rerank_score,
        "return_reasoning must surface rerank_reasoning metadata"
    );

    mock_chat.stop().await;
}

/// A per-request `enable_rerank = false` override must skip
/// reranking even when the handler is available.
#[tokio::test]
async fn test_rerank_request_override_disables() {
    init_minimal_logging();

    let mock_chat = MockChatServer::start_rerank(true).await;
    let handler = rerank_handler(&mock_chat.base_url);

    let mut query_test = QueryWorkflowTest::new(
        fixture_with_functions().into_test_fixture(),
        mock_embedding(),
    )
    .with_sources(SearchSources::none().with_bm25())
    .with_config(rerank_enabled_config())
    .with_rerank_handler(handler)
    .with_enable_rerank(false);

    query_test.index().await.expect("Index failed");

    let query_result = query_test
        .search_bm25("process user", 10)
        .await
        .expect("BM25 query failed");

    assert!(query_result.total > 0, "Expected at least one result");
    assert!(
        query_result
            .items
            .iter()
            .all(|r| !r.metadata.contains_key("rerank_score")),
        "request-level disable must suppress rerank_score metadata"
    );

    mock_chat.stop().await;
}
