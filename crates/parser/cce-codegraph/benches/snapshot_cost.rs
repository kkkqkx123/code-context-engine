//! T-8 — codegraph snapshot publish-cost benchmark (covers P-7).
//!
//! Compares two publish forms across project scales (1k / 10k / 100k
//! entities). Both rebuild the derived query/dependency indexes per publish,
//! so both grow with project size; they differ in map handover:
//!
//! * `from_index_shared` — shares the source maps by `Arc` clone (no entry
//!   copy) and is the production publish path.
//! * `from_index` — deep-copies every map first (`detached_clone`), then
//!   publishes the copy (the regression path).
//!
//! The former per-impact-set copy-on-write primitive was removed: it had no
//! production callers (the hot-update path uses sparse candidates plus delta
//! publish), so its micro-benchmark numbers characterized no production
//! regression path.
//!
//! Run: `cargo bench -p cce-codegraph --bench snapshot_cost`
//!
//! Manual timings are appended to `benches/results/snapshot_cost.tsv`.

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

fn build_index(entity_count: usize) -> RelationIndex {
    let file_count = entity_count / FUNCTIONS_PER_FILE;
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

fn main() {
    println!("snapshot_cost benchmark (codegraph relation snapshot publish cost)");
    println!("{:<10} {:<22} {:>12}", "entities", "publish_form", "ms");

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/results/snapshot_cost.tsv"))
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(f, "# entities\tpublish_form\tms");
    }

    for entities in [1_000usize, 10_000, 100_000] {
        // Build (once) and keep alive for the duration of the run.
        let index = build_index(entities);
        let file_count = entities / FUNCTIONS_PER_FILE;

        // O(1) shared publish.
        let t = Instant::now();
        for _ in 0..20 {
            let _ = RelationSnapshotIndex::from_index_shared(&index);
        }
        let shared_ms = t.elapsed().as_secs_f64() * 1000.0 / 20.0;
        println!(
            "{:<10} {:<22} {:>12.3}",
            entities, "from_index_shared(O1)", shared_ms
        );
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "{entities}\tfrom_index_shared\t{shared_ms:.3}");
        }

        // O(N) full deep clone publish.
        let t = Instant::now();
        for _ in 0..5 {
            let _ = RelationSnapshotIndex::from_index(&index);
        }
        let full_ms = t.elapsed().as_secs_f64() * 1000.0 / 5.0;
        println!("{:<10} {:<22} {:>12.3}", "", "from_index(ON)", full_ms);
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "{entities}\tfrom_index\t{full_ms:.3}");
        }

        // Suppress unused warnings for the helper signature.
        let _ = file_count;
        drop(index);
    }

    // Criterion timing for the full O(N) publish at each scale (regression point).
    let mut criterion = criterion::Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(15))
        .warm_up_time(Duration::from_secs(3));
    for entities in [1_000usize, 10_000, 100_000] {
        let index = build_index(entities);
        criterion.bench_function(&format!("from_index_{entities}"), |b| {
            b.iter(|| {
                let _ = RelationSnapshotIndex::from_index(criterion::black_box(&index));
            })
        });
        drop(index);
    }
    criterion.final_summary();
}
