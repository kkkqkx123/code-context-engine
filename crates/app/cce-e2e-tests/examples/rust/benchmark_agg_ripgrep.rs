//! Aggregation-enhance benchmark for the ripgrep fixture.
//!
//! Reuses `data/benchmark/full_pipeline/ripgrep/bge-m3/bench_data.rkyv` and
//! compares plain base rankings against offline relation/summary boosted
//! rankings. Reports are written to
//! `outputs/benchmark/ripgrep/aggregation_enhance/`.

use cce_e2e_tests::BenchmarkProject;
use cce_e2e_tests::bench_data::EvaluationScope;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project = BenchmarkProject::Ripgrep;
    let all_judgments = project.judgments();
    let scope = EvaluationScope::CoreRetrieval;
    let judgments: Vec<_> = all_judgments
        .iter()
        .filter(|j| scope.includes_query_type(j.query_type))
        .cloned()
        .collect();

    println!("=== Aggregation-Enhance Benchmark Configuration ===");
    println!("Evaluation scope: {scope}");
    println!(
        "Total judgments: {} ({} excluded)",
        judgments.len(),
        all_judgments.len() - judgments.len()
    );

    cce_e2e_tests::aggregation_enhance::run_aggregation_enhance_benchmark(
        project.name(),
        &judgments,
    )?;
    Ok(())
}
