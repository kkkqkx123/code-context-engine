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
//! equivalent without analyzer plugins. Every analyzed field is therefore
//! pre-tokenized on the client with the shared [`MixedTokenizer`]: the token
//! stream is joined into a `*_tokens` sidecar field indexed with the
//! `whitespace` analyzer, and the query is tokenized the same way before it
//! is sent. Term-level parity holds; phrase queries degrade to token
//! conjunctions (documented approximation, covered by the recall
//! backoff-range acceptance).
//!
//! # Write visibility
//!
//! Bulk writes do not force a refresh. Call [`ElasticsearchClient::flush`]
//! (index refresh) after the last batch before a generation is marked ready
//! or activated so counts and snapshot readbacks observe every document.

use std::sync::Arc;
use std::time::{Duration, Instant};

use cce_circuit_breaker::CircuitBreaker;
use cce_config::Validate;
use cce_config::modules::{Bm25Config, FulltextRemoteConfig};
use cce_text::MixedTokenizer;
use cce_types::error::common::ErrorClassify;
use reqwest::Client;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::metrics::Bm25Metrics;
use crate::{Bm25Document, Bm25Error, Bm25SearchOptions, Bm25SearchResult, TermOperator};

/// Upper bound for a single retrieval window, mirroring the local branch.
const MAX_RETRIEVAL_WINDOW: usize = 200;
/// Page size for snapshot readback pagination.
const SNAPSHOT_PAGE_SIZE: usize = 1000;
/// Circuit breaker failure threshold and reset timeout, mirroring Qdrant.
const BREAKER_THRESHOLD: u32 = 3;
const BREAKER_TIMEOUT: Duration = Duration::from_secs(30);

/// Elasticsearch connection and behavior configuration.
#[derive(Debug, Clone)]
pub struct ElasticsearchConfig {
    /// Search-service base URL (no trailing slash).
    pub base_url: String,
    /// API key for `ApiKey` authorization, when set.
    pub api_key: Option<String>,
    /// Basic-auth user name, when set.
    pub username: Option<String>,
    /// Basic-auth password, when set.
    pub password: Option<String>,
    /// Target index name.
    pub index_name: String,
    /// Bulk batch size for write requests.
    pub bulk_size: usize,
    /// Per-request timeout.
    pub request_timeout: Duration,
    /// Desired index refresh interval (index setting value).
    pub refresh_interval: Option<String>,
    /// BM25 term-frequency saturation, mirrored into the index similarity.
    pub k1: f32,
    /// BM25 length normalization, mirrored into the index similarity.
    pub b: f32,
}

impl ElasticsearchConfig {
    /// Build the client configuration from the database configuration.
    ///
    /// The index name falls back to the local `bm25.index_name`; the BM25
    /// algorithm parameters travel into the index similarity so remote
    /// scoring uses the same saturation and normalization as local scoring.
    pub fn from_remote(
        remote: &FulltextRemoteConfig,
        bm25: &Bm25Config,
    ) -> Result<Self, Bm25Error> {
        let url = remote
            .url
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                Bm25Error::config("database.fulltext_remote.url must be set for the remote branch")
            })?;
        remote
            .validate_structured()
            .map_err(|e| Bm25Error::config(format!("invalid fulltext remote config: {e}")))?;
        Ok(Self {
            base_url: url.trim_end_matches('/').to_string(),
            api_key: remote.api_key.clone(),
            username: remote.username.clone(),
            password: remote.password.clone(),
            index_name: remote.effective_index_name(&bm25.index_name).to_string(),
            bulk_size: remote.bulk_size,
            request_timeout: Duration::from_millis(remote.request_timeout_ms),
            refresh_interval: remote.refresh_interval.clone(),
            k1: bm25.algorithm.k1,
            b: bm25.algorithm.b,
        })
    }

    fn index_url(&self) -> String {
        format!("{}/{}", self.base_url, self.index_name)
    }
}

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

    /// Index mapping for the single shared index.
    ///
    /// `title` keeps the standard analyzer plus a whitespace-analyzed
    /// pre-tokenized sidecar; body and keyword fields only exist as
    /// pre-tokenized sidecars excluded from `_source` (indexed, not stored).
    /// The BM25 similarity carries the local `k1`/`b` parameters.
    pub fn index_mapping(&self) -> Value {
        let similarity = json!({
            "type": "BM25",
            "k1": self.config.k1,
            "b": self.config.b,
        });
        let mut settings = json!({
            "number_of_shards": 1,
            "number_of_replicas": 0,
            "similarity": { "code_bm25": similarity },
        });
        if let Some(ref interval) = self.config.refresh_interval {
            settings["refresh_interval"] = json!(interval);
        }
        json!({
            "settings": settings,
            "mappings": {
                "_source": { "excludes": ["content", "keywords", "content_tokens", "keywords_tokens", "title_tokens"] },
                "properties": {
                    "document_id": { "type": "keyword", "store": true },
                    "chunk_id": { "type": "keyword", "store": true },
                    "file_path": { "type": "keyword" },
                    "project_id": { "type": "keyword" },
                    "epoch": { "type": "long" },
                    "title": { "type": "text", "similarity": "code_bm25", "store": true },
                    "title_tokens": { "type": "text", "analyzer": "whitespace", "similarity": "code_bm25" },
                    "content_tokens": { "type": "text", "analyzer": "whitespace", "similarity": "code_bm25" },
                    "keywords_tokens": { "type": "text", "analyzer": "whitespace", "similarity": "code_bm25" },
                    "entity_id": { "type": "keyword", "index": false },
                    "segment_id": { "type": "keyword", "index": false },
                    "test": { "type": "integer" },
                    "category": { "type": "integer" },
                }
            }
        })
    }

    /// Create the index when it does not exist yet.
    pub async fn ensure_index(&self) -> Result<bool, Bm25Error> {
        self.check_breaker().await?;
        let head = self
            .apply_auth(self.http.head(self.config.index_url()))
            .send()
            .await
            .map_err(|e| Bm25Error::remote(format!("elasticsearch index probe failed: {e}")))?;
        if head.status().is_success() {
            self.record_success().await;
            return Ok(false);
        }
        let response = self
            .apply_auth(self.http.put(self.config.index_url()))
            .json(&self.index_mapping())
            .send()
            .await
            .map_err(|e| Bm25Error::remote(format!("elasticsearch index create failed: {e}")))?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if status.is_success() {
            info!(index = %self.config.index_name, "Elasticsearch index created");
            self.record_success().await;
            return Ok(true);
        }
        if status == reqwest::StatusCode::BAD_REQUEST && body.contains("resource_already_exists") {
            self.record_success().await;
            return Ok(false);
        }
        let err = Self::classify_status(status, &body, "index create");
        self.record_failure(&err).await;
        Err(err)
    }

    /// Lightweight reachability probe.
    pub async fn health(&self) -> Result<bool, Bm25Error> {
        self.check_breaker().await?;
        let result = self
            .apply_auth(self.http.get(format!(
                "{}/_cluster/health/{}",
                self.config.base_url, self.config.index_name
            )))
            .send()
            .await
            .map_err(|e| Bm25Error::remote(format!("elasticsearch health probe failed: {e}")));
        let ok = match result {
            Ok(response) => response.status().is_success(),
            Err(e) => {
                self.record_failure(&e).await;
                return Err(e);
            }
        };
        self.record_success().await;
        Ok(ok)
    }

    /// Client-side pre-tokenization shared by writes and queries.
    fn pretokenize(&self, text: &str) -> String {
        self.tokenizer.tokenize(text).join(" ")
    }

    /// Build the indexed source document for one neutral document.
    ///
    /// Raw body and keyword text never enter `_source`; only the
    /// pre-tokenized sidecars are indexed for those fields.
    fn index_source(&self, document: &Bm25Document) -> Value {
        let field = |name: &str| document.get_field(name).cloned().unwrap_or_default();
        let title = field("title");
        let content = field("content");
        let keywords = field("keywords");
        let entity_ids: Vec<&str> = document
            .get_field("entity_id")
            .map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let parse_i64 = |name: &str| {
            document
                .get_field(name)
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0)
        };
        json!({
            "document_id": document.document_id,
            "chunk_id": field("chunk_id"),
            "file_path": field("file_path"),
            "project_id": field("project_id"),
            "epoch": parse_i64("epoch"),
            "title": title,
            "title_tokens": self.pretokenize(&title),
            "content_tokens": self.pretokenize(&content),
            "keywords_tokens": self.pretokenize(&keywords),
            "entity_id": entity_ids,
            "segment_id": field("segment_id"),
            "test": parse_i64("test"),
            "category": parse_i64("category"),
        })
    }

    /// Bulk request body for one batch (newline-delimited action pairs).
    fn bulk_body(&self, documents: &[Bm25Document]) -> String {
        let mut body = String::new();
        for document in documents {
            body.push_str(&json!({ "index": { "_id": document.document_id } }).to_string());
            body.push('\n');
            body.push_str(&self.index_source(document).to_string());
            body.push('\n');
        }
        body
    }

    /// Index a batch of documents (idempotent per document id).
    pub async fn batch_index(
        &self,
        index_name: &str,
        documents: &[Bm25Document],
    ) -> Result<usize, Bm25Error> {
        self.validate_index_name(index_name)?;
        if documents.is_empty() {
            return Ok(0);
        }
        self.check_breaker().await?;
        let start = Instant::now();
        let mut total = 0usize;
        for chunk in documents.chunks(self.config.bulk_size.max(1)) {
            let result = self.bulk_once(chunk).await;
            match &result {
                Ok(_) => self.record_success().await,
                Err(err) => self.record_failure(err).await,
            }
            total += result?;
        }
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        if let Some(metrics) = &self.metrics {
            metrics.record_index(elapsed_ms, total, true);
        }
        debug!(count = total, "Elasticsearch bulk index completed");
        Ok(total)
    }

    async fn bulk_once(&self, documents: &[Bm25Document]) -> Result<usize, Bm25Error> {
        let body = self.bulk_body(documents);
        let response = self
            .apply_auth(
                self.http
                    .post(format!("{}/_bulk", self.config.index_url()))
                    .header("Content-Type", "application/x-ndjson")
                    .body(body),
            )
            .send()
            .await
            .map_err(|e| Bm25Error::remote(format!("elasticsearch bulk failed: {e}")))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| Bm25Error::remote(format!("elasticsearch bulk read failed: {e}")))?;
        if !status.is_success() {
            return Err(Self::classify_status(status, &text, "bulk index"));
        }
        let payload: Value = serde_json::from_str(&text)
            .map_err(|e| Bm25Error::remote(format!("elasticsearch bulk parse failed: {e}")))?;
        if payload
            .get("errors")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(Bm25Error::remote(format!(
                "elasticsearch bulk reported item errors: {text}"
            )));
        }
        Ok(documents.len())
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

    /// Refresh the index so recent writes become visible to counts and
    /// snapshot readbacks. Called after the last batch before a generation
    /// is marked ready or activated (local branch: no-op, its reader is
    /// reloaded per batch).
    pub async fn flush(&self) -> Result<(), Bm25Error> {
        self.check_breaker().await?;
        let result = self
            .apply_auth(
                self.http
                    .post(format!("{}/_refresh", self.config.index_url())),
            )
            .send()
            .await
            .map_err(|e| Bm25Error::remote(format!("elasticsearch refresh failed: {e}")));
        let result = match result {
            Ok(response) if response.status().is_success() => Ok(()),
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                Err(Self::classify_status(status, &body, "refresh"))
            }
            Err(e) => Err(e),
        };
        match &result {
            Ok(_) => self.record_success().await,
            Err(err) => self.record_failure(err).await,
        }
        result
    }

    /// Shared term filter for project and generation scoping.
    fn scope_filter(project_id: i64, epochs: &[i64]) -> Value {
        let mut filters = vec![json!({ "term": { "project_id": project_id.to_string() } })];
        if !epochs.is_empty() {
            filters.push(json!({ "terms": { "epoch": epochs } }));
        }
        json!(filters)
    }

    /// Query DSL for a neutral retrieval request.
    ///
    /// Analyzed fields are queried with the client-tokenized form; the raw
    /// title keeps a standard-analyzed clause. Quoted phrases degrade to
    /// token conjunctions. Generation exclusion removes parent-generation
    /// rows for overridden files.
    pub fn search_body(&self, query: &str, options: &Bm25SearchOptions) -> Value {
        let weights =
            |name: &str, default: f32| options.field_weights.get(name).copied().unwrap_or(default);
        let title_weight = weights("title", 2.0);
        let content_weight = weights("content", 1.0);
        let keywords_weight = weights("keywords", 2.0);
        let tokens = self.pretokenize(query);
        let occur = match options.term_operator {
            TermOperator::And => "must",
            TermOperator::Or => "should",
        };
        let mut must: Vec<Value> = Vec::new();
        if !tokens.trim().is_empty() {
            must.push(json!({
                "bool": {
                    occur: [
                        { "match": { "title": { "query": query, "boost": title_weight } } },
                        { "match": { "title_tokens": { "query": tokens, "boost": title_weight, "operator": "or" } } },
                        { "match": { "content_tokens": { "query": tokens, "boost": content_weight, "operator": "or" } } },
                        { "match": { "keywords_tokens": { "query": tokens, "boost": keywords_weight, "operator": "or" } } },
                    ],
                    "minimum_should_match": if matches!(options.term_operator, TermOperator::Or) { 1 } else { 0 },
                }
            }));
        } else {
            must.push(json!({ "match_all": {} }));
        }
        let mut filter = Self::scope_filter(options.project_id, &options.epochs);
        if options.exclude_test {
            filter
                .as_array_mut()
                .expect("scope filter is an array")
                .push(json!({ "bool": { "must_not": [{ "term": { "test": 1 } }] } }));
        }
        if !options.include_categories.is_empty() {
            let values: Vec<u8> = options
                .include_categories
                .iter()
                .map(|c| c.as_u8())
                .collect();
            filter
                .as_array_mut()
                .expect("scope filter is an array")
                .push(json!({ "terms": { "category": values } }));
        }
        let mut bool_query = json!({
            "bool": { "must": must, "filter": filter }
        });
        let mut must_not: Vec<Value> = Vec::new();
        if options.epochs.len() > 1
            && let Some(excluded) = options.excluded_files.as_ref().filter(|f| !f.is_empty())
        {
            must_not.push(json!({
                "bool": {
                    "filter": [
                        { "term": { "epoch": options.epochs[0] } },
                        { "terms": { "file_path": excluded } },
                    ]
                }
            }));
        }
        for category in &options.exclude_categories {
            must_not.push(json!({ "term": { "category": category.as_u8() } }));
        }
        if !must_not.is_empty() {
            bool_query["bool"]["must_not"] = json!(must_not);
        }
        let window = (options.limit + options.offset).min(MAX_RETRIEVAL_WINDOW);
        json!({
            "query": bool_query,
            "size": window,
            "from": options.offset.min(window),
            "track_scores": true,
            "_source": ["document_id", "chunk_id", "file_path", "title", "project_id", "epoch", "entity_id", "segment_id", "test", "category"],
        })
    }

    /// Run a keyword retrieval with project and generation filters.
    pub async fn search(
        &self,
        query: &str,
        options: &Bm25SearchOptions,
    ) -> Result<Vec<Bm25SearchResult>, Bm25Error> {
        self.check_breaker().await?;
        let body = self.search_body(query, options);
        let result = self.search_once(&body).await;
        match &result {
            Ok(_) => self.record_success().await,
            Err(err) => self.record_failure(err).await,
        }
        result
    }

    async fn search_once(&self, body: &Value) -> Result<Vec<Bm25SearchResult>, Bm25Error> {
        let response = self
            .apply_auth(
                self.http
                    .post(format!("{}/_search", self.config.index_url())),
            )
            .json(body)
            .send()
            .await
            .map_err(|e| Bm25Error::remote(format!("elasticsearch search failed: {e}")))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| Bm25Error::remote(format!("elasticsearch search read failed: {e}")))?;
        if !status.is_success() {
            return Err(Self::classify_status(status, &text, "search"));
        }
        let payload: Value = serde_json::from_str(&text)
            .map_err(|e| Bm25Error::remote(format!("elasticsearch search parse failed: {e}")))?;
        Ok(Self::parse_hits(&payload))
    }

    fn parse_hits(payload: &Value) -> Vec<Bm25SearchResult> {
        let mut hits = Vec::new();
        let empty = Vec::new();
        let entries = payload
            .pointer("/hits/hits")
            .and_then(Value::as_array)
            .unwrap_or(&empty);
        for entry in entries {
            let score = entry.get("_score").and_then(Value::as_f64).unwrap_or(0.0) as f32;
            let source = entry.get("_source").cloned().unwrap_or(Value::Null);
            let document_id = entry
                .get("_id")
                .and_then(Value::as_str)
                .or_else(|| source.get("document_id").and_then(Value::as_str))
                .unwrap_or_default()
                .to_string();
            let mut fields = std::collections::HashMap::new();
            let put_str = |fields: &mut std::collections::HashMap<String, String>,
                           source: &Value,
                           name: &str| {
                if let Some(value) = source.get(name).and_then(Value::as_str)
                    && !value.is_empty()
                {
                    fields.insert(name.to_string(), value.to_string());
                }
            };
            for name in ["chunk_id", "file_path", "title", "segment_id"] {
                put_str(&mut fields, &source, name);
            }
            let put_num = |fields: &mut std::collections::HashMap<String, String>,
                           source: &Value,
                           name: &str| {
                if let Some(value) = source.get(name).and_then(Value::as_i64) {
                    fields.insert(name.to_string(), value.to_string());
                }
            };
            for name in ["test", "category"] {
                put_num(&mut fields, &source, name);
            }
            if let Some(ids) = source.get("entity_id") {
                let joined = match ids {
                    Value::Array(items) => items
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(","),
                    Value::String(s) => s.clone(),
                    _ => String::new(),
                };
                if !joined.is_empty() {
                    fields.insert("entity_id".to_string(), joined);
                }
            }
            hits.push(Bm25SearchResult {
                document_id,
                score,
                fields,
            });
        }
        hits
    }

    async fn delete_by_query(&self, query: &Value) -> Result<usize, Bm25Error> {
        self.check_breaker().await?;
        let body = json!({ "query": query });
        let result = self
            .apply_auth(
                self.http
                    .post(format!("{}/_delete_by_query", self.config.index_url())),
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| Bm25Error::remote(format!("elasticsearch delete failed: {e}")));
        let result = match result {
            Ok(response) => {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                if !status.is_success() {
                    Err(Self::classify_status(status, &text, "delete"))
                } else {
                    serde_json::from_str::<Value>(&text)
                        .map(|v| v.get("deleted").and_then(Value::as_u64).unwrap_or(0) as usize)
                        .map_err(|e| {
                            Bm25Error::remote(format!("elasticsearch delete parse failed: {e}"))
                        })
                }
            }
            Err(e) => Err(e),
        };
        match &result {
            Ok(_) => self.record_success().await,
            Err(err) => self.record_failure(err).await,
        }
        let deleted = result?;
        if let Some(metrics) = &self.metrics {
            metrics.record_delete(0.0, deleted, true);
        }
        Ok(deleted)
    }

    /// Delete documents for one file within one project.
    pub async fn delete_by_file_path_scoped(
        &self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
    ) -> Result<usize, Bm25Error> {
        self.validate_index_name(index_name)?;
        self.delete_by_query(&json!({
            "bool": { "filter": [
                { "term": { "file_path": file_path } },
                { "term": { "project_id": project_id.to_string() } },
            ]}
        }))
        .await
    }

    /// Delete documents for one file in one data epoch.
    pub async fn delete_by_file_path_scoped_epoch(
        &self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, Bm25Error> {
        self.validate_index_name(index_name)?;
        self.delete_by_query(&json!({
            "bool": { "filter": [
                { "term": { "file_path": file_path } },
                { "term": { "project_id": project_id.to_string() } },
                { "term": { "epoch": epoch } },
            ]}
        }))
        .await
    }

    /// Delete all documents for one project and data epoch.
    pub async fn delete_by_project_epoch(
        &self,
        index_name: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, Bm25Error> {
        self.validate_index_name(index_name)?;
        self.delete_by_query(&json!({
            "bool": { "filter": [
                { "term": { "project_id": project_id.to_string() } },
                { "term": { "epoch": epoch } },
            ]}
        }))
        .await
    }

    /// Delete all documents for a project.
    pub async fn delete_all_project_docs(
        &self,
        index_name: &str,
        project_id: i64,
    ) -> Result<usize, Bm25Error> {
        self.validate_index_name(index_name)?;
        self.delete_by_query(&json!({
            "bool": { "filter": [
                { "term": { "project_id": project_id.to_string() } },
            ]}
        }))
        .await
    }

    /// Read back the stored fields needed to copy an epoch into a candidate
    /// generation (paginated cursor over the project and generation filter).
    pub async fn snapshot_documents(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<Bm25Document>, Bm25Error> {
        self.check_breaker().await?;
        let mut documents = Vec::new();
        let mut search_after: Option<Value> = None;
        loop {
            let mut body = json!({
                "query": { "bool": { "filter": Self::scope_filter(project_id, &[epoch]) } },
                "size": SNAPSHOT_PAGE_SIZE,
                "sort": [{ "document_id": "asc" }],
                "_source": ["document_id", "chunk_id", "file_path", "title", "project_id", "epoch", "entity_id", "segment_id", "test", "category"],
            });
            if let Some(after) = search_after.as_ref() {
                body["search_after"] = json!([after]);
            }
            let response = self
                .apply_auth(
                    self.http
                        .post(format!("{}/_search", self.config.index_url())),
                )
                .json(&body)
                .send()
                .await
                .map_err(|e| {
                    Bm25Error::remote(format!("elasticsearch snapshot read failed: {e}"))
                })?;
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            if !status.is_success() {
                let err = Self::classify_status(status, &text, "snapshot read");
                self.record_failure(&err).await;
                return Err(err);
            }
            let payload: Value = serde_json::from_str(&text).map_err(|e| {
                Bm25Error::remote(format!("elasticsearch snapshot parse failed: {e}"))
            })?;
            let empty = Vec::new();
            let entries = payload
                .pointer("/hits/hits")
                .and_then(Value::as_array)
                .unwrap_or(&empty);
            if entries.is_empty() {
                break;
            }
            for entry in entries {
                let source = entry.get("_source").cloned().unwrap_or(Value::Null);
                let document_id = entry
                    .get("_id")
                    .and_then(Value::as_str)
                    .or_else(|| source.get("document_id").and_then(Value::as_str))
                    .unwrap_or_default()
                    .to_string();
                let mut fields = std::collections::HashMap::new();
                for name in ["chunk_id", "file_path", "title", "project_id", "segment_id"] {
                    if let Some(value) = source.get(name).and_then(Value::as_str) {
                        fields.insert(name.to_string(), value.to_string());
                    }
                }
                for name in ["epoch", "test", "category"] {
                    if let Some(value) = source.get(name).and_then(Value::as_i64) {
                        fields.insert(name.to_string(), value.to_string());
                    }
                }
                if let Some(ids) = source.get("entity_id") {
                    let joined = match ids {
                        Value::Array(items) => items
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(","),
                        Value::String(s) => s.clone(),
                        _ => String::new(),
                    };
                    if !joined.is_empty() {
                        fields.insert("entity_id".to_string(), joined);
                    }
                }
                search_after = entry.get("sort").and_then(|s| s.get(0)).cloned();
                documents.push(Bm25Document {
                    document_id,
                    fields,
                });
            }
            if entries.len() < SNAPSHOT_PAGE_SIZE {
                break;
            }
            if search_after.is_none() {
                break;
            }
        }
        self.record_success().await;
        Ok(documents)
    }

    async fn count_with_query(&self, query: &Value) -> Result<usize, Bm25Error> {
        self.check_breaker().await?;
        let body = json!({ "query": query });
        let result = self
            .apply_auth(
                self.http
                    .post(format!("{}/_count", self.config.index_url())),
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| Bm25Error::remote(format!("elasticsearch count failed: {e}")));
        let result = match result {
            Ok(response) => {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                if !status.is_success() {
                    Err(Self::classify_status(status, &text, "count"))
                } else {
                    serde_json::from_str::<Value>(&text)
                        .map(|v| v.get("count").and_then(Value::as_u64).unwrap_or(0) as usize)
                        .map_err(|e| {
                            Bm25Error::remote(format!("elasticsearch count parse failed: {e}"))
                        })
                }
            }
            Err(e) => Err(e),
        };
        match &result {
            Ok(_) => self.record_success().await,
            Err(err) => self.record_failure(err).await,
        }
        result
    }

    /// Count all documents in the index.
    pub async fn document_count(&self) -> Result<usize, Bm25Error> {
        self.count_with_query(&json!({ "match_all": {} })).await
    }

    /// Count documents belonging to one project.
    pub async fn document_count_by_project(&self, project_id: i64) -> Result<usize, Bm25Error> {
        self.count_with_query(&json!({
            "bool": { "filter": [{ "term": { "project_id": project_id.to_string() } }] }
        }))
        .await
    }

    /// List data epochs currently present for a project.
    pub async fn epochs_by_project(&self, project_id: i64) -> Result<Vec<i64>, Bm25Error> {
        self.check_breaker().await?;
        let body = json!({
            "size": 0,
            "query": { "bool": { "filter": [{ "term": { "project_id": project_id.to_string() } }] } },
            "aggs": { "epochs": { "terms": { "field": "epoch", "size": 10000 } } },
        });
        let result = self
            .apply_auth(
                self.http
                    .post(format!("{}/_search", self.config.index_url())),
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| Bm25Error::remote(format!("elasticsearch epochs read failed: {e}")));
        let result = match result {
            Ok(response) => {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                if !status.is_success() {
                    Err(Self::classify_status(status, &text, "epochs"))
                } else {
                    serde_json::from_str::<Value>(&text)
                        .map(|v| {
                            v.pointer("/aggregations/epochs/buckets")
                                .and_then(Value::as_array)
                                .map(|buckets| {
                                    buckets
                                        .iter()
                                        .filter_map(|b| b.get("key").and_then(Value::as_i64))
                                        .collect()
                                })
                                .unwrap_or_default()
                        })
                        .map_err(|e| {
                            Bm25Error::remote(format!("elasticsearch epochs parse failed: {e}"))
                        })
                }
            }
            Err(e) => Err(e),
        };
        match &result {
            Ok(_) => self.record_success().await,
            Err(err) => self.record_failure(err).await,
        }
        result
    }

    /// Recreate the index from scratch (generation rebuild/cleanup).
    pub async fn clear_index(&self, index_name: &str) -> Result<usize, Bm25Error> {
        self.validate_index_name(index_name)?;
        let before = self.document_count().await.unwrap_or(0);
        self.check_breaker().await?;
        let response = self
            .apply_auth(self.http.delete(self.config.index_url()))
            .send()
            .await
            .map_err(|e| Bm25Error::remote(format!("elasticsearch index delete failed: {e}")))?;
        let status = response.status();
        if !status.is_success() && status != reqwest::StatusCode::NOT_FOUND {
            let body = response.text().await.unwrap_or_default();
            let err = Self::classify_status(status, &body, "index delete");
            self.record_failure(&err).await;
            return Err(err);
        }
        self.record_success().await;
        self.ensure_index().await?;
        Ok(before)
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
                .and_then(Value::as_f64),
            Some(1.8)
        );
        assert_eq!(
            mapping
                .pointer("/settings/similarity/code_bm25/b")
                .and_then(Value::as_f64),
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
    fn config_from_remote_requires_url() {
        let remote = FulltextRemoteConfig::default();
        let bm25 = Bm25Config::default();
        assert!(ElasticsearchConfig::from_remote(&remote, &bm25).is_err());
        let remote = FulltextRemoteConfig {
            url: Some("http://localhost:9200/".to_string()),
            ..FulltextRemoteConfig::default()
        };
        let config = ElasticsearchConfig::from_remote(&remote, &bm25).expect("url suffices");
        assert_eq!(config.base_url, "http://localhost:9200");
        assert_eq!(config.index_name, "code_index");
    }
}
