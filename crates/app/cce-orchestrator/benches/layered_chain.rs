//! Layered delta-chain benchmark.
//!
//! Quantifies how the hot-update read path degrades as the overlaid delta
//! chain grows from 1 to 20 entries: view construction, single-entity
//! lookup, and snapshot fingerprinting. The compaction threshold lives in
//! the snapshot index module; this bench validates that read costs stay
//! flat up to that threshold on a mid-size synthetic project.
//! Local data only; comparisons matter, not absolute numbers.
//!
//! Run with: `cargo run -p cce-orchestrator --bench layered_chain`
//!
//! Results are printed to stdout and appended to
//! `benches/results/layered_chain.tsv`.

use std::collections::HashSet;
use std::io::Write;
use std::sync::Arc;

use cce_bench_kit::bench_ms;

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
const MAX_CHAIN: usize = 20;

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

fn main() {
    println!("layered_chain benchmark ({FILE_COUNT} files, chain 1..={MAX_CHAIN})");
    println!(
        "{:<8} {:>12} {:>12} {:>14} {:>10}",
        "chain", "view", "lookup", "fingerprint", "compact?"
    );
    println!(
        "{:<8} {:>12} {:>12} {:>14} {:>10}",
        "", "(ms)", "(ms)", "(ms)", ""
    );

    let mut out = cce_bench_kit::open_report_with_header(
        env!("CARGO_MANIFEST_DIR"),
        "layered_chain.tsv",
        "# chain\tview_ms\tlookup_ms\tfingerprint_ms\tshould_compact",
    );

    let base = build_index(FILE_COUNT, None);
    let base_snapshot = Arc::new(RelationSnapshotIndex::from_index_shared(&base));

    let mut deltas = Vec::new();
    for edited in 0..MAX_CHAIN {
        let candidate = build_index(FILE_COUNT, Some(edited));
        let affected: HashSet<String> = [format!("src/mod_{edited:05}.rs")].into_iter().collect();
        deltas.push(Arc::new(candidate.compute_delta(
            &base,
            100 + edited as i64,
            99,
            "benchmark-fingerprint".to_string(),
            Some(&affected),
        )));
    }

    for chain in 1..=MAX_CHAIN {
        let chain_deltas: Vec<_> = deltas[..chain].to_vec();
        let view_ms = bench_ms(5, || {
            let _ =
                LayeredSnapshotIndex::with_deltas(Arc::clone(&base_snapshot), chain_deltas.clone());
        });
        let layered = LayeredSnapshotIndex::with_deltas(Arc::clone(&base_snapshot), chain_deltas);
        let lookup_ms = bench_ms(20, || {
            let _ = layered.contains_entity(EntityId(1));
        });
        let fingerprint_ms = bench_ms(3, || {
            let _ = layered.compute_fingerprint();
        });
        let compact = layered.should_compact();
        println!(
            "{chain:<8} {view_ms:>12.3} {lookup_ms:>12.4} {fingerprint_ms:>14.2} {compact:>10}"
        );
        let _ = writeln!(
            out,
            "{chain}\t{view_ms:.3}\t{lookup_ms:.4}\t{fingerprint_ms:.2}\t{compact}"
        );
    }
}
