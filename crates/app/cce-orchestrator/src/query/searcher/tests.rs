//! Unified searcher implementation
//!
//! Provides a unified search interface that combines vector retrieval,
//! BM25 enhancement, and result ranking into a cohesive search flow.
//!
//! # Architecture
//!
//! The searcher delegates to specialized components:
//! - ResultProcessor: Ranking, filtering, and threshold application
//! - AssemblyHandler: SPSR-Graph assembly operations

use crate::query::error::QueryError;
use crate::query::types::SearchResult;

use super::post_processing::{
    apply_result_filter_chain, merge_fusion_weights_override, query_fusion_weights_from_plugins,
    rewrite_query_via_plugins,
};

use super::*;
use cce_types::EntityId;

fn make_result(id: &str, entity_ids: Vec<u64>, score: f32) -> SearchResult {
    SearchResult {
        id: id.to_string(),
        entity_ids: entity_ids.iter().map(|&id| EntityId(id)).collect(),
        score,
        original_score: score,
        vector_score: score,
        sources: vec!["vector".to_string()],
        ..Default::default()
    }
}

#[test]
fn test_expand_multi_entity_results_single_entity() {
    let results = vec![make_result("chunk_1", vec![100], 0.9)];
    let expanded = expand_multi_entity_results(results);
    assert_eq!(expanded.len(), 1);
    assert_eq!(expanded[0].entity_ids, vec![EntityId(100)]);
}

#[test]
fn test_expand_multi_entity_results_multiple_entities() {
    let results = vec![make_result("chunk_1", vec![100, 200, 300], 0.9)];
    let expanded = expand_multi_entity_results(results);
    assert_eq!(expanded.len(), 3);

    let entity_ids: Vec<u64> = expanded
        .iter()
        .filter_map(|r| r.entity_ids.first())
        .map(|e| e.0)
        .collect();
    assert!(entity_ids.contains(&100));
    assert!(entity_ids.contains(&200));
    assert!(entity_ids.contains(&300));

    // All expanded results should have the same score
    for r in &expanded {
        assert!((r.score - 0.9).abs() < 0.001);
    }
}

#[test]
fn test_expand_multi_entity_results_mixed() {
    let results = vec![
        make_result("chunk_1", vec![100], 0.9),
        make_result("chunk_2", vec![200, 300], 0.8),
        make_result("chunk_3", vec![], 0.7),
    ];
    let expanded = expand_multi_entity_results(results);
    // chunk_1 → 1 result, chunk_2 → 2 results, chunk_3 → 1 result (no expansion)
    assert_eq!(expanded.len(), 4);
}

#[test]
fn test_expand_multi_entity_results_preserves_other_fields() {
    let mut result = make_result("chunk_1", vec![100, 200], 0.85);
    result.file_path = "src/main.rs".to_string();
    result.start_line = 10;
    result.end_line = 20;

    let expanded = expand_multi_entity_results(vec![result]);
    for r in &expanded {
        assert_eq!(r.file_path, "src/main.rs");
        assert_eq!(r.start_line, 10);
        assert_eq!(r.end_line, 20);
        assert_eq!(r.entity_ids.len(), 1);
    }
}

fn fusion_config_with(vector: f32, bm25: f32) -> crate::query::retrieval::HybridFusionConfig {
    crate::query::retrieval::HybridFusionConfig {
        vector_weight: vector,
        bm25_weight: bm25,
        ..Default::default()
    }
}

fn fusion_weights(vector: Option<f32>, bm25: Option<f32>) -> cce_types::plugin::FusionWeights {
    cce_types::plugin::FusionWeights {
        vector_weight: vector,
        bm25_weight: bm25,
        min_score: None,
    }
}

#[test]
fn test_merge_fusion_weights_first_non_none_wins() {
    let config = fusion_config_with(0.5, 0.5);
    let merged = merge_fusion_weights_override(
        config,
        vec![
            None,
            Some(fusion_weights(Some(0.3), Some(0.7))),
            // A lower-priority plugin must NOT override the first winner.
            Some(fusion_weights(Some(0.9), Some(0.1))),
        ],
    );
    assert!((merged.vector_weight - 0.3).abs() < 1e-6);
    assert!((merged.bm25_weight - 0.7).abs() < 1e-6);
}

#[test]
fn test_merge_fusion_weights_all_declined_keeps_default() {
    let config = fusion_config_with(0.4, 0.6);
    let merged = merge_fusion_weights_override(config, vec![None, None]);
    assert!((merged.vector_weight - 0.4).abs() < 1e-6);
    assert!((merged.bm25_weight - 0.6).abs() < 1e-6);
}

#[test]
fn test_merge_fusion_weights_partial_fields_keep_defaults() {
    let config = fusion_config_with(0.5, 0.5);
    let merged = merge_fusion_weights_override(config, vec![Some(fusion_weights(Some(1.7), None))]);
    // Out-of-range weight is rejected; valid fields apply; bm25 keeps default.
    assert!((merged.vector_weight - 0.5).abs() < 1e-6);
    assert!((merged.bm25_weight - 0.5).abs() < 1e-6);
}

fn project_meta_table(conn: &rusqlite::Connection) {
    conn.execute_batch(
        "CREATE TABLE project_meta (
                project_id INTEGER NOT NULL,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                PRIMARY KEY (project_id, key)
             );",
    )
    .expect("create project_meta");
}

#[test]
fn test_read_legacy_active_epoch_missing_row_defaults_to_zero() {
    let conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
    project_meta_table(&conn);
    let epoch = crate::query::filter::read_legacy_active_epoch(&conn, 7).expect("read epoch");
    assert_eq!(epoch, 0);
}

#[test]
fn test_read_legacy_active_epoch_reads_stored_value() {
    let conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
    project_meta_table(&conn);
    conn.execute(
        "INSERT INTO project_meta (project_id, key, value) VALUES (?1, 'active_epoch', '5')",
        rusqlite::params![7],
    )
    .expect("insert active_epoch");
    let epoch = crate::query::filter::read_legacy_active_epoch(&conn, 7).expect("read epoch");
    assert_eq!(epoch, 5);
}

#[test]
fn test_read_legacy_active_epoch_unparseable_value_is_error() {
    let conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
    project_meta_table(&conn);
    conn.execute(
        "INSERT INTO project_meta (project_id, key, value) VALUES (?1, 'active_epoch', 'abc')",
        rusqlite::params![7],
    )
    .expect("insert active_epoch");
    let err = crate::query::filter::read_legacy_active_epoch(&conn, 7)
        .expect_err("unparseable value must fail");
    assert!(matches!(err, QueryError::Storage(_)));
    assert!(!err.is_retryable());
    assert!(!err.is_config_error());
}

#[test]
fn test_read_legacy_active_epoch_missing_table_is_error_not_silent() {
    let conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
    let err = crate::query::filter::read_legacy_active_epoch(&conn, 7)
        .expect_err("missing table must fail");
    assert!(matches!(err, QueryError::Storage(_)));
}

#[test]
fn test_read_legacy_active_epoch_ignores_other_projects() {
    let conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
    project_meta_table(&conn);
    conn.execute(
        "INSERT INTO project_meta (project_id, key, value) VALUES (?1, 'active_epoch', '9')",
        rusqlite::params![7],
    )
    .expect("insert active_epoch");
    let epoch = crate::query::filter::read_legacy_active_epoch(&conn, 42).expect("read epoch");
    assert_eq!(epoch, 0);
}

// ── Query-side plugin hooks (QueryRewrite / Fusion / ResultFilter) ──

use cce_plugin::{CodePlugin, PluginBundle, PluginError, PluginMetadata, PluginRegistry};
use cce_types::plugin::{FusionWeights, QueryRewriteResult, ResultFilterEntry};

type RewriteFn = fn(&str) -> std::result::Result<Option<QueryRewriteResult>, PluginError>;
type FusionFn = fn(&str, usize, usize) -> std::result::Result<Option<FusionWeights>, PluginError>;
type ResultFilterFn = fn(
    &str,
    Vec<cce_types::RerankCandidate>,
) -> std::result::Result<Option<Vec<ResultFilterEntry>>, PluginError>;

/// Configurable `CodePlugin` test double for the query-side hooks.
struct QueryMockPlugin {
    meta: PluginMetadata,
    rewrite: Option<RewriteFn>,
    fusion: Option<FusionFn>,
    filter: Option<ResultFilterFn>,
}

impl QueryMockPlugin {
    fn with_id(id: &str, priority: i32) -> Self {
        Self {
            meta: PluginMetadata {
                id: id.to_string(),
                name: id.to_string(),
                version: "0.1.0".to_string(),
                priority,
                capabilities: Vec::new(),
                capability_priorities: std::collections::HashMap::new(),
                description: None,
            },
            rewrite: None,
            fusion: None,
            filter: None,
        }
    }

    fn rewrites(mut self, f: RewriteFn) -> Self {
        self.rewrite = Some(f);
        self
    }

    fn fusions(mut self, f: FusionFn) -> Self {
        self.fusion = Some(f);
        self
    }

    fn filters(mut self, f: ResultFilterFn) -> Self {
        self.filter = Some(f);
        self
    }
}

impl CodePlugin for QueryMockPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.meta
    }
    fn supports_query_rewrite(&self) -> bool {
        self.rewrite.is_some()
    }
    fn supports_fusion(&self) -> bool {
        self.fusion.is_some()
    }
    fn supports_result_filter(&self) -> bool {
        self.filter.is_some()
    }
    fn rewrite_query(
        &self,
        query: &str,
    ) -> std::result::Result<Option<QueryRewriteResult>, PluginError> {
        match self.rewrite {
            Some(f) => f(query),
            None => Ok(None),
        }
    }
    fn fusion_weights(
        &self,
        query: &str,
        vector_count: usize,
        bm25_count: usize,
    ) -> std::result::Result<Option<FusionWeights>, PluginError> {
        match self.fusion {
            Some(f) => f(query, vector_count, bm25_count),
            None => Ok(None),
        }
    }
    fn filter_results(
        &self,
        query: &str,
        results: Vec<cce_types::RerankCandidate>,
    ) -> std::result::Result<Option<Vec<ResultFilterEntry>>, PluginError> {
        match self.filter {
            Some(f) => f(query, results),
            None => Ok(None),
        }
    }
}

fn query_register(registry: &mut PluginRegistry, plugin: QueryMockPlugin) {
    registry.register_bundle(PluginBundle::new(std::sync::Arc::new(plugin)));
}

/// Result with a single (dummy) entity id, for plugin-hook tests.
fn result(id: &str, score: f32) -> SearchResult {
    SearchResult {
        id: id.to_string(),
        entity_ids: vec![EntityId(1)],
        score,
        original_score: score,
        vector_score: score,
        sources: vec!["vector".to_string()],
        ..Default::default()
    }
}

// ── QueryRewrite chain ──

#[tokio::test]
async fn test_query_rewrite_chain_rewrites_and_accumulates_expansions() {
    fn add_suffix(query: &str) -> std::result::Result<Option<QueryRewriteResult>, PluginError> {
        Ok(Some(QueryRewriteResult {
            rewritten_query: format!("{query} rust"),
            expansion_terms: vec!["cargo".to_string()],
        }))
    }
    fn add_more(query: &str) -> std::result::Result<Option<QueryRewriteResult>, PluginError> {
        Ok(Some(QueryRewriteResult {
            rewritten_query: format!("{query} async"),
            expansion_terms: vec!["tokio".to_string(), "async".to_string()],
        }))
    }
    let mut registry = PluginRegistry::new();
    query_register(
        &mut registry,
        QueryMockPlugin::with_id("a", 100).rewrites(add_suffix),
    );
    query_register(
        &mut registry,
        QueryMockPlugin::with_id("b", 10).rewrites(add_more),
    );

    let rewritten = rewrite_query_via_plugins(&registry, "io").await;
    // Chain semantics: plugin A's output feeds plugin B.
    assert!(rewritten.contains("io rust async"));
    // Expansion terms from both plugins accumulate (dedup "async").
    assert!(rewritten.contains("(cargo OR tokio OR async)"));
    assert!(rewritten.contains("io rust async (cargo OR tokio OR async)"));
}

#[tokio::test]
async fn test_query_rewrite_failure_keeps_previous_query() {
    fn fail(_query: &str) -> std::result::Result<Option<QueryRewriteResult>, PluginError> {
        Err(PluginError::ExecutionFailed("broken".to_string()))
    }
    fn add_suffix(query: &str) -> std::result::Result<Option<QueryRewriteResult>, PluginError> {
        Ok(Some(QueryRewriteResult {
            rewritten_query: format!("{query} ok"),
            expansion_terms: vec![],
        }))
    }
    let mut registry = PluginRegistry::new();
    // Higher priority rewrites successfully; the failure comes second and
    // must not clobber the previous rewrite.
    query_register(
        &mut registry,
        QueryMockPlugin::with_id("ok", 100).rewrites(add_suffix),
    );
    query_register(
        &mut registry,
        QueryMockPlugin::with_id("broken", 10).rewrites(fail),
    );

    let rewritten = rewrite_query_via_plugins(&registry, "base").await;
    assert_eq!(rewritten, "base ok");
}

#[tokio::test]
async fn test_query_rewrite_decline_and_empty_keep_query() {
    fn decline(_query: &str) -> std::result::Result<Option<QueryRewriteResult>, PluginError> {
        Ok(None)
    }
    let mut registry = PluginRegistry::new();
    query_register(
        &mut registry,
        QueryMockPlugin::with_id("decline", 10).rewrites(decline),
    );

    let rewritten = rewrite_query_via_plugins(&registry, "original").await;
    assert_eq!(rewritten, "original");
}

#[tokio::test]
async fn test_query_rewrite_skips_plugins_not_matching_capability() {
    // Plugin registered without the QueryRewrite capability is not consulted.
    let registry = PluginRegistry::new();
    let rewritten = rewrite_query_via_plugins(&registry, "original").await;
    assert_eq!(rewritten, "original");
}

// ── Fusion override ──

#[tokio::test]
async fn test_fusion_override_first_non_none_plugin_wins() {
    fn heavy_vector(
        _query: &str,
        _v: usize,
        _b: usize,
    ) -> std::result::Result<Option<FusionWeights>, PluginError> {
        Ok(Some(FusionWeights {
            vector_weight: Some(0.8),
            bm25_weight: Some(0.2),
            min_score: None,
        }))
    }
    fn heavy_bm25(
        _query: &str,
        _v: usize,
        _b: usize,
    ) -> std::result::Result<Option<FusionWeights>, PluginError> {
        Ok(Some(FusionWeights {
            vector_weight: Some(0.1),
            bm25_weight: Some(0.9),
            min_score: None,
        }))
    }
    let mut registry = PluginRegistry::new();
    query_register(
        &mut registry,
        QueryMockPlugin::with_id("vec", 100).fusions(heavy_vector),
    );
    query_register(
        &mut registry,
        QueryMockPlugin::with_id("bm25", 10).fusions(heavy_bm25),
    );

    let config = fusion_config_with(0.5, 0.5);
    let merged = merge_fusion_weights_override(
        config,
        query_fusion_weights_from_plugins(&registry, "q", 10, 5).await,
    );
    assert!((merged.vector_weight - 0.8).abs() < 1e-6);
    assert!((merged.bm25_weight - 0.2).abs() < 1e-6);
}

#[tokio::test]
async fn test_fusion_override_failure_and_decline_keep_default() {
    fn fail(
        _query: &str,
        _v: usize,
        _b: usize,
    ) -> std::result::Result<Option<FusionWeights>, PluginError> {
        Err(PluginError::ExecutionFailed("broken".to_string()))
    }
    let mut registry = PluginRegistry::new();
    query_register(
        &mut registry,
        QueryMockPlugin::with_id("broken", 100).fusions(fail),
    );

    let config = fusion_config_with(0.4, 0.6);
    let merged = merge_fusion_weights_override(
        config,
        query_fusion_weights_from_plugins(&registry, "q", 10, 5).await,
    );
    assert!((merged.vector_weight - 0.4).abs() < 1e-6);
    assert!((merged.bm25_weight - 0.6).abs() < 1e-6);
}

// ── ResultFilter chain ──

#[tokio::test]
async fn test_result_filter_removes_and_boosts() {
    fn filter_noise(
        _query: &str,
        results: Vec<cce_types::RerankCandidate>,
    ) -> std::result::Result<Option<Vec<ResultFilterEntry>>, PluginError> {
        Ok(Some(
            results
                .iter()
                .map(|r| ResultFilterEntry {
                    id: r.id.clone(),
                    remove: r.id == "noise",
                    boost: if r.id == "boosted" { Some(0.2) } else { None },
                    note: None,
                })
                .collect(),
        ))
    }
    let mut registry = PluginRegistry::new();
    query_register(
        &mut registry,
        QueryMockPlugin::with_id("filter", 10).filters(filter_noise),
    );

    let results = vec![
        result("noise", 0.8),
        result("boosted", 0.5),
        result("keep", 0.4),
    ];
    let filtered = apply_result_filter_chain(&registry, &results, "q").await;
    let ids: Vec<&str> = filtered.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["boosted", "keep"], "removed entry must be gone");
    let boosted = filtered
        .iter()
        .find(|r| r.id == "boosted")
        .expect("boosted present");
    assert!((boosted.score - 0.7).abs() < 1e-6, "boost must be additive");
}

#[tokio::test]
async fn test_result_filter_chain_applies_sequentially() {
    fn remove_a(
        _query: &str,
        results: Vec<cce_types::RerankCandidate>,
    ) -> std::result::Result<Option<Vec<ResultFilterEntry>>, PluginError> {
        Ok(Some(
            results
                .iter()
                .map(|r| ResultFilterEntry {
                    id: r.id.clone(),
                    remove: r.id == "a",
                    boost: None,
                    note: None,
                })
                .collect(),
        ))
    }
    fn boost_remaining(
        _query: &str,
        results: Vec<cce_types::RerankCandidate>,
    ) -> std::result::Result<Option<Vec<ResultFilterEntry>>, PluginError> {
        Ok(Some(
            results
                .iter()
                .map(|r| ResultFilterEntry {
                    id: r.id.clone(),
                    remove: false,
                    boost: Some(0.1),
                    note: None,
                })
                .collect(),
        ))
    }
    let mut registry = PluginRegistry::new();
    query_register(
        &mut registry,
        QueryMockPlugin::with_id("remove", 100).filters(remove_a),
    );
    query_register(
        &mut registry,
        QueryMockPlugin::with_id("boost", 10).filters(boost_remaining),
    );

    let results = vec![result("a", 0.6), result("b", 0.5)];
    let filtered = apply_result_filter_chain(&registry, &results, "q").await;
    assert_eq!(filtered.len(), 1, "first plugin removes a");
    assert_eq!(filtered[0].id, "b");
    assert!(
        (filtered[0].score - 0.6).abs() < 1e-6,
        "second plugin boosts remaining"
    );
}

#[tokio::test]
async fn test_result_filter_error_keeps_results() {
    fn fail(
        _query: &str,
        _results: Vec<cce_types::RerankCandidate>,
    ) -> std::result::Result<Option<Vec<ResultFilterEntry>>, PluginError> {
        Err(PluginError::ExecutionFailed("broken".to_string()))
    }
    let mut registry = PluginRegistry::new();
    query_register(
        &mut registry,
        QueryMockPlugin::with_id("broken", 10).filters(fail),
    );

    let results = vec![result("a", 0.6)];
    let filtered = apply_result_filter_chain(&registry, &results, "q").await;
    assert_eq!(filtered.len(), 1, "failed filter keeps results");
}
