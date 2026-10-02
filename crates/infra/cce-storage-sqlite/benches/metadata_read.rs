//! SQLite metadata / chunk read benchmark.
//!
//! Covers the metadata-read benchmark need: batched chunk lookup versus
//! per-row lookup, file-scoped reads, paged scans with large offsets, and the
//! `max_entity_id` scan used for hot-update ID seeding. In-memory DB,
//! small data, comparison-oriented.
//!
//! Run with: `cargo run --bench metadata_read`
//!
//! Results are printed to stdout and appended to
//! `benches/results/metadata_read.tsv`.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use cce_storage_sqlite::{
    ChunkRecord, ChunkRepository, FileRecord, FileRepository, NewProjectRecord, ProjectRepository,
    SqliteClient,
};

fn bench_ms(iters: usize, mut f: impl FnMut()) -> f64 {
    f();
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    start.elapsed().as_secs_f64() * 1000.0 / iters as f64
}

fn main() {
    println!("metadata_read benchmark (debug, 500 chunks in-memory)");
    println!("{:<26} {:>12}", "case", "ms");

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/results/metadata_read.tsv"))
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(f, "# case\tms");
    }
    let mut row = |label: &str, ms: f64| {
        println!("{label:<26} {ms:>12.2}");
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "{label}\t{ms:.2}");
        }
    };

    let client = SqliteClient::in_memory().expect("in-memory");
    let now = 1_700_000_000i64;
    client
        .with_transaction(|tx| {
            let pid = ProjectRepository::insert(
                tx,
                &NewProjectRecord::new("bench".to_string(), "/tmp/bench".to_string()),
            )?;
            assert_eq!(pid, 1);
            for i in 0..50 {
                FileRepository::insert(
                    tx,
                    &FileRecord {
                        id: 0,
                        path: format!("src/mod_{i:02}.rs"),
                        language: "Rust".to_string(),
                        category: 4,
                        last_modified: now,
                        created_at: now,
                        project_id: 1,
                        content_hash: Some(format!("hash{i}")),
                    },
                )?;
            }
            for i in 0..500 {
                let chunk = ChunkRecord::new(
                    format!("chunk_{i:04}"),
                    format!("src/mod_{:02}.rs", i % 50),
                    format!("content of chunk {i} with some code tokens fn_{i}"),
                    i as i64,
                    i as i64 + 1,
                )
                .with_project_id(1)
                .with_epoch(0)
                .with_entity_ids(&[i as i64, i as i64 + 1000]);
                ChunkRepository::insert(tx, &chunk)?;
            }
            Ok(())
        })
        .expect("seed");

    let conn = client.read_connection().expect("read conn");

    // Single-row lookup vs batched lookup (hybrid recall fan-out).
    let single = bench_ms(20, || {
        let _ = ChunkRepository::get_by_id(&conn, "chunk_0042", 1).expect("get");
    });
    row("get_by_id x1", single);

    for n in [10usize, 50, 200] {
        let ids: Vec<String> = (0..n).map(|i| format!("chunk_{i:04}")).collect();
        let ms = bench_ms(10, || {
            let _ = ChunkRepository::get_by_chunk_ids(&conn, &ids, 1, None).expect("batch");
        });
        row(&format!("get_by_chunk_ids x{n}"), ms);
    }
    // Per-row loop for the same 50 ids (shows N-round-trip cost).
    let ids50: Vec<String> = (0..50).map(|i| format!("chunk_{i:04}")).collect();
    let loop_ms = bench_ms(5, || {
        for id in &ids50 {
            let _ = ChunkRepository::get_by_id(&conn, id, 1).expect("get");
        }
    });
    row("get_by_id loop x50", loop_ms);
    // Oversized batch spanning the chunking boundary (500 present + 700
    // missing ids exercise the multi-batch path).
    let ids_big: Vec<String> = (0..500)
        .map(|i| format!("chunk_{i:04}"))
        .chain((0..700).map(|i| format!("ghost_{i:04}")))
        .collect();
    let big_ms = bench_ms(5, || {
        let _ = ChunkRepository::get_by_chunk_ids(&conn, &ids_big, 1, None).expect("batch");
    });
    row("get_by_chunk_ids x1200", big_ms);

    let file_ms = bench_ms(10, || {
        let _ =
            ChunkRepository::get_by_file_and_project(&conn, "src/mod_01.rs", 1).expect("by file");
    });
    row("get_by_file", file_ms);

    let path_ms = bench_ms(10, || {
        let _ = FileRepository::get_by_path_and_project(&conn, "src/mod_01.rs", 1)
            .expect("file by path");
    });
    row("file_by_path", path_ms);

    // Paged scan: small vs large offset (pagination skip cost).
    let page0 = bench_ms(10, || {
        let _ = ChunkRepository::get_by_project_id_paged(&conn, 1, 20, 0).expect("page");
    });
    row("page limit20 off0", page0);
    let page400 = bench_ms(10, || {
        let _ = ChunkRepository::get_by_project_id_paged(&conn, 1, 20, 400).expect("page");
    });
    row("page limit20 off400", page400);

    // Full-epoch scan + per-row JSON parse for ID seeding.
    let max_ms = bench_ms(10, || {
        let _ = ChunkRepository::max_entity_id_for_epoch(&conn, 1, 0).expect("max id");
    });
    row("max_entity_id epoch", max_ms);
}
