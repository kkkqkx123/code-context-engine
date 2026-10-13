//! T-7 — finalize relation-build benchmark (covers P-5).
//!
//! P-5 flags that relation finalization replays the *entire* parsed relation
//! set from the operation spool even when only a handful of files changed in an
//! incremental update. This bench isolates the finalize cost and contrasts two
//! strategies at project scale (1k / 10k / 100k entities):
//!
//!   * full replay  — rebuild the `RelationIndex` from scratch (the current
//!     cost; O(N) in the number of entities).
//!   * incremental resume — keep the prior snapshot and replay only the
//!     changed files via `selective_cow_copy` (O(affected); the proposed fix).
//!
//! The full/incremental ratio is the headline number: for a small change set
//! it should be orders of magnitude, which is exactly what the resume design
//! buys. Memory materialization (`detached_clone`) during finalize is also
//! timed, since a full rebuild pays it on every run while a resume skips it.
//!
//! Run: `cargo run -p cce-codegraph --bench finalize_relation`
//! Baseline numbers are appended to `benches/results/finalize_relation.tsv`.

use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use cce_codegraph::index::{IndexBuilder, RelationIndex, RelationSnapshotIndex};
use cce_types::entity::ParseStatus;
use cce_types::relation::CallContext;
use cce_types::{
    Entity, EntityId, EntityKind, FileInfo, Position, RelationType, ResolvedRelation, Span,
};

const FUNCTIONS_PER_FILE: usize = 4;

fn make_entity(id: u64, name: &str) -> Entity {
    Entity {
        id: EntityId(id),
        kind: EntityKind::Function,
        name: name.to_string(),
        signature: String::new(),
        parameters: Vec::new(),
        return_type: None,
        span: Span {
            start_position: Position { row: 0, column: 0 },
            end_position: Position { row: 1, column: 0 },
            start_byte: 0,
            end_byte: 1,
        },
        depth: 0,
        parent: None,
        children: Vec::new(),
        doc_comment: None,
        modifiers: Vec::new(),
        attributes: Default::default(),
        metadata: Default::default(),
        is_stdlib: false,
        stdlib_category: None,
        subtype: None,
    }
}

fn make_file_info(id: String, path: String, entity_count: usize) -> FileInfo {
    FileInfo {
        id,
        path,
        language: "Rust".to_string(),
        file_hash: String::new(),
        file_size: 0,
        modified_time: 0,
        parse_status: ParseStatus::Success,
        parse_errors: Vec::new(),
        parse_version: 0,
        entity_count,
        relation_count: 0,
        export_count: 0,
        import_count: 0,
        depends_on: Vec::new(),
    }
}

/// Build a `RelationIndex` with `file_count` files, each carrying
/// `FUNCTIONS_PER_FILE` functions and intra-file calls.
fn build_index(file_count: usize) -> RelationIndex {
    let builder = IndexBuilder::new();
    let mut next_id: u64 = 1;
    for file_idx in 0..file_count {
        let path = format!("src/mod_{file_idx:05}.rs");
        let mut functions = Vec::new();
        let mut relations = Vec::new();
        let mut file_ids = Vec::new();
        for _ in 0..FUNCTIONS_PER_FILE {
            let id = next_id;
            next_id += 1;
            let name = format!("fn_{file_idx:05}_{id}");
            functions.push((EntityId(id), make_entity(id, &name)));
            file_ids.push(id);
        }
        let next_idx = file_idx + 1;
        for caller_id in &file_ids {
            for fn_idx in 0..FUNCTIONS_PER_FILE {
                relations.push(ResolvedRelation {
                    caller: EntityId(*caller_id),
                    callee_id: Some(EntityId(*caller_id + 1_000_000)),
                    callee_name: format!("fn_{next_idx:05}_{fn_idx}"),
                    relation_type: RelationType::DirectCall,
                    span: Span::default(),
                    is_external: false,
                    external_type: None,
                    callee_symbol: None,
                    stdlib_category: None,
                    owner_type: None,
                    call_context: CallContext::Direct,
                    overload_signature: None,
                    call_frequency: 1,
                    cfg_condition: None,
                });
            }
        }
        builder.process_file(
            make_file_info(path.clone(), path, functions.len()),
            functions,
            relations,
            None,
            vec![],
        );
    }
    builder.build()
}

fn bench_ms(iters: usize, mut f: impl FnMut()) -> f64 {
    f();
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    start.elapsed().as_secs_f64() * 1000.0 / iters as f64
}

fn main() {
    println!("finalize_relation benchmark (relation finalize / P-5 full-replay cost)");
    println!(
        "{:<16} {:<22} {:>12}",
        "entities", "strategy", "ms"
    );

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("benches/results/finalize_relation.tsv"),
        )
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(f, "# entities\tstrategy\tchanged_files\tms");
    }

    for entities in [1_000usize, 10_000, 100_000] {
        let file_count = entities / FUNCTIONS_PER_FILE;
        let index = build_index(file_count);

        // Full replay: rebuild the whole index from scratch (current cost).
        let full_ms = bench_ms(3, || {
            let _ = build_index(file_count);
        });
        println!(
            "{:<16} {:<22} {:>12.2}",
            entities, "full_rebuild(O(N))", full_ms
        );
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "{entities}\tfull_rebuild\t{file_count}\t{full_ms:.2}");
        }

        // Memory materialization paid on every full rebuild.
        let clone_ms = bench_ms(3, || {
            let _ = index.detached_clone();
        });
        println!(
            "{:<16} {:<22} {:>12.2}",
            entities, "memory_materialize", clone_ms
        );
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "{entities}\tmemory_materialize\t{file_count}\t{clone_ms:.2}");
        }

        // Incremental resume: replay only the changed files onto the snapshot
        // (O(affected) — the proposed fix). Fixed small change sets.
        for changed in [1usize, 10, 100] {
            let set: HashSet<String> = (0..changed)
                .map(|i| format!("src/mod_{i:05}.rs"))
                .collect();
            let inc_ms = bench_ms(10, || {
                let _ = index.selective_cow_copy(&set);
            });
            let ratio = full_ms / inc_ms.max(0.001);
            println!(
                "{:<16} {:<22} {:>12.2}  (full/incremental = {ratio:.1}x)",
                entities,
                format!("incremental({changed}f)"),
                inc_ms
            );
            if let Some(f) = out.as_mut() {
                let _ = writeln!(f, "{entities}\tincremental\t{changed}\t{inc_ms:.2}");
            }
        }
        drop(index);
    }

    // Criterion timing for the full rebuild at each scale (the P-5 regression point).
    let mut criterion = criterion::Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(15))
        .warm_up_time(Duration::from_secs(3));
    for entities in [1_000usize, 10_000, 100_000] {
        let file_count = entities / FUNCTIONS_PER_FILE;
        criterion.bench_function(&format!("full_rebuild_{entities}"), |b| {
            b.iter(|| {
                let _ = build_index(criterion::black_box(file_count));
            })
        });
    }
    criterion.final_summary();
}
