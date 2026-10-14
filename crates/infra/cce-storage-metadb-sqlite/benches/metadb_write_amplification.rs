//! T-4 — metadb batch-write amplification benchmark (covers P-3).
//!
//! Compares the current per-row `tx.execute` hot path (exposed through the
//! `RelationStorage` `*_upsert` methods) against the batch prepared-statement
//! path (`*Repository::insert_batch`, which reuses one prepared statement),
//! at the 100k-entity scale. Also compares checkpoint write amplification:
//! one transaction per work unit versus one merged transaction for the whole
//! batch. Each transaction under `synchronous=FULL` on a real file implies one
//! fsync, so the transaction count is reported as a proxy for fsync count.
//!
//! Run: `cargo run -p cce-storage-metadb-sqlite --bench metadb_write_amplification`
//! Baseline numbers are appended to `benches/results/metadb_write_amplification.tsv`.

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use cce_storage_common::metadb::{
    CheckpointRecord, CheckpointStatus, RelationStorage, WorkUnitCheckpointRecord, WorkUnitStatus,
};
use cce_storage_metadb_sqlite::repo::{
    CheckpointRepository, ChunkRepository, EntityRepository, FileRepository,
};
use cce_storage_metadb_sqlite::{ChunkRecord, EntityRecord, FileRecord, SqliteClient};

/// Scale of the write workload. 100k rows exercises the production write path
/// the way a medium-large project does.
const N: usize = 100_000;
const PROJECT_ID: i64 = 1;
const EPOCH: i64 = 1;

struct Dataset {
    files: Vec<FileRecord>,
    entities: Vec<EntityRecord>,
    chunks: Vec<ChunkRecord>,
}

fn make_files(n: usize) -> Vec<FileRecord> {
    (0..n)
        .map(|i| FileRecord {
            id: 0,
            path: format!("src/mod_{i:06}.rs"),
            language: "rust".to_string(),
            category: 4,
            last_modified: 1,
            created_at: 1,
            project_id: PROJECT_ID,
            content_hash: Some(format!("h{i:06}")),
        })
        .collect()
}

fn make_entities(n: usize) -> Vec<EntityRecord> {
    (0..n)
        .map(|i| EntityRecord {
            id: i as i64,
            name: format!("fn_{i:06}"),
            kind: "function".to_string(),
            file_id: ((i % 1000) + 1) as i64,
            signature: None,
            span_start_row: Some(0),
            span_end_row: Some(1),
            span_start_column: Some(0),
            span_end_column: Some(0),
            span_start_byte: Some(0),
            span_end_byte: Some(1),
            scoped_name: Some(format!("fn_{i:06}")),
            overload_discriminator: String::new(),
            depth: Some(0),
            parent_id: None,
            metadata: None,
            parameters_json: None,
            return_type: None,
            doc_comment: None,
            modifiers_json: None,
            project_id: PROJECT_ID,
            epoch: EPOCH,
            batch_id: 0,
            rank: 0.0,
        })
        .collect()
}

fn make_chunks(n: usize) -> Vec<ChunkRecord> {
    (0..n)
        .map(|i| {
            ChunkRecord::new(
                format!("chunk_{i:06}"),
                format!("src/mod_{:06}.rs", i % 1000),
                format!("content of chunk {i}"),
                i as i64,
                (i + 1) as i64,
            )
            .with_project_id(PROJECT_ID)
            .with_epoch(EPOCH)
            .with_entity_ids(&[i as i64])
        })
        .collect()
}

fn dataset() -> &'static Dataset {
    static D: OnceLock<Dataset> = OnceLock::new();
    D.get_or_init(|| Dataset {
        files: make_files(N),
        entities: make_entities(N),
        chunks: make_chunks(N),
    })
}

/// One transaction per write (the current `*_upsert` hot path: each row is a
/// separate `tx.execute`, no reused prepared statement).
/// Shared multi-thread runtime for driving the async `*_upsert` hot path.
fn rt() -> &'static tokio::runtime::Runtime {
    static R: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    R.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

async fn write_row_by_row(client: &SqliteClient, d: &Dataset) -> usize {
    RelationStorage::files_upsert(client, PROJECT_ID, EPOCH, &d.files)
        .await
        .expect("files");
    RelationStorage::entities_upsert(client, &d.entities)
        .await
        .expect("entities");
    RelationStorage::chunks_upsert(client, &d.chunks)
        .await
        .expect("chunks");
    N * 3
}

/// Batch prepared-statement path (`*_insert_batch` reuses one prepared
/// statement per call inside a single transaction).
fn write_batch(client: &SqliteClient, d: &Dataset) -> usize {
    client
        .with_transaction(|tx| FileRepository::insert_batch(tx, &d.files))
        .expect("files");
    client
        .with_transaction(|tx| EntityRepository::insert_batch(tx, &d.entities))
        .expect("entities");
    client
        .with_transaction(|tx| ChunkRepository::insert_batch(tx, &d.chunks))
        .expect("chunks");
    N * 3
}

/// Checkpoint: one transaction per work unit (current cost).
fn checkpoint_per_unit(client: &SqliteClient, units: usize) -> usize {
    for u in 0..units {
        let rec = WorkUnitCheckpointRecord {
            id: None,
            project_id: PROJECT_ID,
            operation_id: "bench-op".to_string(),
            stage: "index".to_string(),
            target_epoch: EPOCH,
            work_unit_hash: format!("wu_{u:08}"),
            status: WorkUnitStatus::Committed,
            item_count: 1,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        client
            .with_transaction(|tx| CheckpointRepository::insert_work_unit(tx, &rec))
            .expect("checkpoint unit");
    }
    units
}

/// Checkpoint: merged into a single transaction (proposed fix).
fn checkpoint_merged(client: &SqliteClient, units: usize) -> usize {
    client
        .with_transaction(|tx| {
            for u in 0..units {
                let rec = WorkUnitCheckpointRecord {
                    id: None,
                    project_id: PROJECT_ID,
                    operation_id: "bench-op".to_string(),
                    stage: "index".to_string(),
                    target_epoch: EPOCH,
                    work_unit_hash: format!("wu_{u:08}"),
                    status: WorkUnitStatus::Committed,
                    item_count: 1,
                    created_at: "2026-01-01T00:00:00Z".to_string(),
                    updated_at: "2026-01-01T00:00:00Z".to_string(),
                };
                CheckpointRepository::insert_work_unit(tx, &rec)?;
            }
            Ok(())
        })
        .expect("checkpoint merged");
    units
}

fn setup_client() -> SqliteClient {
    let client = SqliteClient::in_memory().expect("in-memory client");
    client
        .with_transaction(|tx| {
            cce_storage_metadb_sqlite::ProjectRepository::insert(
                tx,
                &cce_storage_metadb_sqlite::NewProjectRecord::new(
                    "bench".to_string(),
                    "/tmp/bench".to_string(),
                ),
            )?;
            CheckpointRepository::create_checkpoint(
                tx,
                PROJECT_ID,
                &CheckpointRecord {
                    id: None,
                    project_id: PROJECT_ID,
                    operation_id: "bench-op".to_string(),
                    operation_type: "index".to_string(),
                    root_dir: "/tmp/bench".to_string(),
                    total_files: 1000,
                    batch_size: 100,
                    current_batch_index: 0,
                    current_phase: "index".to_string(),
                    file_list_hash: None,
                    created_at: "2026-01-01T00:00:00Z".to_string(),
                    updated_at: "2026-01-01T00:00:00Z".to_string(),
                    last_error: None,
                    failure_count: 0,
                    status: CheckpointStatus::InProgress,
                    active_flag: true,
                    priority: 0,
                    last_heartbeat: None,
                    failed_at: None,
                },
            )?;
            Ok(())
        })
        .expect("seed project");
    client
}

fn main() {
    let d = dataset();

    // Human-readable baseline (single timed pass) + TSV append.
    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("benches/results/metadb_write_amplification.tsv"),
        )
        .ok();
    let _ = out.as_mut().map(|f| {
        let _ = writeln!(
            f,
            "# scenario\trows\trows_per_sec\ttxns\ttxns_per_sec\twrite_ms"
        );
    });

    // --- row-by-row vs batch ---
    let client = setup_client();
    let t = Instant::now();
    let rows = rt().block_on(write_row_by_row(&client, d));
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    let rps = rows as f64 / t.elapsed().as_secs_f64();
    println!("row_by_row        : {rows} rows in {ms:8.1} ms -> {rps:12.0} rows/s");
    let _ = out.as_mut().map(|f| {
        let _ = writeln!(
            f,
            "row_by_row\t{rows}\t{rps:.0}\t3\t{:.0}\t{ms:.1}",
            3.0 / t.elapsed().as_secs_f64()
        );
    });

    let client = setup_client();
    let t = Instant::now();
    let rows = write_batch(&client, d);
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    let rps = rows as f64 / t.elapsed().as_secs_f64();
    println!("batch_prepared   : {rows} rows in {ms:8.1} ms -> {rps:12.0} rows/s");
    let _ = out.as_mut().map(|f| {
        let _ = writeln!(
            f,
            "batch_prepared\t{rows}\t{rps:.0}\t3\t{:.0}\t{ms:.1}",
            3.0 / t.elapsed().as_secs_f64()
        );
    });

    // --- checkpoint per-unit vs merged ---
    let units = 1_000;
    let client = setup_client();
    let t = Instant::now();
    let txns = checkpoint_per_unit(&client, units);
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    let tps = txns as f64 / t.elapsed().as_secs_f64();
    println!("checkpoint_per_unit: {txns} txns in {ms:8.1} ms -> {tps:10.0} txns/s (fsyncs)");
    let _ = out.as_mut().map(|f| {
        let _ = writeln!(f, "checkpoint_per_unit\t0\t0\t{txns}\t{tps:.0}\t{ms:.1}");
    });

    let client = setup_client();
    let t = Instant::now();
    let txns = checkpoint_merged(&client, units);
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    let tps = txns as f64 / t.elapsed().as_secs_f64();
    println!("checkpoint_merged  : {txns} txns in {ms:8.1} ms -> {tps:10.0} txns/s (fsyncs)");
    let _ = out.as_mut().map(|f| {
        let _ = writeln!(f, "checkpoint_merged\t0\t0\t{txns}\t{tps:.0}\t{ms:.1}");
    });

    // --- criterion timing for the two main paths ---
    let mut criterion = criterion::Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(10))
        .warm_up_time(Duration::from_secs(3));
    let dref = dataset();

    criterion.bench_function("row_by_row_100k", |b| {
        b.iter(|| {
            let client = setup_client();
            rt().block_on(write_row_by_row(&client, dref));
        })
    });
    criterion.bench_function("batch_prepared_100k", |b| {
        b.iter(|| {
            let client = setup_client();
            write_batch(&client, dref);
        })
    });
    criterion.final_summary();
}
