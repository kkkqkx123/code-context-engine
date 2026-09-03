//! Hot-update scaling benchmark.
//!
//! Verifies the acceptance criterion for the relation hot-update path: the
//! per-operation cost must no longer grow linearly with the total project file
//! count. The benchmark generates synthetic projects of increasing size, then
//! measures the three core hot-update operations of the layered pipeline:
//!
//! - `view` — wrapping the cached base into a `LayeredSnapshotIndex` and
//!   appending one delta (the operation that replaced the O(N) working copy);
//! - `compute_delta` — scoped to the single affected file (should stay flat);
//! - `apply_delta` — replay of a single-file delta (should stay flat).
//!
//! Run with: `cargo run --release --bench hot_update_scaling`
//!
//! The results table is printed to stdout and appended to
//! `benches/results/hot_update_scaling.tsv` for trend tracking.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cce_relation::index::{
    IndexBuilder, LayeredSnapshotIndex, RelationDeltaOps, RelationIndex, RelationSnapshotIndex,
};
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
        attributes: HashMap::new(),
        metadata: HashMap::new(),
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

/// Build a synthetic project: `file_count` files, each defining
/// `FUNCTIONS_PER_FILE` functions, with each file calling into the next file
/// (cross-file relations). `changed` selects the single file whose symbol
/// names differ, simulating a hot-update edit.
fn build_index(file_count: usize, changed: Option<usize>) -> RelationIndex {
    let builder = IndexBuilder::new();
    let mut next_id: u64 = 1;

    for file_idx in 0..file_count {
        let path = format!("src/mod_{file_idx:05}.rs");
        let mut functions = Vec::new();
        let mut relations = Vec::new();
        let mut file_ids = Vec::new();

        for fn_idx in 0..FUNCTIONS_PER_FILE {
            let id = next_id;
            next_id += 1;
            let changed = changed == Some(file_idx);
            let name = if changed {
                format!("fn_{file_idx:05}_{fn_idx}_edited")
            } else {
                format!("fn_{file_idx:05}_{fn_idx}")
            };
            functions.push((EntityId(id), make_entity(id, &name)));
            file_ids.push((id, name));
        }

        // Cross-file relations: each function of this file calls a function
        // of the next file (external/cross-file edge).
        let next_idx = file_idx + 1;
        for (caller_id, _) in &file_ids {
            for fn_idx in 0..FUNCTIONS_PER_FILE {
                let callee_name = format!("fn_{next_idx:05}_{fn_idx}");
                relations.push(ResolvedRelation {
                    caller: EntityId(*caller_id),
                    callee_id: Some(EntityId(*caller_id + 1_000_000)),
                    callee_name,
                    relation_type: RelationType::DirectCall,
                    span: Span::default(),
                    is_external: false,
                    external_type: None,
                    callee_symbol: None,
                    stdlib_category: None,
                    owner_type: None,
                    call_context: CallContext::Direct,
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

fn bench<F>(_label: &str, f: F) -> Duration
where
    F: FnOnce(),
{
    let start = Instant::now();
    f();
    start.elapsed()
}

fn main() {
    println!("hot_update_scaling benchmark");
    println!("synthetic projects: {FUNCTIONS_PER_FILE} functions/file, one changed file per run");
    println!();
    println!(
        "{:<10} {:>14} {:>14} {:>14} {:>14}",
        "files", "build", "view", "compute_delta", "apply_delta"
    );
    println!(
        "{:<10} {:>14} {:>14} {:>14} {:>14}",
        "", "(ms)", "(ms)", "(ms)", "(ms)"
    );

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("benches/results/hot_update_scaling.tsv"),
        )
        .ok();
    let mut write_row = |row: &str| {
        if let Some(file) = out.as_mut() {
            let _ = writeln!(file, "{row}");
        }
    };
    write_row("# files\tbuild_ms\tview_ms\tcompute_delta_ms\tapply_delta_ms");

    for file_count in [100usize, 300, 1000, 3000] {
        // Warm-up build (tree-sitter grammars / allocations already warm from
        // previous iterations; keep one full build for stable steady-state).
        let _ = build_index(50, None);

        let build_ms = bench("build", || {
            let _ = build_index(file_count, None);
        })
        .as_secs_f64()
            * 1000.0;

        // Base index: the materialized state that RelationBaseCache holds.
        let base = build_index(file_count, None);

        // Candidate: rebuild the whole project with file 0 edited.
        let candidate = build_index(file_count, Some(0));

        let affected: std::collections::HashSet<String> = [PathBuf::from("src/mod_00000.rs")
            .to_string_lossy()
            .into_owned()]
        .into_iter()
        .collect();

        let delta = candidate.compute_delta(
            &base,
            100,
            99,
            "benchmark-fingerprint".to_string(),
            Some(&affected),
        );

        // Layered hot-update path: the cached base is wrapped into a layered
        // view and the published delta is appended to the chain. No full copy
        // is made; this is the operation that replaced detached_clone.
        let view_ms = bench("view", || {
            let _ = LayeredSnapshotIndex::with_deltas(
                Arc::new(RelationSnapshotIndex::from_index_shared(&base)),
                vec![Arc::new(delta.clone())],
            );
        })
        .as_secs_f64()
            * 1000.0;

        let compute_ms = bench("compute_delta", || {
            let _ = candidate.compute_delta(
                &base,
                100,
                99,
                "benchmark-fingerprint".to_string(),
                Some(&affected),
            );
        })
        .as_secs_f64()
            * 1000.0;

        // Apply the delta to a detached copy of the base (the copy itself is
        // the compaction cold path; here only the replay is timed).
        let apply_target = base.detached_clone();
        let apply_ms = bench("apply_delta", || {
            apply_target.apply_delta(&delta);
        })
        .as_secs_f64()
            * 1000.0;

        println!(
            "{file_count:<10} {build_ms:>14.2} {view_ms:>14.2} {compute_ms:>14.2} {apply_ms:>14.2}"
        );
        write_row(&format!(
            "{file_count}\t{build_ms:.2}\t{view_ms:.2}\t{compute_ms:.2}\t{apply_ms:.2}"
        ));
    }
}
