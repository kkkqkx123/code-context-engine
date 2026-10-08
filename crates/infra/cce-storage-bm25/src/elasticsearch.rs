//! Elasticsearch remote branch for fulltext storage.
//!
//! Implements the same [`FulltextStorage`] contract as the embedded Tantivy
//! branch over the Elasticsearch HTTP API with the workspace `reqwest`
//! client (no heavyweight vendor SDK, mirroring the Qdrant branch style).
//!
//! # Mapping
//!
//! One index carries every project; `project_id` is a keyword filter field.
//! Identifier, block, file, and project fields are keywords; the title is
//! analyzed text with readback; the body and keyword fields are indexed but
//! excluded from `_source` so they never participate in readback (the local
//! "indexed but not stored" equivalent).
//!
//! # Tokenization alignment
//!
//! The local mixed tokenizer (identifier splitting plus whole and subword
//! tokens at one position, CJK dictionary segmentation) has no server-side
//! equivalent without analyzer plugins, which the offline deployment cannot
//! rely on. Every analyzed field is therefore pre-tokenized on the client
//! with the shared segmentation core: the token stream is joined into a
//! `*_tokens` sidecar field indexed with the `whitespace` analyzer (chosen
//! deliberately so the server never re-splits tokens like `get_or_init`),
//! and the query is tokenized the same way before it is sent. Term-level
//! parity holds; phrase queries degrade to token conjunctions (documented
//! approximation, covered by the recall backoff-range acceptance). The
//! term operator governs combination within each field, mirroring the local
//! branch, while fields stay disjunctive.
//!
//! # Write visibility
//!
//! Bulk writes do not force a refresh. Call [`ElasticsearchClient::flush`]
//! (index refresh) after the last batch before a generation is marked ready
//! or activated so counts and snapshot readbacks observe every document.
//!
//! # Layout
//!
//! The remote branch is split by responsibility: [`config`] holds the
//! connection configuration, [`index`] the index mapping and lifecycle,
//! [`source`] document serialization, [`write`] the batch write path,
//! [`query`] the retrieval DSL, [`delete`] the delete-by-query operations,
//! [`snapshot`] the generation readback, and [`stats`] count/aggregation
//! reads. This root module owns the client type, its construction, and the
//! shared transport concerns (auth, circuit breaker, status classification).

mod config;
mod delete;
mod index;
mod query;
mod snapshot;
mod source;
mod stats;
mod write;

pub use config::ElasticsearchConfig;

use std::sync::Arc;
use std::time::Duration;

use cce_circuit_breaker::CircuitBreaker;
use cce_text::MixedTokenizer;
use cce_types::error::common::ErrorClassify;
use reqwest::Client;
use tokio::sync::Mutex;
use tracing::warn;

use crate::Bm25Error;
use crate::metrics::Bm25Metrics;

/// Circuit breaker failure threshold and reset timeout, mirroring Qdrant.
const BREAKER_THRESHOLD: u32 = 3;
const BREAKER_TIMEOUT: Duration = Duration::from_secs(30);

/// Elasticsearch-backed fulltext client (remote branch).
#[derive(Clone)]
pub struct ElasticsearchClient {
    config: ElasticsearchConfig,
    http: Client,
    tokenizer: MixedTokenizer,
    breaker: Arc<Mutex<CircuitBreaker>>,
    metrics: Option<Arc<dyn Bm25Metrics>>,
}

impl ElasticsearchClient {
    /// Create a client from an explicit configuration (no I/O performed).
    pub fn new(config: ElasticsearchConfig) -> Result<Self, Bm25Error> {
        let mut builder = Client::builder()
            .timeout(config.request_timeout)
            .user_agent("CodeContextEngine")
            .pool_max_idle_per_host(10);
        if let Some(ref api_key) = config.api_key {
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(
                "ApiKey",
                reqwest::header::HeaderValue::from_str(api_key)
                    .map_err(|e| Bm25Error::config(format!("invalid fulltext api key: {e}")))?,
            );
            builder = builder.default_headers(headers);
        }
        let http = builder
            .build()
            .map_err(|e| Bm25Error::remote(format!("failed to build http client: {e}")))?;
        Ok(Self {
            config,
            http,
            tokenizer: MixedTokenizer::new(),
            breaker: Arc::new(Mutex::new(CircuitBreaker::new(
                BREAKER_THRESHOLD,
                BREAKER_TIMEOUT,
            ))),
            metrics: None,
        })
    }

    /// Build a client directly from the database configuration.
    pub fn from_database_config(
        database: &cce_config::global::DatabaseConfig,
    ) -> Result<Self, Bm25Error> {
        let config = ElasticsearchConfig::from_remote(&database.fulltext_remote, &database.bm25)?;
        Self::new(config)
    }

    /// Attach a metrics collector.
    pub fn with_metrics(mut self, metrics: Arc<dyn Bm25Metrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Effective configuration (index name, timeouts, similarity).
    pub fn config(&self) -> &ElasticsearchConfig {
        &self.config
    }

    /// Whether the remote branch is configured (a URL is present).
    pub fn is_enabled(&self) -> bool {
        !self.config.base_url.is_empty()
    }

    /// Backend name for logging.
    pub fn backend_name(&self) -> &'static str {
        "remote"
    }

    /// Basic-auth credential pair, when both parts are configured.
    fn basic_auth(&self) -> Option<(String, String)> {
        match (&self.config.username, &self.config.password) {
            (Some(user), Some(pass)) => Some((user.clone(), pass.clone())),
            _ => None,
        }
    }

    fn apply_auth(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self.basic_auth() {
            Some((user, pass)) => request.basic_auth(user, Some(pass)),
            None => request,
        }
    }

    async fn check_breaker(&self) -> Result<(), Bm25Error> {
        let breaker = self.breaker.lock().await;
        if breaker.is_open() {
            return Err(Bm25Error::remote(
                "circuit breaker is open, rejecting fulltext request",
            ));
        }
        Ok(())
    }

    async fn record_success(&self) {
        self.breaker.lock().await.record_success();
    }

    async fn record_failure(&self, err: &Bm25Error) {
        if err.is_transient() {
            warn!(error = %err, "Fulltext remote failure recorded by circuit breaker");
            self.breaker.lock().await.record_failure();
        }
    }

    fn classify_status(status: reqwest::StatusCode, body: &str, op: &str) -> Bm25Error {
        if status.is_server_error()
            || status == reqwest::StatusCode::TOO_MANY_REQUESTS
            || status == reqwest::StatusCode::REQUEST_TIMEOUT
        {
            Bm25Error::remote(format!("elasticsearch {op} failed with {status}: {body}"))
        } else {
            Bm25Error::config(format!("elasticsearch {op} rejected with {status}: {body}"))
        }
    }

    fn validate_index_name(&self, index_name: &str) -> Result<(), Bm25Error> {
        if index_name != self.config.index_name {
            return Err(Bm25Error::Index(format!(
                "Index name '{index_name}' does not match configured name '{}'",
                self.config.index_name
            )));
        }
        Ok(())
    }
}

impl std::fmt::Debug for ElasticsearchClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ElasticsearchClient")
            .field("base_url", &self.config.base_url)
            .field("index_name", &self.config.index_name)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use cce_config::modules::{Bm25Config, FulltextRemoteConfig};
    use serde_json::{Value, json};

    use crate::{Bm25Document, Bm25SearchOptions, TermOperator};

    fn test_config() -> ElasticsearchConfig {
        ElasticsearchConfig {
            base_url: "http://localhost:9200".to_string(),
            api_key: None,
            username: None,
            password: None,
            index_name: "code_index".to_string(),
            bulk_size: 500,
            request_timeout: Duration::from_millis(1000),
            refresh_interval: None,
            k1: 1.8,
            b: 0.6,
        }
    }

    fn test_client() -> ElasticsearchClient {
        ElasticsearchClient::new(test_config()).expect("test client builds without I/O")
    }

    fn sample_document() -> Bm25Document {
        Bm25Document::new("1::2::group_1_bm25_0")
            .with_field("chunk_id", "group_1_bm25_0")
            .with_field("title", "calculator.calculate_total")
            .with_field("content", "fn calculate_total() {}")
            .with_field("keywords", "calculate_total calculator")
            .with_field("file_path", "calculator.rs")
            .with_field("project_id", "7")
            .with_field("epoch", "3")
            .with_field("entity_id", "10,20")
            .with_field("segment_id", "doc_group_1")
            .with_field("test", "0")
            .with_field("category", "2")
    }

    #[test]
    fn mapping_carries_local_bm25_similarity() {
        let mapping = test_client().index_mapping();
        assert_eq!(
            mapping
                .pointer("/settings/similarity/code_bm25/type")
                .and_then(Value::as_str),
            Some("BM25")
        );
        assert_eq!(
            mapping
                .pointer("/settings/similarity/code_bm25/k1")
                .and_then(Value::as_f64)
                .map(|v| v as f32),
            Some(1.8)
        );
        assert_eq!(
            mapping
                .pointer("/settings/similarity/code_bm25/b")
                .and_then(Value::as_f64)
                .map(|v| v as f32),
            Some(0.6)
        );
    }

    #[test]
    fn mapping_hides_non_readback_fields_from_source() {
        let mapping = test_client().index_mapping();
        let excludes = mapping
            .pointer("/mappings/_source/excludes")
            .and_then(Value::as_array)
            .expect("source excludes must exist");
        let names: Vec<&str> = excludes.iter().filter_map(Value::as_str).collect();
        for hidden in ["content", "keywords", "content_tokens", "keywords_tokens"] {
            assert!(names.contains(&hidden), "{hidden} must stay out of _source");
        }
        assert_eq!(
            mapping
                .pointer("/mappings/properties/entity_id/index")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            mapping
                .pointer("/mappings/properties/segment_id/index")
                .and_then(Value::as_bool),
            Some(false)
        );
    }

    #[test]
    fn index_source_keeps_only_readback_fields() {
        let source = test_client().index_source(&sample_document());
        assert_eq!(
            source.get("title").and_then(Value::as_str),
            Some("calculator.calculate_total")
        );
        assert!(source.get("content").is_none());
        assert!(source.get("keywords").is_none());
        assert!(
            source
                .get("content_tokens")
                .and_then(Value::as_str)
                .is_some_and(|s| s.contains("calculate"))
        );
        assert_eq!(
            source.get("entity_id"),
            Some(&json!(["10", "20"])),
            "entity ids stay stored as an array"
        );
    }

    #[test]
    fn bulk_body_pairs_index_action_with_source() {
        let body = test_client().bulk_body(&[sample_document()]);
        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(lines.len(), 2);
        let action: Value = serde_json::from_str(lines[0]).expect("action line is JSON");
        assert_eq!(
            action.pointer("/index/_id").and_then(Value::as_str),
            Some("1::2::group_1_bm25_0"),
            "document id is the remote primary key (idempotent overwrite)"
        );
        let source: Value = serde_json::from_str(lines[1]).expect("source line is JSON");
        assert_eq!(
            source.get("file_path").and_then(Value::as_str),
            Some("calculator.rs")
        );
    }

    #[test]
    fn search_body_scopes_project_and_generations() {
        let client = test_client();
        let mut weights = HashMap::new();
        weights.insert("title".to_string(), 2.0);
        let options = Bm25SearchOptions {
            limit: 10,
            offset: 0,
            field_weights: weights,
            project_id: 7,
            epochs: vec![2, 3],
            excluded_files: Some(vec!["old.rs".to_string()]),
            exclude_test: true,
            include_categories: vec![],
            exclude_categories: vec![],
            term_operator: TermOperator::Or,
        };
        let body = client.search_body("calculate total", &options);
        let text = body.to_string();
        assert!(
            text.contains("\"project_id\":\"7\""),
            "project filter: {text}"
        );
        assert!(text.contains("old.rs"), "excluded file filter: {text}");
        assert_eq!(
            body.pointer("/size").and_then(Value::as_u64),
            Some(10),
            "window is limit plus offset"
        );
        assert!(
            body.pointer("/query/bool/filter").is_some(),
            "filters constrain without scoring"
        );
    }

    #[test]
    fn search_body_empty_query_matches_scoped_set() {
        let client = test_client();
        let options = Bm25SearchOptions {
            limit: 5,
            offset: 0,
            field_weights: HashMap::new(),
            project_id: 7,
            epochs: vec![],
            excluded_files: None,
            exclude_test: false,
            include_categories: vec![],
            exclude_categories: vec![],
            term_operator: TermOperator::Or,
        };
        let body = client.search_body("   ", &options);
        assert!(
            body.to_string().contains("match_all"),
            "empty query degrades to a scoped match_all"
        );
    }

    #[test]
    fn search_body_term_operator_reaches_fields() {
        let client = test_client();
        let options_for = |operator| Bm25SearchOptions {
            limit: 5,
            offset: 0,
            field_weights: HashMap::new(),
            project_id: 7,
            epochs: vec![],
            excluded_files: None,
            exclude_test: false,
            include_categories: vec![],
            exclude_categories: vec![],
            term_operator: operator,
        };
        for (operator, expected) in [(TermOperator::Or, "or"), (TermOperator::And, "and")] {
            let body = client.search_body("calculate total", &options_for(operator));
            let clauses = body
                .pointer("/query/bool/must/0/bool/should")
                .and_then(Value::as_array)
                .expect("fields stay disjunctive");
            assert_eq!(clauses.len(), 4);
            for clause in clauses {
                let params = clause
                    .as_object()
                    .and_then(|obj| obj.get("match"))
                    .and_then(Value::as_object)
                    .and_then(|obj| obj.values().next())
                    .expect("match clause carries params");
                assert_eq!(
                    params.get("operator").and_then(Value::as_str),
                    Some(expected),
                    "field operator follows {operator:?}"
                );
            }
        }
    }

    #[test]
    fn config_from_remote_requires_url_and_index_name() {
        let remote = FulltextRemoteConfig::default();
        let bm25 = Bm25Config::default();
        assert!(ElasticsearchConfig::from_remote(&remote, &bm25).is_err());
        let remote = FulltextRemoteConfig {
            url: Some("http://localhost:9200/".to_string()),
            ..FulltextRemoteConfig::default()
        };
        assert!(ElasticsearchConfig::from_remote(&remote, &bm25).is_err());
        let remote = FulltextRemoteConfig {
            url: Some("http://localhost:9200/".to_string()),
            index_name: Some("code_index".to_string()),
            ..FulltextRemoteConfig::default()
        };
        let config = ElasticsearchConfig::from_remote(&remote, &bm25).expect("url suffices");
        assert_eq!(config.base_url, "http://localhost:9200");
        assert_eq!(config.index_name, "code_index");
    }
}
