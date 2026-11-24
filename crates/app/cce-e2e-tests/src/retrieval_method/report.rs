//! Report writers for the retrieval-method benchmark.
//!
//! Output layout (under `outputs/benchmark/{fixture}/retrieval_method/`):
//! `run_manifest.txt`, `alignment_coverage.md`, `aggregate_top{k}.md`,
//! `aggregate_by_query_type_top{k}.md`, `per_query_top{k}.md`,
//! `fusion_gain_by_query_type_top{k}.md`, `weight_sensitivity.md`,
//! `rrf_k_sensitivity.md`, `single_path_policy.md`, `aggregation_strategy.md`,
//! `fusion_score_baseline.md`, `relevance_top5.md`.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::bench_data::RelevanceLevel;
use crate::judgments::evaluate::{DEFAULT_TOP_K, TOP_K_VALUES};
use crate::retrieval_method::benchmark::{
    AGGREGATION_STRATEGIES, AggregationBaseline, AlignmentStat, MINMAX_WEIGHTS, MethodRelevance,
    MethodResult, POLICY_VARIANTS, POLICY_WEIGHT, RRF_K_VALUES, ScoreStat,
};

/// Write the full report set for a benchmark run.
#[allow(clippy::too_many_arguments)]
pub fn write_all(
    out: &Path,
    project: &str,
    judgment_count: usize,
    methods: &[String],
    alignment: &[AlignmentStat],
    scores: &[ScoreStat],
    aggregation: &[AggregationBaseline],
    results: &[MethodResult],
    relevance: &[MethodRelevance],
) -> Result<(), Box<dyn std::error::Error>> {
    write_run_manifest(out, project, judgment_count, methods, alignment)?;
    write_alignment_coverage(out, alignment)?;
    for &top_k in TOP_K_VALUES {
        write_aggregate(out, results, top_k)?;
        write_aggregate_by_query_type(out, results, top_k)?;
        write_per_query(out, results, top_k)?;
        write_fusion_gain(out, results, top_k)?;
        write_relative_baseline(out, results, top_k)?;
    }
    write_weight_sensitivity(out, results)?;
    write_rrf_sensitivity(out, results)?;
    write_single_path_policy(out, results)?;
    write_aggregation_strategy(out, results, aggregation)?;
    write_fusion_score_baseline(out, scores)?;
    write_relevance(out, relevance)?;
    Ok(())
}

/// Per-(baseline, query_type) sensitivity group: unique query ids plus
/// (method, F1) observation pairs.
type SensitivityGroup =
    BTreeMap<(String, String), (std::collections::HashSet<String>, Vec<(String, f64)>)>;

/// Write `run_manifest.txt` describing data sources and the method matrix.
fn write_run_manifest(
    out: &Path,
    project: &str,
    judgment_count: usize,
    methods: &[String],
    alignment: &[AlignmentStat],
) -> Result<(), Box<dyn std::error::Error>> {
    let path = out.join("run_manifest.txt");
    let mut f = File::create(&path)?;
    writeln!(f, "=== Retrieval-Method Benchmark Run Manifest ===")?;
    writeln!(f)?;
    writeln!(f, "project: {project}")?;
    writeln!(f, "judgments: {judgment_count}")?;
    writeln!(f, "methods: {}", methods.join(", "))?;
    writeln!(f, "minmax weights (vector/bm25):")?;
    for (alpha, beta) in MINMAX_WEIGHTS {
        writeln!(f, "  minmax-{alpha:.1}: {alpha} / {beta}")?;
    }
    writeln!(f, "rrf k values:")?;
    for k in RRF_K_VALUES {
        writeln!(f, "  rrf-{k:.0}: k={k}")?;
    }
    writeln!(
        f,
        "single-path policy variants (at {POLICY_WEIGHT:.1}/{POLICY_WEIGHT:.1}):"
    )?;
    for (suffix, policy) in POLICY_VARIANTS {
        let (include_vector, include_bm25) = policy.switches();
        writeln!(
            f,
            "  minmax-{POLICY_WEIGHT:.1}{suffix}: include_vector_only_keys={include_vector}, \
             include_bm25_only_keys={include_bm25}"
        )?;
    }
    writeln!(f, "aggregation strategies (two recall paths as sources):")?;
    for (name, _) in AGGREGATION_STRATEGIES {
        writeln!(f, "  {name}")?;
    }
    writeln!(
        f,
        "alignment key: entity_id (multi-entity expanded) -> segment_id -> chunk_id"
    )?;
    writeln!(
        f,
        "measurement semantics: single paths on raw chunk ranking; fused methods per alignment key"
    )?;
    writeln!(f, "test chunks: included (All variant)")?;
    writeln!(f)?;
    for stat in alignment {
        writeln!(
            f,
            "baseline {}: {} emb chunks ({} keys, {} unalignable), {} bm25 chunks ({} keys, {} unalignable), {} common keys (emb-only {}, bm25-only {})",
            stat.baseline,
            stat.emb_chunks,
            stat.emb_keys,
            stat.emb_unalignable,
            stat.bm25_chunks,
            stat.bm25_keys,
            stat.bm25_unalignable,
            stat.common_keys,
            stat.emb_only_keys,
            stat.bm25_only_keys,
        )?;
    }
    Ok(())
}

/// Write `alignment_coverage.md` with cross-path key coverage per baseline.
fn write_alignment_coverage(
    out: &Path,
    stats: &[AlignmentStat],
) -> Result<(), Box<dyn std::error::Error>> {
    let path = out.join("alignment_coverage.md");
    let mut f = File::create(&path)?;

    let headers = &[
        "baseline",
        "emb_chunks",
        "bm25_chunks",
        "emb_keys",
        "bm25_keys",
        "common",
        "emb_only",
        "bm25_only",
        "single_path_share",
        "coverage",
        "emb_unalignable",
        "bm25_unalignable",
        "unalignable_share",
    ];
    let rows: Vec<Vec<String>> = stats
        .iter()
        .map(|s| {
            let total = s.emb_keys.max(s.bm25_keys);
            let coverage = if total == 0 {
                0.0
            } else {
                s.common_keys as f64 / total as f64
            };
            vec![
                s.baseline.clone(),
                s.emb_chunks.to_string(),
                s.bm25_chunks.to_string(),
                s.emb_keys.to_string(),
                s.bm25_keys.to_string(),
                s.common_keys.to_string(),
                s.emb_only_keys.to_string(),
                s.bm25_only_keys.to_string(),
                format!("{:.1}%", s.single_path_share() * 100.0),
                format!("{:.1}%", coverage * 100.0),
                s.emb_unalignable.to_string(),
                s.bm25_unalignable.to_string(),
                format!("{:.1}%", s.unalignable_ratio() * 100.0),
            ]
        })
        .collect();
    write_md_table(&mut f, headers, &rows)?;

    writeln!(f)?;
    writeln!(f, "Coverage = common keys / max(emb_keys, bm25_keys).")?;
    writeln!(
        f,
        "Single-path share = (emb_only + bm25_only) / distinct keys; the offline \
         counterpart of search_hybrid_alignment_single_path_ratio."
    )?;
    writeln!(
        f,
        "Single-path keys join the fused list according to their own switch \
         (include_vector_only_keys / include_bm25_only_keys, both on by default)."
    )?;
    writeln!(
        f,
        "Unalignable = chunks with no entity and no segment distinct from the chunk id; \
         their key carries a path suffix and can never match. This is the offline \
         counterpart of search_hybrid_alignment_unalignable_ratio: a few percent is \
         normal (index fallback), a growing share means the index is missing entity or \
         segment fields."
    )?;
    Ok(())
}

/// Write `single_path_policy.md`.
///
/// Compares the balanced fusion under each single-path policy. The F1 table
/// answers "what does keeping this side's single-path keys buy"; the entry
/// table shows how the union splits, which is what explains a row that does not
/// move.
fn write_single_path_policy(
    out: &Path,
    results: &[MethodResult],
) -> Result<(), Box<dyn std::error::Error>> {
    let path = out.join("single_path_policy.md");
    let mut f = File::create(&path)?;

    let method_of = |suffix: &str| format!("minmax-{POLICY_WEIGHT:.1}{suffix}");
    let policies: Vec<(String, &str)> = std::iter::once((method_of(""), "union"))
        .chain(
            POLICY_VARIANTS
                .iter()
                .map(|(suffix, _)| (method_of(suffix), *suffix)),
        )
        .collect();

    let mut grouped: BTreeMap<(String, String, String), Vec<f64>> = BTreeMap::new();
    for r in results.iter().filter(|r| r.top_k == DEFAULT_TOP_K) {
        let Some((_, label)) = policies.iter().find(|(name, _)| *name == r.method) else {
            continue;
        };
        grouped
            .entry((
                r.baseline.clone(),
                r.query_type.to_string(),
                (*label).to_string(),
            ))
            .or_default()
            .push(r.score.range.f1_any);
    }

    let headers = &[
        "baseline",
        "query_type",
        "union",
        "vector_only",
        "bm25_only",
        "strict",
        "union - strict",
    ];
    let mut baselines: Vec<String> = grouped.keys().map(|k| k.0.clone()).collect();
    baselines.sort();
    baselines.dedup();
    let mut rows = Vec::new();
    for baseline in &baselines {
        let mut query_types: Vec<String> = grouped
            .keys()
            .filter(|k| k.0 == *baseline)
            .map(|k| k.1.clone())
            .collect();
        query_types.sort();
        query_types.dedup();
        for query_type in query_types {
            let mean_of_label = |label: &str| -> f64 {
                grouped
                    .get(&(baseline.clone(), query_type.clone(), label.to_string()))
                    .map_or(0.0, |vals| mean_of(vals))
            };
            let union = mean_of_label("union");
            let strict = mean_of_label("-strict");
            rows.push(vec![
                baseline.clone(),
                query_type.clone(),
                format!("{union:.4}"),
                format!("{:.4}", mean_of_label("-vec")),
                format!("{:.4}", mean_of_label("-bm25")),
                format!("{strict:.4}"),
                format!("{:+.4}", union - strict),
            ]);
        }
    }
    write_md_table(&mut f, headers, &rows)?;

    writeln!(f)?;
    writeln!(
        f,
        "All policies use identical weights ({POLICY_WEIGHT:.1} vector / {POLICY_WEIGHT:.1} BM25); \
         only the single-path switch pair differs. Keys returned by both paths always survive, so \
         disabling both switches equals the strict intersection (5th column of the F1 row set is \
         the cost of dropping every single-path key)."
    )?;
    Ok(())
}

/// Write `aggregation_strategy.md`.
///
/// Two tables: the ranking effect of the strategy (F1 at the default cutoff),
/// and how much multi-source overlap the data actually has. Without the second
/// table a "no difference" result would be indistinguishable from a broken
/// merge.
fn write_aggregation_strategy(
    out: &Path,
    results: &[MethodResult],
    aggregation: &[AggregationBaseline],
) -> Result<(), Box<dyn std::error::Error>> {
    let path = out.join("aggregation_strategy.md");
    let mut f = File::create(&path)?;

    let mut grouped: BTreeMap<(String, String, String), Vec<f64>> = BTreeMap::new();
    for r in results.iter().filter(|r| r.top_k == DEFAULT_TOP_K) {
        if !AGGREGATION_STRATEGIES
            .iter()
            .any(|(name, _)| *name == r.method)
        {
            continue;
        }
        grouped
            .entry((
                r.baseline.clone(),
                r.query_type.to_string(),
                r.method.clone(),
            ))
            .or_default()
            .push(r.score.range.f1_any);
    }

    let headers = &["baseline", "query_type", "agg-max", "agg-noisy-or", "delta"];
    let mut rows = Vec::new();
    let mut baselines: Vec<String> = grouped.keys().map(|k| k.0.clone()).collect();
    baselines.sort();
    baselines.dedup();
    for baseline in &baselines {
        let mut query_types: Vec<String> = grouped
            .keys()
            .filter(|k| k.0 == *baseline)
            .map(|k| k.1.clone())
            .collect();
        query_types.sort();
        query_types.dedup();
        for query_type in query_types {
            let mean_of_method = |method: &str| -> f64 {
                grouped
                    .get(&(baseline.clone(), query_type.clone(), method.to_string()))
                    .map_or(0.0, |vals| mean_of(vals))
            };
            let max = mean_of_method("agg-max");
            let noisy = mean_of_method("agg-noisy-or");
            rows.push(vec![
                baseline.clone(),
                query_type.clone(),
                format!("{max:.4}"),
                format!("{noisy:.4}"),
                format!("{:+.4}", noisy - max),
            ]);
        }
    }
    write_md_table(&mut f, headers, &rows)?;

    writeln!(f)?;
    writeln!(f, "### Multi-source overlap")?;
    writeln!(f)?;
    let headers = &[
        "baseline",
        "strategy",
        "multi_source_keys",
        "single_source_keys",
        "contributions",
        "multi_source_share",
        "contributions_per_key",
    ];
    let rows: Vec<Vec<String>> = aggregation
        .iter()
        .map(|entry| {
            vec![
                entry.baseline.clone(),
                entry.strategy.clone(),
                entry.stat.multi_source_keys.to_string(),
                entry.stat.single_source_keys.to_string(),
                entry.stat.contributions.to_string(),
                format!("{:.1}%", entry.stat.multi_source_ratio() * 100.0),
                format!("{:.2}", entry.stat.contributions_per_key()),
            ]
        })
        .collect();
    write_md_table(&mut f, headers, &rows)?;

    writeln!(f)?;
    writeln!(
        f,
        "Each recall path stands in for one sub-query result list: both are min-max \
         normalized per query (the production normalization), then merged on the production \
         dedup key with the production score-combination formula. Rows are F1_any at top-{DEFAULT_TOP_K}."
    )?;
    writeln!(
        f,
        "The strategies can only differ on keys hit by more than one source, so read the \
         overlap table first: a low multi-source share explains a flat delta."
    )?;
    Ok(())
}

/// Write `fusion_score_baseline.md`.
///
/// The numeric baseline for the fused score scale. F1 cannot observe it: the
/// ranking is unchanged when only the score semantics move, so the ceiling each
/// entry reaches is recorded explicitly. A single-path key must never reach
/// `1.0` — its ceiling is its own weight share.
fn write_fusion_score_baseline(
    out: &Path,
    scores: &[ScoreStat],
) -> Result<(), Box<dyn std::error::Error>> {
    let path = out.join("fusion_score_baseline.md");
    let mut f = File::create(&path)?;

    let headers = &[
        "baseline",
        "method",
        "vector_weight",
        "bm25_weight",
        "entries",
        "dual_path",
        "vector_only",
        "bm25_only",
        "at_ceiling",
        "ceiling_share",
        "mean_score",
        "max_score",
    ];
    let rows: Vec<Vec<String>> = scores
        .iter()
        .map(|stat| {
            let s = &stat.summary;
            vec![
                stat.baseline.clone(),
                stat.method.clone(),
                format!("{:.1}", stat.vector_weight),
                format!("{:.1}", stat.bm25_weight),
                s.entries.to_string(),
                s.dual_path.to_string(),
                s.vector_only.to_string(),
                s.bm25_only.to_string(),
                s.at_ceiling.to_string(),
                format!("{:.1}%", s.ceiling_ratio() * 100.0),
                format!("{:.4}", s.mean_score()),
                format!("{:.4}", s.max_score),
            ]
        })
        .collect();
    write_md_table(&mut f, headers, &rows)?;

    writeln!(f)?;
    writeln!(
        f,
        "Ceiling = the fused score of an entry whose every contributing path gives full \
         normalized credit: w_v for a vector-only key, w_b for a BM25-only key, 1.0 for a \
         dual-path key (weights sum to one). at_ceiling counts entries within 1e-4 of it."
    )?;
    writeln!(
        f,
        "A single-path entry can therefore never report 1.0; a regression that restores the \
         old single-path shortcut shows up as max_score 1.0 together with at_ceiling covering \
         the single-path rows as well."
    )?;
    Ok(())
}

/// Write `aggregate_top{k}.md` grouped by (baseline, method).
fn write_aggregate(
    out: &Path,
    results: &[MethodResult],
    top_k: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = out.join(format!("aggregate_top{top_k}.md"));
    let mut f = File::create(&path)?;

    let mut map: BTreeMap<(String, String), Vec<&MethodResult>> = BTreeMap::new();
    for r in results.iter().filter(|r| r.top_k == top_k) {
        map.entry((r.baseline.clone(), r.method.clone()))
            .or_default()
            .push(r);
    }

    let headers = &[
        "k", "baseline", "method", "n", "R_s", "R_r", "R_a", "P_s", "P_r", "P_a", "F1_s", "F1_r",
        "F1_a", "1st_hit", "redund",
    ];
    let rows: Vec<Vec<String>> = map
        .iter()
        .map(|((baseline, method), group)| {
            vec![
                format!("{top_k}"),
                baseline.clone(),
                method.clone(),
                format!("{}", group.len()),
                avg_str(group.iter().map(|r| r.score.range.recall_strong)),
                avg_str(group.iter().map(|r| r.score.range.recall_related)),
                avg_str(group.iter().map(|r| r.score.range.recall_any)),
                avg_str(group.iter().map(|r| r.score.range.precision_strong)),
                avg_str(group.iter().map(|r| r.score.range.precision_related)),
                avg_str(group.iter().map(|r| r.score.range.precision_any)),
                avg_str(group.iter().map(|r| r.score.range.f1_strong)),
                avg_str(group.iter().map(|r| r.score.range.f1_related)),
                avg_str(group.iter().map(|r| r.score.range.f1_any)),
                first_hit_str(group),
                avg_str(group.iter().map(|r| r.score.redundant_coverage as f64)),
            ]
        })
        .collect();
    write_md_table(&mut f, headers, &rows)?;
    Ok(())
}

/// Write `aggregate_by_query_type_top{k}.md` grouped by (baseline, method,
/// query_type).
fn write_aggregate_by_query_type(
    out: &Path,
    results: &[MethodResult],
    top_k: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = out.join(format!("aggregate_by_query_type_top{top_k}.md"));
    let mut f = File::create(&path)?;

    let mut map: BTreeMap<(String, String, String), Vec<&MethodResult>> = BTreeMap::new();
    for r in results.iter().filter(|r| r.top_k == top_k) {
        map.entry((
            r.baseline.clone(),
            r.method.clone(),
            r.query_type.to_string(),
        ))
        .or_default()
        .push(r);
    }

    let headers = &[
        "k",
        "baseline",
        "method",
        "query_type",
        "n",
        "R_s",
        "R_r",
        "R_a",
        "P_s",
        "P_r",
        "P_a",
        "F1_s",
        "F1_r",
        "F1_a",
        "1st_hit",
    ];
    let rows: Vec<Vec<String>> = map
        .iter()
        .map(|((baseline, method, query_type), group)| {
            vec![
                format!("{top_k}"),
                baseline.clone(),
                method.clone(),
                query_type.clone(),
                format!("{}", group.len()),
                avg_str(group.iter().map(|r| r.score.range.recall_strong)),
                avg_str(group.iter().map(|r| r.score.range.recall_related)),
                avg_str(group.iter().map(|r| r.score.range.recall_any)),
                avg_str(group.iter().map(|r| r.score.range.precision_strong)),
                avg_str(group.iter().map(|r| r.score.range.precision_related)),
                avg_str(group.iter().map(|r| r.score.range.precision_any)),
                avg_str(group.iter().map(|r| r.score.range.f1_strong)),
                avg_str(group.iter().map(|r| r.score.range.f1_related)),
                avg_str(group.iter().map(|r| r.score.range.f1_any)),
                first_hit_str(group),
            ]
        })
        .collect();
    write_md_table(&mut f, headers, &rows)?;
    Ok(())
}

/// Write `per_query_top{k}.md` with per-query × per-method detail rows.
fn write_per_query(
    out: &Path,
    results: &[MethodResult],
    top_k: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = out.join(format!("per_query_top{top_k}.md"));
    let mut f = File::create(&path)?;

    let headers = &[
        "k",
        "baseline",
        "method",
        "query_id",
        "query_type",
        "s_m",
        "r_m",
        "a_m",
        "rel",
        "R_s",
        "R_r",
        "R_a",
        "P_s",
        "P_r",
        "P_a",
        "F1_s",
        "F1_r",
        "F1_a",
        "1st_hit",
        "redund",
    ];
    let rows: Vec<Vec<String>> = results
        .iter()
        .filter(|r| r.top_k == top_k)
        .map(|r| {
            let s = &r.score;
            vec![
                format!("{}", r.top_k),
                r.baseline.clone(),
                r.method.clone(),
                r.query_id.clone(),
                r.query_type.to_string(),
                format!("{}", s.range.strong_matches),
                format!("{}", s.range.related_matches),
                format!("{}", s.range.any_matches),
                format!("{}", s.range.total_relevant),
                format!("{:.4}", s.range.recall_strong),
                format!("{:.4}", s.range.recall_related),
                format!("{:.4}", s.range.recall_any),
                format!("{:.4}", s.range.precision_strong),
                format!("{:.4}", s.range.precision_related),
                format!("{:.4}", s.range.precision_any),
                format!("{:.4}", s.range.f1_strong),
                format!("{:.4}", s.range.f1_related),
                format!("{:.4}", s.range.f1_any),
                first_hit_cell(s.avg_first_hit_rank),
                format!("{}", s.redundant_coverage),
            ]
        })
        .collect();
    write_md_table(&mut f, headers, &rows)?;
    Ok(())
}

/// Write `fusion_gain_by_query_type_top{k}.md`.
///
/// Per-query fusion gain is `(F1_hybrid - max(F1_emb, F1_bm25)) / max(...)`,
/// averaged per (baseline, query_type) for each hybrid method.
fn write_fusion_gain(
    out: &Path,
    results: &[MethodResult],
    top_k: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = out.join(format!("fusion_gain_by_query_type_top{top_k}.md"));
    let mut f = File::create(&path)?;

    // Baseline per-query F1 for the two single-path methods.
    let mut single: BTreeMap<(String, String), (f64, f64)> = BTreeMap::new();
    for r in results.iter().filter(|r| r.top_k == top_k) {
        let entry = single
            .entry((r.baseline.clone(), r.query_id.clone()))
            .or_insert((0.0, 0.0));
        match r.method.as_str() {
            "emb" => entry.0 = r.score.range.f1_any,
            "bm25" => entry.1 = r.score.range.f1_any,
            _ => {}
        }
    }

    let mut map: BTreeMap<(String, String, String), Vec<f64>> = BTreeMap::new();
    for r in results.iter().filter(|r| r.top_k == top_k) {
        if r.method == "emb" || r.method == "bm25" {
            continue;
        }
        let Some(&(f1_emb, f1_bm25)) = single.get(&(r.baseline.clone(), r.query_id.clone())) else {
            continue;
        };
        let best = f1_emb.max(f1_bm25);
        let gain = if best > 0.0 {
            (r.score.range.f1_any - best) / best
        } else {
            0.0
        };
        map.entry((
            r.baseline.clone(),
            r.query_type.to_string(),
            r.method.clone(),
        ))
        .or_default()
        .push(gain);
    }

    let headers = &["k", "baseline", "query_type", "method", "n", "fusion_gain"];
    let rows: Vec<Vec<String>> = map
        .iter()
        .map(|((baseline, query_type, method), gains)| {
            vec![
                format!("{top_k}"),
                baseline.clone(),
                query_type.clone(),
                method.clone(),
                format!("{}", gains.len()),
                avg_str(gains.iter().copied()),
            ]
        })
        .collect();
    write_md_table(&mut f, headers, &rows)?;
    Ok(())
}

/// Write `weight_sensitivity.md`: F1@5 spread across `minmax-*` weights per
/// (baseline, query_type), plus the std across weights.
fn write_weight_sensitivity(
    out: &Path,
    results: &[MethodResult],
) -> Result<(), Box<dyn std::error::Error>> {
    let path = out.join("weight_sensitivity.md");
    let mut f = File::create(&path)?;

    let mut per_query: BTreeMap<(String, String, String), f64> = BTreeMap::new();
    for r in results.iter().filter(|r| r.top_k == DEFAULT_TOP_K) {
        per_query.insert(
            (r.baseline.clone(), r.query_id.clone(), r.method.clone()),
            r.score.range.f1_any,
        );
    }

    let mut grouped: SensitivityGroup = BTreeMap::new();
    for (key, f1) in &per_query {
        if !key.2.starts_with("minmax-") {
            continue;
        }
        let (query_ids, entries) = grouped
            .entry((key.0.clone(), query_type_of(results, &key.1, &key.0)))
            .or_default();
        query_ids.insert(key.1.clone());
        entries.push((key.2.clone(), *f1));
    }

    let headers = &[
        "baseline",
        "query_type",
        "n",
        "minmax-0.9",
        "minmax-0.7",
        "minmax-0.6",
        "minmax-0.5",
        "minmax-0.4",
        "minmax-0.3",
        "minmax-0.1",
        "std",
    ];
    let mut rows = Vec::new();
    for ((baseline, query_type), (query_ids, entries)) in &grouped {
        let mut by_weight: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        for (method, f1) in entries {
            by_weight.entry(method.clone()).or_default().push(*f1);
        }
        let mut row = vec![
            baseline.clone(),
            query_type.clone(),
            format!("{}", query_ids.len()),
        ];
        let mut means = Vec::new();
        for (alpha, _) in MINMAX_WEIGHTS {
            let alpha_key = format!("minmax-{alpha:.1}");
            let vals = by_weight.get(&alpha_key).cloned().unwrap_or_default();
            let mean = mean_of(&vals);
            row.push(format!("{mean:.4}"));
            means.push(mean);
        }
        row.push(format!("{:.4}", std_dev(&means)));
        rows.push(row);
    }
    write_md_table(&mut f, headers, &rows)?;
    Ok(())
}

/// Write `rrf_k_sensitivity.md`: F1@5 spread across `rrf-*` k values per
/// (baseline, query_type), plus the std across k values.
fn write_rrf_sensitivity(
    out: &Path,
    results: &[MethodResult],
) -> Result<(), Box<dyn std::error::Error>> {
    let path = out.join("rrf_k_sensitivity.md");
    let mut f = File::create(&path)?;

    let mut per_query: BTreeMap<(String, String, String), f64> = BTreeMap::new();
    for r in results.iter().filter(|r| r.top_k == DEFAULT_TOP_K) {
        per_query.insert(
            (r.baseline.clone(), r.query_id.clone(), r.method.clone()),
            r.score.range.f1_any,
        );
    }

    let mut grouped: SensitivityGroup = BTreeMap::new();
    for (key, f1) in &per_query {
        if !key.2.starts_with("rrf-") {
            continue;
        }
        let (query_ids, entries) = grouped
            .entry((key.0.clone(), query_type_of(results, &key.1, &key.0)))
            .or_default();
        query_ids.insert(key.1.clone());
        entries.push((key.2.clone(), *f1));
    }

    let headers = &[
        "baseline",
        "query_type",
        "n",
        "rrf-30",
        "rrf-60",
        "rrf-100",
        "std",
    ];
    let mut rows = Vec::new();
    for ((baseline, query_type), (query_ids, entries)) in &grouped {
        let mut by_k: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        for (method, f1) in entries {
            by_k.entry(method.clone()).or_default().push(*f1);
        }
        let mut row = vec![
            baseline.clone(),
            query_type.clone(),
            format!("{}", query_ids.len()),
        ];
        let mut means = Vec::new();
        for k in RRF_K_VALUES {
            let key = format!("rrf-{k:.0}");
            let vals = by_k.get(&key).cloned().unwrap_or_default();
            let mean = mean_of(&vals);
            row.push(format!("{mean:.4}"));
            means.push(mean);
        }
        row.push(format!("{:.4}", std_dev(&means)));
        rows.push(row);
    }
    write_md_table(&mut f, headers, &rows)?;
    Ok(())
}

/// Write `relevance_top5.md` with top-5 strong/related hits per method.
fn write_relevance(
    out: &Path,
    relevance: &[MethodRelevance],
) -> Result<(), Box<dyn std::error::Error>> {
    let path = out.join("relevance_top5.md");
    let mut f = File::create(&path)?;

    let headers = &[
        "baseline", "method", "query_id", "rank", "level", "location",
    ];
    let mut rows = Vec::new();
    for info in relevance {
        let mut collect = |chunks: &[(usize, String, RelevanceLevel)]| {
            for (rank, loc, level) in chunks {
                rows.push(vec![
                    info.baseline.clone(),
                    info.method.clone(),
                    info.query_id.clone(),
                    rank.to_string(),
                    format!("{:?}", level),
                    loc.clone(),
                ]);
            }
        };
        collect(&info.strong_chunks);
        collect(&info.related_chunks);
    }
    write_md_table(&mut f, headers, &rows)?;
    Ok(())
}

/// Resolve the query type for a query id inside a baseline.
fn query_type_of(results: &[MethodResult], query_id: &str, baseline: &str) -> String {
    results
        .iter()
        .find(|r| r.baseline == baseline && r.query_id == query_id)
        .map(|r| r.query_type.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Write `relative_baseline_top{k}.md`.
///
/// Compares each hybrid method against the single-path baselines on a
/// per-query basis: best/worst single path, oracle (per-query best of emb
/// and bm25), and the gap between the best hybrid and the oracle.
fn write_relative_baseline(
    out: &Path,
    results: &[MethodResult],
    top_k: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = out.join(format!("relative_baseline_top{top_k}.md"));
    let mut f = File::create(&path)?;

    // Per-query single-path F1 values.
    let mut single: BTreeMap<(String, String), (f64, f64)> = BTreeMap::new();
    for r in results.iter().filter(|r| r.top_k == top_k) {
        let entry = single
            .entry((r.baseline.clone(), r.query_id.clone()))
            .or_insert((0.0, 0.0));
        match r.method.as_str() {
            "emb" => entry.0 = r.score.range.f1_any,
            "bm25" => entry.1 = r.score.range.f1_any,
            _ => {}
        }
    }

    // Per-(baseline, method) per-query F1 values for hybrid methods.
    let mut fusion: BTreeMap<(String, String), BTreeMap<String, f64>> = BTreeMap::new();
    for r in results.iter().filter(|r| r.top_k == top_k) {
        if r.method == "emb" || r.method == "bm25" {
            continue;
        }
        fusion
            .entry((r.baseline.clone(), r.method.clone()))
            .or_default()
            .insert(r.query_id.clone(), r.score.range.f1_any);
    }

    let mut baselines: Vec<String> = single.keys().map(|k| k.0.clone()).collect();
    baselines.sort();
    baselines.dedup();

    let headers = &[
        "k",
        "baseline",
        "n",
        "emb F1",
        "bm25 F1",
        "best single F1",
        "worst single F1",
        "oracle F1",
        "best fusion F1",
        "gain vs best",
        "fusion gap",
    ];
    let mut rows: Vec<Vec<String>> = Vec::new();
    for baseline in &baselines {
        let queries: Vec<&String> = single
            .keys()
            .filter(|k| k.0 == *baseline)
            .map(|k| &k.1)
            .collect();
        if queries.is_empty() {
            continue;
        }
        let n = queries.len();
        let mut emb_vals = Vec::new();
        let mut bm25_vals = Vec::new();
        let mut best_vals = Vec::new();
        let mut worst_vals = Vec::new();
        for q in &queries {
            let (e, b) = single.get(&(baseline.clone(), (*q).clone())).unwrap();
            emb_vals.push(*e);
            bm25_vals.push(*b);
            best_vals.push(e.max(*b));
            worst_vals.push(e.min(*b));
        }
        let emb_avg = mean_of(&emb_vals);
        let bm25_avg = mean_of(&bm25_vals);
        let best_single_avg = mean_of(&best_vals);
        let worst_single_avg = mean_of(&worst_vals);
        let oracle_avg = best_single_avg;

        // Best hybrid method for this baseline.
        let mut best_fusion_name = "-".to_string();
        let mut best_fusion_avg = 0.0;
        for ((b, method), qmap) in &fusion {
            if b != baseline {
                continue;
            }
            let vals: Vec<f64> = queries
                .iter()
                .map(|q| qmap.get(*q).copied().unwrap_or(0.0))
                .collect();
            let avg = mean_of(&vals);
            if avg > best_fusion_avg {
                best_fusion_avg = avg;
                best_fusion_name = method.clone();
            }
        }

        rows.push(vec![
            format!("{top_k}"),
            baseline.clone(),
            format!("{n}"),
            format!("{:.4}", emb_avg),
            format!("{:.4}", bm25_avg),
            format!("{:.4}", best_single_avg),
            format!("{:.4}", worst_single_avg),
            format!("{:.4}", oracle_avg),
            format!("{:.4}", best_fusion_avg),
            format!("{:+.4}", best_fusion_avg - best_single_avg),
            format!("{:.4}", oracle_avg - best_fusion_avg),
        ]);
        let _ = best_fusion_name;
    }
    write_md_table(&mut f, headers, &rows)?;
    Ok(())
}

/// Format an average of a float iterator.
fn avg_str(iter: impl Iterator<Item = f64>) -> String {
    let vals: Vec<f64> = iter.collect();
    format!("{:.4}", mean_of(&vals))
}

/// Format an optional average first-hit rank.
fn first_hit_str(results: &[&MethodResult]) -> String {
    let hits: Vec<f64> = results
        .iter()
        .filter_map(|r| r.score.avg_first_hit_rank)
        .collect();
    if hits.is_empty() {
        "n/a".to_string()
    } else {
        format!("{:.1}", hits.iter().sum::<f64>() / hits.len() as f64)
    }
}

/// Format an optional average first-hit rank for a per-query cell.
fn first_hit_cell(value: Option<f64>) -> String {
    value.map_or_else(|| "none".to_string(), |v| format!("{v:.1}"))
}

fn mean_of(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn std_dev(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let mean = mean_of(values);
    let variance =
        values.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / values.len() as f64;
    variance.sqrt()
}

/// Write a formatted markdown table with auto-sized columns.
fn write_md_table(
    f: &mut File,
    headers: &[&str],
    rows: &[Vec<String>],
) -> Result<(), Box<dyn std::error::Error>> {
    let ncols = headers.len();
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in rows {
        for (i, val) in row.iter().enumerate() {
            widths[i] = widths[i].max(val.len());
        }
    }

    let hdr: String = (0..ncols)
        .map(|i| format!("| {:<w$} ", headers[i], w = widths[i]))
        .collect::<Vec<_>>()
        .join("")
        + "|";
    writeln!(f, "{hdr}")?;

    let sep: String = (0..ncols)
        .map(|i| format!("|{:-<w$}", "", w = widths[i] + 2))
        .collect::<Vec<_>>()
        .join("")
        + "|";
    writeln!(f, "{sep}")?;

    for row in rows {
        let line: String = (0..ncols)
            .map(|i| format!("| {:<w$} ", row[i], w = widths[i]))
            .collect::<Vec<_>>()
            .join("")
            + "|";
        writeln!(f, "{line}")?;
    }
    Ok(())
}
