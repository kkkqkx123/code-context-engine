//! Shared report formatting for benchmark evaluations.
//!
//! Provides CSV generators, Markdown reports, chunk dumping, and
//! summary printers used by all benchmark runners.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::bench_data::{BenchmarkData, RelevanceJudgment, RelevanceLevel};
use crate::judgments::evaluate::{
    BaselineResult, FileDocumentationMetric, FileDocumentationScore, RelevanceInfo, TOP_K_VALUES,
    collect_test_diagnostics,
};

/// Write the standard aggregate/per-query CSV set for all evaluation
/// variants into a single set of files.
///
/// Extra slices hold filtered variants (`no_test` drops test chunks,
/// `impl_only` also drops demo/sample trees). Rows carry a `variant` column
/// (`impl_only` | `no_test` | `all`) ordered with `impl_only` first: it is the
/// primary caliber, `no_test` the secondary, and `all` the unfiltered
/// reference only.
pub fn write_aggregate_reports(
    base: &Path,
    results: &[BaselineResult],
    no_test_results: &[BaselineResult],
    impl_only_results: &[BaselineResult],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut combined: Vec<&BaselineResult> = results.iter().collect();
    combined.extend(no_test_results.iter());
    combined.extend(impl_only_results.iter());
    for &top_k in TOP_K_VALUES {
        generate_aggregate_csv(base, &combined, top_k)?;
        generate_aggregate_by_query_type_csv(base, &combined, top_k)?;
        generate_per_query_csv(base, &combined, top_k)?;
    }
    Ok(())
}

/// Rank an evaluation variant for report row ordering.
///
/// `impl_only` is the primary caliber (test and demo trees excluded),
/// `no_test` the secondary, and `all` the unfiltered reference. Unknown
/// variant labels sort with `all`.
fn variant_rank(variant: &str) -> u8 {
    match variant {
        "impl_only" => 0,
        "no_test" => 1,
        _ => 2,
    }
}

/// Sort report rows by variant caliber, then by the remaining columns for
/// determinism. The variant label lives at column index 1 in every table.
fn sort_rows_by_variant(rows: &mut [Vec<String>]) {
    rows.sort_by(|a, b| {
        variant_rank(a.get(1).map(String::as_str).unwrap_or(""))
            .cmp(&variant_rank(b.get(1).map(String::as_str).unwrap_or("")))
            .then_with(|| a.cmp(b))
    });
}

/// Write aggregate metrics as a markdown table at a given top-k cutoff.
pub fn generate_aggregate_csv(
    base: &Path,
    results: &[&BaselineResult],
    top_k: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::collections::BTreeMap;

    let path = base.join(format!("aggregate_metrics_top{top_k}.md"));
    let mut f = File::create(&path)?;

    let mut map: BTreeMap<(String, String, String), Vec<&BaselineResult>> = BTreeMap::new();
    for r in results.iter().filter(|r| r.top_k == top_k) {
        map.entry((
            r.variant.clone(),
            r.baseline.clone(),
            r.retriever_type.clone(),
        ))
        .or_default()
        .push(r);
    }

    let headers = &[
        "k",
        "variant",
        "baseline",
        "retriever",
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
    ];
    let mut rows = Vec::new();

    for ((variant, baseline, retriever), group) in &map {
        let n = group.len() as f64;
        let avg_recall_strong = group.iter().map(|r| r.score.recall_strong).sum::<f64>() / n;
        let avg_recall_related = group.iter().map(|r| r.score.recall_related).sum::<f64>() / n;
        let avg_recall_any = group.iter().map(|r| r.score.recall_any).sum::<f64>() / n;
        let avg_precision_strong = group.iter().map(|r| r.score.precision_strong).sum::<f64>() / n;
        let avg_precision_related =
            group.iter().map(|r| r.score.precision_related).sum::<f64>() / n;
        let avg_precision_any = group.iter().map(|r| r.score.precision_any).sum::<f64>() / n;
        let avg_f1_strong = group.iter().map(|r| r.score.f1_strong).sum::<f64>() / n;
        let avg_f1_related = group.iter().map(|r| r.score.f1_related).sum::<f64>() / n;
        let avg_f1_any = group.iter().map(|r| r.score.f1_any).sum::<f64>() / n;

        rows.push(vec![
            format!("{top_k}"),
            variant.clone(),
            baseline.clone(),
            retriever.clone(),
            format!("{}", group.len()),
            format!("{:.4}", avg_recall_strong),
            format!("{:.4}", avg_recall_related),
            format!("{:.4}", avg_recall_any),
            format!("{:.4}", avg_precision_strong),
            format!("{:.4}", avg_precision_related),
            format!("{:.4}", avg_precision_any),
            format!("{:.4}", avg_f1_strong),
            format!("{:.4}", avg_f1_related),
            format!("{:.4}", avg_f1_any),
        ]);
    }

    sort_rows_by_variant(&mut rows);
    write_md_table(&mut f, headers, &rows)?;
    Ok(())
}

/// Write aggregate metrics split by query type at a given top-k cutoff.
pub fn generate_aggregate_by_query_type_csv(
    base: &Path,
    results: &[&BaselineResult],
    top_k: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::collections::BTreeMap;

    let path = base.join(format!("aggregate_metrics_by_query_type_top{top_k}.md"));
    let mut f = File::create(&path)?;

    let mut map: BTreeMap<(String, String, String, String), Vec<&BaselineResult>> = BTreeMap::new();
    for result in results.iter().filter(|result| result.top_k == top_k) {
        map.entry((
            result.variant.clone(),
            result.baseline.clone(),
            result.retriever_type.clone(),
            result.query_type.to_string(),
        ))
        .or_default()
        .push(result);
    }

    let headers = &[
        "k",
        "variant",
        "baseline",
        "retriever",
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
    ];
    let mut rows = Vec::new();

    for ((variant, baseline, retriever, query_type), group) in &map {
        let n = group.len() as f64;
        rows.push(vec![
            format!("{top_k}"),
            variant.clone(),
            baseline.clone(),
            retriever.clone(),
            query_type.clone(),
            format!("{}", group.len()),
            format!(
                "{:.4}",
                group.iter().map(|r| r.score.recall_strong).sum::<f64>() / n
            ),
            format!(
                "{:.4}",
                group.iter().map(|r| r.score.recall_related).sum::<f64>() / n
            ),
            format!(
                "{:.4}",
                group.iter().map(|r| r.score.recall_any).sum::<f64>() / n
            ),
            format!(
                "{:.4}",
                group.iter().map(|r| r.score.precision_strong).sum::<f64>() / n
            ),
            format!(
                "{:.4}",
                group.iter().map(|r| r.score.precision_related).sum::<f64>() / n
            ),
            format!(
                "{:.4}",
                group.iter().map(|r| r.score.precision_any).sum::<f64>() / n
            ),
            format!(
                "{:.4}",
                group.iter().map(|r| r.score.f1_strong).sum::<f64>() / n
            ),
            format!(
                "{:.4}",
                group.iter().map(|r| r.score.f1_related).sum::<f64>() / n
            ),
            format!(
                "{:.4}",
                group.iter().map(|r| r.score.f1_any).sum::<f64>() / n
            ),
        ]);
    }

    sort_rows_by_variant(&mut rows);
    write_md_table(&mut f, headers, &rows)?;
    Ok(())
}

/// Write per-query results as a markdown table for a given top-k.
pub fn generate_per_query_csv(
    base: &Path,
    results: &[&BaselineResult],
    top_k: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = base.join(format!("per_query_results_top{top_k}.md"));
    let mut f = File::create(&path)?;

    let headers = &[
        "k",
        "variant",
        "query",
        "query_type",
        "baseline",
        "retriever",
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
    ];
    let mut rows = Vec::new();

    for r in results.iter().filter(|r| r.top_k == top_k) {
        rows.push(vec![
            format!("{}", r.top_k),
            r.variant.clone(),
            r.query_id.clone(),
            r.query_type.to_string(),
            r.baseline.clone(),
            r.retriever_type.clone(),
            format!("{}", r.score.strong_matches),
            format!("{}", r.score.related_matches),
            format!("{}", r.score.any_matches),
            format!("{}", r.score.total_relevant),
            format!("{:.4}", r.score.recall_strong),
            format!("{:.4}", r.score.recall_related),
            format!("{:.4}", r.score.recall_any),
            format!("{:.4}", r.score.precision_strong),
            format!("{:.4}", r.score.precision_related),
            format!("{:.4}", r.score.precision_any),
            format!("{:.4}", r.score.f1_strong),
            format!("{:.4}", r.score.f1_related),
            format!("{:.4}", r.score.f1_any),
        ]);
    }

    sort_rows_by_variant(&mut rows);
    write_md_table(&mut f, headers, &rows)?;
    Ok(())
}

/// Write per-query BM25 differences between `full_pipeline` and
/// `full_pipeline_raw_source`.
///
/// The two baselines share chunk boundaries, titles and keywords; only the
/// content field differs (template plus sidecar versus raw source slice). A
/// sustained raw-source lead on semantic queries signals missing doc/code
/// co-occurrence or diluted term statistics rather than random fluctuation.
/// Only the primary `impl_only` plus `BM25` single path is compared so the
/// gate stays focused on the production BM25 content shape.
pub fn write_baseline_diff(
    base: &Path,
    results: &[BaselineResult],
) -> Result<(), Box<dyn std::error::Error>> {
    use std::collections::BTreeMap;
    for &top_k in TOP_K_VALUES {
        let mut full: BTreeMap<String, &BaselineResult> = BTreeMap::new();
        let mut raw: BTreeMap<String, &BaselineResult> = BTreeMap::new();
        for r in results
            .iter()
            .filter(|r| r.top_k == top_k && r.variant == "impl_only" && r.retriever_type == "BM25")
        {
            match r.baseline.as_str() {
                "full_pipeline" => {
                    full.insert(r.query_id.clone(), r);
                }
                "full_pipeline_raw_source" => {
                    raw.insert(r.query_id.clone(), r);
                }
                _ => {}
            }
        }
        if full.is_empty() || raw.is_empty() {
            continue;
        }
        let path = base.join(format!("baseline_diff_top{top_k}.md"));
        let mut f = File::create(&path)?;
        let headers = &[
            "query",
            "query_type",
            "full_F1a",
            "raw_F1a",
            "diff_raw_minus_full",
            "winner",
        ];
        let mut rows = Vec::new();
        let mut full_wins = 0usize;
        let mut raw_wins = 0usize;
        let mut ties = 0usize;
        for (query_id, full_result) in &full {
            let Some(raw_result) = raw.get(query_id) else {
                continue;
            };
            let full_f1 = full_result.score.f1_any;
            let raw_f1 = raw_result.score.f1_any;
            let diff = raw_f1 - full_f1;
            let winner = if diff > 1e-9 {
                raw_wins += 1;
                "raw"
            } else if diff < -1e-9 {
                full_wins += 1;
                "full"
            } else {
                ties += 1;
                "tie"
            };
            rows.push(vec![
                query_id.clone(),
                full_result.query_type.to_string(),
                format!("{full_f1:.4}"),
                format!("{raw_f1:.4}"),
                format!("{diff:+.4}"),
                winner.to_string(),
            ]);
        }
        rows.sort();
        write_md_table(&mut f, headers, &rows)?;
        writeln!(f, "\nfull_wins={full_wins} raw_wins={raw_wins} ties={ties}")?;
    }
    Ok(())
}

/// Write evaluation scope manifest as a markdown table.
pub fn generate_evaluation_scope_csv(
    base: &Path,
    all_judgments: &[RelevanceJudgment],
    filtered: &[RelevanceJudgment],
) -> Result<(), Box<dyn std::error::Error>> {
    let path = base.join("evaluation_scope.md");
    let mut f = File::create(&path)?;

    let headers = &["query_id", "query_text", "query_type", "included"];
    let mut rows = Vec::new();
    for j in all_judgments {
        let included = filtered.iter().any(|fj| fj.id == j.id);
        rows.push(vec![
            j.id.clone(),
            j.query_text.clone(),
            j.query_type.to_string(),
            if included {
                "yes".to_string()
            } else {
                "no".to_string()
            },
        ]);
    }

    write_md_table(&mut f, headers, &rows)?;
    Ok(())
}

/// Dump chunk metadata to text files for manual review.
///
/// Only metadata (chunk_id, entity, file, line range) is written; the full
/// text is omitted to avoid polluting version history with large generated
/// files on every benchmark run.
pub fn dump_chunks(
    base: &Path,
    label: &str,
    chunks: &[crate::bench_data::ChunkData],
    _texts: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = base.join(label);
    std::fs::create_dir_all(&dir)?;
    for (i, chunk) in chunks.iter().enumerate() {
        let safe = chunk
            .chunk_id
            .replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "_");
        let path = dir.join(format!(
            "{:04}_{}_{}_{}.txt",
            i + 1,
            safe,
            chunk.start_line,
            chunk.end_line
        ));
        let mut f = File::create(&path)?;
        writeln!(f, "chunk_id: {}", chunk.chunk_id)?;
        writeln!(
            f,
            "entity: {} | file: {} | lines: {}-{}",
            chunk.entity_name, chunk.file_path, chunk.start_line, chunk.end_line
        )?;
    }
    Ok(())
}

/// Write file-documentation scores as a markdown table.
pub fn write_file_documentation_scores(
    base: &Path,
    scores: &[FileDocumentationScore],
) -> Result<(), Box<dyn std::error::Error>> {
    let path = base.join("file_documentation_scores.md");
    let mut f = File::create(&path)?;

    let headers = &[
        "baseline",
        "query_id",
        "retriever",
        "rank",
        "document_id",
        "chunk_id",
        "score",
        "is_expected",
    ];
    let mut rows = Vec::new();
    for s in scores {
        rows.push(vec![
            s.baseline.clone(),
            s.query_id.clone(),
            s.retriever.clone(),
            format!("{}", s.rank),
            s.document_id.clone(),
            s.chunk_id.clone(),
            format!("{:.6}", s.score),
            format!("{}", s.is_expected),
        ]);
    }

    write_md_table(&mut f, headers, &rows)?;
    Ok(())
}

/// Write file-documentation metrics as a markdown table.
pub fn write_file_documentation_metrics(
    base: &Path,
    metrics: &[FileDocumentationMetric],
) -> Result<(), Box<dyn std::error::Error>> {
    let path = base.join("file_documentation_metrics.md");
    let mut f = File::create(&path)?;

    let headers = &["baseline", "query_id", "retriever", "RR"];
    let mut rows = Vec::new();
    for m in metrics {
        rows.push(vec![
            m.baseline.clone(),
            m.query_id.clone(),
            m.retriever.clone(),
            format!("{:.4}", m.rr),
        ]);
    }

    write_md_table(&mut f, headers, &rows)?;
    Ok(())
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

/// Normalize path for consistent file references.
pub fn normalize_path(path: &str) -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let normalized = path.replace('\\', "/");
    if let Some(stripped) = normalized.strip_prefix(&manifest_dir.replace('\\', "/")) {
        stripped.trim_start_matches('/').to_string()
    } else {
        normalized
    }
}

/// Print a console summary of evaluation results.
pub fn print_console_summary(results: &[BaselineResult]) {
    use std::collections::BTreeMap;

    println!("\n=== Console Summary ===");
    let top5: Vec<_> = results.iter().filter(|r| r.top_k == 5).collect();
    if top5.is_empty() {
        println!("(no top-5 results)");
        return;
    }

    let mut group_map: BTreeMap<(&str, &str, &str), Vec<&BaselineResult>> = BTreeMap::new();
    for r in &top5 {
        group_map
            .entry((
                r.variant.as_str(),
                r.retriever_type.as_str(),
                r.baseline.as_str(),
            ))
            .or_default()
            .push(r);
    }

    let mut current_variant = "";
    for ((variant, retriever, baseline), results) in &group_map {
        if *variant != current_variant {
            println!("\n  [variant: {variant}]");
            current_variant = variant;
        }
        let avg_p5: f64 =
            results.iter().map(|r| r.score.precision_any).sum::<f64>() / results.len() as f64;
        let avg_r5: f64 =
            results.iter().map(|r| r.score.recall_any).sum::<f64>() / results.len() as f64;
        let avg_f1: f64 =
            results.iter().map(|r| r.score.f1_any).sum::<f64>() / results.len() as f64;
        let total = results.len();
        println!(
            "    {baseline:20} [{retriever:4}]: P@5={avg_p5:.3} R@5={avg_r5:.3} F1@5={avg_f1:.3}  (n={total})"
        );
    }
}

/// Write run manifest with metadata about each baseline.
pub fn write_run_manifest(
    base: &Path,
    all_bench: &[(String, BenchmarkData)],
) -> Result<(), Box<dyn std::error::Error>> {
    let path = base.join("run_manifest.txt");
    let mut f = File::create(&path)?;
    writeln!(f, "=== Benchmark Run Manifest ===")?;
    writeln!(f)?;
    for (baseline, bench) in all_bench {
        writeln!(f, "Baseline: {baseline}")?;
        writeln!(f, "  Queries: {}", bench.queries.len())?;
        writeln!(
            f,
            "  Embedding chunks: {} (dim={})",
            bench.embedding.chunks.len(),
            bench.embedding.dimension
        )?;
        writeln!(f, "  BM25 chunks: {}", bench.bm25.chunks.len())?;
        writeln!(f)?;
    }
    Ok(())
}

/// Write test-code diagnostics for each baseline.
///
/// Lets readers distinguish "filtering mechanism not working" from "metrics
/// merely insensitive": the `no_test` variant removes `filtered` chunks, and
/// `top5_test_occ` shows how often test chunks would surface in the top-5 of
/// the `all` variant.
pub fn write_test_diagnostics(
    base: &Path,
    all_bench: &[(String, BenchmarkData)],
) -> Result<(), Box<dyn std::error::Error>> {
    let path = base.join("test_diagnostics.md");
    let mut f = File::create(&path)?;

    let headers = &[
        "baseline",
        "retriever",
        "total",
        "test",
        "unknown",
        "filtered",
        "top5_test_occ",
        "top5_queries_affected",
    ];
    let mut rows = Vec::new();
    for (baseline, bench) in all_bench {
        for diag in collect_test_diagnostics(bench) {
            rows.push(vec![
                baseline.clone(),
                diag.retriever.clone(),
                format!("{}", diag.total_chunks),
                format!("{}", diag.test_chunks),
                format!("{}", diag.unknown_chunks),
                format!("{}", diag.filtered_chunks),
                format!("{}", diag.top_k_test_occurrences),
                format!("{}", diag.top_k_test_queries_affected),
            ]);
        }
    }

    write_md_table(&mut f, headers, &rows)?;
    Ok(())
}

/// Chunk-shape statistics for one retriever path.
struct ChunkShape {
    chunks: usize,
    empty_ranges: usize,
    shared_range_chunks: usize,
    largest_range_group: usize,
    duplicate_texts: usize,
    /// BM25-only: duplicate titles across `bm25_documents` (header repetition
    /// proxy). Zero for the embedding path which carries no title index.
    title_duplicate_texts: usize,
}

/// Range signature of a chunk: its file plus its sorted coverage ranges.
///
/// Two chunks with the same signature claim exactly the same source lines, so
/// at most one of them can contribute unique coverage to a query.
fn range_signature(chunk: &crate::bench_data::ChunkData) -> String {
    let mut parts: Vec<String> = chunk
        .source_ranges
        .iter()
        .map(|range| format!("{}-{}", range.start_line, range.end_line))
        .collect();
    parts.sort();
    format!("{}|{}", chunk.file_path, parts.join(","))
}

fn chunk_shape_with_docs(
    chunks: &[crate::bench_data::ChunkData],
    texts: &[String],
    docs: &[crate::bench_data::Bm25DocRecord],
) -> ChunkShape {
    let mut signatures: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut empty_ranges = 0;
    for chunk in chunks {
        if chunk.source_ranges.is_empty() {
            empty_ranges += 1;
            continue;
        }
        *signatures.entry(range_signature(chunk)).or_insert(0) += 1;
    }
    let shared_range_chunks: usize = signatures.values().filter(|count| **count > 1).sum();
    let largest_range_group = signatures.values().copied().max().unwrap_or(0);

    let mut text_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for text in texts {
        *text_counts.entry(text.as_str()).or_insert(0) += 1;
    }
    let duplicate_texts: usize = text_counts.values().filter(|count| **count > 1).sum();
    let title_duplicate_texts = if docs.is_empty() {
        0
    } else {
        title_duplicates(docs)
    };
    ChunkShape {
        chunks: chunks.len(),
        empty_ranges,
        shared_range_chunks,
        largest_range_group,
        duplicate_texts,
        title_duplicate_texts,
    }
}

/// Title duplication across BM25 documents: repeated titles mean the same
/// header identity was indexed in many chunks instead of once.
fn title_duplicates(docs: &[crate::bench_data::Bm25DocRecord]) -> usize {
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for doc in docs {
        if doc.title.trim().is_empty() {
            continue;
        }
        *counts.entry(doc.title.as_str()).or_insert(0) += 1;
    }
    counts.values().filter(|count| **count > 1).sum()
}

/// Entity groups whose one source span produced several chunks.
///
/// Groups chunks by (entity set + range signature): a group with more than
/// one chunk is a single entity whose documentation and code were split into
/// separate index entries sharing the same coverage.
///
/// Write chunk-shape diagnostics: how many chunks claim a source range no
/// other chunk claims, and how many repeat another chunk's text.
///
/// A range shared by many chunks makes precision collapse even when recall
/// holds: the chunks cover each other, so none of them contributes unique
/// coverage. Repeated text is the same problem stated in the retrieval path.
/// BM25 rows additionally carry title duplication (header repetition
/// proxy): repeated titles mean the same header identity was indexed in
/// many chunks instead of once.
pub fn write_chunk_shape_diagnostics(
    base: &Path,
    all_bench: &[(String, BenchmarkData)],
) -> Result<(), Box<dyn std::error::Error>> {
    let path = base.join("chunk_shape_diagnostics.md");
    let mut f = File::create(&path)?;

    let headers = &[
        "baseline",
        "retriever",
        "chunks",
        "empty_ranges",
        "shared_range_chunks",
        "shared_range_share",
        "largest_range_group",
        "duplicate_texts",
        "duplicate_text_share",
        "title_dup_texts",
        "title_dup_share",
    ];
    let mut rows = Vec::new();
    for (baseline, bench) in all_bench {
        for (retriever, chunks, texts, docs) in [
            (
                "embedding",
                &bench.embedding.chunks,
                &bench.embedding.texts,
                &[][..],
            ),
            (
                "bm25",
                &bench.bm25.chunks,
                &bench.bm25.texts,
                &bench.bm25_documents[..],
            ),
        ] {
            let shape = chunk_shape_with_docs(chunks, texts, docs);
            let total = shape.chunks.max(1) as f64;
            rows.push(vec![
                baseline.clone(),
                retriever.to_string(),
                format!("{}", shape.chunks),
                format!("{}", shape.empty_ranges),
                format!("{}", shape.shared_range_chunks),
                format!("{:.3}", shape.shared_range_chunks as f64 / total),
                format!("{}", shape.largest_range_group),
                format!("{}", shape.duplicate_texts),
                format!("{:.3}", shape.duplicate_texts as f64 / total),
                format!("{}", shape.title_duplicate_texts),
                format!("{:.3}", shape.title_duplicate_texts as f64 / total),
            ]);
        }
    }

    write_md_table(&mut f, headers, &rows)?;
    Ok(())
}

/// Write top-5 relevance detail rows for every evaluation variant.
///
/// The `impl_only` variant is the primary observability source; `no_test`
/// rows show the test-filtered ranking, and `all` rows the unfiltered
/// reference. All variants now emit their relevance info instead of
/// discarding it at the runner entry point.
pub fn write_relevance_reports(
    base: &Path,
    all_relevance: &[RelevanceInfo],
    no_test_relevance: &[RelevanceInfo],
    impl_only_relevance: &[RelevanceInfo],
) -> Result<(), Box<dyn std::error::Error>> {
    let path = base.join("relevance_top5.md");
    let mut f = File::create(&path)?;

    let headers = &[
        "variant",
        "baseline",
        "query_id",
        "retriever",
        "rank",
        "level",
        "location",
    ];
    let mut rows = Vec::new();
    let push_rows = |variant: &str, infos: &[RelevanceInfo], rows: &mut Vec<Vec<String>>| {
        for info in infos {
            let mut collect = |chunks: &[(usize, String, RelevanceLevel)]| {
                for (rank, loc, level) in chunks {
                    rows.push(vec![
                        variant.to_string(),
                        info.baseline.clone(),
                        info.query_id.clone(),
                        info.retriever_type.clone(),
                        rank.to_string(),
                        format!("{:?}", level),
                        loc.clone(),
                    ]);
                }
            };
            collect(&info.strong_chunks);
            collect(&info.related_chunks);
        }
    };
    push_rows("impl_only", impl_only_relevance, &mut rows);
    push_rows("no_test", no_test_relevance, &mut rows);
    push_rows("all", all_relevance, &mut rows);

    write_md_table(&mut f, headers, &rows)?;
    Ok(())
}

/// Verify chunk directory has expected files.
pub fn verify_chunks_dir(dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("Failed to read dir {}: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "txt"))
        .collect();

    if entries.is_empty() {
        return Err(format!("No chunk files found in {}", dir.display()).into());
    }

    let mut names: Vec<String> = entries
        .iter()
        .filter_map(|e| {
            e.path()
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
        })
        .collect();
    names.sort();

    let expected = format!("{:04}", 1);
    if names.first().is_none_or(|n| !n.starts_with(&expected)) {
        return Err(format!("Expected first chunk to start with '{expected}'").into());
    }
    Ok(())
}
