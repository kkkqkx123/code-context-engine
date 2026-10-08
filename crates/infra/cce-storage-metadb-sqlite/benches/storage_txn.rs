//! Storage micro-batch transaction benchmark.
//!
//! Covers the storage micro-batch benchmark need: how write cost scales
//! with the embed batch size. A fixed chunk population is written through
//! one transaction per batch (`with_transaction` plus `insert_batch`), so
//! the sweep separates per-transaction fixed cost from per-row cost and
//! shows whether large batches develop a source-loading-style tail.
//!
//! Run with: `cargo run -p cce-storage-metadb-sqlite --bench storage_txn`
//!
//! Results are printed to stdout and appended to
//! `benches/results/storage_txn.tsv`.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use cce_storage_metadb_sqlite::{
    ChunkRecord, ChunkRepository, NewProjectRecord, ProjectRepository, SqliteClient,
};

const TOTAL_CHUNKS: usize = 2000;

fn make_chunk(i: usize) -> ChunkRecord {
    ChunkRecord::new(
        format!("chunk_{i:05}"),
        format!("src/mod_{:02}.rs", i % 50),
        format!("content of chunk {i} with some code tokens fn_{i}"),
        i as i64,
        i as i64 + 1,
    )
    .with_project_id(1)
    .with_epoch(0)
    .with_entity_ids(&[i as i64, i as i64 + 1000])
}

fn write_batch_size(client: &SqliteClient, chunks: &[ChunkRecord], batch: usize) -> f64 {
    let start = Instant::now();
    for window in chunks.chunks(batch) {
        client
            .with_transaction(|tx| ChunkRepository::insert_batch(tx, window))
            .expect("insert batch");
    }
    start.elapsed().as_secs_f64() * 1000.0
}

fn main() {
    println!("storage_txn benchmark (debug, {TOTAL_CHUNKS} chunks in-memory)");
    println!(
        "{:<16} {:>12} {:>12} {:>12}",
        "write_batch", "total(ms)", "per_txn(ms)", "txns"
    );

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/results/storage_txn.tsv"))
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(f, "# write_batch\ttotal_ms\tper_txn_ms\ttxns");
    }

    let chunks: Vec<ChunkRecord> = (0..TOTAL_CHUNKS).map(make_chunk).collect();

    for batch in [16usize, 32, 128, 500] {
        let client = SqliteClient::in_memory().expect("in-memory");
        client
            .with_transaction(|tx| {
                ProjectRepository::insert(
                    tx,
                    &NewProjectRecord::new("bench".to_string(), "/tmp/bench".to_string()),
                )?;
                Ok(())
            })
            .expect("seed project");
        let txns = TOTAL_CHUNKS.div_ceil(batch);
        let total_ms = write_batch_size(&client, &chunks, batch);
        let per_txn = total_ms / txns as f64;
        println!("{batch:<16} {total_ms:>12.2} {per_txn:>12.3} {txns:>12}");
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "{batch}\t{total_ms:.2}\t{per_txn:.3}\t{txns}");
        }

        // Read-back tail: one batched lookup over the whole population
        // versus a per-row loop over the first 200 ids.
        let conn = client.read_connection().expect("read conn");
        let ids: Vec<String> = (0..TOTAL_CHUNKS).map(|i| format!("chunk_{i:05}")).collect();
        let start = Instant::now();
        let _ = ChunkRepository::get_by_chunk_ids(&conn, &ids, 1, None).expect("batched read");
        let batched_ms = start.elapsed().as_secs_f64() * 1000.0;
        let first200: Vec<String> = ids[..200].to_vec();
        let start = Instant::now();
        for id in &first200 {
            let _ = ChunkRepository::get_by_id(&conn, id, 1).expect("single read");
        }
        let loop_ms = start.elapsed().as_secs_f64() * 1000.0;
        println!(
            "  read-back: batched x{TOTAL_CHUNKS} {batched_ms:.2} ms vs per-row loop x200 {loop_ms:.2} ms"
        );
        if let Some(f) = out.as_mut() {
            let _ = writeln!(
                f,
                "# batch{batch} readback batched_ms={batched_ms:.2} loop200_ms={loop_ms:.2}"
            );
        }
    }
}
