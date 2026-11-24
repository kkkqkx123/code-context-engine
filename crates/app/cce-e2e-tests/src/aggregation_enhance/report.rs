//! Report writers for the aggregation-enhance benchmark.
//!
//! Output layout (under `outputs/benchmark/{fixture}/aggregation_enhance/`):
//! `run_manifest.txt`, `aggregate_top{k}.md`,
//! `aggregate_by_query_type_top{k}.md`, `per_query_top{k}.md`,
//! `enhance_gain_by_query_type_top{k}.md`, `relevance_top5.md`.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::aggregation_enhance::benchmark::{BASELINE, MINMAX_WEIGHT, SignalStats};
use crate::aggregation_enhance::enhance::EnhanceParams;
use crate::judgments::evaluate::TOP_K_VALUES;
use crate::retrieval_method::benchmark::{MethodRelevance, MethodResult};

/// Write the full report set for a benchmark run.
#[allow(clippy::too_many_arguments)]
pub fn write_all(
    out: &Path,
    project: &str,
    judgment_count: usize,
    methods: &[String],
    results: &[MethodResult],
    relevance: &[MethodRelevance],
    signals: &SignalStats,
    skipped: &[String],
    params: &EnhanceParams,
) -> Result<(), Box<dyn std::error::Error>> {
    write_run_manifest(
        out,
        project,
        judgment_count,
        methods,
        signals,
        skipped,
        params,
    )?;
    for &top_k in TOP_K_VALUES {
        write_aggregate(out, results, top_k)?;
        write_aggregate_by_query_type(out, results, top_k)?;
        write_per_query(out, results, top_k)?;
        write_enhance_gain(out, results, top_k)?;
    }
    write_relevance(out, relevance)?;
    Ok(())
}

fn write_run_manifest(
    out: &Path,
    project: &str,
    judgment_count: usize,
    methods: &[String],
    signals: &SignalStats,
    skipped: &[String],
    params: &EnhanceParams,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = out.join("run_manifest.txt");
    let mut f = File::create(&path)?;
    writeln!(f, "=== Aggregation-Enhance Benchmark Run Manifest ===")?;
    writeln!(f)?;
    writeln!(f, "project: {project}")?;
    writeln!(f, "baseline: {BASELINE} (only)")?;
    writeln!(f, "judgments: {judgment_count}")?;
    writeln!(f, "methods: {}", methods.join(", "))?;
    writeln!(
        f,
        "hybrid base: minmax-{:.1} (vector {:.1} / bm25 {:.1})",
        MINMAX_WEIGHT.0, MINMAX_WEIGHT.0, MINMAX_WEIGHT.1
    )?;
    writeln!(f, "production parity: emb, emb+rel, emb+sum, emb+both")?;
    writeln!(
        f,
        "exploratory (deviating from production, hybrid/bm25 never boost):"
    )?;
    writeln!(
        f,
        "  bm25+rel, bm25+sum, bm25+both, minmax-0.5+rel, minmax-0.5+sum, minmax-0.5+both"
    )?;
    writeln!(
        f,
        "relation graph: file-cohort proxy (undirected, same-file entities),"
    )?;
    writeln!(
        f,
        "  seeds top_n={}, max_hops={}, decay 1/sqrt(hops), max_boost={}",
        params.relation.top_n, params.relation.max_hops, params.relation.max_boost
    )?;
    writeln!(
        f,
        "summary signal: mean-pooled file vectors from chunk vectors,"
    )?;
    writeln!(
        f,
        "  top_k={} files, min_score={}, summary_cap={}",
        params.summary.top_k,
        params.summary.min_score,
        params.agg.cap_for("summary")
    )?;
    writeln!(
        f,
        "aggregation: score * (1 + capped), global max_addition={}",
        params.agg.max_addition
    )?;
    writeln!(
        f,
        "measurement: chunk-level ranking (single paths) and fused coverage (minmax-0.5*)"
    )?;
    writeln!(f)?;
    writeln!(
        f,
        "signals: {} files, {} entities, {} emb chunks, {} bm25 chunks",
        signals.files, signals.entities, signals.emb_chunks, signals.bm25_chunks
    )?;
    if skipped.is_empty() {
        writeln!(f, "skipped: none")?;
    } else {
        writeln!(f, "skipped:")?;
        for reason in skipped {
            writeln!(f, "  - {reason}")?;
        }
    }
    Ok(())
}

fn write_aggregate(
    out: &Path,
    results: &[MethodResult],
    top_k: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = out.join(format!("aggregate_top{top_k}.md"));
    let mut f = File::create(&path)?;

    let mut map: BTreeMap<String, Vec<&MethodResult>> = BTreeMap::new();
    for r in results.iter().filter(|r| r.top_k == top_k) {
        map.entry(r.method.clone()).or_default().push(r);
    }

    let headers = &[
        "k", "baseline", "method", "parity", "n", "R_s", "R_r", "R_a", "P_s", "P_r", "P_a", "F1_s",
        "F1_r", "F1_a", "1st_hit", "redund",
    ];
    let rows: Vec<Vec<String>> = map
        .iter()
        .map(|(method, group)| {
            vec![
                format!("{top_k}"),
                BASELINE.to_string(),
                method.clone(),
                parity_mark(method),
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
    writeln!(f)?;
    writeln!(
        f,
        "parity=yes follows the production boost path; exploratory rows deviate (see run_manifest.txt)."
    )?;
    Ok(())
}

fn write_aggregate_by_query_type(
    out: &Path,
    results: &[MethodResult],
    top_k: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = out.join(format!("aggregate_by_query_type_top{top_k}.md"));
    let mut f = File::create(&path)?;

    let mut map: BTreeMap<(String, String), Vec<&MethodResult>> = BTreeMap::new();
    for r in results.iter().filter(|r| r.top_k == top_k) {
        map.entry((r.method.clone(), r.query_type.to_string()))
            .or_default()
            .push(r);
    }

    let headers = &[
        "k",
        "baseline",
        "method",
        "parity",
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
        .map(|((method, query_type), group)| {
            vec![
                format!("{top_k}"),
                BASELINE.to_string(),
                method.clone(),
                parity_mark(method),
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
        "parity",
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
                parity_mark(&r.method),
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

/// Write `enhance_gain_by_query_type_top{k}.md`.
///
/// Per-query enhance gain is `(F1_enhanced - F1_plain) / max(F1_plain, eps)`
/// against the plain method of the same base, averaged per query type.
fn write_enhance_gain(
    out: &Path,
    results: &[MethodResult],
    top_k: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = out.join(format!("enhance_gain_by_query_type_top{top_k}.md"));
    let mut f = File::create(&path)?;

    let mut plain: BTreeMap<(String, String), f64> = BTreeMap::new();
    for r in results.iter().filter(|r| r.top_k == top_k) {
        if r.method == "emb" || r.method == "bm25" || r.method == "minmax-0.5" {
            plain.insert((r.query_id.clone(), r.method.clone()), r.score.range.f1_any);
        }
    }

    let mut map: BTreeMap<(String, String), Vec<f64>> = BTreeMap::new();
    for r in results.iter().filter(|r| r.top_k == top_k) {
        let base = match r.method.as_str() {
            m if m.starts_with("emb+") => "emb",
            m if m.starts_with("bm25+") => "bm25",
            m if m.starts_with("minmax-0.5+") => "minmax-0.5",
            _ => continue,
        };
        let Some(&f1_plain) = plain.get(&(r.query_id.clone(), base.to_string())) else {
            continue;
        };
        let gain = if f1_plain > 1e-9 {
            (r.score.range.f1_any - f1_plain) / f1_plain
        } else if r.score.range.f1_any > 1e-9 {
            1.0
        } else {
            0.0
        };
        map.entry((r.query_type.to_string(), r.method.clone()))
            .or_default()
            .push(gain);
    }

    let headers = &[
        "k",
        "baseline",
        "query_type",
        "method",
        "parity",
        "n",
        "enhance_gain",
    ];
    let rows: Vec<Vec<String>> = map
        .iter()
        .map(|((query_type, method), gains)| {
            vec![
                format!("{top_k}"),
                BASELINE.to_string(),
                query_type.clone(),
                method.clone(),
                parity_mark(method),
                format!("{}", gains.len()),
                avg_str(gains.iter().copied()),
            ]
        })
        .collect();
    write_md_table(&mut f, headers, &rows)?;
    writeln!(f)?;
    writeln!(
        f,
        "enhance_gain = (F1_enhanced - F1_plain_same_base) / F1_plain_same_base."
    )?;
    Ok(())
}

fn write_relevance(
    out: &Path,
    relevance: &[MethodRelevance],
) -> Result<(), Box<dyn std::error::Error>> {
    let path = out.join("relevance_top5.md");
    let mut f = File::create(&path)?;
    let headers = &[
        "baseline",
        "method",
        "parity",
        "query_id",
        "strong_hits",
        "related_hits",
    ];
    let rows: Vec<Vec<String>> = relevance
        .iter()
        .map(|r| {
            let strong = r
                .strong_chunks
                .iter()
                .map(|(rank, loc, _)| format!("{rank}:{loc}"))
                .collect::<Vec<_>>()
                .join("; ");
            let related = r
                .related_chunks
                .iter()
                .map(|(rank, loc, _)| format!("{rank}:{loc}"))
                .collect::<Vec<_>>()
                .join("; ");
            vec![
                r.baseline.clone(),
                r.method.clone(),
                parity_mark(&r.method),
                r.query_id.clone(),
                strong,
                related,
            ]
        })
        .collect();
    write_md_table(&mut f, headers, &rows)?;
    Ok(())
}

fn parity_mark(method: &str) -> String {
    if super::benchmark::is_production_parity(method) {
        "yes".to_string()
    } else {
        "exploratory".to_string()
    }
}

fn avg_str(values: impl Iterator<Item = f64>) -> String {
    let values: Vec<f64> = values.collect();
    if values.is_empty() {
        return "-".to_string();
    }
    format!("{:.4}", values.iter().sum::<f64>() / values.len() as f64)
}

fn first_hit_str(group: &[&MethodResult]) -> String {
    let hits: Vec<f64> = group
        .iter()
        .filter_map(|r| r.score.avg_first_hit_rank)
        .collect();
    if hits.is_empty() {
        "-".to_string()
    } else {
        format!("{:.2}", hits.iter().sum::<f64>() / hits.len() as f64)
    }
}

fn first_hit_cell(value: Option<f64>) -> String {
    value
        .map(|v| format!("{v:.2}"))
        .unwrap_or_else(|| "-".to_string())
}

fn write_md_table(
    f: &mut File,
    headers: &[&str],
    rows: &[Vec<String>],
) -> Result<(), Box<dyn std::error::Error>> {
    writeln!(f, "| {} |", headers.join(" | "))?;
    writeln!(
        f,
        "|{}|",
        headers.iter().map(|_| "---").collect::<Vec<_>>().join("|")
    )?;
    for row in rows {
        writeln!(f, "| {} |", row.join(" | "))?;
    }
    Ok(())
}
