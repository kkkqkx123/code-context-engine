//! Retrieval-method benchmark for the flask fixture.
//!
//! Reuses `data/benchmark/{baseline}/flask/bge-m3/bench_data.rkyv` and
//! compares emb / bm25 / minmax-* / rrf-* recall methods. Reports are written
//! to `outputs/benchmark/flask/retrieval_method/`.

use cce_e2e_tests::BenchmarkProject;
use cce_e2e_tests::bench_data::EvaluationScope;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project = BenchmarkProject::Flask;
    let all_judgments = project.judgments();
    let scope = EvaluationScope::CoreRetrieval;
    let judgments: Vec<_> = all_judgments
        .iter()
        .filter(|j| scope.includes_query_type(j.query_type))
        .cloned()
        .collect();

    println!("=== Retrieval-Method Benchmark Configuration ===");
    println!("Evaluation scope: {scope}");
    println!(
        "Total judgments: {} ({} excluded)",
        judgments.len(),
        all_judgments.len() - judgments.len()
    );

    cce_e2e_tests::retrieval_method::run_retrieval_method_benchmark(project.name(), &judgments)?;
    Ok(())
}
