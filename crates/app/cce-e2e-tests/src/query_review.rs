//! Full-result visualization export for the real semantic query pipeline.
//!
//! For every judgment query this job indexes the review fixture once with
//! [`QueryWorkflowTest`], runs the production [`QueryCoordinator::search`]
//! path (retrieval, fusion, enrichment, boost, sort, threshold), and renders
//! one self-contained markdown file per query under:
//!
//! ```text
//! outputs/scenarios/{lang}/query_review/{project}/
//! ├── run_manifest.txt
//! ├── index.md
//! ├── {query_id}.md
//! ├── aggregated_demo.md
//! └── aggregation_metrics.md
//! ```
//!
//! Each `{query_id}.md` carries the judgment expectation (relevant ranges),
//! the top-K score table, and every hit with complete metadata and the exact
//! response body an `expand=true` API call would return: hits run through the
//! production expansion post-pass, so expanded hits show the full annotated
//! document and unexpanded hits stay raw. The aggregated demo exercises
//! [`QueryCoordinator::search_aggregated`] with two sub-queries derived from
//! the same judgment set, with the same production post-pass applied.
//! The aggregation metrics page evaluates deterministic sub-query pairs by
//! query-type category, merge strategy and weight. Outputs are for manual
//! review only and never participate in assertions.
//!
//! The vector backend is the embedded local engine, so hybrid output needs no
//! external service and the default scenario directory always carries hybrid
//! output. Explicit single-source comparison runs must use `scenario_suffix`
//! so they never overwrite the default directory.
//!
//! Review scope is dense+BM25: the summary source flag stays off, which gates
//! both summary recall and summary boost in the production query path, so
//! summary vectors are never consumed by any reviewed query. Indexing
//! therefore skips summary embedding (summary text/BM25 are still stored);
//! this keeps mock item-budget shortfalls out of the manifest and the
//! `outcome` flag truthful about the reviewed scope.

use std::sync::Arc;

use anyhow::Context;
use cce_llm_client::OpenAICompatibleProvider;
use cce_orchestrator::query::annotation::RelationAnnotationConfig;
use cce_orchestrator::query::expansion::annotate_hits_with_expansion;
use cce_orchestrator::query::types::ExecutionStrategy;
use cce_orchestrator::query::types::SearchResult;
use cce_orchestrator::{
    AggregatedQueryOptions, QueryResult, SearchConfig, SearchSources, SubQuery,
};

use crate::bench_data::{ChunkSourceRange, QueryType, RelevanceJudgment, RelevanceLevel};
use crate::embedding::EmbeddingConfig;
use crate::fixture::{FixtureSpec, TestFixture};
use crate::mock_embedding_server::{MockEmbeddingServer, mock_embedding_config};
use crate::output_manager::{OutputBuilder, OutputCategory};
use crate::query_aggregation::{AggregationBenchmarkConfig, run_aggregation_benchmark};
use crate::query_test::QueryWorkflowTest;
use crate::range_evaluator::{RetrievedCoverage, coverage_over_retrieved};

/// One query-review export job over a single fixture and judgment set.
pub struct QueryReviewConfig {
    /// Benchmark project name, also the output directory name.
    pub project: &'static str,
    /// Language directory under `outputs/scenarios/`.
    pub language: &'static str,
    /// Fixture holding the indexed corpus.
    pub spec: FixtureSpec,
    /// Query set with human-judged expectations.
    pub judgments: Vec<RelevanceJudgment>,
    /// Number of hits rendered per query.
    pub top_k: usize,
    /// Optional output directory suffix for explicit single-source comparison
    /// runs. `None` (default) writes the hybrid output to the scenario
    /// directory; any comparison run must set a suffix so it never overwrites
    /// the default directory.
    pub scenario_suffix: Option<&'static str>,
}

/// Rendered per-query outcome kept for the index page.
struct QueryOutcome {
    id: String,
    query_type: String,
    total: usize,
    elapsed_ms: u64,
    top_file: String,
    top_score: f32,
}

/// Run the full query-review export.
pub async fn run_query_review(config: QueryReviewConfig) -> anyhow::Result<()> {
    let scenario = match config.scenario_suffix {
        Some(suffix) => format!("query_review/{}_{}", config.project, suffix),
        None => format!("query_review/{}", config.project),
    };
    let manager = OutputBuilder::new()
        .category(OutputCategory::Scenarios)
        .language(config.language)
        .scenario(&scenario)
        .build();
    manager.clean().ok();

    let server = MockEmbeddingServer::start().await;
    let embedder = Arc::new(
        OpenAICompatibleProvider::from_model(&mock_embedding_config(&server.base_url), "mock")
            .context("mock embedder")?,
    );

    let sources = SearchSources::default();

    let mut search_config = SearchConfig::default();
    search_config.result.limit = config.top_k;

    let fixture = TestFixture::load(config.spec.clone()).context("load review fixture")?;
    let mut query_test = QueryWorkflowTest::new(fixture, EmbeddingConfig::mock())
        .with_embedder(embedder)
        .with_sources(sources)
        .with_config(search_config.clone());
    let index_result = query_test.index().await.context("index fixture")?.clone();

    let fixture_root = query_test.fixture().root_path().to_path_buf();
    // Production post-pass configuration: same defaults the API handlers use
    // (primary body kept, full neighbour spans). The workspace root is filled
    // in per call by the post-pass itself.
    let annotation_config = RelationAnnotationConfig::new()
        .enable(true)
        .with_expansion(true);

    let mut outcomes = Vec::new();
    let mut single_results = Vec::new();
    for judgment in &config.judgments {
        let raw_result = query_test
            .query(&judgment.query_text)
            .await
            .with_context(|| format!("query {}", judgment.id))?
            .clone();
        single_results.push(raw_result.clone());
        let mut result = raw_result;
        // Wrap hits exactly like an expand=true API response so the rendered
        // bodies are verbatim what callers receive: expanded hits carry the
        // full annotated document, the rest stay raw.
        annotate_hits_with_expansion(
            query_test.coordinator().relation_searcher(),
            &fixture_root,
            &mut result.items,
            &annotation_config,
            &judgment.query_text,
        )
        .await;
        let page = render_query_page(QueryPageInput {
            judgment,
            result: &result,
            sources,
            search_config: &search_config,
        });
        manager.write(&format!("{}.md", judgment.id), &page)?;
        outcomes.push(QueryOutcome {
            id: judgment.id.clone(),
            query_type: judgment.query_type.to_string(),
            total: result.total,
            elapsed_ms: result.elapsed_ms,
            top_file: result
                .items
                .first()
                .map(|item| format!("{}:{}-{}", item.file_path, item.start_line, item.end_line))
                .unwrap_or_else(|| "(none)".to_string()),
            top_score: result.items.first().map(|item| item.score).unwrap_or(0.0),
        });
    }

    let aggregated_note = render_aggregated_demo(
        &mut query_test,
        &config,
        sources,
        &search_config,
        &annotation_config,
    )
    .await?;
    if let Some(page) = aggregated_note {
        manager.write("aggregated_demo.md", &page)?;
    }

    let aggregation_config = AggregationBenchmarkConfig {
        project: config.project,
        project_id: 1,
        judgments: &config.judgments,
        top_k: config.top_k,
        sources,
        search_config: &search_config,
    };
    let aggregation_report = run_aggregation_benchmark(
        query_test.coordinator(),
        &aggregation_config,
        &single_results,
    )
    .await?;
    manager.write("aggregation_metrics.md", &aggregation_report)?;

    manager.write(
        "run_manifest.txt",
        &render_manifest(
            &config,
            sources,
            &search_config,
            &index_result,
            &fixture_root,
        ),
    )?;
    manager.write("index.md", &render_index(&config, &outcomes))?;

    println!("Query review written to {}", manager.output_dir().display());
    query_test.cleanup().await;
    server.stop().await;
    Ok(())
}

/// Demonstrate the aggregated path with two sub-queries from the same set.
async fn render_aggregated_demo<F>(
    query_test: &mut QueryWorkflowTest<F>,
    config: &QueryReviewConfig,
    sources: SearchSources,
    search_config: &SearchConfig,
    annotation_config: &RelationAnnotationConfig,
) -> anyhow::Result<Option<String>>
where
    F: crate::fixture::FixtureAccess,
{
    if config.judgments.len() < 2 {
        return Ok(None);
    }
    let first = &config.judgments[0];
    let second = &config.judgments[1];
    let agg = AggregatedQueryOptions {
        original_query: format!("{} + {}", first.query_text, second.query_text),
        project_id: 1,
        sub_queries: vec![
            SubQuery {
                text: first.query_text.clone(),
                sources,
                weight: 1.0,
            },
            SubQuery {
                text: second.query_text.clone(),
                sources,
                weight: 0.7,
            },
        ],
        global_config: search_config.clone(),
        filters: None,
        exclude_patterns: Vec::new(),
        include_patterns: Vec::new(),
        enable_rerank: None,
        enable_result_filter: None,
        query_intent: None,
        relation_epoch: None,
        neighbour_resolver: None,
    };
    let mut result = query_test
        .coordinator()
        .search_aggregated(&agg)
        .await
        .context("aggregated search")?;

    // The aggregated API handler applies the same opt-in post-pass; mirror it
    // so demo bodies match aggregated responses.
    let fixture_root = query_test.fixture().root_path().to_path_buf();
    annotate_hits_with_expansion(
        query_test.coordinator().relation_searcher(),
        &fixture_root,
        &mut result.items,
        annotation_config,
        &agg.original_query,
    )
    .await;

    let mut page = String::from("# Aggregated Search Demo\n\n");
    page.push_str(&format!(
        "- Sub-query 1 (weight 1.0): {} `{}`\n",
        first.id, first.query_text
    ));
    page.push_str(&format!(
        "- Sub-query 2 (weight 0.7): {} `{}`\n",
        second.id, second.query_text
    ));
    page.push_str(&format!(
        "- Merged total: {} | elapsed: {}ms | sources: {} | failed: {:?}\n\n",
        result.total,
        result.elapsed_ms,
        result.sources.join(","),
        result.failed_sub_queries,
    ));
    page.push_str(&render_hit_table_aggregated(&result, &[first, second]));
    for (rank, item) in result.items.iter().enumerate() {
        page.push_str(&render_hit_detail(rank, item));
    }
    Ok(Some(page))
}

/// Borrowed inputs for rendering one per-query review page.
///
/// Groups the judgment, its (already expansion-wrapped) search result, and the
/// run-level search context so the renderer takes a single argument.
struct QueryPageInput<'a> {
    judgment: &'a RelevanceJudgment,
    result: &'a QueryResult,
    sources: SearchSources,
    search_config: &'a SearchConfig,
}

/// Render one self-contained per-query markdown page.
fn render_query_page(input: QueryPageInput<'_>) -> String {
    let QueryPageInput {
        judgment,
        result,
        sources,
        search_config,
    } = input;
    let strategy = ExecutionStrategy::from_sources(&sources, search_config);
    let mut page = format!("# Query: {}\n\n", judgment.id);
    page.push_str(&format!("- Text: `{}`\n", judgment.query_text));
    page.push_str(&format!("- Type: {}\n", judgment.query_type));
    if let Some(subtype) = &judgment.fuzzy_subtype {
        page.push_str(&format!("- Fuzzy subtype: {subtype}\n"));
    }
    page.push_str(&format!("- Request sources: {sources}\n"));
    page.push_str(&format!("- Execution strategy: {strategy}\n"));
    page.push_str("- Intent: default hybrid (no per-query override)\n");
    page.push_str(
        "- Expansion: production post-pass applied (mirrors expand=true API responses; top-N hits wrapped, rest raw)\n",
    );
    page.push_str(&format!(
        "- Fusion: vector_weight={:.2} bm25_weight={:.2} algorithm={:?}\n",
        search_config.fusion.vector_weight,
        search_config.fusion.bm25_weight,
        search_config.fusion.algorithm,
    ));
    page.push_str("- Capability: hybrid local (vector+bm25, embedded simvec)\n");
    page.push_str(&format!(
        "- Total: {} | elapsed: {}ms | executed as: {} | cache: {}\n",
        result.total,
        result.elapsed_ms,
        result.sources.join(","),
        if result.from_cache { "hit" } else { "miss" },
    ));
    page.push_str(&format!(
        "- Config: limit={} min_score={:.2} vector_top_k={} similarity_threshold={:.2} term_weight_threshold={:.2} timeout_ms={}\n",
        search_config.result.limit,
        search_config.result.min_score,
        search_config.vector.top_k,
        search_config.vector.similarity_threshold,
        search_config.bm25.term_weight_threshold,
        search_config.timeout_ms,
    ));
    if !result.failed_sub_queries.is_empty() {
        page.push_str(&format!(
            "- Failed sub-queries: {:?}\n",
            result.failed_sub_queries
        ));
    }

    page.push_str("\n## Expected ranges\n\n");
    if judgment.relevant_ranges.is_empty() {
        page.push_str("(no judged ranges)\n");
    } else {
        for (range, level) in &judgment.relevant_ranges {
            let marker = match level {
                RelevanceLevel::Strong => "strong",
                RelevanceLevel::Related => "related",
                RelevanceLevel::Irrelevant => "irrelevant",
            };
            page.push_str(&format!(
                "- `{}`:{}-{} ({marker})\n",
                range.file, range.start_line, range.end_line
            ));
        }
    }
    page.push_str(&format!("\n{}\n", coverage_summary(judgment, result)));

    if result.items.is_empty() {
        page.push_str(&format!("{}\n", zero_hit_note(judgment)));
    }

    page.push_str("\n## Hits (top-K)\n\n");
    page.push_str(&render_hit_table(result, judgment));

    for (rank, item) in result.items.iter().enumerate() {
        page.push_str(&render_hit_detail(rank, item));
    }
    page
}

/// The retrieved items' claimed source coverage.
fn retrieved_coverage(items: &[SearchResult]) -> Vec<RetrievedCoverage> {
    items
        .iter()
        .map(|item| RetrievedCoverage {
            file: crate::bench_data::normalize_path(&item.file_path),
            ranges: vec![ChunkSourceRange {
                start_line: item.start_line.min(item.end_line) as usize,
                end_line: item.start_line.max(item.end_line) as usize,
            }],
        })
        .collect()
}

/// Judgment level each retrieved item contributes to.
///
/// The labeled targets are covered by the whole result set: the retrieval may
/// split a target across several items, and the items carrying the missing
/// parts are the ones that contribute to the recovery. This is the relation
/// the range evaluator scores, so the review pages and the metrics agree.
fn hit_match_levels(
    judgment: &RelevanceJudgment,
    items: &[SearchResult],
) -> Vec<Option<RelevanceLevel>> {
    let coverage = coverage_over_retrieved(judgment, &retrieved_coverage(items));
    items
        .iter()
        .enumerate()
        .map(|(index, _)| coverage.level_for(index))
        .collect()
}

/// One-line expected-vs-hit coverage summary for the page header.
fn coverage_summary(judgment: &RelevanceJudgment, result: &QueryResult) -> String {
    if judgment.relevant_ranges.is_empty() {
        return String::from("- Coverage: no judged ranges; nothing to cover.");
    }
    let strong_total = judgment
        .relevant_ranges
        .iter()
        .filter(|(_, level)| *level == RelevanceLevel::Strong)
        .count();
    let related_total = judgment
        .relevant_ranges
        .iter()
        .filter(|(_, level)| *level == RelevanceLevel::Related)
        .count();
    let coverage = coverage_over_retrieved(judgment, &retrieved_coverage(&result.items));
    // The rank of the first item that contributes to a target.
    let mut first_contribution: Vec<Option<usize>> = vec![None; judgment.relevant_ranges.len()];
    for (rank, hits) in coverage.contributions.iter().enumerate() {
        for (target, _) in hits {
            if first_contribution[*target].is_none() {
                first_contribution[*target] = Some(rank + 1);
            }
        }
    }
    let mut strong_covered = 0_usize;
    let mut related_covered = 0_usize;
    let mut covered_targets = std::collections::HashSet::new();
    let mut first_strong_rank: Option<usize> = None;
    for (index, (_, level)) in judgment.relevant_ranges.iter().enumerate() {
        if !coverage.recovered[index] {
            continue;
        }
        covered_targets.insert(index);
        match level {
            RelevanceLevel::Strong => {
                strong_covered += 1;
                if first_strong_rank.is_none() {
                    first_strong_rank = first_contribution[index];
                }
            }
            RelevanceLevel::Related => related_covered += 1,
            RelevanceLevel::Irrelevant => {}
        }
    }
    if strong_total + related_total == 0 {
        return String::from("- Coverage: judged ranges carry no strong/related target.");
    }
    if covered_targets.is_empty() {
        return format!(
            "- Coverage: 0/{strong_total} strong, 0/{related_total} related covered in top-K; no expected range hit."
        );
    }
    match first_strong_rank {
        Some(rank) => format!(
            "- Coverage: {strong_covered}/{strong_total} strong, {related_covered}/{related_total} related covered in top-K; first strong hit at rank {rank}."
        ),
        None => format!(
            "- Coverage: {strong_covered}/{strong_total} strong, {related_covered}/{related_total} related covered in top-K."
        ),
    }
}

/// Mechanism note for zero-hit pages so reviewers do not misread them as index gaps.
fn zero_hit_note(judgment: &RelevanceJudgment) -> String {
    if judgment.query_type == QueryType::CrossLang {
        return String::from(
            "- Note: zero hits here mean the hybrid recall found no overlap; check the mock-embedding lexical signal when triaging.",
        );
    }
    String::from(
        "- Note: zero hits after min_score filtering; lower min_score or check index coverage when triaging.",
    )
}

/// Compact score table over the returned hits.
fn render_hit_table(result: &QueryResult, judgment: &RelevanceJudgment) -> String {
    if result.items.is_empty() {
        return String::from("(no hits)\n");
    }
    let mut table = String::from(
        "| rank | score | original | vector | bm25 | sources | boosted | boost reason | key | file | entity | in_expected |\n|---:|---:|---:|---:|---:|---|---|---|---|---|---|---|\n",
    );
    let levels = hit_match_levels(judgment, &result.items);
    for (rank, item) in result.items.iter().enumerate() {
        let bm25 = item
            .bm25_score
            .map(|score| format!("{score:.4}"))
            .unwrap_or_else(|| "-".to_string());
        let in_expected = levels[rank]
            .map(|level| level.to_string())
            .unwrap_or_else(|| "-".to_string());
        table.push_str(&format!(
            "| {} | {:.4} | {:.4} | {:.4} | {} | {} | {} | {} | {} | {}:{}-{} | {} {} | {} |\n",
            rank + 1,
            item.score,
            item.original_score,
            item.vector_score,
            bm25,
            item.sources.join("+"),
            if item.is_boosted { "yes" } else { "no" },
            cell(item.boost_reason.as_deref().unwrap_or("-")),
            cell(&alignment_key_of(item)),
            cell(&item.file_path),
            item.start_line,
            item.end_line,
            cell(&item.kind),
            cell(&item.name),
            in_expected,
        ));
    }
    table.push('\n');
    table
}

/// Compact score table for the aggregated demo, which merges two judgments and
/// therefore carries no single expectation to align against.
fn render_hit_table_aggregated(result: &QueryResult, judgments: &[&RelevanceJudgment]) -> String {
    if result.items.is_empty() {
        return String::from("(no hits)\n");
    }
    let mut table = String::from(
        "| rank | score | original | vector | bm25 | sources | boosted | boost reason | key | file | entity | in_expected |\n|---:|---:|---:|---:|---:|---|---|---|---|---|---|---|\n",
    );
    let levels: Vec<Vec<Option<RelevanceLevel>>> = judgments
        .iter()
        .map(|judgment| hit_match_levels(judgment, &result.items))
        .collect();
    for (rank, item) in result.items.iter().enumerate() {
        let bm25 = item
            .bm25_score
            .map(|score| format!("{score:.4}"))
            .unwrap_or_else(|| "-".to_string());
        let level = levels.iter().filter_map(|levels| levels[rank]).max();
        let level = match level {
            Some(RelevanceLevel::Strong) => "strong",
            Some(RelevanceLevel::Related) => "related",
            Some(RelevanceLevel::Irrelevant) => "irrelevant",
            None => "-",
        };
        table.push_str(&format!(
            "| {} | {:.4} | {:.4} | {:.4} | {} | {} | {} | {} | {} | {}:{}-{} | {} {} | {} |\n",
            rank + 1,
            item.score,
            item.original_score,
            item.vector_score,
            bm25,
            item.sources.join("+"),
            if item.is_boosted { "yes" } else { "no" },
            cell(item.boost_reason.as_deref().unwrap_or("-")),
            cell(&alignment_key_of(item)),
            cell(&item.file_path),
            item.start_line,
            item.end_line,
            cell(&item.kind),
            cell(&item.name),
            level,
        ));
    }
    table.push('\n');
    table
}

/// Complete metadata plus the verbatim API response body for one hit.
///
/// The body is whatever an `expand=true` API call returns for this hit: the
/// full annotated document when the production post-pass wrapped it, otherwise
/// the raw recall body. The expansion summary (when present) comes from the
/// counts the post-pass records on the hit itself.
fn render_hit_detail(rank: usize, item: &SearchResult) -> String {
    let mut out = format!(
        "### {}. {} `{}`\n\n",
        rank + 1,
        cell(&item.kind),
        cell(&item.name)
    );
    out.push_str(&format!("- id: `{}`\n", item.id));
    out.push_str(&format!(
        "- file: `{}:{}-{}`\n",
        item.file_path, item.start_line, item.end_line
    ));
    out.push_str(&format!(
        "- entity_ids: [{}] | segment_id: {}\n",
        item.entity_ids
            .iter()
            .map(|id| id.0.to_string())
            .collect::<Vec<_>>()
            .join(", "),
        item.segment_id.as_deref().unwrap_or("(none)"),
    ));
    out.push_str(&format!("- alignment key: `{}`\n", alignment_key_of(item)));
    out.push_str(&format!(
        "- score: {:.4} | original: {:.4} | vector: {:.4} | bm25: {} | sources: {}\n",
        item.score,
        item.original_score,
        item.vector_score,
        item.bm25_score
            .map(|score| format!("{score:.4}"))
            .unwrap_or_else(|| "-".to_string()),
        item.sources.join("+"),
    ));
    out.push_str(&format!(
        "- boosted: {} | reason: {}\n",
        item.is_boosted,
        item.boost_reason.as_deref().unwrap_or("-"),
    ));
    out.push_str(&format!(
        "- content_state: {:?} | truncated: {} | category: {}\n",
        item.content_state,
        item.truncated,
        item.category.as_deref().unwrap_or("-"),
    ));
    if !item.metadata.is_empty() {
        let mut entries: Vec<String> = item
            .metadata
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect();
        entries.sort();
        out.push_str(&format!("- metadata: {}\n", entries.join(", ")));
    }
    if let Some(pattern) = &item.pattern_info {
        out.push_str(&format!("- pattern_info: `{}`\n", cell(pattern)));
    }

    out.push_str("\n#### Content (API response body)\n\n```text\n");
    out.push_str(&fenced(&item.content));
    out.push_str("\n```\n");
    if item.metadata.get("expanded").map(String::as_str) == Some("true") {
        let count = |key: &str| item.metadata.get(key).map(String::as_str).unwrap_or("0");
        out.push_str(&format!(
            "\n- expansion: {} nodes (forward {}, backward {}, self-evident omitted {}) — body above is the annotated document\n",
            count("expanded_nodes"),
            count("forward_nodes"),
            count("backward_nodes"),
            count("omitted_self_evident"),
        ));
    }
    out.push('\n');
    out
}

/// Manifest describing how the export was produced.
fn render_manifest(
    config: &QueryReviewConfig,
    sources: SearchSources,
    search_config: &SearchConfig,
    index_result: &cce_orchestrator::IndexResult,
    fixture_root: &std::path::Path,
) -> String {
    let mut manifest = format!("project: {}\n", config.project);
    manifest.push_str(&format!("language: {}\n", config.language));
    manifest.push_str(&format!("queries: {}\n", config.judgments.len()));
    manifest.push_str(&format!("top_k: {}\n", config.top_k));
    manifest.push_str(&format!("request sources: {sources}\n"));
    manifest.push_str(&format!(
        "scope: full fixture tree at {} (no path filter)\n",
        fixture_root.display()
    ));
    manifest.push_str("scope note: structured/chunks reports exclude tests; entity and relation counts are not comparable across jobs\n");
    manifest.push_str("vector_backend: local (embedded simvec, per-run temp data dir)\n");
    manifest.push_str(
        "summaries: text-only (vectors skipped: review scope is dense+bm25, summary source disabled)\n",
    );
    manifest.push_str("embedder: deterministic mock (no LLM service)\n");
    manifest.push_str("rerank: disabled (request-level override unset)\n");
    manifest.push_str(&format!(
        "config: limit={} min_score={:.2} vector_top_k={} timeout_ms={}\n",
        search_config.result.limit,
        search_config.result.min_score,
        search_config.vector.top_k,
        search_config.timeout_ms,
    ));
    manifest.push_str(&format!(
        "fusion: vector_weight={:.2} bm25_weight={:.2} algorithm={:?}\n",
        search_config.fusion.vector_weight,
        search_config.fusion.bm25_weight,
        search_config.fusion.algorithm,
    ));
    manifest.push_str(&format!(
        "index: files={} entities={} relations={} vectors={}\n",
        index_result.total_files,
        index_result.total_entities,
        index_result.total_relations,
        index_result.total_vectors,
    ));
    manifest.push_str(&format!(
        "outcome: {}\n",
        if index_result.is_success() {
            "success".to_string()
        } else {
            "incomplete".to_string()
        },
    ));
    if index_result.errors().is_empty() {
        manifest.push_str("errors: none\n");
    } else {
        manifest.push_str("errors:\n");
        for error in index_result.errors().iter().take(20) {
            manifest.push_str(&format!("- {error}\n"));
        }
        if index_result.errors().len() > 20 {
            manifest.push_str(&format!(
                "- ... ({} more)\n",
                index_result.errors().len() - 20
            ));
        }
    }
    manifest.push_str("capability: hybrid local (vector+bm25)\n");
    manifest.push_str(&format!(
        "expectations: src/judgments/{}.rs relevant_ranges\n",
        config.project
    ));
    manifest
}

/// Entry page linking every query file with its top-1 hit.
fn render_index(config: &QueryReviewConfig, outcomes: &[QueryOutcome]) -> String {
    let mut page = format!("# Query Review: {}\n\n", config.project);
    page.push_str("Capability: hybrid local (vector+bm25, embedded simvec).\n\n");
    page.push_str(&format!("{} queries, top-{}. Expected ranges come from the judgment set; open each file to compare hits against them.\n\n", outcomes.len(), config.top_k));
    page.push_str("Scope: this job indexes the full fixture tree (tests included); structured/chunks reports exclude tests, so their counts differ by design.\n\n");
    page.push_str("| query | type | total | elapsed_ms | top score | top hit |\n|---|---|---:|---:|---:|---|\n");
    for outcome in outcomes {
        page.push_str(&format!(
            "| [{}](./{}.md) | {} | {} | {} | {:.4} | `{}` |\n",
            outcome.id,
            outcome.id,
            outcome.query_type,
            outcome.total,
            outcome.elapsed_ms,
            outcome.top_score,
            cell(&outcome.top_file),
        ));
    }
    if config.judgments.len() >= 2 {
        page.push_str(
            "\n- [aggregated_demo.md](./aggregated_demo.md): two-sub-query merge demo.\n",
        );
        page.push_str(
            "- [aggregation_metrics.md](./aggregation_metrics.md): category-aware aggregation benchmark.\n",
        );
    }
    page
}

/// Production alignment key for cross-path comparison.
fn alignment_key_of(item: &SearchResult) -> String {
    cce_types::alignment_key(&item.entity_ids, item.segment_id.as_deref(), &item.id)
        .unwrap_or_else(|| format!("c:{}", item.id))
}

/// Keep markdown table cells on one line.
fn cell(text: &str) -> String {
    text.replace('|', "/").replace('\n', " ")
}

/// Keep hit content inside fenced blocks.
fn fenced(text: &str) -> String {
    text.replace("```", "~~~")
}
