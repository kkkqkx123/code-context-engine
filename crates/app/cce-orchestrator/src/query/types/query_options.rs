//! Query options and search sources types

use cce_types::FileCategory;

use super::search_config::SearchConfig;
use crate::query::retrieval::core::vector::FilterOptions;

/// Search source flags - user-facing options
/// Users only need to care about "which search sources to enable",
/// internal execution strategy is determined automatically by the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SearchSources {
    /// Enable vector semantic search
    pub vector: bool,
    /// Enable BM25 keyword search
    pub bm25: bool,
    /// Enable relation/call chain search
    pub relation: bool,
    /// Enable summary-level search
    pub summary: bool,
}

impl Default for SearchSources {
    fn default() -> Self {
        Self {
            vector: true,
            bm25: true,
            relation: false,
            summary: false,
        }
    }
}

impl SearchSources {
    /// Create a new SearchSources with all sources disabled
    pub fn none() -> Self {
        Self {
            vector: false,
            bm25: false,
            relation: false,
            summary: false,
        }
    }

    /// Enable vector search
    pub fn with_vector(mut self) -> Self {
        self.vector = true;
        self
    }

    /// Enable BM25 search
    pub fn with_bm25(mut self) -> Self {
        self.bm25 = true;
        self
    }

    /// Enable relation search
    pub fn with_relation(mut self) -> Self {
        self.relation = true;
        self
    }

    /// Enable summary search
    pub fn with_summary(mut self) -> Self {
        self.summary = true;
        self
    }

    /// Check if any source is enabled
    pub fn is_empty(&self) -> bool {
        !self.vector && !self.bm25 && !self.relation && !self.summary
    }

    /// Check if any source is enabled (alias for !is_empty)
    pub fn is_any_enabled(&self) -> bool {
        !self.is_empty()
    }

    /// Check if only relation search is enabled
    pub fn is_relation_only(&self) -> bool {
        self.relation && !self.vector && !self.bm25 && !self.summary
    }

    /// Check if only summary search is enabled
    pub fn is_summary_only(&self) -> bool {
        self.summary && !self.vector && !self.bm25 && !self.relation
    }
}

impl std::fmt::Display for SearchSources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();
        if self.vector {
            parts.push("vector");
        }
        if self.bm25 {
            parts.push("bm25");
        }
        if self.relation {
            parts.push("relation");
        }
        if self.summary {
            parts.push("summary");
        }

        if parts.is_empty() {
            write!(f, "none")
        } else {
            write!(f, "{}", parts.join("+"))
        }
    }
}

/// Content types that can be excluded from search results
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExcludableContentType {
    /// Test files (detected by path patterns like test/, spec/, __tests__, etc.)
    Test,
}

/// Query intent: describes the type of user query for determining fusion weights.
///
/// Different query types benefit from different vector/BM25 weight ratios:
/// - Semantic: natural language queries benefit from higher vector weight
/// - Keyword: precise code terms benefit from higher BM25 weight
/// - Hybrid: mixed queries use balanced weights
/// - Entity: code symbol lookups lean toward vector + relation
///
/// The intent must be explicitly configured by the caller (via `QueryConfigBuilder::with_query_intent`).
/// When `None`, the system defaults to `Hybrid` (balanced) weights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryIntent {
    /// Semantic/natural language query (e.g., "how does X work")
    /// Higher vector weight, lower BM25 weight
    Semantic,
    /// Precise keyword query (e.g., "fn parse_query term")
    /// Higher BM25 weight, lower vector weight
    Keyword,
    /// Mixed query (natural language + keywords)
    /// Balanced weights
    Hybrid,
    /// Entity/code symbol lookup (function name, class name, etc.)
    /// Vector-leaning with potential relation expansion
    Entity,
}

impl std::fmt::Display for QueryIntent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryIntent::Semantic => write!(f, "semantic"),
            QueryIntent::Keyword => write!(f, "keyword"),
            QueryIntent::Hybrid => write!(f, "hybrid"),
            QueryIntent::Entity => write!(f, "entity"),
        }
    }
}

/// Unified query options
#[derive(Debug, Clone, Default)]
pub struct QueryOptions {
    /// Query text
    pub query: String,
    /// Project ID for query scoping
    pub project_id: i64,
    /// Which search sources to enable
    pub sources: SearchSources,
    /// Search configuration
    pub config: SearchConfig,
    /// Query intent override (None defaults to Hybrid)
    pub query_intent: Option<QueryIntent>,
    /// Directory prefix filter
    pub directory_prefix: Option<String>,
    /// Content types to exclude (e.g., test files, generated code)
    pub exclude_content_types: Vec<ExcludableContentType>,
    /// Include only specific categories
    pub include_categories: Vec<FileCategory>,
    /// Exclude specific categories
    pub exclude_categories: Vec<FileCategory>,
    /// Exclude patterns (Glob)
    pub exclude_patterns: Vec<String>,
    /// Include patterns (Glob)
    pub include_patterns: Vec<String>,
    /// Include source code in results
    pub with_source: bool,
    /// Per-request rerank override.
    ///
    /// `None` defers to the config layer: reranking runs whenever the
    /// rerank handler / plugins are available. `Some(true)` forces it on,
    /// `Some(false)` forces it off for this query.
    pub enable_rerank: Option<bool>,
}

impl QueryOptions {
    /// Create options with query text and project ID
    pub fn new(query: impl Into<String>, project_id: i64) -> Self {
        assert!(project_id > 0, "project_id must be positive");
        Self {
            query: query.into(),
            project_id,
            sources: SearchSources::default(),
            config: SearchConfig::default(),
            query_intent: None,
            directory_prefix: None,
            exclude_content_types: Vec::new(),
            include_categories: Vec::new(),
            exclude_categories: Vec::new(),
            exclude_patterns: Vec::new(),
            include_patterns: Vec::new(),
            with_source: true,
            enable_rerank: None,
        }
    }

    /// Create project-scoped query
    pub fn for_project(query: impl Into<String>, project_id: i64) -> Self {
        Self::new(query, project_id)
    }

    /// Set search sources
    pub fn with_sources(mut self, sources: SearchSources) -> Self {
        self.sources = sources;
        self
    }

    /// Set query intent override (None means auto-detect)
    pub fn with_query_intent(mut self, intent: Option<QueryIntent>) -> Self {
        self.query_intent = intent;
        self
    }

    /// Set project ID
    pub fn with_project_id(mut self, project_id: i64) -> Self {
        assert!(project_id > 0, "project_id must be positive");
        self.project_id = project_id;
        self
    }

    /// Set search configuration
    pub fn with_config(mut self, config: SearchConfig) -> Self {
        self.config = config;
        self
    }

    /// Set result limit
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.config.result.limit = limit;
        self
    }

    /// Enable relation search
    pub fn with_relations(mut self) -> Self {
        self.sources.relation = true;
        self
    }

    /// Set directory prefix filter
    pub fn with_directory_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.directory_prefix = Some(prefix.into());
        self
    }

    /// Add content type to exclude
    pub fn add_exclude_content_type(mut self, content_type: ExcludableContentType) -> Self {
        self.exclude_content_types.push(content_type);
        self
    }

    /// Exclude test files
    pub fn exclude_tests(mut self) -> Self {
        self.exclude_content_types.push(ExcludableContentType::Test);
        self
    }

    /// Set include categories (only results whose category matches are kept)
    pub fn with_include_categories(mut self, categories: Vec<FileCategory>) -> Self {
        self.include_categories = categories;
        self
    }

    /// Set exclude categories (results whose category matches are dropped)
    pub fn with_exclude_categories(mut self, categories: Vec<FileCategory>) -> Self {
        self.exclude_categories = categories;
        self
    }

    /// Set exclude patterns
    pub fn with_exclude_patterns(mut self, patterns: Vec<String>) -> Self {
        self.exclude_patterns = patterns;
        self
    }

    /// Set include patterns
    pub fn with_include_patterns(mut self, patterns: Vec<String>) -> Self {
        self.include_patterns = patterns;
        self
    }

    /// Set whether to include source code
    pub fn with_source(mut self, with_source: bool) -> Self {
        self.with_source = with_source;
        self
    }

    /// Set a per-request rerank override (takes precedence over the
    /// config-level enablement).
    pub fn with_enable_rerank(mut self, enable: bool) -> Self {
        self.enable_rerank = Some(enable);
        self
    }

    /// Get the execution strategy based on current options
    pub fn execution_strategy(&self) -> super::execution_strategy::ExecutionStrategy {
        super::execution_strategy::ExecutionStrategy::from_options(self)
    }

    /// Convert to filter options for retrieval
    pub fn to_filter_options(&self) -> FilterOptions {
        // Note: FilterOptions doesn't support exclude/include patterns yet.
        // These are handled at a higher level in the search pipeline.
        FilterOptions {
            directory_prefix: self.directory_prefix.clone(),
            exclude_content_types: self.exclude_content_types.clone(),
            include_categories: self.include_categories.clone(),
            exclude_categories: self.exclude_categories.clone(),
        }
    }
}

/// Builder for creating QueryOptions with fluent API
#[derive(Debug, Clone)]
pub struct QueryConfigBuilder {
    project_id: i64,
    sources: SearchSources,
    config: SearchConfig,
    query_intent: Option<QueryIntent>,
    directory_prefix: Option<String>,
    exclude_content_types: Vec<ExcludableContentType>,
    include_categories: Vec<FileCategory>,
    exclude_categories: Vec<FileCategory>,
    exclude_patterns: Vec<String>,
    include_patterns: Vec<String>,
    with_source: bool,
    enable_rerank: Option<bool>,
}

impl QueryConfigBuilder {
    /// Create a new builder for a specific project
    pub fn new(project_id: i64) -> Self {
        assert!(project_id > 0, "project_id must be positive");
        Self {
            project_id,
            sources: SearchSources::default(),
            config: SearchConfig::default(),
            query_intent: None,
            directory_prefix: None,
            exclude_content_types: Vec::new(),
            include_categories: Vec::new(),
            exclude_categories: Vec::new(),
            exclude_patterns: Vec::new(),
            include_patterns: Vec::new(),
            with_source: true,
            enable_rerank: None,
        }
    }

    /// Create a builder for a specific project (alias for new())
    pub fn for_project(project_id: i64) -> Self {
        Self::new(project_id)
    }

    /// Set project ID
    pub fn project_id(mut self, project_id: i64) -> Self {
        self.project_id = project_id;
        self
    }

    /// Set search sources
    pub fn sources(mut self, sources: SearchSources) -> Self {
        self.sources = sources;
        self
    }

    /// Set query intent override (None means auto-detect)
    pub fn with_query_intent(mut self, intent: Option<QueryIntent>) -> Self {
        self.query_intent = intent;
        self
    }

    /// Enable/disable vector search
    pub fn vector_enabled(mut self, enabled: bool) -> Self {
        self.sources.vector = enabled;
        self
    }

    /// Enable/disable BM25 search
    pub fn bm25_enabled(mut self, enabled: bool) -> Self {
        self.sources.bm25 = enabled;
        self
    }

    /// Enable/disable relation search
    pub fn relation_enabled(mut self, enabled: bool) -> Self {
        self.sources.relation = enabled;
        self
    }

    /// Enable/disable summary search
    pub fn summary_enabled(mut self, enabled: bool) -> Self {
        self.sources.summary = enabled;
        self
    }

    /// Set vector top-k
    pub fn vector_top_k(mut self, k: usize) -> Self {
        self.config.vector.top_k = k;
        self
    }

    /// Set vector minimum score
    pub fn vector_min_score(mut self, score: f32) -> Self {
        self.config.vector.min_score = score;
        self
    }

    /// Set result limit
    pub fn result_limit(mut self, limit: usize) -> Self {
        self.config.result.limit = limit;
        self
    }

    /// Set result minimum score
    pub fn result_min_score(mut self, score: f32) -> Self {
        self.config.result.min_score = score;
        self
    }

    /// Set max results per file
    pub fn max_per_file(mut self, max: usize) -> Self {
        self.config.result.max_per_file = max;
        self
    }

    /// Enable SPSR-Graph assembly
    pub fn with_assembly(mut self, depth: usize) -> Self {
        self.config.spsr_graph.enable_assembly = true;
        self.config.spsr_graph.max_expansion_depth = depth;
        self
    }

    /// Enable relation expansion search at specified depth
    pub fn with_relation_expansion(mut self, depth: usize) -> Self {
        self.sources.relation = true;
        self.config.relation.depth = depth;
        self
    }

    /// Set assembly expansion strategy
    pub fn assembly_strategy(
        mut self,
        strategy: crate::query::assembly::ExpansionStrategy,
    ) -> Self {
        self.config.spsr_graph.expansion_strategy = strategy;
        self
    }

    /// Enable summary pre-filter
    pub fn with_summary_pre_filter(mut self) -> Self {
        self.config.summary.enable_pre_filter = true;
        self
    }

    /// Set summary top-k
    pub fn summary_top_k(mut self, k: usize) -> Self {
        self.config.summary.top_k = k;
        self
    }

    /// Set summary minimum score
    pub fn summary_min_score(mut self, score: f32) -> Self {
        self.config.summary.min_score = score;
        self
    }

    /// Set summary boost factor
    pub fn summary_boost_factor(mut self, factor: f32) -> Self {
        self.config.summary.boost_factor = factor;
        self
    }

    /// Enable reranking as an explicit per-query override
    pub fn with_reranking(mut self, model: impl Into<String>) -> Self {
        self.config.rerank.model = model.into();
        self.enable_rerank = Some(true);
        self
    }

    /// Set the rerank enable override (takes precedence over the
    /// config-level enablement)
    pub fn rerank_enabled(mut self, enabled: bool) -> Self {
        self.enable_rerank = Some(enabled);
        self
    }

    /// Set rerank max candidates
    pub fn rerank_max_candidates(mut self, max: usize) -> Self {
        self.config.rerank.max_candidates = max;
        self
    }

    /// Set directory prefix
    pub fn directory_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.directory_prefix = Some(prefix.into());
        self
    }

    /// Set BM25 minimum score threshold
    pub fn bm25_min_score(mut self, score: f32) -> Self {
        self.config.bm25.min_score = score;
        self
    }

    /// Set HNSW ef parameter
    pub fn hnsw_ef(mut self, ef: u32) -> Self {
        self.config.vector.hnsw_ef = ef;
        self
    }

    /// Add exclude content type
    pub fn add_exclude_content_type(mut self, content_type: ExcludableContentType) -> Self {
        self.exclude_content_types.push(content_type);
        self
    }

    /// Exclude test files
    pub fn exclude_tests(mut self) -> Self {
        self.exclude_content_types.push(ExcludableContentType::Test);
        self
    }

    /// Set exclude patterns
    pub fn exclude_patterns(mut self, patterns: Vec<String>) -> Self {
        self.exclude_patterns = patterns;
        self
    }

    /// Set include patterns
    pub fn include_patterns(mut self, patterns: Vec<String>) -> Self {
        self.include_patterns = patterns;
        self
    }

    /// Set include categories
    pub fn include_categories(mut self, categories: Vec<FileCategory>) -> Self {
        self.include_categories = categories;
        self
    }

    /// Set exclude categories
    pub fn exclude_categories(mut self, categories: Vec<FileCategory>) -> Self {
        self.exclude_categories = categories;
        self
    }

    /// Set with_source flag
    pub fn with_source(mut self, with_source: bool) -> Self {
        self.with_source = with_source;
        self
    }

    /// Build QueryOptions with the given query text
    pub fn build(self, query: impl Into<String>) -> QueryOptions {
        QueryOptions {
            query: query.into(),
            project_id: self.project_id,
            sources: self.sources,
            config: self.config,
            query_intent: self.query_intent,
            directory_prefix: self.directory_prefix,
            exclude_content_types: self.exclude_content_types,
            include_categories: self.include_categories,
            exclude_categories: self.exclude_categories,
            exclude_patterns: self.exclude_patterns,
            include_patterns: self.include_patterns,
            with_source: self.with_source,
            enable_rerank: self.enable_rerank,
        }
    }

    /// Fast search preset: quick results with minimal processing
    pub fn fast_search(project_id: i64, query: impl Into<String>) -> QueryOptions {
        QueryConfigBuilder::new(project_id)
            .vector_top_k(20)
            .bm25_enabled(false)
            .result_limit(5)
            .build(query)
    }

    /// Precise search preset: comprehensive search with high recall
    pub fn precise_search(project_id: i64, query: impl Into<String>) -> QueryOptions {
        QueryConfigBuilder::new(project_id)
            .vector_top_k(100)
            .bm25_enabled(true)
            .result_limit(15)
            .build(query)
    }

    /// Code exploration preset: with assembly for context-rich results
    pub fn explore_code(project_id: i64, query: impl Into<String>) -> QueryOptions {
        QueryConfigBuilder::new(project_id)
            .with_assembly(2)
            .result_limit(10)
            .build(query)
    }

    /// Summary-based search: hierarchical file filtering
    ///
    /// Uses summary index to pre-filter files before detailed search.
    pub fn summary_search(project_id: i64, query: impl Into<String>) -> QueryOptions {
        QueryConfigBuilder::new(project_id)
            .with_summary_pre_filter()
            .result_limit(20)
            .build(query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_sources_default() {
        let sources = SearchSources::default();
        assert!(sources.vector);
        assert!(sources.bm25);
        assert!(!sources.relation);
        assert!(!sources.summary);
    }

    #[test]
    fn test_search_sources_builder() {
        let sources = SearchSources::none().with_vector().with_relation();
        assert!(sources.vector);
        assert!(!sources.bm25);
        assert!(sources.relation);
        assert!(!sources.summary);
    }

    #[test]
    fn test_search_sources_display() {
        let sources = SearchSources::default();
        assert_eq!(format!("{}", sources), "vector+bm25");

        let sources = SearchSources::none().with_vector();
        assert_eq!(format!("{}", sources), "vector");

        let sources = SearchSources::none();
        assert_eq!(format!("{}", sources), "none");
    }

    #[test]
    fn test_query_options_builder() {
        let options = QueryConfigBuilder::new(1)
            .build("test query")
            .with_sources(SearchSources::default())
            .with_limit(20)
            .with_relations();

        assert_eq!(options.query, "test query");
        assert_eq!(options.project_id, 1);
        assert!(options.sources.vector);
        assert!(options.sources.bm25);
        assert!(options.sources.relation);
        assert_eq!(options.config.result.limit, 20);
    }

    #[test]
    fn test_query_config_builder_basic() {
        let options = QueryConfigBuilder::new(2)
            .vector_top_k(30)
            .bm25_enabled(true)
            .result_limit(8)
            .build("test query");

        assert_eq!(options.query, "test query");
        assert_eq!(options.project_id, 2);
        assert_eq!(options.config.vector.top_k, 30);
        assert!(options.sources.bm25);
        assert_eq!(options.config.result.limit, 8);
    }

    #[test]
    fn test_query_config_builder_fast_search() {
        let options = QueryConfigBuilder::fast_search(1, "quick search");

        assert_eq!(options.query, "quick search");
        assert_eq!(options.project_id, 1);
        assert!(!options.sources.bm25); // BM25 disabled for fast search
        assert_eq!(options.config.result.limit, 5);
        assert_eq!(options.config.vector.top_k, 20);
    }

    #[test]
    fn test_query_config_builder_precise_search() {
        let options = QueryConfigBuilder::precise_search(3, "detailed search");

        assert_eq!(options.query, "detailed search");
        assert_eq!(options.project_id, 3);
        assert!(options.sources.bm25);
        assert_eq!(options.config.result.limit, 15);
        assert_eq!(options.config.vector.top_k, 100);
    }

    #[test]
    fn test_query_config_builder_with_assembly() {
        let options = QueryConfigBuilder::new(1)
            .with_assembly(2)
            .build("explore code");

        assert!(options.config.spsr_graph.enable_assembly);
        assert_eq!(options.config.spsr_graph.max_expansion_depth, 2);
    }

    #[test]
    fn test_excludable_content_type_serialization() {
        // Test serialization of ExcludableContentType
        let test_type = ExcludableContentType::Test;
        let json = serde_json::to_string(&test_type).unwrap();
        assert_eq!(json, "\"test\"");
    }

    #[test]
    fn test_excludable_content_type_deserialization() {
        // Test deserialization of ExcludableContentType
        let test_type: ExcludableContentType = serde_json::from_str("\"test\"").unwrap();
        assert_eq!(test_type, ExcludableContentType::Test);
    }

    #[test]
    fn test_query_options_exclude_content_types() {
        // Test that exclude_content_types can be set in QueryOptions
        let options = QueryConfigBuilder::new(1)
            .build("test query")
            .exclude_tests()
            .add_exclude_content_type(ExcludableContentType::Test);

        assert_eq!(options.exclude_content_types.len(), 2);
        assert!(
            options
                .exclude_content_types
                .contains(&ExcludableContentType::Test)
        );
    }

    #[test]
    fn test_query_options_default_exclude_empty() {
        // Test that default QueryOptions has empty exclude_content_types
        let options = QueryConfigBuilder::new(1).build("test query");
        assert!(options.exclude_content_types.is_empty());
    }

    #[test]
    fn test_query_intent_display() {
        assert_eq!(format!("{}", QueryIntent::Semantic), "semantic");
        assert_eq!(format!("{}", QueryIntent::Keyword), "keyword");
        assert_eq!(format!("{}", QueryIntent::Hybrid), "hybrid");
        assert_eq!(format!("{}", QueryIntent::Entity), "entity");
    }

    #[test]
    fn test_query_options_include_exclude_categories() {
        let options = QueryConfigBuilder::new(1)
            .build("test query")
            .with_include_categories(vec![FileCategory::Config, FileCategory::Schema])
            .with_exclude_categories(vec![FileCategory::Code]);

        assert_eq!(
            options.include_categories,
            vec![FileCategory::Config, FileCategory::Schema]
        );
        assert_eq!(options.exclude_categories, vec![FileCategory::Code]);

        let filter_options = options.to_filter_options();
        assert_eq!(filter_options.include_categories.len(), 2);
        assert_eq!(filter_options.exclude_categories.len(), 1);
    }
}
