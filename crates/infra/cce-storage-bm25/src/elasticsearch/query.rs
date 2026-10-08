//! Retrieval query DSL construction and hit parsing.

use cce_types::normalize_project_path;
use serde_json::{Value, json};

use cce_storage_common::{FulltextHit, FulltextSearchOptions, TermOperator};

use crate::Bm25Error;

use super::ElasticsearchClient;

/// Upper bound for a single retrieval window, mirroring the local branch.
const MAX_RETRIEVAL_WINDOW: usize = 200;

impl ElasticsearchClient {
    /// Shared term filter for project and generation scoping.
    pub(super) fn scope_filter(project_id: i64, epochs: &[i64]) -> Value {
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
    ///
    /// Term combination mirrors the local branch: the operator governs how
    /// query terms combine *within* each field (`and`/`or`), while the
    /// fields themselves stay disjunctive (any field may satisfy the query).
    pub fn search_body(&self, query: &str, options: &FulltextSearchOptions) -> Value {
        let weights =
            |name: &str, default: f32| options.field_weights.get(name).copied().unwrap_or(default);
        let title_weight = weights("title", 2.0);
        let content_weight = weights("content", 1.0);
        let keywords_weight = weights("keywords", 2.0);
        let tokens = self.pretokenize(query);
        let field_operator = match options.term_operator {
            TermOperator::And => "and",
            TermOperator::Or => "or",
        };
        let mut must: Vec<Value> = Vec::new();
        if !tokens.trim().is_empty() {
            must.push(json!({
                "bool": {
                    "should": [
                        { "match": { "title": { "query": query, "boost": title_weight, "operator": field_operator } } },
                        { "match": { "title_tokens": { "query": tokens, "boost": title_weight, "operator": field_operator } } },
                        { "match": { "content_tokens": { "query": tokens, "boost": content_weight, "operator": field_operator } } },
                        { "match": { "keywords_tokens": { "query": tokens, "boost": keywords_weight, "operator": field_operator } } },
                    ],
                    "minimum_should_match": 1,
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
            let normalized: Vec<String> = excluded
                .iter()
                .map(|path| normalize_project_path(path))
                .collect();
            must_not.push(json!({
                "bool": {
                    "filter": [
                        { "term": { "epoch": options.epochs[0] } },
                        { "terms": { "file_path": normalized } },
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
        options: &FulltextSearchOptions,
    ) -> Result<Vec<FulltextHit>, Bm25Error> {
        self.check_breaker().await?;
        let body = self.search_body(query, options);
        let result = self.search_once(&body).await;
        match &result {
            Ok(_) => self.record_success().await,
            Err(err) => self.record_failure(err).await,
        }
        result
    }

    async fn search_once(&self, body: &Value) -> Result<Vec<FulltextHit>, Bm25Error> {
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

    fn parse_hits(payload: &Value) -> Vec<FulltextHit> {
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
            hits.push(FulltextHit {
                document_id,
                score,
                fields,
            });
        }
        hits
    }
}
