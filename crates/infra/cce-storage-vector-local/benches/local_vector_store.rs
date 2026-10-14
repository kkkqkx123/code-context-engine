//! T-6 — local vector-store wrapper benchmark (covers P-6).
//!
//! Exercises the `LocalVectorStore` wrapper (the `VectorStorage` trait path) the
//! way the orchestrator does: batched `upsert_points` through the
//! `spawn_blocking` wrapper, then `search_dense` reads. The simvec engine's WAL
//! fsync and index publish are internal to `upsert_points`, so this bench
//! measures the wrapper-visible behaviour — batch upsert throughput across
//! growing collection scales, per-batch latency (p50/p99) growth as the
//! collection grows (the proxy for publish-stall), and read latency while
//! writes are in flight. The true WAL-fsync share must be isolated inside simvec
//! (a separate submodule) and is out of scope here.
//!
//! Scale policy: this bench exists to compare *shapes* (sequential vs
//! concurrent throughput, batch-latency growth, read jitter during writes), not
//! to push absolute volume. The former 50w tier exhausted the sandbox memory
//! and the box was OOM-rebooted, so tiers default to 2k/5k/10k and
//! `CCE_BENCH_VECTOR_SCALES` (comma-separated) raises them on bigger hardware.
//! The scratch index lives under `target/bench-vector/` (a real filesystem)
//! rather than the system temp dir, which is tmpfs here and turns index bytes
//! straight into RAM pressure. A production-microbatch probe (32 points per
//! upsert, the orchestrator default) additionally runs at 5k vectors,
//! quantifying the fsync-per-point amplification the 1000-point microbatch
//! hides.
//!
//! Run: `cargo bench -p cce-storage-vector-local --bench local_vector_store`
//!
//! NOTE: this bench writes a real on-disk mmap index; sizes are bounded and the
//! directory is created under the system temp dir.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use cce_storage_common::{DenseSearchQuery, Payload, VectorPoint, VectorStorage};
use cce_storage_vector_local::LocalVectorStore;
use cce_types::{FileCategory, PointKind};

const DIM: usize = 128;
const MICROBATCH: usize = 1000;

/// Upsert size the orchestrator actually uses in production; the 1000-point
/// microbatch above exists to keep batch counts low, so this probe measures the
/// fsync-per-point amplification the production batch size really pays.
const PROD_MICROBATCH: usize = 32;
const PROD_PROBE_VECTORS: usize = 5_000;

/// Background writer payload for the read-jitter probe. Only "publishes are in
/// flight" is needed, so a single microbatch suffices; a full re-upsert would
/// triple the tier's write cost for no added signal.
const WRITER_MICROBATCHES: usize = 1;

/// Read samples taken while the background writer is publishing.
const READ_SAMPLES: usize = 100;

/// Collection sizes swept per run. Override on bigger hardware with
/// `CCE_BENCH_VECTOR_SCALES=10000,50000,100000`.
///
/// The defaults are deliberately small: the engine performs one fsync per
/// point (payload page write plus tracker save), so on a rotational disk the
/// sweep is fsync-bound at roughly 60 points/s and a 10k tier alone costs
/// ~3 minutes. The sweep only needs a range wide enough to expose growth, and
/// 5x is enough once the dominant cost is measured rather than guessed.
fn scales() -> Vec<usize> {
    match std::env::var("CCE_BENCH_VECTOR_SCALES") {
        Ok(raw) => {
            let parsed: Vec<usize> = raw
                .split(',')
                .filter_map(|part| part.trim().parse::<usize>().ok())
                .filter(|n| *n > 0)
                .collect();
            if parsed.is_empty() {
                vec![2_000, 5_000, 10_000]
            } else {
                parsed
            }
        }
        Err(_) => vec![2_000, 5_000, 10_000],
    }
}

fn make_point(i: usize) -> VectorPoint {
    let vec: Vec<f32> = (0..DIM)
        .map(|d| ((i as f32 + d as f32) * 0.001).sin())
        .collect();
    let payload = Payload::new(format!("src/mod_{i:06}.rs"))
        .with_group_id("proj-1".to_string())
        .with_epoch(1)
        .with_type(PointKind::Chunk)
        .with_category(FileCategory::Code);
    VectorPoint::new(format!("pt_{i:06}"), vec, payload)
}

fn make_points(n: usize) -> Vec<VectorPoint> {
    (0..n).map(make_point).collect()
}

/// Owned scratch directory under the crate's `target/` tree, removed on drop.
///
/// The default system temp dir is tmpfs on this box, so a multi-hundred-megabyte
/// mmap index written there consumes RAM directly and the large tiers get
/// OOM-killed mid-run. `target/` lives on a real filesystem, so the bench
/// exercises the same code path without charging the page cache for the index.
struct ScratchDir(PathBuf);

impl ScratchDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../target/bench-vector")
            .join(format!("{label}-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create scratch dir");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn open_store(dir: &ScratchDir) -> LocalVectorStore {
    let config = cce_config::modules::storage::LocalVectorConfig::default()
        .with_vector_size(DIM)
        .with_data_dir(dir.path().to_string_lossy().into_owned());
    let store = LocalVectorStore::open_at(dir.path(), &config).expect("open store");
    let rt = tokio::runtime::Runtime::new().expect("rt");
    rt.block_on(store.ensure_collection())
        .expect("ensure collection");
    store
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    println!("local_vector_store benchmark (dim={DIM}, microbatch={MICROBATCH})");

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/results/local_vector_store.tsv"),
        )
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(
            f,
            "# scenario\tvectors\ttotal_ms\tpts_per_sec\tbatch_p50_ms\tbatch_p99_ms"
        );
    }

    for target in scales() {
        let dir = ScratchDir::new(&format!("tier{target}"));
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
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "upsert_seq\t{target}\t{total_ms:.1}\t{dps:.0}\t{p50:.2}\t{p99:.2}");
        }

        // Concurrent upsert: split the index range across fixed tasks. Tasks
        // share one `Arc` point set and clone only their own microbatches, so
        // the phase costs one extra collection rather than a full duplicate of
        // the point set (the old `make_points` re-materialization doubled peak
        // bench memory at every tier).
        let dir2 = ScratchDir::new(&format!("tier{target}conc"));
        let store2 = open_store(&dir2);
        let tasks = 8;
        let per_task = target.div_ceil(tasks);
        let t = Instant::now();
        rt.block_on(async {
            let mut set = tokio::task::JoinSet::new();
            for task in 0..tasks {
                let start = task * per_task;
                let end = (start + per_task).min(target);
                if start >= end {
                    break;
                }
                let s = store2.clone();
                let pts = points.clone();
                set.spawn(async move {
                    for chunk in pts[start..end].chunks(MICROBATCH) {
                        s.upsert_points(chunk).await.expect("upsert");
                    }
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
        if let Some(f) = out.as_mut() {
            let _ = writeln!(
                f,
                "upsert_conc\t{target}\t{conc_ms:.1}\t{cdps:.0}\t{:.2}\t-",
                total_ms / conc_ms
            );
        }

        // Read latency while not writing: a single representative query.
        let q = vec![0.0f32; DIM];
        let t = Instant::now();
        let _ = rt.block_on(store.search_dense(DenseSearchQuery::new(q.clone(), 10)));
        let read_ms = t.elapsed().as_secs_f64() * 1000.0;
        println!("    read latency (warm index): {read_ms:7.2} ms");

        // Read jitter while a background writer is publishing. `rt.spawn`
        // (not `tokio::spawn`) because this block runs outside the runtime
        // context; the ambient `tokio::spawn` panics here.
        let store_r = store.clone();
        let writer = {
            let store_r = store_r.clone();
            // Only a few microbatches are re-published: the probe measures read
            // latency while writes are in flight, not full-tier rewrite cost.
            let chunks: Vec<Vec<VectorPoint>> = points
                .iter()
                .take(WRITER_MICROBATCHES * MICROBATCH)
                .cloned()
                .collect::<Vec<_>>()
                .chunks(MICROBATCH)
                .map(|c| c.to_vec())
                .collect();
            rt.spawn(async move {
                for chunk in chunks {
                    store_r.upsert_points(&chunk).await.expect("upsert");
                }
            })
        };
        let mut read_samples = Vec::with_capacity(READ_SAMPLES);
        rt.block_on(async {
            for _ in 0..READ_SAMPLES {
                let st = Instant::now();
                let _ = store_r
                    .search_dense(DenseSearchQuery::new(q.clone(), 10))
                    .await;
                read_samples.push(st.elapsed().as_secs_f64() * 1000.0);
            }
        });
        let _ = rt.block_on(writer);
        read_samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let rp50 = read_samples[read_samples.len() / 2];
        let rp99 = read_samples[read_samples.len() * 99 / 100];
        println!("    read during write : p50 {rp50:7.2} ms  p99 {rp99:7.2} ms");
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "read_idle\t{target}\t{read_ms:.2}\t-\t-\t-");
            let _ = writeln!(f, "read_during_write\t{target}\t-\t-\t{rp50:.2}\t{rp99:.2}");
        }
    }

    // Production-microbatch probe. The orchestrator's default upsert size is 32
    // points, so the 1000-point sweep understates the per-batch fixed cost
    // (WAL append + fsync + publish) the production path really pays per point.
    {
        let dir = ScratchDir::new(&format!("prodbatch{PROD_PROBE_VECTORS}"));
        let store = open_store(&dir);
        let points = make_points(PROD_PROBE_VECTORS);
        let t = Instant::now();
        let mut batch_ms = Vec::with_capacity(PROD_PROBE_VECTORS / PROD_MICROBATCH);
        rt.block_on(async {
            for chunk in points.chunks(PROD_MICROBATCH) {
                let bt = Instant::now();
                store.upsert_points(chunk).await.expect("upsert");
                batch_ms.push(bt.elapsed().as_secs_f64() * 1000.0);
            }
        });
        let total_ms = t.elapsed().as_secs_f64() * 1000.0;
        batch_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p50 = batch_ms[batch_ms.len() / 2];
        let p99 = batch_ms[batch_ms.len() * 99 / 100];
        let dps = PROD_PROBE_VECTORS as f64 / (total_ms / 1000.0);
        println!(
            "upsert {PROD_PROBE_VECTORS:>7} prodbatch({PROD_MICROBATCH}): {total_ms:9.1} ms  {dps:11.0} pts/s  batch p50 {p50:7.2} p99 {p99:7.2} ms"
        );
        if let Some(f) = out.as_mut() {
            let _ = writeln!(
                f,
                "upsert_seq_prodbatch\t{PROD_PROBE_VECTORS}\t{total_ms:.1}\t{dps:.0}\t{p50:.2}\t{p99:.2}"
            );
        }
    }

    // No criterion section here: the sweep above already records per-batch
    // p50/p99 for every tier, and criterion's minimum sample count (10) would
    // cost six minutes on the fsync-bound write path for a duplicate number.

    let _ = rt;
}
