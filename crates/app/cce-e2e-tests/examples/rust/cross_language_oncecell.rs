//! Independent cross-language benchmark for the once_cell fixture.
//!
//! Reports are written to `outputs/benchmark/cross_language/once_cell/`.
//! Cross-language queries are paired with English judgments sharing their
//! judged ranges; BM25 rows are diagnostic only.

use cce_e2e_tests::BenchmarkProject;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project = BenchmarkProject::OnceCell;
    cce_e2e_tests::cross_language::run_cross_language_benchmark(
        project.name(),
        &project.judgments(),
    )
}
