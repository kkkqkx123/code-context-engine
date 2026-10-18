//! T-2 — embedding scheduling concurrency benchmark (covers P-1).
//!
//! P-1 calls out that the embedding stage currently serializes batches: each
//! `embed_batch` call awaits the previous one, so a fixed per-call latency
//! (network/rate-limit/compute) compounds linearly with the batch count.
//!
//! This bench drives the real `SuiteEmbeddingTransport::embed_batch` (the same
//! wrapper the orchestrator uses) with a deterministic fixed-delay mock
//! provider and compares two schedules:
//!
//!   * serial  — one `embed_batch` awaited after the previous one (today)
//!   * concurrent — all batches dispatched through `buffer_unordered(K)` so up
//!     to K calls overlap (the proposed fix)
//!
//! With a fixed per-call delay the concurrent schedule must finish in roughly
//! `ceil(BATCHES / K) * delay` wall time versus `BATCHES * delay` for serial,
//! which is exactly the speedup P-1 targets.
//!
//! Run: `cargo run -p cce-llm-client --bench embedding_scheduling`
//! Baseline numbers are appended to `benches/results/embedding_scheduling.tsv`.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use cce_llm_client::services::embedding::handler::SuiteEmbeddingTransport;
use futures::stream::{self, StreamExt};
use llm_suite_embedding::mock::{MockEmbeddingProvider, MockEmbeddingStep};

/// Vector dimension returned by the mock (must match the transport contract).
const DIM: usize = 384;
/// Number of embedding batches in one timed pass.
const BATCHES: usize = 64;
/// Texts per embedding batch.
const BATCH_SIZE: usize = 16;
/// Fixed per-call latency injected by the mock (milliseconds).
const DELAY_MS: u64 = 3;
/// Concurrency for the proposed (concurrent) schedule.
const CONCURRENCY: usize = 16;

/// Build one fixed-delay mock provider holding `BATCHES` `Delayed` steps so
/// every `embed_batch` call in a pass incurs exactly `DELAY_MS`.
fn make_transport() -> Arc<SuiteEmbeddingTransport<MockEmbeddingProvider>> {
    let steps: Vec<MockEmbeddingStep> = (0..BATCHES)
        .map(|_| MockEmbeddingStep::Delayed { delay_ms: DELAY_MS })
        .collect();
    let provider = MockEmbeddingProvider::with_steps(steps).with_dimension(DIM);
    Arc::new(SuiteEmbeddingTransport::for_testing(provider))
}

/// One pass of `BATCHES` fixed-size text batches.
fn make_batches() -> Vec<Vec<String>> {
    (0..BATCHES)
        .map(|b| {
            (0..BATCH_SIZE)
                .map(|i| format!("doc batch={b} line={i} content for embedding benchmark"))
                .collect()
        })
        .collect()
}

/// Serial schedule: each `embed_batch` awaits the previous one.
async fn run_serial(
    transport: &SuiteEmbeddingTransport<MockEmbeddingProvider>,
    batches: &[Vec<String>],
) {
    for batch in batches {
        transport
            .embed_batch(batch.clone())
            .await
            .expect("embed_batch");
    }
}

/// Concurrent schedule: up to `CONCURRENCY` batches overlap.
async fn run_concurrent(
    transport: Arc<SuiteEmbeddingTransport<MockEmbeddingProvider>>,
    batches: &[Vec<String>],
) {
    stream::iter(batches.iter().cloned())
        .map(|batch| {
            let transport = Arc::clone(&transport);
            async move { transport.embed_batch(batch).await }
        })
        .buffer_unordered(CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
}

fn main() {
    let batches = make_batches();
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");

    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("benches/results/embedding_scheduling.tsv"),
        )
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(
            f,
            "# scenario\tbatches\tbatch_size\tdelay_ms\tconcurrency\tms\tbatches_per_sec"
        );
    }

    // --- serial ---
    let serial_ms = {
        let transport = make_transport();
        let t = std::time::Instant::now();
        rt.block_on(run_serial(&transport, &batches));
        t.elapsed().as_secs_f64() * 1000.0
    };
    let serial_rate = BATCHES as f64 / (serial_ms / 1000.0);
    println!(
        "serial     : {BATCHES} batches x {BATCH_SIZE} texts, {DELAY_MS}ms delay -> {serial_ms:8.1} ms ({serial_rate:8.1} batches/s)"
    );
    if let Some(f) = out.as_mut() {
        let _ = writeln!(
            f,
            "serial\t{BATCHES}\t{BATCH_SIZE}\t{DELAY_MS}\t1\t{serial_ms:.1}\t{serial_rate:.1}"
        );
    }

    // --- concurrent ---
    let concurrent_ms = {
        let transport = make_transport();
        let t = std::time::Instant::now();
        rt.block_on(run_concurrent(transport, &batches));
        t.elapsed().as_secs_f64() * 1000.0
    };
    let concurrent_rate = BATCHES as f64 / (concurrent_ms / 1000.0);
    println!(
        "concurrent : {BATCHES} batches x {BATCH_SIZE} texts, {DELAY_MS}ms delay, K={CONCURRENCY} -> {concurrent_ms:8.1} ms ({concurrent_rate:8.1} batches/s)"
    );
    if let Some(f) = out.as_mut() {
        let _ = writeln!(
            f,
            "concurrent\t{BATCHES}\t{BATCH_SIZE}\t{DELAY_MS}\t{CONCURRENCY}\t{concurrent_ms:.1}\t{concurrent_rate:.1}"
        );
    }

    let speedup = serial_ms / concurrent_ms.max(0.001);
    println!("speedup (concurrent/serial): {speedup:.2}x");

    // --- criterion timing for the two schedules ---
    let mut criterion = criterion::Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(10))
        .warm_up_time(Duration::from_secs(3));

    let batches_ref = &batches;
    criterion.bench_function("serial_64x16", |b| {
        b.iter(|| {
            let transport = make_transport();
            rt.block_on(run_serial(&transport, batches_ref));
        })
    });
    criterion.bench_function("concurrent_64x16_k16", |b| {
        b.iter(|| {
            let transport = make_transport();
            rt.block_on(run_concurrent(transport, batches_ref));
        })
    });
    criterion.final_summary();
}
