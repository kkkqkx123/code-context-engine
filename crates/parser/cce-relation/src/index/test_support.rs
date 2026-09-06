//! Shared test helpers for relation index unit tests.
//!
//! Test-only: seeded indexes, edge/entity builders, and identity comparison
//! helpers reused across the index module's unit tests.

#![cfg(test)]

use std::collections::HashMap;

use cce_types::relation::CallContext;
use cce_types::{Entity, EntityId, EntityKind, RelationType, ResolvedRelation, Span};

use super::core::{RelationEdgeIdentity, RelationIndex};
use super::entity_index::EntityIndexOps;

/// Create a test function entity.
pub(super) fn create_test_function_entity(id: u32, name: &str) -> Entity {
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

/// Internal edge helper.
pub(super) fn internal_edge(
    caller: EntityId,
    callee_id: EntityId,
    callee_name: &str,
    relation_type: RelationType,
) -> ResolvedRelation {
    ResolvedRelation {
        caller,
        callee_id: Some(callee_id),
        callee_name: callee_name.to_string(),
        relation_type,
        span: Span::default(),
        is_external: false,
        external_type: None,
        callee_symbol: None,
        stdlib_category: None,
        owner_type: None,
        call_context: CallContext::Direct,
        overload_signature: None,
    }
}

/// External edge helper.
pub(super) fn external_edge(caller: EntityId, callee_name: &str) -> ResolvedRelation {
    ResolvedRelation {
        caller,
        callee_id: None,
        callee_name: callee_name.to_string(),
        relation_type: RelationType::DirectCall,
        span: Span::default(),
        is_external: true,
        external_type: Some(cce_types::ExternalCallType::ExternalLibrary {
            package: "libc".to_string(),
        }),
        callee_symbol: None,
        stdlib_category: None,
        owner_type: None,
        call_context: CallContext::Direct,
        overload_signature: None,
    }
}

/// Unresolved edge helper.
pub(super) fn unresolved_edge(
    caller: EntityId,
    callee_name: &str,
    relation_type: RelationType,
) -> ResolvedRelation {
    ResolvedRelation {
        caller,
        callee_id: None,
        callee_name: callee_name.to_string(),
        relation_type,
        span: Span::default(),
        is_external: false,
        external_type: None,
        callee_symbol: None,
        stdlib_category: None,
        owner_type: None,
        call_context: CallContext::Direct,
        overload_signature: None,
    }
}

/// Populate a fresh index with the given entities and relations.
pub(super) fn seed_index(
    entities: &[(EntityId, &str)],
    relations: &[ResolvedRelation],
) -> RelationIndex {
    seed_index_in("src/lib.rs", entities, relations)
}

/// Populate a fresh index with entities in an explicit file.
pub(super) fn seed_index_in(
    file_path: &str,
    entities: &[(EntityId, &str)],
    relations: &[ResolvedRelation],
) -> RelationIndex {
    let index = RelationIndex::new();
    for (id, name) in entities {
        let entity = create_test_function_entity(id.0 as u32, name);
        index.add_function_with_path(*id, entity.clone(), file_path.to_string());
        index.register_symbol_key(file_path, name, &entity, *id);
    }
    for relation in relations {
        index.add_resolved_relation(relation.clone());
    }
    index
}

/// Populate a fresh index with entities spread across multiple files.
pub(super) fn seed_multi_file_index(
    files: &[(&str, &[(EntityId, &str)])],
    relations: &[ResolvedRelation],
) -> RelationIndex {
    let index = RelationIndex::new();
    for (file_path, entities) in files {
        for (id, name) in *entities {
            let entity = create_test_function_entity(id.0 as u32, name);
            index.add_function_with_path(*id, entity.clone(), file_path.to_string());
            index.register_symbol_key(file_path, name, &entity, *id);
        }
    }
    for relation in relations {
        index.add_resolved_relation(relation.clone());
    }
    index
}

/// Collect the edge identities of an index as a sorted vector.
pub(super) fn edge_identities(index: &RelationIndex) -> Vec<RelationEdgeIdentity> {
    let mut identities: Vec<RelationEdgeIdentity> = Vec::new();
    for entry in index.resolved_relation_index.iter() {
        identities.extend(entry.value().iter().map(super::delta::relation_identity));
    }
    identities.sort_by(|a, b| {
        (a.caller.0, format!("{:?}", a.kind)).cmp(&(b.caller.0, format!("{:?}", b.kind)))
    });
    identities
}
