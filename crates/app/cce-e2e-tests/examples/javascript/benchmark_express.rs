use cce_e2e_tests::BenchmarkProject;
use cce_e2e_tests::bench_data::EvaluationScope;
use cce_e2e_tests::benchmark_runner::run_standard_benchmark;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project = BenchmarkProject::Express;
    run_standard_benchmark(
        project.name(),
        project.judgments(),
        EvaluationScope::CoreRetrieval,
    )
}
