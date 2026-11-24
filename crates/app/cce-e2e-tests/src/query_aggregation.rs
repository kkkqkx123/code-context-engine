//! Category-aware evaluation of the production aggregated search path.
//!
//! Deterministic sub-query pairs are taken from the review judgment set and
//! executed through [`QueryCoordinator::search_aggregated`]. Each merged list
//! is scored against the union of both judgments, alongside the individual
//! sub-queries and their pre-merge union. The report separates same-type and
//! mixed-type pairs, compares merge strategies and weights, and records the
//! dedup and failure signals needed to explain a gain or a regression.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::time::Instant;

use anyhow::Context;
use cce_config::modules::search::AggregationScoreStrategy;
use cce_orchestrator::query::types::{
    AggregatedQueryOptions, QueryOptions, QueryResult, SearchResult, SubQuery,
};
use cce_orchestrator::{QueryCoordinator, SearchConfig, SearchSources};

use crate::bench_data::{
    ChunkSourceRange, QueryType, RelevanceJudgment, RelevanceLevel, SourceRange,
};
use crate::range_evaluator::{RetrievedCoverage, coverage_over_retrieved};

/// Number of sub-queries in every evaluated request.
const SUBQUERY_COUNT: usize = 2;

/// Second-sub-query weights evaluated for every pair.
const WEIGHTS: &[f32] = &[1.0, 0.7, 0.3];

/// Score-combination strategies evaluated for every pair and weight.
const STRATEGIES: &[AggregationScoreStrategy] = &[
    AggregationScoreStrategy::NoisyOr,
    AggregationScoreStrategy::Max,
];

/// Explicitly paired query types for the multi-intent cases.
const MIXED_PAIRS: &[(QueryType, QueryType)] = &[
    (QueryType::Qualified, QueryType::Semantic),
    (QueryType::Fuzzy, QueryType::Qualified),
    (QueryType::CrossLang, QueryType::Semantic),
    (QueryType::Fuzzy, QueryType::Semantic),
];

/// Inputs shared by one aggregation benchmark run.
pub struct AggregationBenchmarkConfig<'a> {
    pub project: &'a str,
    pub project_id: i64,
    pub judgments: &'a [RelevanceJudgment],
    pub top_k: usize,
    pub sources: SearchSources,
    pub search_config: &'a SearchConfig,
}

/// One deterministic sub-query pair.
#[derive(Debug, Clone)]
struct Scenario {
    category: String,
    first: usize,
    second: usize,
}

/// Range-based score of one ranked list against one judgment.
#[derive(Debug, Clone, Copy, Default)]
struct Metrics {
    precision: f64,
    recall: f64,
    f1: f64,
    mrr: f64,
}

/// One measured (pair, strategy, weight) row.
#[derive(Debug, Clone)]
struct ScenarioRow {
    category: String,
    pair: String,
    types: String,
    strategy: &'static str,
    weight: f32,
    first_f1: f64,
    second_f1: f64,
    best_sub_f1: f64,
    union_recall: f64,
    merged_recall: f64,
    merge_delta: f64,
    f1: f64,
    gain: f64,
    precision: f64,
    recall: f64,
    mrr: f64,
    dedup_ratio: f64,
    agreement: f64,
    failed: usize,
    wall_ms: u64,
    reported_ms: u64,
    executed: usize,
    cache_hits: usize,
    from_cache: bool,
    outcome: &'static str,
}

/// Run the full matrix and render its markdown report.
pub async fn run_aggregation_benchmark(
    coordinator: &QueryCoordinator,
    config: &AggregationBenchmarkConfig<'_>,
    single_results: &[QueryResult],
) -> anyhow::Result<String> {
    let scenarios = build_scenarios(config.judgments);
    let mut rows = Vec::new();
    let mut wide_cache: HashMap<usize, QueryResult> = HashMap::new();

    for scenario in &scenarios {
        let first = config
            .judgments
            .get(scenario.first)
            .context("missing first judgment")?;
        let second = config
            .judgments
            .get(scenario.second)
            .context("missing second judgment")?;
        let first_result = single_results
            .get(scenario.first)
            .context("missing first single-query result")?;
        let second_result = single_results
            .get(scenario.second)
            .context("missing second single-query result")?;
        let wide_first =
            wide_subquery_result(coordinator, config, scenario.first, &mut wide_cache).await?;
        let wide_second =
            wide_subquery_result(coordinator, config, scenario.second, &mut wide_cache).await?;
        let combined = combined_judgment(first, second);

        let first_metrics = evaluate(
            &top_items(&first_result.items, config.top_k),
            &combined,
            config.top_k,
        );
        let second_metrics = evaluate(
            &top_items(&second_result.items, config.top_k),
            &combined,
            config.top_k,
        );
        let best_sub_f1 = first_metrics.f1.max(second_metrics.f1);

        for strategy in STRATEGIES {
            for weight in WEIGHTS {
                let union_items = weighted_union(
                    &wide_first,
                    &wide_second,
                    *weight,
                    config.top_k,
                    config.search_config.result.min_score,
                );
                let union_metrics = evaluate(&union_items, &combined, config.top_k);
                let options = aggregated_options(config, first, second, *weight, *strategy);
                let call_start = Instant::now();
                let result = coordinator
                    .search_aggregated(&options)
                    .await
                    .with_context(|| format!("aggregated query '{}'", combined.id))?;
                let wall_ms = call_start.elapsed().as_millis() as u64;
                let metrics = evaluate(
                    &top_items(&result.items, config.top_k),
                    &combined,
                    config.top_k,
                );
                let gain = relative_gain(metrics.f1, best_sub_f1);
                let outcome = if metrics.f1 > best_sub_f1 + 1e-9 {
                    "win"
                } else if metrics.f1 + 1e-9 < best_sub_f1 {
                    "loss"
                } else {
                    "tie"
                };
                rows.push(ScenarioRow {
                    category: scenario.category.clone(),
                    pair: combined.id.clone(),
                    types: format!("{}+{}", first.query_type, second.query_type),
                    strategy: strategy_name(strategy),
                    weight: *weight,
                    first_f1: first_metrics.f1,
                    second_f1: second_metrics.f1,
                    best_sub_f1,
                    union_recall: union_metrics.recall,
                    merged_recall: metrics.recall,
                    merge_delta: metrics.recall - union_metrics.recall,
                    f1: metrics.f1,
                    gain,
                    precision: metrics.precision,
                    recall: metrics.recall,
                    mrr: metrics.mrr,
                    dedup_ratio: dedup_ratio(&result.items, &wide_first, &wide_second),
                    agreement: agreement_ratio(&result.items, &wide_first, &wide_second),
                    failed: result.failed_sub_queries.len(),
                    wall_ms,
                    reported_ms: result.elapsed_ms,
                    executed: result.executed_sub_queries,
                    cache_hits: result.sub_query_cache_hits,
                    from_cache: result.from_cache,
                    outcome,
                });
            }
        }
    }

    Ok(render_report(config, &rows))
}

/// Build same-type pairs plus the explicit mixed-type pairs present in the set.
fn build_scenarios(judgments: &[RelevanceJudgment]) -> Vec<Scenario> {
    let mut scenarios = Vec::new();
    for query_type in [
        QueryType::Qualified,
        QueryType::Fuzzy,
        QueryType::Semantic,
        QueryType::CrossLang,
    ] {
        let indices: Vec<usize> = judgments
            .iter()
            .enumerate()
            .filter_map(|(index, judgment)| (judgment.query_type == query_type).then_some(index))
            .collect();
        if indices.len() >= 2 {
            scenarios.push(Scenario {
                category: format!("same:{query_type}"),
                first: indices[0],
                second: indices[1],
            });
        }
    }
    for (left, right) in MIXED_PAIRS {
        let first = judgments
            .iter()
            .position(|judgment| judgment.query_type == *left);
        let second = judgments
            .iter()
            .position(|judgment| judgment.query_type == *right);
        if let (Some(first), Some(second)) = (first, second) {
            if first != second {
                scenarios.push(Scenario {
                    category: format!("mixed:{left}+{right}"),
                    first,
                    second,
                });
            }
        }
    }
    scenarios
}

/// Treat the union of both judgments as the task of the merged list.
fn combined_judgment(first: &RelevanceJudgment, second: &RelevanceJudgment) -> RelevanceJudgment {
    let mut ranges: BTreeMap<(String, usize, usize), RelevanceLevel> = BTreeMap::new();
    for judgment in [first, second] {
        for (range, level) in &judgment.relevant_ranges {
            let key = (range.file.clone(), range.start_line, range.end_line);
            let entry = ranges.entry(key).or_insert(*level);
            *entry = (*entry).max(*level);
        }
    }
    RelevanceJudgment {
        id: format!("{}+{}", first.id, second.id),
        query_text: format!("{} + {}", first.query_text, second.query_text),
        query_type: first.query_type,
        fuzzy_subtype: None,
        relevant_ranges: ranges
            .into_iter()
            .map(|((file, start_line, end_line), level)| {
                (
                    SourceRange {
                        file,
                        start_line,
                        end_line,
                    },
                    level,
                )
            })
            .collect(),
    }
}

/// Build one production aggregated-search request.
fn aggregated_options(
    config: &AggregationBenchmarkConfig<'_>,
    first: &RelevanceJudgment,
    second: &RelevanceJudgment,
    weight: f32,
    strategy: AggregationScoreStrategy,
) -> AggregatedQueryOptions {
    let mut global_config = config.search_config.clone();
    global_config.aggregation.score_strategy = strategy;
    AggregatedQueryOptions {
        original_query: format!("{} + {}", first.query_text, second.query_text),
        project_id: config.project_id,
        sub_queries: vec![
            SubQuery {
                text: first.query_text.clone(),
                sources: config.sources,
                weight: 1.0,
            },
            SubQuery {
                text: second.query_text.clone(),
                sources: config.sources,
                weight,
            },
        ],
        global_config,
        filters: None,
        exclude_patterns: Vec::new(),
        include_patterns: Vec::new(),
        enable_rerank: None,
        enable_result_filter: None,
        query_intent: None,
        relation_epoch: None,
        neighbour_resolver: None,
    }
}

/// Run a sub-query with the same threshold-free, inflated limit the aggregation path uses.
async fn wide_subquery_result(
    coordinator: &QueryCoordinator,
    config: &AggregationBenchmarkConfig<'_>,
    index: usize,
    cache: &mut HashMap<usize, QueryResult>,
) -> anyhow::Result<QueryResult> {
    if let Some(cached) = cache.get(&index) {
        return Ok(cached.clone());
    }
    let judgment = config
        .judgments
        .get(index)
        .context("missing judgment for wide sub-query")?;
    let limit = subquery_limit(config.top_k);
    let mut sub_config = config.search_config.clone();
    sub_config.result.limit = limit;
    sub_config.result.min_score = 0.0;
    let options = QueryOptions {
        query: judgment.query_text.clone(),
        project_id: config.project_id,
        sources: config.sources,
        config: sub_config,
        directory_prefix: None,
        exclude_content_types: Vec::new(),
        include_categories: Vec::new(),
        exclude_categories: Vec::new(),
        exclude_patterns: Vec::new(),
        include_patterns: Vec::new(),
        with_source: true,
        query_intent: None,
        enable_rerank: None,
        enable_result_filter: None,
        neighbour_resolver: None,
    };
    let result = coordinator
        .search(&options)
        .await
        .with_context(|| format!("wide sub-query '{}'", judgment.id))?;
    cache.insert(index, result.clone());
    Ok(result)
}

fn subquery_limit(top_k: usize) -> usize {
    top_k
        .saturating_mul(SUBQUERY_COUNT)
        .max(top_k)
        .clamp(10, 100)
}

fn top_items(items: &[SearchResult], top_k: usize) -> &[SearchResult] {
    &items[..items.len().min(top_k)]
}

/// Build the pre-merge baseline: apply weights, dedup once, keep the best
/// representative per key, then truncate with the same top-k budget.
fn weighted_union(
    first: &QueryResult,
    second: &QueryResult,
    weight: f32,
    top_k: usize,
    min_score: f32,
) -> Vec<SearchResult> {
    let mut by_key: HashMap<String, SearchResult> = HashMap::new();
    for (result, factor) in [(first, 1.0_f32), (second, weight)] {
        for item in &result.items {
            let mut item = item.clone();
            item.score *= factor;
            item.original_score *= factor;
            let key = item_key(&item);
            match by_key.get_mut(&key) {
                Some(existing) if item.score > existing.score => *existing = item,
                None => {
                    by_key.insert(key, item);
                }
                _ => {}
            }
        }
    }
    let mut items: Vec<SearchResult> = by_key.into_values().collect();
    if min_score > 0.0 {
        items.retain(|item| item.score >= min_score);
    }
    items.sort_by(|left, right| left.cmp_score_desc_then_stable(right));
    items.truncate(top_k);
    items
}

/// Score a ranked list with the same range semantics as the other benchmarks.
fn evaluate(items: &[SearchResult], judgment: &RelevanceJudgment, top_k: usize) -> Metrics {
    let retrieved = retrieved_coverage(items);
    let coverage = coverage_over_retrieved(judgment, &retrieved);
    let mut targets = 0_usize;
    let mut recovered = 0_usize;
    for ((_, level), is_recovered) in judgment.relevant_ranges.iter().zip(&coverage.recovered) {
        if *level == RelevanceLevel::Irrelevant {
            continue;
        }
        targets += 1;
        if *is_recovered {
            recovered += 1;
        }
    }
    let mut contributing = 0_usize;
    let mut first_rank = None;
    for (rank, hits) in coverage.contributions.iter().enumerate() {
        if hits
            .iter()
            .any(|(_, level)| *level != RelevanceLevel::Irrelevant)
        {
            contributing += 1;
            if first_rank.is_none() {
                first_rank = Some(rank + 1);
            }
        }
    }
    let precision = contributing as f64 / top_k.max(1) as f64;
    let recall = if targets == 0 {
        0.0
    } else {
        recovered as f64 / targets as f64
    };
    let f1 = f1_of(precision, recall);
    Metrics {
        precision,
        recall,
        f1,
        mrr: first_rank.map_or(0.0, |rank| 1.0 / rank as f64),
    }
}

fn retrieved_coverage(items: &[SearchResult]) -> Vec<RetrievedCoverage> {
    items
        .iter()
        .map(|item| RetrievedCoverage {
            file: item.file_path.clone(),
            ranges: vec![ChunkSourceRange {
                start_line: item.start_line.min(item.end_line) as usize,
                end_line: item.start_line.max(item.end_line) as usize,
            }],
        })
        .collect()
}

fn dedup_ratio(merged: &[SearchResult], first: &QueryResult, second: &QueryResult) -> f64 {
    let before = first.items.len() + second.items.len();
    if before == 0 {
        0.0
    } else {
        1.0 - merged.len() as f64 / before as f64
    }
}

fn agreement_ratio(merged: &[SearchResult], first: &QueryResult, second: &QueryResult) -> f64 {
    if merged.is_empty() {
        return 0.0;
    }
    let first_keys: HashSet<String> = first.items.iter().map(item_key).collect();
    let second_keys: HashSet<String> = second.items.iter().map(item_key).collect();
    let agreed = merged
        .iter()
        .filter(|item| {
            let key = item_key(item);
            first_keys.contains(&key) && second_keys.contains(&key)
        })
        .count();
    agreed as f64 / merged.len() as f64
}

fn item_key(item: &SearchResult) -> String {
    cce_types::alignment_key(&item.entity_ids, item.segment_id.as_deref(), &item.id)
        .unwrap_or_else(|| format!("c:{}", item.id))
}

fn f1_of(precision: f64, recall: f64) -> f64 {
    if precision + recall <= 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    }
}

fn relative_gain(value: f64, baseline: f64) -> f64 {
    if baseline > 1e-9 {
        (value - baseline) / baseline
    } else if value > 1e-9 {
        1.0
    } else {
        0.0
    }
}

fn strategy_name(strategy: &AggregationScoreStrategy) -> &'static str {
    match strategy {
        AggregationScoreStrategy::NoisyOr => "noisy_or",
        AggregationScoreStrategy::Max => "max",
    }
}

fn render_report(config: &AggregationBenchmarkConfig<'_>, rows: &[ScenarioRow]) -> String {
    let mut page = String::new();
    writeln!(page, "# Query Aggregation Metrics: {}", config.project).expect("write report title");
    writeln!(page).expect("write blank line");
    writeln!(
        page,
        "- Sub-query pairs: same-type first two plus explicit mixed-type combinations."
    )
    .expect("write pair policy");
    writeln!(
        page,
        "- Target judgment: union of both sub-query judgments, deduplicated by file and range."
    )
    .expect("write target policy");
    writeln!(
        page,
        "- Strategies: noisy_or, max. Weights (first:second): 1.0:1.0, 1.0:0.7, 1.0:0.3."
    )
    .expect("write strategy policy");
    writeln!(
        page,
        "- Sources: {}. top_k: {}. min_score: {:.2}.",
        config.sources, config.top_k, config.search_config.result.min_score
    )
    .expect("write query policy");
    writeln!(
        page,
        "- Union baseline: threshold-free inflated sub-queries, weighted and deduped before score combination, filtered by the same min_score, then truncated to top_k."
    )
    .expect("write union policy");
    writeln!(
        page,
        "- Metrics: P/R/F1 and MRR use the range evaluator; merge_delta = merged_recall - union_recall (positive means the merge recovered more targets)."
    )
    .expect("write metric policy");
    writeln!(page).expect("write blank line");

    writeln!(page, "## Per scenario").expect("write section title");
    writeln!(page).expect("write blank line");
    let headers = [
        "category",
        "diagnostic",
        "pair",
        "types",
        "strategy",
        "weight",
        "sub1_f1",
        "sub2_f1",
        "best_sub_f1",
        "union_recall",
        "merged_recall",
        "merge_delta",
        "f1",
        "gain",
        "p",
        "r",
        "mrr",
        "dedup",
        "agreement",
        "failed",
        "wall_ms",
        "reported_ms",
        "executed",
        "cache_hits",
        "cache",
        "result",
    ];
    let table: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            vec![
                row.category.clone(),
                if is_diagnostic(row) { "yes" } else { "no" }.to_string(),
                row.pair.clone(),
                row.types.clone(),
                row.strategy.to_string(),
                format!("{:.1}", row.weight),
                format!("{:.4}", row.first_f1),
                format!("{:.4}", row.second_f1),
                format!("{:.4}", row.best_sub_f1),
                format!("{:.4}", row.union_recall),
                format!("{:.4}", row.merged_recall),
                format!("{:+.4}", row.merge_delta),
                format!("{:.4}", row.f1),
                format!("{:+.4}", row.gain),
                format!("{:.4}", row.precision),
                format!("{:.4}", row.recall),
                format!("{:.4}", row.mrr),
                format!("{:.4}", row.dedup_ratio),
                format!("{:.4}", row.agreement),
                row.failed.to_string(),
                row.wall_ms.to_string(),
                row.reported_ms.to_string(),
                row.executed.to_string(),
                row.cache_hits.to_string(),
                if row.from_cache { "hit" } else { "miss" }.to_string(),
                row.outcome.to_string(),
            ]
        })
        .collect();
    write_table(&mut page, &headers, &table);

    writeln!(page).expect("write blank line");
    writeln!(page, "## By category (settings)").expect("write section title");
    writeln!(page).expect("write blank line");
    let grouped = group_rows(rows, |row| row.category.clone());
    let headers = [
        "category",
        "scenarios",
        "best_sub_f1",
        "agg_f1",
        "gain",
        "union_recall",
        "merge_delta",
        "dedup",
        "agreement",
        "wins",
        "ties",
        "losses",
        "failed",
    ];
    let mut categories: Vec<String> = grouped.keys().cloned().collect();
    categories.sort();
    let settings_categories: Vec<String> = categories
        .iter()
        .filter(|category| !category.contains("cross_lang"))
        .cloned()
        .collect();
    let diagnostic_categories: Vec<String> = categories
        .iter()
        .filter(|category| category.contains("cross_lang"))
        .cloned()
        .collect();
    let category_rows = |keys: &[String]| -> Vec<Vec<String>> {
        keys.iter()
            .map(|category| {
                let group = grouped
                    .get(category)
                    .expect("category came from the grouped map");
                vec![
                    category.clone(),
                    group.len().to_string(),
                    format!("{:.4}", mean(group.iter().map(|row| row.best_sub_f1))),
                    format!("{:.4}", mean(group.iter().map(|row| row.f1))),
                    format!("{:+.4}", mean(group.iter().map(|row| row.gain))),
                    format!("{:.4}", mean(group.iter().map(|row| row.union_recall))),
                    format!("{:+.4}", mean(group.iter().map(|row| row.merge_delta))),
                    format!("{:.4}", mean(group.iter().map(|row| row.dedup_ratio))),
                    format!("{:.4}", mean(group.iter().map(|row| row.agreement))),
                    group
                        .iter()
                        .filter(|row| row.outcome == "win")
                        .count()
                        .to_string(),
                    group
                        .iter()
                        .filter(|row| row.outcome == "tie")
                        .count()
                        .to_string(),
                    group
                        .iter()
                        .filter(|row| row.outcome == "loss")
                        .count()
                        .to_string(),
                    group
                        .iter()
                        .map(|row| row.failed)
                        .sum::<usize>()
                        .to_string(),
                ]
            })
            .collect()
    };
    write_table(&mut page, &headers, &category_rows(&settings_categories));

    writeln!(page).expect("write blank line");
    writeln!(page, "## Cross-language diagnostics").expect("write section title");
    writeln!(page).expect("write blank line");
    writeln!(
        page,
        "Excluded from settings: BM25 has no lexical signal for these queries and the mock embedder cannot model cross-lingual alignment."
    )
    .expect("write diagnostic note");
    writeln!(page).expect("write blank line");
    write_table(&mut page, &headers, &category_rows(&diagnostic_categories));

    writeln!(page).expect("write blank line");
    writeln!(page, "## Weight and strategy sensitivity").expect("write section title");
    writeln!(page).expect("write blank line");
    let sensitivity = group_rows(rows, |row| format!("{}|{}", row.category, row.strategy));
    let headers = [
        "category", "strategy", "w=1.0", "w=0.7", "w=0.3", "best", "spread",
    ];
    let mut keys: Vec<String> = sensitivity
        .keys()
        .filter(|key| !key.contains("cross_lang"))
        .cloned()
        .collect();
    keys.sort();
    let table: Vec<Vec<String>> = keys
        .iter()
        .map(|key| {
            let (category, strategy) = key
                .split_once('|')
                .expect("grouped key always carries both fields");
            let group = sensitivity.get(key).expect("key came from the grouped map");
            let score_at = |weight: f32| {
                mean(
                    group
                        .iter()
                        .filter(|row| (row.weight - weight).abs() < 1e-6)
                        .map(|row| row.f1),
                )
            };
            let scores: Vec<f64> = WEIGHTS.iter().map(|weight| score_at(*weight)).collect();
            let best_index = scores
                .iter()
                .enumerate()
                .max_by(|(left_index, left), (right_index, right)| {
                    left.total_cmp(right)
                        .then_with(|| right_index.cmp(left_index))
                })
                .map(|(index, _)| index)
                .expect("the weight list is non-empty");
            let spread = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                - scores.iter().copied().fold(f64::INFINITY, f64::min);
            vec![
                category.to_string(),
                strategy.to_string(),
                format!("{:.4}", scores[0]),
                format!("{:.4}", scores[1]),
                format!("{:.4}", scores[2]),
                format!("{:.1}", WEIGHTS[best_index]),
                format!("{:.4}", spread),
            ]
        })
        .collect();
    write_table(&mut page, &headers, &table);

    writeln!(page).expect("write blank line");
    writeln!(page, "## Best measured setting by category").expect("write section title");
    writeln!(page).expect("write blank line");
    let headers = [
        "category",
        "strategy",
        "weight",
        "f1",
        "gain",
        "merge_delta",
        "failed",
    ];
    let table: Vec<Vec<String>> = settings_categories
        .iter()
        .map(|category| {
            let best = best_setting(rows.iter().filter(|row| row.category == *category));
            vec![
                category.clone(),
                best.strategy.to_string(),
                format!("{:.1}", best.weight),
                format!("{:.4}", best.f1),
                format!("{:+.4}", best.gain),
                format!("{:+.4}", best.merge_delta),
                best.failed.to_string(),
            ]
        })
        .collect();
    write_table(&mut page, &headers, &table);
    page
}

fn is_diagnostic(row: &ScenarioRow) -> bool {
    row.category.contains("cross_lang")
}

/// Prefer the production default strategy and the balanced weight when scores tie.
fn best_setting<'a>(mut rows: impl Iterator<Item = &'a ScenarioRow>) -> &'a ScenarioRow {
    let mut best = rows
        .next()
        .expect("a category always has at least one measured row");
    for row in rows {
        let better = row.f1 > best.f1 + 1e-9
            || ((row.f1 - best.f1).abs() <= 1e-9 && setting_rank(row) > setting_rank(best));
        if better {
            best = row;
        }
    }
    best
}

fn setting_rank(row: &ScenarioRow) -> (u8, u8) {
    let strategy = u8::from(row.strategy == "noisy_or");
    let weight = if (row.weight - 1.0).abs() < 1e-6 {
        2
    } else if (row.weight - 0.7).abs() < 1e-6 {
        1
    } else {
        0
    };
    (strategy, weight)
}

fn group_rows<F>(rows: &[ScenarioRow], key: F) -> BTreeMap<String, Vec<&ScenarioRow>>
where
    F: Fn(&ScenarioRow) -> String,
{
    let mut grouped: BTreeMap<String, Vec<&ScenarioRow>> = BTreeMap::new();
    for row in rows {
        grouped.entry(key(row)).or_default().push(row);
    }
    grouped
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, count) = values.fold((0.0, 0_usize), |(sum, count), value| {
        (sum + value, count + 1)
    });
    if count == 0 { 0.0 } else { sum / count as f64 }
}

fn write_table(page: &mut String, headers: &[&str], rows: &[Vec<String>]) {
    writeln!(page, "| {} |", headers.join(" | ")).expect("write table header");
    writeln!(
        page,
        "|{}|",
        headers.iter().map(|_| "---").collect::<Vec<_>>().join("|")
    )
    .expect("write table separator");
    for row in rows {
        writeln!(page, "| {} |", row.join(" | ")).expect("write table row");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench_data::QueryType;

    fn judgment(id: &str, query_type: QueryType, start: usize) -> RelevanceJudgment {
        RelevanceJudgment {
            id: id.to_string(),
            query_text: id.to_string(),
            query_type,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/lib.rs".to_string(),
                    start_line: start,
                    end_line: start + 4,
                },
                RelevanceLevel::Strong,
            )],
        }
    }

    #[test]
    fn combined_judgment_keeps_the_stronger_level_for_a_duplicate_range() {
        let first = judgment("a", QueryType::Qualified, 10);
        let mut second = judgment("b", QueryType::Semantic, 10);
        second.relevant_ranges[0].1 = RelevanceLevel::Related;
        let combined = combined_judgment(&first, &second);
        assert_eq!(combined.relevant_ranges.len(), 1);
        assert_eq!(combined.relevant_ranges[0].1, RelevanceLevel::Strong);
        assert_eq!(combined.id, "a+b");
    }

    #[test]
    fn scenarios_cover_same_and_mixed_type_pairs_only_when_available() {
        let judgments = vec![
            judgment("q1", QueryType::Qualified, 10),
            judgment("q2", QueryType::Qualified, 20),
            judgment("s1", QueryType::Semantic, 30),
        ];
        let scenarios = build_scenarios(&judgments);
        assert!(
            scenarios
                .iter()
                .any(|item| item.category == "same:qualified")
        );
        assert!(
            scenarios
                .iter()
                .any(|item| item.category == "mixed:qualified+semantic")
        );
        assert!(!scenarios.iter().any(|item| item.category == "same:fuzzy"));
    }
}
