//! Relation hot-update / query benchmark.
//!
//! Covers the relation hot-update and query benchmark needs: layered chain
//! degradation, scoped versus full diff, and call-chain depth. Extends the
//! `hot_update_scaling` bench with chain-length, diff-scope, clone, and
//! call-chain contrasts. Small synthetic graphs only.
//!
//! Run with: `cargo run --bench relation_perf`
//!
//! Results are printed to stdout and appended to
//! `benches/results/relation_perf.tsv`.

use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use cce_relation::index::{
    IndexBuilder, LayeredSnapshotIndex, RelationDeltaOps, RelationIndex, RelationSnapshotIndex,
};
use cce_relation::query::CallChainQuery;
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
    println!("relation_perf benchmark (debug, synthetic chain graphs)");
    println!("{:<28} {:>12}", "case", "ms");

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/results/relation_perf.tsv"))
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(f, "# case\tms");
    }
    let mut row = |label: &str, ms: f64| {
        println!("{label:<28} {ms:>12.2}");
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "{label}\t{ms:.2}");
        }
    };

    // Scoped vs full diff on a mid-size project.
    let file_count = 500;
    let base = build_index(file_count, None);
    let candidate = build_index(file_count, Some(0));
    let affected: HashSet<String> = ["src/mod_00000.rs".to_string()].into_iter().collect();
    let scoped = candidate.compute_delta(&base, 100, 99, "bench-fp".to_string(), Some(&affected));
    let scoped_ms = bench_ms(3, || {
        let _ = candidate.compute_delta(&base, 100, 99, "bench-fp".to_string(), Some(&affected));
    });
    row("compute_delta scoped 500", scoped_ms);
    let full_ms = bench_ms(3, || {
        let _ = candidate.compute_delta(&base, 100, 99, "bench-fp".to_string(), None);
    });
    row("compute_delta FULL 500", full_ms);

    // Filtered-copy vs full detached clone (single-file scope).
    let layered = LayeredSnapshotIndex::with_deltas(
        Arc::new(RelationSnapshotIndex::from_index_shared(&base)),
        vec![Arc::new(scoped.clone())],
    );
    let view_ms = bench_ms(5, || {
        let _ = LayeredSnapshotIndex::with_deltas(
            Arc::new(RelationSnapshotIndex::from_index_shared(&base)),
            vec![Arc::new(scoped.clone())],
        );
    });
    row("layered view wrap 500", view_ms);
    let clone_ms = bench_ms(2, || {
        let _ = base.detached_clone();
    });
    row("detached_clone FULL 500", clone_ms);
    let apply_target = base.detached_clone();
    let apply_ms = bench_ms(5, || {
        apply_target.apply_delta(&scoped);
    });
    row("apply_delta single 500", apply_ms);

    // Chain-length degradation: same delta repeated to isolate merge cost.
    for len in [1usize, 5, 10, 20] {
        let deltas: Vec<Arc<_>> = (0..len).map(|_| Arc::new(scoped.clone())).collect();
        let view = LayeredSnapshotIndex::with_deltas(
            Arc::new(RelationSnapshotIndex::from_index_shared(&base)),
            deltas,
        );
        let ms = bench_ms(3, || {
            let _ = view.to_canonical_snapshot("bench-fp".to_string());
        });
        row(&format!("canonical chain x{len}"), ms);
    }
    let _ = layered;

    // Call-chain depth on a long chain (query boundary).
    let chain = build_index(200, None);
    let query = CallChainQuery::from_index(chain);
    // Entity 1 is the first function; walk forward with different depths.
    for depth in [2usize, 6, 20] {
        let ms = bench_ms(5, || {
            let _ = query.query_forward_by_entity(EntityId(1), depth);
        });
        row(&format!("callchain fwd depth{depth}"), ms);
    }
}
