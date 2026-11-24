//! Report writers for the rerank benchmark.
//!
//! Output layout (under `outputs/benchmark/{fixture}/rerank/`):
//! `run_manifest.txt`, `aggregate_top{k}.md`,
//! `aggregate_by_query_type_top{k}.md`, `per_query_top{k}.md`,
//! `rerank_gain_by_query_type_top{k}.md`, `latency_cost.md`,
//! `relevance_top5.md`.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::judgments::evaluate::TOP_K_VALUES;
use crate::rerank_benchmark::{
    RERANK_METHODS, RerankBenchmarkRun, RerankLatency, RerankRelevance, RerankRow,
};

/// Write the full report set for a benchmark run.
pub fn write_all(run: &RerankBenchmarkRun) -> Result<(), Box<dyn std::error::Error>> {
    let out = &run.output_dir;
    write_run_manifest(run)?;
    for &top_k in TOP_K_VALUES {
        write_aggregate(out, &run.rows, top_k)?;
        write_aggregate_by_query_type(out, &run.rows, top_k)?;
        write_per_query(out, &run.rows, top_k)?;
        write_rerank_gain(out, &run.rows, top_k)?;
    }
    write_latency_cost(out, &run.latency)?;
    write_relevance(out, &run.relevance)?;
    Ok(())
}

fn write_table(
    path: &Path,
    title: &str,
    headers: &[&str],
    rows: &[Vec<String>],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::create(path)?;
    writeln!(file, "# {title}")?;
    writeln!(file)?;
    writeln!(file, "| {} |", headers.join(" | "))?;
    writeln!(
        file,
        "|{}|",
        headers.iter().map(|_| "---").collect::<Vec<_>>().join("|")
    )?;
    for row in rows {
        writeln!(file, "| {} |", row.join(" | "))?;
    }
    Ok(())
}

fn format_first_hit(values: &[Option<f64>]) -> String {
    let present: Vec<f64> = values.iter().filter_map(|value| *value).collect();
    if present.is_empty() {
        return "n/a".to_string();
    }
    format!("{:.1}", present.iter().sum::<f64>() / present.len() as f64)
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

/// Rows for one top-k cutoff grouped by (baseline, method).
fn grouped(rows: &[RerankRow], top_k: usize) -> BTreeMap<(&str, &str), Vec<&RerankRow>> {
    let mut map: BTreeMap<(&str, &str), Vec<&RerankRow>> = BTreeMap::new();
    for row in rows.iter().filter(|row| row.top_k == top_k) {
        map.entry((row.baseline.as_str(), row.method.as_str()))
            .or_default()
            .push(row);
    }
    map
}

/// Write `run_manifest.txt` describing data sources and the rerank matrix.
fn write_run_manifest(run: &RerankBenchmarkRun) -> Result<(), Box<dyn std::error::Error>> {
    let path = run.output_dir.join("run_manifest.txt");
    let mut file = File::create(&path)?;
    writeln!(file, "=== Rerank Benchmark Run Manifest ===")?;
    writeln!(file)?;
    writeln!(file, "project: {}", run.project)?;
    writeln!(file, "rerank model key: {}", run.model_key)?;
    writeln!(file, "rerank model: {}", run.model_name)?;
    writeln!(file, "mode: {}", run.mode)?;
    writeln!(file, "candidate depth: {}", run.depth)?;
    writeln!(file, "score fusion: {}", run.fusion)?;
    writeln!(file, "judgments: {}", run.judgment_count)?;
    writeln!(file, "methods: {}", run.methods.join(", "))?;
    writeln!(
        file,
        "text source rule: {}",
        crate::rerank_benchmark::TEXT_SOURCE_RULE
    )?;
    writeln!(
        file,
        "truncate chars: {}",
        crate::rerank_benchmark::RERANK_TRUNCATE_CHARS
    )?;
    writeln!(
        file,
        "control semantics: control and reranked rows share the candidate list; rerank applies to the embedding path only"
    )?;
    writeln!(file)?;
    for baseline in &run.baselines {
        writeln!(
            file,
            "baseline {}: source_hash={} emb_chunks={} bm25_chunks={} methods=[{}]",
            baseline.baseline,
            baseline.source_hash,
            baseline.emb_chunks,
            baseline.bm25_chunks,
            baseline.methods.join(", ")
        )?;
    }
    Ok(())
}

/// Write `aggregate_top{k}.md` with mean P/R/F1 per (baseline, method).
fn write_aggregate(
    out: &Path,
    rows: &[RerankRow],
    top_k: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let headers = [
        "baseline",
        "method",
        "queries",
        "P_any",
        "R_any",
        "F1_any",
        "P_strong",
        "R_strong",
        "F1_strong",
        "1st_hit",
        "redund",
    ];
    let mut table = Vec::new();
    for ((baseline, method), group) in grouped(rows, top_k) {
        let first_hits: Vec<Option<f64>> = group.iter().map(|row| row.avg_first_hit_rank).collect();
        table.push(vec![
            baseline.to_string(),
            method.to_string(),
            group.len().to_string(),
            format!(
                "{:.3}",
                mean(
                    &group
                        .iter()
                        .map(|row| row.range.precision_any)
                        .collect::<Vec<_>>()
                )
            ),
            format!(
                "{:.3}",
                mean(
                    &group
                        .iter()
                        .map(|row| row.range.recall_any)
                        .collect::<Vec<_>>()
                )
            ),
            format!(
                "{:.3}",
                mean(&group.iter().map(|row| row.range.f1_any).collect::<Vec<_>>())
            ),
            format!(
                "{:.3}",
                mean(
                    &group
                        .iter()
                        .map(|row| row.range.precision_strong)
                        .collect::<Vec<_>>()
                )
            ),
            format!(
                "{:.3}",
                mean(
                    &group
                        .iter()
                        .map(|row| row.range.recall_strong)
                        .collect::<Vec<_>>()
                )
            ),
            format!(
                "{:.3}",
                mean(
                    &group
                        .iter()
                        .map(|row| row.range.f1_strong)
                        .collect::<Vec<_>>()
                )
            ),
            format_first_hit(&first_hits),
            format!(
                "{:.1}",
                mean(
                    &group
                        .iter()
                        .map(|row| row.redundant_coverage as f64)
                        .collect::<Vec<_>>()
                )
            ),
        ]);
    }
    write_table(
        &out.join(format!("aggregate_top{top_k}.md")),
        &format!("Rerank aggregate (top-{top_k})"),
        &headers,
        &table,
    )
}

/// Write `aggregate_by_query_type_top{k}.md`.
fn write_aggregate_by_query_type(
    out: &Path,
    rows: &[RerankRow],
    top_k: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let headers = [
        "baseline",
        "method",
        "query_type",
        "queries",
        "R_any",
        "F1_any",
        "R_strong",
        "F1_strong",
        "1st_hit",
    ];
    let mut map: BTreeMap<(String, String, String), Vec<&RerankRow>> = BTreeMap::new();
    for row in rows.iter().filter(|row| row.top_k == top_k) {
        map.entry((
            row.baseline.clone(),
            row.method.clone(),
            row.query_type.to_string(),
        ))
        .or_default()
        .push(row);
    }
    let mut table = Vec::new();
    for ((baseline, method, query_type), group) in &map {
        let first_hits: Vec<Option<f64>> = group.iter().map(|row| row.avg_first_hit_rank).collect();
        table.push(vec![
            baseline.to_string(),
            method.to_string(),
            query_type.to_string(),
            group.len().to_string(),
            format!(
                "{:.3}",
                mean(
                    &group
                        .iter()
                        .map(|row| row.range.recall_any)
                        .collect::<Vec<_>>()
                )
            ),
            format!(
                "{:.3}",
                mean(&group.iter().map(|row| row.range.f1_any).collect::<Vec<_>>())
            ),
            format!(
                "{:.3}",
                mean(
                    &group
                        .iter()
                        .map(|row| row.range.recall_strong)
                        .collect::<Vec<_>>()
                )
            ),
            format!(
                "{:.3}",
                mean(
                    &group
                        .iter()
                        .map(|row| row.range.f1_strong)
                        .collect::<Vec<_>>()
                )
            ),
            format_first_hit(&first_hits),
        ]);
    }
    write_table(
        &out.join(format!("aggregate_by_query_type_top{top_k}.md")),
        &format!("Rerank aggregate by query type (top-{top_k})"),
        &headers,
        &table,
    )
}

/// Write `per_query_top{k}.md` with one row per (baseline, method, query).
fn write_per_query(
    out: &Path,
    rows: &[RerankRow],
    top_k: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let headers = [
        "baseline",
        "method",
        "query",
        "query_type",
        "P_any",
        "R_any",
        "F1_any",
        "R_strong",
        "F1_strong",
        "1st_hit",
        "redund",
    ];
    let mut filtered: Vec<&RerankRow> = rows.iter().filter(|row| row.top_k == top_k).collect();
    filtered.sort_by(|left, right| {
        (
            left.baseline.as_str(),
            left.method.as_str(),
            left.query_id.as_str(),
        )
            .cmp(&(
                right.baseline.as_str(),
                right.method.as_str(),
                right.query_id.as_str(),
            ))
    });
    let table: Vec<Vec<String>> = filtered
        .iter()
        .map(|row| {
            vec![
                row.baseline.clone(),
                row.method.clone(),
                row.query_id.clone(),
                row.query_type.to_string(),
                format!("{:.3}", row.range.precision_any),
                format!("{:.3}", row.range.recall_any),
                format!("{:.3}", row.range.f1_any),
                format!("{:.3}", row.range.recall_strong),
                format!("{:.3}", row.range.f1_strong),
                row.avg_first_hit_rank
                    .map(|rank| format!("{rank:.1}"))
                    .unwrap_or_else(|| "n/a".to_string()),
                row.redundant_coverage.to_string(),
            ]
        })
        .collect();
    write_table(
        &out.join(format!("per_query_top{top_k}.md")),
        &format!("Rerank per-query results (top-{top_k})"),
        &headers,
        &table,
    )
}

/// Write `rerank_gain_by_query_type_top{k}.md`: reranked minus control deltas.
fn write_rerank_gain(
    out: &Path,
    rows: &[RerankRow],
    top_k: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let headers = [
        "baseline",
        "method",
        "query_type",
        "queries",
        "dR_any",
        "dF1_any",
        "dR_strong",
        "wins",
        "ties",
        "losses",
        "1st_hit_control",
        "1st_hit_reranked",
    ];
    let mut table = Vec::new();
    for method in RERANK_METHODS {
        let reranked_label = format!("{method}+rerank");
        // Join control and reranked rows on (baseline, query).
        let mut controls: BTreeMap<(&str, &str), &RerankRow> = BTreeMap::new();
        let mut reranked: BTreeMap<(&str, &str), &RerankRow> = BTreeMap::new();
        for row in rows.iter().filter(|row| row.top_k == top_k) {
            if row.method == *method {
                controls.insert((row.baseline.as_str(), row.query_id.as_str()), row);
            } else if row.method == reranked_label {
                reranked.insert((row.baseline.as_str(), row.query_id.as_str()), row);
            }
        }
        let mut by_type: BTreeMap<(&str, String), Vec<(&RerankRow, &RerankRow)>> = BTreeMap::new();
        for ((baseline, query_id), control_row) in &controls {
            if let Some(reranked_row) = reranked.get(&(*baseline, *query_id)) {
                by_type
                    .entry((*baseline, control_row.query_type.to_string()))
                    .or_default()
                    .push((*control_row, *reranked_row));
            }
        }
        for ((baseline, query_type), pairs) in &by_type {
            let delta_recall: Vec<f64> = pairs
                .iter()
                .map(|(control_row, reranked_row)| {
                    reranked_row.range.recall_any - control_row.range.recall_any
                })
                .collect();
            let delta_f1: Vec<f64> = pairs
                .iter()
                .map(|(control_row, reranked_row)| {
                    reranked_row.range.f1_any - control_row.range.f1_any
                })
                .collect();
            let delta_strong: Vec<f64> = pairs
                .iter()
                .map(|(control_row, reranked_row)| {
                    reranked_row.range.recall_strong - control_row.range.recall_strong
                })
                .collect();
            let wins = delta_f1.iter().filter(|delta| **delta > 0.0).count();
            let losses = delta_f1.iter().filter(|delta| **delta < 0.0).count();
            let control_hits: Vec<Option<f64>> = pairs
                .iter()
                .map(|(control_row, _)| control_row.avg_first_hit_rank)
                .collect();
            let reranked_hits: Vec<Option<f64>> = pairs
                .iter()
                .map(|(_, reranked_row)| reranked_row.avg_first_hit_rank)
                .collect();
            table.push(vec![
                baseline.to_string(),
                (*method).to_string(),
                query_type.to_string(),
                pairs.len().to_string(),
                format!("{:+.3}", mean(&delta_recall)),
                format!("{:+.3}", mean(&delta_f1)),
                format!("{:+.3}", mean(&delta_strong)),
                wins.to_string(),
                (pairs.len() - wins - losses).to_string(),
                losses.to_string(),
                format_first_hit(&control_hits),
                format_first_hit(&reranked_hits),
            ]);
        }
    }
    write_table(
        &out.join(format!("rerank_gain_by_query_type_top{top_k}.md")),
        &format!("Rerank gain by query type (top-{top_k}, reranked minus control)"),
        &headers,
        &table,
    )
}

/// Write `latency_cost.md` with per-(baseline, method) call accounting.
fn write_latency_cost(
    out: &Path,
    latency: &[RerankLatency],
) -> Result<(), Box<dyn std::error::Error>> {
    let headers = [
        "baseline",
        "method",
        "scored",
        "failed",
        "failure_rate",
        "calls",
        "avg_ms",
        "p50_ms",
        "max_ms",
        "avg_candidates",
    ];
    let mut table = Vec::new();
    for entry in latency {
        let total = entry.scored_queries + entry.failed_queries;
        let mut sorted = entry.elapsed_ms.clone();
        sorted.sort_unstable();
        let p50 = sorted.get(sorted.len() / 2).copied().unwrap_or(0);
        table.push(vec![
            entry.baseline.clone(),
            entry.method.clone(),
            entry.scored_queries.to_string(),
            entry.failed_queries.to_string(),
            format!(
                "{:.3}",
                if total == 0 {
                    0.0
                } else {
                    entry.failed_queries as f64 / total as f64
                }
            ),
            entry.elapsed_ms.len().to_string(),
            format!(
                "{:.1}",
                mean(
                    &entry
                        .elapsed_ms
                        .iter()
                        .map(|ms| *ms as f64)
                        .collect::<Vec<_>>()
                )
            ),
            p50.to_string(),
            sorted.last().copied().unwrap_or(0).to_string(),
            format!(
                "{:.1}",
                mean(
                    &entry
                        .candidate_counts
                        .iter()
                        .map(|count| *count as f64)
                        .collect::<Vec<_>>()
                )
            ),
        ]);
    }
    write_table(
        &out.join("latency_cost.md"),
        "Rerank latency and cost",
        &headers,
        &table,
    )
}

/// Write `relevance_top5.md` with top-5 hit detail per (baseline, method, query).
fn write_relevance(
    out: &Path,
    relevance: &[RerankRelevance],
) -> Result<(), Box<dyn std::error::Error>> {
    let path = out.join("relevance_top5.md");
    let mut file = File::create(&path)?;
    writeln!(file, "# Rerank relevance (top-5)")?;
    let mut sorted: Vec<&RerankRelevance> = relevance.iter().collect();
    sorted.sort_by(|left, right| {
        (
            left.baseline.as_str(),
            left.method.as_str(),
            left.query_id.as_str(),
        )
            .cmp(&(
                right.baseline.as_str(),
                right.method.as_str(),
                right.query_id.as_str(),
            ))
    });
    for entry in sorted {
        writeln!(
            file,
            "\n## {} / {} / {}",
            entry.baseline, entry.method, entry.query_id
        )?;
        writeln!(file, "strong hits: {}", entry.strong.len())?;
        for (rank, location, _) in &entry.strong {
            writeln!(file, "- rank {rank}: {location}")?;
        }
        writeln!(file, "related hits: {}", entry.related.len())?;
        for (rank, location, _) in &entry.related {
            writeln!(file, "- rank {rank}: {location}")?;
        }
    }
    Ok(())
}
