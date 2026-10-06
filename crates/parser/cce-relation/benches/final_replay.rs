//! Relation terminal-build / stdlib-filter / transitive-closure benchmark.
//!
//! Covers three remaining relation benchmark needs with production code:
//! terminal full-build scaling (build, memory materialization via
//! `detached_clone`, layered-view wrapping, canonical snapshot) to ground
//! the memory-cache threshold; the transitive-dependency whole-package
//! reject branch (`resolve_batch` with the stdlib filter on versus off,
//! plus an unknown-package accept control); and transitive-closure depth
//! scaling on chain and fan-out graphs.
//!
//! Run with: `cargo run -p cce-relation --bench final_replay`
//!
//! Results are printed to stdout and appended to
//! `benches/results/final_replay.tsv`.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use cce_relation::SymbolTableBuilder;
use cce_relation::index::{
    IndexBuilder, LayeredSnapshotIndex, RelationIndex, RelationResolver, RelationSnapshotIndex,
};
use cce_relation::query::CallChainQuery;
use cce_types::entity::ParseStatus;
use cce_types::relation::CallContext;
use cce_types::{
    Entity, EntityId, EntityKind, FileInfo, Language, ParsedFile, Position, RawRelationData,
    RelationLevel, RelationType, ResolvedRelation, Span,
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

fn build_index(file_count: usize) -> RelationIndex {
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
            let name = format!("fn_{file_idx:05}_{fn_idx}");
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
            make_file_info(path.clone(), path, functions.len()),
            functions,
            relations,
            None,
            vec![],
        );
    }
    builder.build()
}

fn build_fanout(leaf_count: usize) -> RelationIndex {
    let builder = IndexBuilder::new();
    let mut leaves = Vec::new();
    for i in 0..leaf_count {
        let id = (i + 2) as u64;
        leaves.push((EntityId(id), make_entity(id, &format!("leaf_{i:05}"))));
    }
    builder.process_file(
        make_file_info(
            "src/leaf.rs".to_string(),
            "src/leaf.rs".to_string(),
            leaves.len(),
        ),
        leaves,
        Vec::new(),
        None,
        vec![],
    );
    let mut relations = Vec::new();
    for i in 0..leaf_count {
        relations.push(ResolvedRelation {
            caller: EntityId(1),
            callee_id: Some(EntityId((i + 2) as u64)),
            callee_name: format!("leaf_{i:05}"),
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
    builder.process_file(
        make_file_info("src/hub.rs".to_string(), "src/hub.rs".to_string(), 1),
        vec![(EntityId(1), make_entity(1, "hub_main"))],
        relations,
        None,
        vec![],
    );
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

/// Caller file with `count` raw relations cycling over stdlib-looking,
/// unknown-package, and internal names. The stdlib-looking entry carries a
/// preset category, mirroring what relation extraction produces in
/// production (`Vec::new` classifies as a collection); the resolver treats
/// the preset field as authoritative and never consults a classifier.
fn stdlib_caller(count: usize) -> ParsedFile {
    let mut caller = ParsedFile::new(Language::Rust, "src/caller.rs".to_string(), "");
    caller.add_entity(make_entity(1, "caller"));
    for i in 0..count {
        let (dst, stdlib_category) = match i % 3 {
            0 => (
                "Vec::new",
                Some(cce_types::stdlib_category::StdlibCategory::Collection),
            ),
            1 => ("my_crate::handler", None),
            _ => ("caller", None),
        };
        caller.add_relation(RawRelationData {
            src: EntityId(1),
            level: RelationLevel::Entity,
            dst_name: dst.to_string(),
            relation_type: RelationType::DirectCall,
            span: Span::default(),
            stdlib_category,
        });
    }
    caller
}

fn main() {
    println!("final_replay benchmark (debug, synthetic graphs)");
    println!("{:<34} {:>12} {:>12}", "case", "ms", "extra");

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/results/final_replay.tsv"))
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(f, "# case\tms\textra");
    }
    let mut row = |label: &str, ms: f64, extra: &str| {
        println!("{label:<34} {ms:>12.2} {extra:>12}");
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "{label}\t{ms:.2}\t{extra}");
        }
    };

    // Terminal build scaling: full register plus build, memory
    // materialization, layered wrapping, canonical snapshot. The memory
    // cache threshold (100k entries) sits far above these sizes; the
    // curve shows how much headroom remains.
    for files in [100usize, 500, 2000] {
        let build_ms = bench_ms(2, || {
            let _ = build_index(files);
        });
        row(
            &format!("terminal build x{files}"),
            build_ms,
            &format!("{}ents", files * FUNCTIONS_PER_FILE),
        );
        let index = build_index(files);
        let clone_ms = bench_ms(2, || {
            let _ = index.detached_clone();
        });
        row(&format!("materialize clone x{files}"), clone_ms, "memory");
        let view_ms = bench_ms(5, || {
            let _ = LayeredSnapshotIndex::with_deltas(
                Arc::new(RelationSnapshotIndex::from_index_shared(&index)),
                vec![],
            );
        });
        row(&format!("layered wrap x{files}"), view_ms, "view");
        let view = LayeredSnapshotIndex::with_deltas(
            Arc::new(RelationSnapshotIndex::from_index_shared(&index)),
            vec![],
        );
        let canon_ms = bench_ms(2, || {
            let _ = view.to_canonical_snapshot("bench-fp".to_string());
        });
        row(&format!("canonical snapshot x{files}"), canon_ms, "memory");
    }

    // Whole-package reject branch: stdlib-looking callee with the filter
    // on (edge dropped) versus off (edge kept as external), with an
    // unknown-package control that is kept in both modes.
    let caller = stdlib_caller(300);
    let files = [&caller];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);
    let table_builder = IndexBuilder::new();
    table_builder.register_file_entities(&caller);
    let index = table_builder.build();
    for filter in [true, false] {
        let mut resolver = RelationResolver::new();
        resolver.with_filter(filter);
        let mut kept = 0;
        let ms = bench_ms(5, || {
            let out = resolver.resolve_batch(&caller.raw_relations, &caller, &symbols, &index);
            kept = out.len();
        });
        row(
            &format!("resolve_batch filter={filter}"),
            ms,
            &format!("kept={kept} dropped={}", resolver.filtered_count()),
        );
    }

    // Transitive closure depth scaling on a chain, plus fan-out neighbor
    // expansion, at larger scale than the hot-update bench.
    let chain = build_index(500);
    let query = CallChainQuery::from_index(chain);
    for depth in [2usize, 6, 20] {
        let ms = bench_ms(5, || {
            let _ = query.query_forward_by_entity(EntityId(1), depth);
        });
        row(&format!("chain500 fwd depth{depth}"), ms, "closure");
    }
    let fanout = build_fanout(400);
    let fanout_query = CallChainQuery::from_index(fanout);
    let ms = bench_ms(5, || {
        let _ = fanout_query.get_callees_by_entity(EntityId(1));
    });
    row("fanout callees x400", ms, "neighbors");
    let ms = bench_ms(5, || {
        let _ = fanout_query.query_forward_by_entity(EntityId(1), 1);
    });
    row("fanout fwd depth1", ms, "closure");
}
