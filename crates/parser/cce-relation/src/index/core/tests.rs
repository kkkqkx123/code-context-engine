use super::*;
use crate::index::snapshot_index::RelationSnapshotIndex;
use crate::index::snapshot_query::SnapshotEntityQueryOps;
use cce_types::{EntityKind, RelationType, Span};
use std::collections::HashMap;

/// Helper function to create a test function entity
fn create_test_function_entity(id: u32, name: &str) -> Entity {
    Entity {
        id: EntityId(id.into()),
        kind: EntityKind::Function,
        name: name.to_string(),
        signature: format!("fn {}()", name),
        parameters: Vec::new(),
        return_type: None,
        span: Span::default(),
        depth: 0,
        parent: None,
        children: Vec::new(),
        doc_comment: None,
        modifiers: Vec::new(),
        attributes: HashMap::new(),
        metadata: HashMap::new(),
        is_stdlib: false,
        subtype: None,
        stdlib_category: None,
    }
}

#[test]
fn test_new_index() {
    let index = RelationIndex::new();
    assert_eq!(index.function_index.len(), 0);
    assert_eq!(index.resolved_relation_index.len(), 0);
}

#[test]
fn test_clone_shares_runtime_state() {
    let index = RelationIndex::new();
    let shared = index.clone();

    shared
        .function_index
        .insert(EntityId(1), create_test_function_entity(1, "shared"));
    shared
        .dependency_graph
        .add_dependency("caller.rs", "target.rs");

    assert!(index.function_index.contains_key(&EntityId(1)));
    assert_eq!(
        index.dependency_graph.get_dependents("target.rs"),
        vec!["caller.rs"]
    );
}

#[test]
fn test_snapshot_query_index_shares_source_maps() {
    let index = RelationIndex::new();
    index
        .function_index
        .insert(EntityId(1), create_test_function_entity(1, "original"));
    let snapshot = RelationSnapshotIndex::from_index(&index);

    // The snapshot's maps are shared with the source index; queries read
    // them in place without any per-query deep copy.
    assert_eq!(snapshot.function_count(), 1);
    assert_eq!(
        snapshot
            .get_function_by_entity_id(EntityId(1))
            .expect("function should exist")
            .name,
        "original"
    );
    assert!(snapshot.contains_function(EntityId(1)));
}

#[test]
fn test_add_resolved_relation() {
    let index = RelationIndex::new();

    // Add two functions first
    index
        .function_index
        .insert(EntityId(1), create_test_function_entity(1, "func_a"));
    index
        .function_index
        .insert(EntityId(2), create_test_function_entity(2, "func_b"));

    // Add resolved relation (caller -> callee)
    let relation = ResolvedRelation {
        caller: EntityId(1),
        callee_id: Some(EntityId(2)),
        callee_name: "func_b".to_string(),
        relation_type: RelationType::DirectCall,
        span: Span::default(),
        is_external: false,
        external_type: None,
        callee_symbol: None,
        stdlib_category: None,
        owner_type: None,
        call_context: cce_types::relation::CallContext::Direct,
        overload_signature: None,
    };
    index.add_resolved_relation(relation);

    // Verify forward index
    assert!(index.resolved_relation_index.contains_key(&EntityId(1)));
    let relations = index
        .resolved_relation_index
        .get(&EntityId(1))
        .expect("Should have relations");
    assert_eq!(relations.len(), 1);

    // Verify reverse index: callers_of scans for callee-only entities
    let callers = index.callers_of(EntityId(2));
    assert_eq!(callers.len(), 1);
    assert!(callers.contains(&EntityId(1)));
}

#[test]
fn test_allocate_entity_id_global_uniqueness() {
    let index = RelationIndex::new();

    // Allocate two IDs from a fresh index.
    let id_a = index.allocate_entity_id();
    let id_b = index.allocate_entity_id();
    assert_ne!(id_a, id_b, "allocated IDs must be unique");
    // Verify monotonic growth.
    assert_eq!(id_a.0, 0);
    assert_eq!(id_b.0, 1);

    // Simulate re-loading from an existing snapshot: set the counter past
    // the max known ID, then verify new allocations continue from there.
    index
        .entity_id_counter
        .store(100, std::sync::atomic::Ordering::Relaxed);
    let id_c = index.allocate_entity_id();
    assert_eq!(id_c.0, 100);

    // Verify that two files starting from local ID 0 get distinct global IDs
    // when index_file_core is simulated through entity_id_remaps.
    let local_to_global_a: HashMap<EntityId, EntityId> = [
        (EntityId(0), index.allocate_entity_id()),
        (EntityId(1), index.allocate_entity_id()),
    ]
    .into_iter()
    .collect();
    let local_to_global_b: HashMap<EntityId, EntityId> = [
        (EntityId(0), index.allocate_entity_id()),
        (EntityId(1), index.allocate_entity_id()),
    ]
    .into_iter()
    .collect();

    // Each file's local ID 0 should map to a different global ID.
    assert_ne!(
        local_to_global_a[&EntityId(0)],
        local_to_global_b[&EntityId(0)]
    );
    assert_ne!(
        local_to_global_a[&EntityId(1)],
        local_to_global_b[&EntityId(1)]
    );
    // Global IDs should all be distinct.
    let mut all: Vec<_> = local_to_global_a
        .values()
        .chain(local_to_global_b.values())
        .collect();
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 4, "all four global IDs must be unique");
}

#[tokio::test]
async fn test_concurrent_reparse_allocates_unique_global_ids() {
    let index = RelationIndex::new();

    let mut handles = Vec::new();
    for _ in 0..4 {
        let idx = index.clone();
        handles.push(tokio::spawn(async move {
            let mut ids = Vec::new();
            // Each "file" allocates 3 entity IDs (simulating 3 entities).
            for _ in 0..3 {
                ids.push(idx.allocate_entity_id());
            }
            ids
        }));
    }

    let mut all_ids: Vec<EntityId> = Vec::new();
    for handle in handles {
        let mut ids = handle.await.expect("concurrent task should succeed");
        all_ids.append(&mut ids);
    }

    // All 12 IDs (4 files × 3 entities) must be unique.
    let unique_count = {
        let mut sorted = all_ids.clone();
        sorted.sort();
        sorted.dedup();
        sorted.len()
    };
    assert_eq!(
        unique_count, 12,
        "all concurrently allocated IDs must be unique"
    );
}

#[test]
fn test_clear() {
    let index = RelationIndex::new();

    // Add some data
    index
        .function_index
        .insert(EntityId(1), create_test_function_entity(1, "func_a"));
    let relation = ResolvedRelation {
        caller: EntityId(1),
        callee_id: Some(EntityId(2)),
        callee_name: "func_b".to_string(),
        relation_type: RelationType::DirectCall,
        span: Span::default(),
        is_external: false,
        external_type: None,
        callee_symbol: None,
        stdlib_category: None,
        owner_type: None,
        call_context: cce_types::relation::CallContext::Direct,
        overload_signature: None,
    };
    index.add_resolved_relation(relation);

    // Clear
    index.clear();

    // Verify all indexes are empty
    assert_eq!(index.function_index.len(), 0);
    assert_eq!(index.resolved_relation_index.len(), 0);

    // Verify SymbolKey maps are cleared
    assert_eq!(index.symbol_key_to_entity.read().len(), 0);
    assert_eq!(index.entity_to_symbol_key.read().len(), 0);

    // Verify conflict diagnostics are reset
    assert_eq!(
        index
            .diagnostics
            .symbol_key_conflict_count
            .load(Ordering::Relaxed),
        0
    );
    assert!(
        index
            .diagnostics
            .symbol_key_conflict_samples
            .lock()
            .expect("samples lock")
            .is_empty()
    );
}

/// A conflicting registration is rejected first-wins and recorded once;
/// re-registering the winning entity is idempotent and does not inflate
/// the diagnostic counter.
#[test]
fn symbol_key_conflict_first_wins_and_idempotent() {
    let index = RelationIndex::new();
    let first = create_test_function_entity(1, "dup");
    let second = create_test_function_entity(2, "dup");

    assert!(index.register_symbol_key("a.rs", "dup", &first, EntityId(1)));
    assert!(
        !index.register_symbol_key("a.rs", "dup", &second, EntityId(2)),
        "a different entity under the same key must be rejected"
    );
    assert!(
        index.register_symbol_key("a.rs", "dup", &first, EntityId(1)),
        "re-registering the winning entity is idempotent"
    );
    assert_eq!(
        index.get_entity_id_by_symbol_key(&SymbolKey::new(
            "a.rs",
            "dup",
            EntityKind::Function,
            "fn dup()"
        )),
        Some(EntityId(1)),
        "the first registration wins"
    );

    assert_eq!(
        index
            .diagnostics
            .symbol_key_conflict_count
            .load(Ordering::Relaxed),
        1,
        "only the first collision is counted"
    );
    let guard = index
        .diagnostics
        .symbol_key_conflict_samples
        .lock()
        .expect("samples lock");
    assert_eq!(guard.len(), 1);
    let record = &guard[0];
    assert_eq!(record.file_path, "a.rs");
    assert_eq!(record.scoped_name, "dup");
    assert_eq!(record.kept_entity, 1);
    assert_eq!(record.rejected_entity, 2);
}

/// The conflict sample buffer is capped; the oldest sample is dropped at
/// capacity while the counter keeps the full total.
#[test]
fn symbol_key_conflict_samples_bounded_capacity() {
    let index = RelationIndex::new();
    let winner = create_test_function_entity(1, "dup");
    assert!(index.register_symbol_key("a.rs", "dup", &winner, EntityId(1)));

    let conflicting_count = SYMBOL_KEY_CONFLICT_SAMPLE_CAP + 2;
    for id in 2..=(conflicting_count + 1) as u32 {
        let entity = create_test_function_entity(id, "dup");
        assert!(
            !index.register_symbol_key("a.rs", "dup", &entity, EntityId(id.into())),
            "registration {id} must conflict"
        );
    }

    assert_eq!(
        index
            .diagnostics
            .symbol_key_conflict_count
            .load(Ordering::Relaxed),
        conflicting_count as u64,
        "the counter keeps the full total"
    );
    let guard = index
        .diagnostics
        .symbol_key_conflict_samples
        .lock()
        .expect("samples lock");
    assert_eq!(guard.len(), SYMBOL_KEY_CONFLICT_SAMPLE_CAP);
    assert_eq!(
        guard[0].rejected_entity, 4u64,
        "the two oldest samples are dropped at capacity"
    );
    assert_eq!(
        guard.back().expect("last sample").rejected_entity,
        (conflicting_count + 1) as u64,
        "the newest sample is retained"
    );
}

/// Conflict diagnostics travel into the canonical snapshot via
/// `build_metadata`, and clearing them leaves the fingerprint unchanged.
#[test]
fn snapshot_carries_conflict_metadata_without_affecting_fingerprint() {
    let index = RelationIndex::new();
    let first = create_test_function_entity(1, "dup");
    let second = create_test_function_entity(2, "dup");
    index.register_symbol_key("a.rs", "dup", &first, EntityId(1));
    index.register_symbol_key("a.rs", "dup", &second, EntityId(2));

    let snapshot = index
        .to_canonical_snapshot(String::new())
        .expect("snapshot must build");
    assert_eq!(snapshot.build_metadata.symbol_key_conflict_count, 1);
    assert_eq!(snapshot.build_metadata.symbol_key_conflict_samples.len(), 1);

    let fingerprint_before = index.compute_fingerprint();
    index.clear();
    index.register_symbol_key("a.rs", "dup", &first, EntityId(1));
    let fingerprint_after = index.compute_fingerprint();
    assert_eq!(
        fingerprint_before, fingerprint_after,
        "conflict diagnostics must not influence the fingerprint"
    );
    let cleared = index
        .to_canonical_snapshot(String::new())
        .expect("snapshot must build");
    assert_eq!(cleared.fingerprint(), fingerprint_after);
    assert_eq!(cleared.build_metadata.symbol_key_conflict_count, 0);
    assert!(
        cleared
            .build_metadata
            .symbol_key_conflict_samples
            .is_empty()
    );
}
