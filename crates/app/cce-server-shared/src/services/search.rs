//! Shared query application service for the HTTP and MCP surfaces.
//!
//! Both transports validate their own wire arguments, then describe the
//! query with the neutral spec types below. Everything after that lives
//! here exactly once: project config merge, expansion resolver injection,
//! coordinator assembly, execution, DTO conversion, and relation stamping.

use std::sync::Arc;

use cce_api::models::{ContentStateDto, SearchResultItem};
use cce_codegraph::CallChainQuery;
use cce_config::modules::search::RelationAnnotationConfig;
use cce_orchestrator::SearchResult as OrchestratorResultItem;
use cce_orchestrator::query::expansion::{NeighbourResolver, stable_id_of};
use cce_orchestrator::query::types::{
    ContentState, DowngradeReason, ExcludableContentType, QueryConfigBuilder, SearchConfig,
    SearchSources,
};
use cce_orchestrator::query::{
    AggregatedQueryOptions, FilterOptions, IndexCapabilities, QueryCoordinator, QueryIntent,
    QueryOptions, RelationSearcher, SubQuery,
};
use cce_types::FileCategory;

use crate::context::SharedContext;
use crate::engine::EngineError;

/// Failure of the shared query service.
///
/// Transports map input-side variants to their invalid-argument shape and
/// execution-side failures to their internal-error shape.
#[derive(Debug, thiserror::Error)]
pub enum SearchServiceError {
    /// A wire value failed to parse (query type, category, intent, ...).
    #[error("invalid query: {0}")]
    InvalidInput(String),
    /// The project entry could not be loaded.
    #[error("failed to load project config: {0}")]
    Project(String),
    /// Searcher construction or query execution failed.
    #[error("{0}")]
    Execution(String),
}

impl From<EngineError> for SearchServiceError {
    fn from(error: EngineError) -> Self {
        Self::Execution(error.to_string())
    }
}

/// Filter and tuning knobs shared by single and aggregated queries.
#[derive(Debug, Clone, Default)]
pub struct SharedQueryFields {
    /// Maximum number of results; `None` keeps the project default.
    pub limit: Option<usize>,
    /// Minimum unified score; `None` keeps the project default.
    pub min_score: Option<f32>,
    /// Rerank master switch override; `None` keeps the project default.
    pub enable_rerank: Option<bool>,
    /// Rerank candidate budget override; `None` keeps the project default.
    pub rerank_max_candidates: Option<usize>,
    /// Directory prefix filter.
    pub directory_prefix: Option<String>,
    /// Already-parsed excluded content types.
    pub exclude_content_types: Vec<ExcludableContentType>,
    /// Already-parsed included file categories.
    pub include_categories: Vec<FileCategory>,
    /// Already-parsed excluded file categories.
    pub exclude_categories: Vec<FileCategory>,
    /// Glob exclude patterns.
    pub exclude_patterns: Vec<String>,
    /// Glob include patterns.
    pub include_patterns: Vec<String>,
    /// Already-parsed query intent; `None` means auto-detect.
    pub query_intent: Option<QueryIntent>,
    /// Relation-expansion opt-in for the final hits.
    pub expand: bool,
}

/// One single-query request described without transport types.
#[derive(Debug, Clone)]
pub struct SingleQuerySpec {
    /// Query text.
    pub query: String,
    /// Retrieval sources.
    pub sources: SearchSources,
    /// Shared knobs.
    pub fields: SharedQueryFields,
}

/// One aggregated sub-query described without transport types.
#[derive(Debug, Clone)]
pub struct SubQuerySpec {
    /// Sub-query text.
    pub text: String,
    /// Retrieval sources.
    pub sources: SearchSources,
    /// Score scale applied before the cross-query merge.
    pub weight: f32,
}

/// One aggregated-query request described without transport types.
#[derive(Debug, Clone)]
pub struct AggregatedQuerySpec {
    /// Sub-queries to run in parallel and merge.
    pub sub_queries: Vec<SubQuerySpec>,
    /// Shared knobs.
    pub fields: SharedQueryFields,
}

/// Converted search outcome ready for transport rendering.
#[derive(Debug)]
pub struct SearchOutcome {
    /// Total hits before limiting.
    pub total: usize,
    /// Converted API items.
    pub items: Vec<SearchResultItem>,
    /// Engine-side execution time in milliseconds.
    pub elapsed_ms: u64,
    /// Retrieval sources that contributed.
    pub sources: Vec<String>,
    /// Sub-query texts that failed during aggregated search.
    pub failed_sub_queries: Vec<String>,
    /// Relation snapshot epoch, when available.
    pub relation_epoch: Option<i64>,
    /// Whether the relation snapshot is stale.
    pub relation_stale: bool,
}

/// Shared query service over one application state.
pub struct SearchService<'a> {
    state: &'a SharedContext,
}

impl<'a> SearchService<'a> {
    /// Borrow the application state for one query service scope.
    pub fn new(state: &'a SharedContext) -> Self {
        Self { state }
    }

    /// Parse a wire query-type label into retrieval sources.
    ///
    /// `scope` names the field for the error message (`query type` for
    /// top-level queries, `query type in sub-query` for aggregated parts).
    pub fn parse_search_sources(
        label: &str,
        scope: &str,
    ) -> Result<SearchSources, SearchServiceError> {
        match label.to_lowercase().as_str() {
            "vector" => Ok(SearchSources::none().with_vector()),
            "bm25" => Ok(SearchSources::none().with_bm25()),
            "hybrid" => Ok(SearchSources::default()),
            "summary" => Ok(SearchSources::none().with_summary()),
            _ => Err(SearchServiceError::InvalidInput(format!(
                "Invalid {scope}: {label}"
            ))),
        }
    }

    /// Parse a wire content-type label. Unknown values stay visible errors.
    pub fn parse_exclude_content_type(
        raw: &str,
    ) -> Result<ExcludableContentType, SearchServiceError> {
        match raw.to_lowercase().as_str() {
            "test" | "tests" => Ok(ExcludableContentType::Test),
            _ => Err(SearchServiceError::InvalidInput(format!(
                "Unknown exclude content type: {raw}"
            ))),
        }
    }

    /// Parse a wire file-category label. Unknown values stay visible errors.
    pub fn parse_file_category(raw: &str, scope: &str) -> Result<FileCategory, SearchServiceError> {
        cce_types::FileCategory::from_name(&raw.to_lowercase()).ok_or_else(|| {
            SearchServiceError::InvalidInput(format!("Unknown {scope} category: {raw}"))
        })
    }

    /// Parse an optional query intent label. Empty means auto-detect.
    pub fn parse_query_intent(
        raw: Option<&str>,
    ) -> Result<Option<QueryIntent>, SearchServiceError> {
        match raw {
            None | Some("") => Ok(None),
            Some(label) => match label.to_lowercase().as_str() {
                "semantic" => Ok(Some(QueryIntent::Semantic)),
                "keyword" => Ok(Some(QueryIntent::Keyword)),
                "hybrid" => Ok(Some(QueryIntent::Hybrid)),
                "entity" => Ok(Some(QueryIntent::Entity)),
                _ => Err(SearchServiceError::InvalidInput(format!(
                    "Unknown query intent: {label}"
                ))),
            },
        }
    }

    /// Run one single query end to end: options, execution, conversion.
    pub async fn run_single(
        &self,
        project_id: i64,
        spec: SingleQuerySpec,
    ) -> Result<SearchOutcome, SearchServiceError> {
        let entry = self.load_entry(project_id).await?;
        let fields = &spec.fields;
        let mut query_opts = QueryConfigBuilder::new(project_id)
            .build(&spec.query)
            .with_sources(spec.sources)
            .with_limit(fields.limit.unwrap_or(entry.config.search.result.limit));

        Self::merge_search_config(&mut query_opts.config, &entry);
        query_opts.config.result.limit = fields.limit.unwrap_or(entry.config.search.result.limit);
        if let Some(min_score) = fields.min_score {
            query_opts.config.result.min_score = min_score;
        }
        if let Some(enable_rerank) = fields.enable_rerank {
            query_opts = query_opts.with_enable_rerank(enable_rerank);
        }
        if let Some(max_candidates) = fields.rerank_max_candidates {
            query_opts.config.rerank.max_candidates = max_candidates;
        }
        if let Some(prefix) = &fields.directory_prefix {
            query_opts = query_opts.with_directory_prefix(prefix);
        }
        for content_type in &fields.exclude_content_types {
            query_opts = query_opts.add_exclude_content_type(*content_type);
        }
        if !fields.exclude_patterns.is_empty() {
            query_opts = query_opts.with_exclude_patterns(fields.exclude_patterns.clone());
        }
        if !fields.include_patterns.is_empty() {
            query_opts = query_opts.with_include_patterns(fields.include_patterns.clone());
        }
        if !fields.include_categories.is_empty() {
            query_opts = query_opts.with_include_categories(fields.include_categories.clone());
        }
        if !fields.exclude_categories.is_empty() {
            query_opts = query_opts.with_exclude_categories(fields.exclude_categories.clone());
        }
        query_opts = query_opts.with_query_intent(fields.query_intent);
        query_opts = self
            .apply_expansion(project_id, query_opts, &entry, fields.expand)
            .await;

        let result = self.state.engine.search(project_id, &query_opts).await?;
        let items = self.convert_items(project_id, result.items).await;
        let (relation_epoch, relation_stale) = self.relation_version(project_id).await;
        Ok(SearchOutcome {
            total: result.total,
            items,
            elapsed_ms: result.elapsed_ms,
            sources: result.sources,
            failed_sub_queries: result.failed_sub_queries,
            relation_epoch,
            relation_stale,
        })
    }

    /// Run one aggregated query end to end.
    ///
    /// Sub-queries run without annotation; expansion applies once to the
    /// merged hits so budgets and cross-hit dedup observe the final Top-N.
    pub async fn run_aggregated(
        &self,
        project_id: i64,
        spec: AggregatedQuerySpec,
    ) -> Result<SearchOutcome, SearchServiceError> {
        let entry = self.load_entry(project_id).await?;
        let fields = &spec.fields;
        let mut global_config = SearchConfig::default();
        Self::merge_search_config(&mut global_config, &entry);
        global_config.result = {
            let mut result = entry.config.search.result.clone();
            result.limit = fields.limit.unwrap_or(result.limit);
            result
        };
        if let Some(min_score) = fields.min_score {
            global_config.result.min_score = min_score;
        }
        if let Some(max_candidates) = fields.rerank_max_candidates {
            global_config.rerank.max_candidates = max_candidates;
        }
        let resolver = self
            .expansion_resolver(
                project_id,
                &entry,
                &mut global_config.annotation,
                fields.expand,
            )
            .await;

        let has_filters = fields.directory_prefix.is_some()
            || !fields.exclude_content_types.is_empty()
            || !fields.include_categories.is_empty()
            || !fields.exclude_categories.is_empty();
        let filters = has_filters.then(|| FilterOptions {
            directory_prefix: fields.directory_prefix.clone(),
            exclude_content_types: fields.exclude_content_types.clone(),
            include_categories: fields.include_categories.clone(),
            exclude_categories: fields.exclude_categories.clone(),
        });
        let agg_options = AggregatedQueryOptions {
            original_query: String::new(),
            project_id,
            sub_queries: spec
                .sub_queries
                .into_iter()
                .map(|sub| SubQuery {
                    text: sub.text,
                    sources: sub.sources,
                    weight: sub.weight,
                })
                .collect(),
            global_config,
            filters,
            exclude_patterns: fields.exclude_patterns.clone(),
            include_patterns: fields.include_patterns.clone(),
            enable_rerank: fields.enable_rerank,
            query_intent: fields.query_intent,
            neighbour_resolver: resolver,
        };

        let coordinator = self.build_coordinator(project_id, &entry).await?;
        let result = coordinator
            .search_aggregated(&agg_options)
            .await
            .map_err(|error| SearchServiceError::Execution(error.to_string()))?;
        let items = self.convert_items(project_id, result.items).await;
        let (relation_epoch, relation_stale) = self.relation_version(project_id).await;
        Ok(SearchOutcome {
            total: result.total,
            items,
            elapsed_ms: result.elapsed_ms,
            sources: result.sources,
            failed_sub_queries: result.failed_sub_queries,
            relation_epoch,
            relation_stale,
        })
    }

    /// Load one project entry or report it as a project failure.
    async fn load_entry(
        &self,
        project_id: i64,
    ) -> Result<cce_config::project_registry::ProjectEntry, SearchServiceError> {
        self.state
            .engine
            .project_registry()
            .get_or_load(project_id)
            .await
            .map_err(|error| SearchServiceError::Project(error.to_string()))
    }

    /// Merge the project's search and rerank configuration into query options
    /// so runtime parameters come from the config layer.
    fn merge_search_config(
        config: &mut SearchConfig,
        entry: &cce_config::project_registry::ProjectEntry,
    ) {
        config.rerank = entry.config.rerank.clone();
        config.vector = entry.config.search.vector.clone();
        config.bm25 = entry.config.search.bm25.clone();
        config.fusion = entry.config.search.fusion.clone();
        config.summary = entry.config.search.summary.clone();
        config.score = entry.config.search.score.clone();
        config.boost = entry.config.search.boost.clone();
        config.plugin = entry.config.search.plugin.clone();
        config.annotation = entry.config.search.annotation.clone();
        config.timeout_ms = entry.config.search.timeout_ms;
    }

    /// Inject the neighbour resolver for single queries when requested.
    ///
    /// Expansion implies annotation, so both switches are enabled together;
    /// without the request the annotation config passes through unchanged.
    async fn apply_expansion(
        &self,
        project_id: i64,
        mut query_opts: QueryOptions,
        entry: &cce_config::project_registry::ProjectEntry,
        expand: bool,
    ) -> QueryOptions {
        let mut annotation = query_opts.config.annotation.clone();
        let resolver = self
            .expansion_resolver(project_id, entry, &mut annotation, expand)
            .await;
        query_opts.config.annotation = annotation;
        query_opts.with_neighbour_resolver(resolver)
    }

    /// Build the neighbour resolver when expansion is requested.
    ///
    /// A missing relation searcher degrades to unexpanded search with a
    /// warning instead of failing the query.
    async fn expansion_resolver(
        &self,
        project_id: i64,
        entry: &cce_config::project_registry::ProjectEntry,
        annotation: &mut RelationAnnotationConfig,
        expand: bool,
    ) -> Option<NeighbourResolver> {
        if !expand {
            return None;
        }
        match self.state.get_relation_searcher(project_id).await {
            Ok(searcher) => {
                annotation.enable_annotation = true;
                annotation.expansion_enabled = true;
                Some(NeighbourResolver::new(
                    searcher,
                    std::path::PathBuf::from(&entry.metadata.root_path),
                ))
            }
            Err(error) => {
                tracing::warn!(%error, "Expansion requested but relation searcher unavailable; skipping");
                None
            }
        }
    }

    /// Assemble the per-request query coordinator from engine components.
    async fn build_coordinator(
        &self,
        project_id: i64,
        entry: &cce_config::project_registry::ProjectEntry,
    ) -> Result<QueryCoordinator, SearchServiceError> {
        let searcher = self
            .state
            .engine
            .get_searcher(project_id)
            .await
            .map_err(|error| {
                SearchServiceError::Execution(format!("Failed to get searcher: {error}"))
            })?;
        let relation_searcher = self
            .state
            .get_relation_searcher(project_id)
            .await
            .unwrap_or_else(|_| Arc::new(RelationSearcher::new(Arc::new(CallChainQuery::new()))));
        let searcher = searcher.lock().await.clone();
        // Expose the relation propagation configuration as a capability so
        // query-side consumers can detect when call chains may lack
        // cross-file edges. The relation config is the authoritative source
        // for the propagation depth; the indexer-derived caps report
        // unlimited (0) and must not override a finite configured depth.
        let capabilities = IndexCapabilities::from(&entry.config.orchestrator.indexer)
            .merge(&IndexCapabilities::from(&entry.config.relation))
            .with_relation_propagation_depth(entry.config.relation.max_propagation_depth);
        Ok(QueryCoordinator::with_capabilities(
            Arc::new(searcher),
            relation_searcher,
            capabilities,
            project_id,
        )
        .with_metrics(cce_metrics::QueryMetrics::new(
            self.state.engine.metrics_registry(),
            project_id,
        ))
        // The cache MUST be shared across requests: a fresh coordinator per
        // request with a fresh cache always starts empty, so identical
        // queries re-run the embedding pipeline and queue behind the shared
        // LLM rate limiter. The clone shares the underlying store.
        .with_shared_cache(self.state.engine.get_query_cache(project_id).await))
    }

    /// Convert orchestrator hits to API items, stamping stable symbol IDs
    /// when a relation snapshot is available.
    async fn convert_items(
        &self,
        project_id: i64,
        items: Vec<OrchestratorResultItem>,
    ) -> Vec<SearchResultItem> {
        let relation_searcher = self.state.get_relation_searcher(project_id).await.ok();
        items
            .into_iter()
            .map(|item| Self::convert_item(item, relation_searcher.as_deref()))
            .collect()
    }

    /// Convert one orchestrator hit to one API item.
    fn convert_item(
        item: OrchestratorResultItem,
        relation_searcher: Option<&RelationSearcher>,
    ) -> SearchResultItem {
        let entity_type = (!item.kind.is_empty()).then(|| item.kind.clone());
        let content_state = match item.content_state {
            ContentState::Full => ContentStateDto::Full,
            ContentState::Reference(DowngradeReason::OverLimit) => {
                ContentStateDto::ReferenceOverLimit
            }
            ContentState::Reference(DowngradeReason::FileMissing) => {
                ContentStateDto::ReferenceFileMissing
            }
            ContentState::Reference(DowngradeReason::FileLevel) => {
                ContentStateDto::ReferenceFileLevel
            }
            ContentState::Reference(DowngradeReason::ChunkMissing) => {
                ContentStateDto::ReferenceChunkMissing
            }
        };
        SearchResultItem {
            file_path: item.file_path,
            code_chunk: item.content,
            start_line: item.start_line,
            end_line: item.end_line,
            entity_type,
            entity_names: if item.name.is_empty() {
                Vec::new()
            } else {
                vec![item.name.clone()]
            },
            entity_ids: item.entity_ids.iter().map(|id| id.0 as i64).collect(),
            stable_symbol_ids: relation_searcher
                .map(|searcher| {
                    item.entity_ids
                        .iter()
                        .map(|id| stable_id_of(searcher, *id))
                        .collect()
                })
                .unwrap_or_default(),
            score: item.score,
            sources: item.sources,
            content_state,
            expanded: item.metadata.get("expanded").is_some_and(|v| v == "true"),
            expanded_nodes: metadata_count(&item.metadata, "expanded_nodes"),
            forward_nodes: metadata_count(&item.metadata, "forward_nodes"),
            backward_nodes: metadata_count(&item.metadata, "backward_nodes"),
            omitted_self_evident: metadata_count(&item.metadata, "omitted_self_evident"),
        }
    }

    /// Best-effort relation version for a project, used to stamp responses
    /// so callers can judge freshness. Never fails the query.
    async fn relation_version(&self, project_id: i64) -> (Option<i64>, bool) {
        let runtime = match self.state.engine.get_relation_runtime(project_id).await {
            Ok(runtime) => runtime,
            Err(_) => return (None, false),
        };
        let info = runtime.get_capability_info().await;
        let epoch = (info.relation_epoch > 0).then_some(info.relation_epoch);
        (epoch, info.stale)
    }
}

/// Read a numeric expansion counter from hit metadata, defaulting to zero
/// when the hit went through no expansion pass.
fn metadata_count(metadata: &std::collections::HashMap<String, String>, key: &str) -> usize {
    metadata.get(key).and_then(|v| v.parse().ok()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_sources_parse_each_wire_label() {
        assert!(SearchService::parse_search_sources("vector", "query type").is_ok());
        assert!(SearchService::parse_search_sources("bm25", "query type").is_ok());
        assert!(SearchService::parse_search_sources("hybrid", "query type").is_ok());
        assert!(SearchService::parse_search_sources("summary", "query type").is_ok());
        let error = SearchService::parse_search_sources("nope", "query type")
            .expect_err("unknown query type must fail");
        assert!(matches!(error, SearchServiceError::InvalidInput(_)));
    }

    #[test]
    fn query_intent_empty_means_auto_detect() {
        assert!(
            SearchService::parse_query_intent(None)
                .expect("none intent must parse")
                .is_none()
        );
        assert!(
            SearchService::parse_query_intent(Some(""))
                .expect("empty intent must parse")
                .is_none()
        );
        assert!(
            SearchService::parse_query_intent(Some("semantic"))
                .expect("known intent must parse")
                .is_some()
        );
        assert!(SearchService::parse_query_intent(Some("nope")).is_err());
    }

    #[test]
    fn convert_carries_expansion_counters() {
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("expanded".to_string(), "true".to_string());
        metadata.insert("expanded_nodes".to_string(), "3".to_string());
        metadata.insert("forward_nodes".to_string(), "2".to_string());
        metadata.insert("backward_nodes".to_string(), "1".to_string());
        metadata.insert("omitted_self_evident".to_string(), "1".to_string());
        let item = OrchestratorResultItem {
            content: "fn a() {}".to_string(),
            metadata,
            ..Default::default()
        };
        let converted = SearchService::convert_item(item, None);
        assert!(converted.expanded);
        assert_eq!(converted.expanded_nodes, 3);
        assert_eq!(converted.forward_nodes, 2);
        assert_eq!(converted.backward_nodes, 1);
        assert_eq!(converted.omitted_self_evident, 1);
    }

    #[test]
    fn convert_defaults_counters_without_expansion() {
        let item = OrchestratorResultItem {
            content: "fn a() {}".to_string(),
            ..Default::default()
        };
        let converted = SearchService::convert_item(item, None);
        assert!(!converted.expanded);
        assert_eq!(converted.expanded_nodes, 0);
        assert_eq!(converted.forward_nodes, 0);
        assert_eq!(converted.backward_nodes, 0);
        assert_eq!(converted.omitted_self_evident, 0);
    }
}
