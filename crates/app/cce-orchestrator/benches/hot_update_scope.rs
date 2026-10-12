//! Hot-update scope benchmark (§2.4, §2.5).
//!
//! Contrasts filtered-copy reads against detached full clones, and scoped
//! deltas against unscoped full-project comparison: view construction via
//! `LayeredSnapshotIndex::with_deltas` versus `detached_clone`, plus
//! `compute_delta` with a one-file scope, with no scope (full comparison,
//! warns), and with an explicitly empty scope (immediate empty delta).
//! Local data only; comparisons matter, not absolute numbers.
//!
//! Run with: `cargo run -p cce-orchestrator --bench hot_update_scope`
//!
//! Results are printed to stdout and appended to
//! `benches/results/hot_update_scope.tsv`.

use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use cce_codegraph::index::{
    IndexBuilder, LayeredSnapshotIndex, RelationDeltaOps, RelationIndex, RelationSnapshotIndex,
};
use cce_types::entity::ParseStatus;
use cce_types::relation::CallContext;
use cce_types::{
    Entity, EntityId, EntityKind, FileInfo, Position, RelationType, ResolvedRelation, Span,
};

const FUNCTIONS_PER_FILE: usize = 4;
const FILE_COUNT: usize = 300;

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
        attributes: std::collections::HashMap::new(),
        metadata: std::collections::HashMap::new(),
        is_stdlib: false,
        stdlib_category: None,
        subtype: None,
    }
}

fn make_file_info(path: String, entity_count: usize) -> FileInfo {
    FileInfo {
        id: path.clone(),
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

fn build_index(file_count: usize, changed: Option<usize>) -> RelationIndex {
    let builder = IndexBuilder::new();
    let mut next_id: u64 = 1;

    for file_idx in 0..file_count {
        let path = format!("src/mod_{file_idx:05}.rs");
        let mut functions = Vec::new();
        let mut file_ids = Vec::new();
        let mut relations = Vec::new();

        for fn_idx in 0..FUNCTIONS_PER_FILE {
            let id = next_id;
            next_id += 1;
            let name = if changed == Some(file_idx) {
                format!("fn_{file_idx:05}_{fn_idx}_edited")
            } else {
                format!("fn_{file_idx:05}_{fn_idx}")
            };
            functions.push((EntityId(id), make_entity(id, &name)));
            file_ids.push((id, name));
        }

        let next_idx = file_idx + 1;
        for (caller_id, _) in &file_ids {
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
            make_file_info(path.clone(), functions.len()),
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
    println!("hot_update_scope benchmark ({FILE_COUNT} files)");
    println!("{:<22} {:>12} {:>12}", "case", "ms", "detail");

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/results/hot_update_scope.tsv"),
        )
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(f, "# case\tms\tdetail");
    }
    let mut row = |label: &str, ms: f64, detail: &str| {
        println!("{label:<22} {ms:>12.3} {detail:>12}");
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "{label}\t{ms:.3}\t{detail}");
        }
    };

    let base = build_index(FILE_COUNT, None);
    let edited = build_index(FILE_COUNT, Some(0));
    let base_snapshot = Arc::new(RelationSnapshotIndex::from_index_shared(&base));
    let scope: HashSet<String> = ["src/mod_00000.rs".to_string()].into_iter().collect();
    let delta = Arc::new(edited.compute_delta(
        &base,
        100,
        99,
        "benchmark-fingerprint".to_string(),
        Some(&scope),
    ));

    let view_ms = bench_ms(5, || {
        let _ =
            LayeredSnapshotIndex::with_deltas(Arc::clone(&base_snapshot), vec![Arc::clone(&delta)]);
    });
    row("view_layered", view_ms, "1-delta");

    let clone_ms = bench_ms(3, || {
        let _ = base.detached_clone();
    });
    row("detached_clone", clone_ms, "full-copy");

    let scoped_ms = bench_ms(3, || {
        let _ = edited.compute_delta(
            &base,
            100,
            99,
            "benchmark-fingerprint".to_string(),
            Some(&scope),
        );
    });
    row("delta_scoped_1file", scoped_ms, "bounded");

    let unscoped_ms = bench_ms(3, || {
        let _ = edited.compute_delta(&base, 100, 99, "benchmark-fingerprint".to_string(), None);
    });
    row("delta_unscoped", unscoped_ms, "full-compare");

    let empty: HashSet<String> = HashSet::new();
    let empty_ms = bench_ms(10, || {
        let _ = edited.compute_delta(
            &base,
            100,
            99,
            "benchmark-fingerprint".to_string(),
            Some(&empty),
        );
    });
    let empty_delta = edited.compute_delta(
        &base,
        100,
        99,
        "benchmark-fingerprint".to_string(),
        Some(&empty),
    );
    assert!(
        empty_delta.added_entities.is_empty() && empty_delta.removed_entities.is_empty(),
        "empty scope must short-circuit to an empty delta"
    );
    row("delta_empty_scope", empty_ms, "short-circuit");
}
