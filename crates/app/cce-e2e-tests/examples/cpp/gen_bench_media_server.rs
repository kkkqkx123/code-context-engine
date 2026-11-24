use cce_e2e_tests::BenchmarkProject;
use cce_e2e_tests::bench_gen::run_bench_gen;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_bench_gen(BenchmarkProject::MediaServer.gen_config()).await
}
