//! Shared driver for the standard chunk-retrieval benchmarks.

use crate::bench_data::{BenchmarkData, EvaluationScope, RelevanceJudgment, load_benchmark_data};
use crate::judgments::evaluate::RelevanceInfo;
use crate::judgments::evaluate::{
    BASELINES, BaselineResult, BenchmarkPaths, RangeMode, evaluate_bm25_variants,
    evaluate_embedding_variants,
};
use crate::judgments::report::{
    generate_evaluation_scope_csv, print_console_summary, write_aggregate_reports,
    write_baseline_diff, write_chunk_shape_diagnostics, write_relevance_reports,
    write_run_manifest, write_test_diagnostics,
};

/// Run the standard precise + group-filled benchmark for one project.
pub fn run_standard_benchmark(
    project_name: &'static str,
    all_judgments: Vec<RelevanceJudgment>,
    scope: EvaluationScope,
) -> Result<(), Box<dyn std::error::Error>> {
    let judgments: Vec<RelevanceJudgment> = all_judgments
        .iter()
        .filter(|j| scope.includes_query_type(j.query_type))
        .cloned()
        .collect();

    if judgments.is_empty() {
        return Err("No queries match the evaluation scope".into());
    }

    println!("=== Benchmark Configuration ===");
    println!("Evaluation scope: {scope}");
    println!(
        "Total judgments: {} ({} excluded)",
        judgments.len(),
        all_judgments.len() - judgments.len()
    );

    let paths = BenchmarkPaths::new(project_name);
    let mut all_results: Vec<BaselineResult> = Vec::new();
    let mut no_test_results: Vec<BaselineResult> = Vec::new();
    let mut impl_only_results: Vec<BaselineResult> = Vec::new();
    let mut all_relevance: Vec<RelevanceInfo> = Vec::new();
    let mut no_test_relevance: Vec<RelevanceInfo> = Vec::new();
    let mut impl_only_relevance: Vec<RelevanceInfo> = Vec::new();
    let mut all_bench: Vec<(String, BenchmarkData)> = Vec::new();

    for baseline in BASELINES {
        let data_path = paths.data_dir(baseline);
        if !data_path.exists() {
            eprintln!(
                "WARNING: {baseline} data not found at {}",
                data_path.display()
            );
            continue;
        }
        println!(
            "Loading baseline '{baseline}' from: {}",
            data_path.display()
        );
        let bench = load_benchmark_data(&data_path)?;
        println!(
            "  {} queries, {} embedding chunks, {} BM25 chunks",
            bench.queries.len(),
            bench.embedding.chunks.len(),
            bench.bm25.chunks.len()
        );

        for range_mode in [RangeMode::Precise, RangeMode::GroupFilled] {
            let emb = evaluate_embedding_variants(baseline, &bench, &judgments, range_mode);
            all_results.extend(emb.all.results);
            all_relevance.extend(emb.all.relevance);
            no_test_results.extend(emb.no_test.results);
            no_test_relevance.extend(emb.no_test.relevance);
            impl_only_results.extend(emb.impl_only.results);
            impl_only_relevance.extend(emb.impl_only.relevance);

            let bm25 = evaluate_bm25_variants(baseline, &bench, &judgments, range_mode);
            all_results.extend(bm25.all.results);
            all_relevance.extend(bm25.all.relevance);
            no_test_results.extend(bm25.no_test.results);
            no_test_relevance.extend(bm25.no_test.relevance);
            impl_only_results.extend(bm25.impl_only.results);
            impl_only_relevance.extend(bm25.impl_only.relevance);
        }

        all_bench.push((baseline.to_string(), bench));
    }

    if all_results.is_empty() {
        eprintln!("No baseline data loaded. Run `gen_bench_{project_name}` first.");
        return Ok(());
    }

    let out = paths.output_dir();
    std::fs::create_dir_all(&out)?;
    write_run_manifest(&out, &all_bench)?;

    generate_evaluation_scope_csv(&out, &all_judgments, &judgments)?;

    write_aggregate_reports(&out, &all_results, &no_test_results, &impl_only_results)?;
    write_baseline_diff(&out, &impl_only_results)?;
    write_test_diagnostics(&out, &all_bench)?;
    write_chunk_shape_diagnostics(&out, &all_bench)?;
    write_relevance_reports(
        &out,
        &all_relevance,
        &no_test_relevance,
        &impl_only_relevance,
    )?;

    println!("\n✓ All evaluation files written to: {}", out.display());
    let mut summary_results = all_results;
    summary_results.extend(no_test_results);
    summary_results.extend(impl_only_results);
    print_console_summary(&summary_results);

    Ok(())
}
