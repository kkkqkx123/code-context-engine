//! T-6 — local vector-store wrapper benchmark (covers P-6).
//!
//! Exercises the `LocalVectorStore` wrapper (the `VectorStorage` trait path) the
//! way the orchestrator does: batched `upsert_points` through the
//! `spawn_blocking` wrapper, then `search_dense` reads. The simvec engine's WAL
//! fsync and index publish are internal to `upsert_points`, so this bench
//! measures the wrapper-visible behaviour — batch upsert throughput across
//! collection scales (1w / 10w / 50w vectors), per-batch latency (p50/p99) as
//! the collection grows (the proxy for publish-stall), and read latency while
//! writes are in flight. The true WAL-fsync share must be isolated inside simvec
//! (a separate submodule) and is out of scope here per §5.6.
//!
//! Run: `cargo bench -p cce-storage-vector-local --bench local_vector_store`
//!
//! NOTE: this bench writes a real on-disk mmap index; sizes are bounded and the
//! directory is created under the system temp dir.

use std::sync::Arc;
use std::time::{Duration, Instant};

use cce_storage_common::{DenseSearchQuery, Payload, VectorPoint, VectorStorage};
use cce_storage_vector_local::LocalVectorStore;
use cce_types::{FileCategory, PointKind};
use tempfile::TempDir;

const DIM: usize = 128;
const MICROBATCH: usize = 1000;

fn make_point(i: usize) -> VectorPoint {
    let vec: Vec<f32> = (0..DIM).map(|d| ((i as f32 + d as f32) * 0.001).sin()).collect();
    let payload = Payload::new(format!("src/mod_{i:06}.rs"))
        .with_group_id(format!("proj-1"))
        .with_epoch(1)
        .with_type(PointKind::Chunk)
        .with_category(FileCategory::Code);
    VectorPoint::new(format!("pt_{i:06}"), vec, payload)
}

fn make_points(n: usize) -> Vec<VectorPoint> {
    (0..n).map(make_point).collect()
}

fn open_store(dir: &TempDir) -> LocalVectorStore {
    let config = cce_config::modules::storage::LocalVectorConfig::default()
        .with_vector_size(DIM)
        .with_data_dir(dir.path().to_string_lossy().into_owned());
    let store = LocalVectorStore::open_at(dir.path(), &config).expect("open store");
    let rt = tokio::runtime::Runtime::new().expect("rt");
    rt.block_on(store.ensure_collection()).expect("ensure collection");
    store
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    println!("local_vector_store benchmark (dim={DIM}, microbatch={MICROBATCH})");

    for target in [10_000usize, 100_000, 500_000] {
        let dir = TempDir::new().expect("tempdir");
        let store = open_store(&dir);
        let points = Arc::new(make_points(target));

        // Sequential batched upsert with per-batch latency capture.
        let t = Instant::now();
        let mut batch_ms = Vec::with_capacity(target / MICROBATCH);
        rt.block_on(async {
            for chunk in points.chunks(MICROBATCH) {
                let bt = Instant::now();
                store.upsert_points(chunk).await.expect("upsert");
                batch_ms.push(bt.elapsed().as_secs_f64() * 1000.0);
            }
        });
        let total_ms = t.elapsed().as_secs_f64() * 1000.0;
        batch_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p50 = batch_ms[batch_ms.len() / 2];
        let p99 = batch_ms[batch_ms.len() * 99 / 100];
        let dps = (target as f64) / (total_ms / 1000.0);
        println!(
            "upsert {target:>7} seq : {total_ms:9.1} ms  {dps:11.0} pts/s  batch p50 {p50:7.2} p99 {p99:7.2} ms"
        );

        // Concurrent upsert: split the work across N tasks cloning the store.
        let dir2 = TempDir::new().expect("tempdir");
        let store2 = open_store(&dir2);
        let points2 = Arc::new(make_points(target));
        let chunks: Vec<_> = points2.chunks(MICROBATCH).map(|c| c.to_vec()).collect();
        let t = Instant::now();
        rt.block_on(async {
            let mut set = tokio::task::JoinSet::new();
            for chunk in chunks {
                let s = store2.clone();
                set.spawn(async move {
                    s.upsert_points(&chunk).await.expect("upsert");
                });
            }
            while set.join_next().await.is_some() {}
        });
        let conc_ms = t.elapsed().as_secs_f64() * 1000.0;
        let cdps = (target as f64) / (conc_ms / 1000.0);
        println!(
            "upsert {target:>7} conc: {conc_ms:9.1} ms  {cdps:11.0} pts/s  (speedup {:.2}x)",
            total_ms / conc_ms
        );

        // Read latency while not writing: a single representative query.
        let q = vec![0.0f32; DIM];
        let t = Instant::now();
        let _ = rt.block_on(store.search_dense(DenseSearchQuery::new(q.clone(), 10)));
        let read_ms = t.elapsed().as_secs_f64() * 1000.0;
        println!("    read latency (warm index): {read_ms:7.2} ms");

        // Read jitter while a background writer is publishing.
        let store_r = store.clone();
        let writer = {
            let store_r = store_r.clone();
            let chunks: Vec<_> = points.chunks(MICROBATCH).map(|c| c.to_vec()).collect();
            tokio::spawn(async move {
                for chunk in chunks {
                    store_r.upsert_points(&chunk).await.expect("upsert");
                }
            })
        };
        let mut read_samples = Vec::with_capacity(200);
        rt.block_on(async {
            for _ in 0..200 {
                let st = Instant::now();
                let _ = store_r.search_dense(DenseSearchQuery::new(q.clone(), 10)).await;
                read_samples.push(st.elapsed().as_secs_f64() * 1000.0);
            }
        });
        let _ = rt.block_on(writer);
        read_samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let rp50 = read_samples[read_samples.len() / 2];
        let rp99 = read_samples[read_samples.len() * 99 / 100];
        println!("    read during write : p50 {rp50:7.2} ms  p99 {rp99:7.2} ms");
    }

    // Criterion timing for the 100k sequential upsert as a stable regression point.
    let mut criterion = criterion::Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(20))
        .warm_up_time(Duration::from_secs(3));
    let target = 100_000;
    let dir = TempDir::new().expect("tempdir");
    let store = open_store(&dir);
    let points = Arc::new(make_points(target));
    criterion.bench_function("upsert_100k_seq", |b| {
        b.iter(|| {
            let rt = tokio::runtime::Runtime::new().expect("rt");
            rt.block_on(async {
                for chunk in points.chunks(MICROBATCH) {
                    store.upsert_points(chunk).await.expect("upsert");
                }
            });
        })
    });
    criterion.final_summary();

    let _ = rt;
}
