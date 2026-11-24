//! Canonical relation snapshot for cross-path equivalence testing.
//!
//! This module defines a deterministic, EntityId-free representation of a
//! project's relationship graph that can be compared across full-index,
//! hot-update, and cold-start paths.

use cce_codegraph::RelationIndex;
use cce_codegraph::index::{EntityIndexOps, RelationQueryOps, core::SymbolKey};
use cce_types::RelationType;

/// A single canonical entry for an entity in a relation snapshot.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CanonicalEntity {
    pub file_path: String,
    pub scoped_name: String,
    pub kind: String,
    pub signature: String,
}

/// A single canonical relation edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalRelation {
    pub caller_file: String,
    pub caller_scoped_name: String,
    pub caller_kind: String,
    pub target_file: Option<String>,
    pub target_scoped_name: String,
    pub target_kind: Option<String>,
    pub relation_type: RelationType,
    pub is_external: bool,
}

impl PartialOrd for CanonicalRelation {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CanonicalRelation {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.caller_file
            .cmp(&other.caller_file)
            .then_with(|| self.caller_scoped_name.cmp(&other.caller_scoped_name))
            .then_with(|| self.target_scoped_name.cmp(&other.target_scoped_name))
            .then_with(|| {
                format!("{:?}", self.relation_type).cmp(&format!("{:?}", other.relation_type))
            })
    }
}

/// A single canonical dependency edge.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CanonicalDependency {
    pub from: String,
    pub to: String,
}

/// Project-level canonical relation snapshot.
///
/// All collections are sorted by stable keys (not EntityId) so that two
/// snapshots can be compared for equality regardless of insertion order
/// or process-local ID assignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationSnapshot {
    pub epoch: i64,
    pub entities: Vec<CanonicalEntity>,
    pub relations: Vec<CanonicalRelation>,
    pub dependencies: Vec<CanonicalDependency>,
    pub fingerprint: String,
    pub parser_version: String,
}

/// Build a [`RelationSnapshot`] from a runtime [`RelationIndex`].
///
/// This is the canonical conversion used by all three paths (full, hot, restart).
/// The returned snapshot must be identical for the same source files regardless
/// of how the index was built.
pub fn snapshot_from_index(index: &RelationIndex, epoch: i64) -> RelationSnapshot {
    let mut entities: Vec<CanonicalEntity> = Vec::new();
    let mut relations: Vec<CanonicalRelation> = Vec::new();
    let mut dependencies: Vec<CanonicalDependency> = Vec::new();

    // Collect entities sorted by file_path + scoped_name.
    for entry in index.function_index() {
        let entity = entry.value();
        let file_path = index
            .get_file_path_by_entity(*entry.key())
            .unwrap_or_default();
        let sk = index
            .get_symbol_key_by_entity_id(*entry.key())
            .unwrap_or_else(|| SymbolKey::for_entity(&file_path, &entity.name, entity));

        entities.push(CanonicalEntity {
            file_path: sk.file_path,
            scoped_name: sk.scoped_name,
            kind: sk.kind.to_string(),
            signature: entity.signature.clone(),
        });
    }
    entities.sort();

    // Collect resolved relations by caller stable key + span.
    for entry in index.resolved_relation_index() {
        let caller_id = *entry.key();
        let caller_sk = index.get_symbol_key_by_entity_id(caller_id);

        for rel in entry.value() {
            let target_sk = rel
                .callee_id
                .and_then(|cid| index.get_symbol_key_by_entity_id(cid));

            relations.push(CanonicalRelation {
                caller_file: caller_sk
                    .as_ref()
                    .map(|s| s.file_path.clone())
                    .unwrap_or_default(),
                caller_scoped_name: caller_sk
                    .as_ref()
                    .map(|s| s.scoped_name.clone())
                    .unwrap_or_default(),
                caller_kind: caller_sk
                    .as_ref()
                    .map(|s| s.kind.to_string())
                    .unwrap_or_default(),
                target_file: target_sk.as_ref().map(|s| s.file_path.clone()),
                target_scoped_name: target_sk
                    .as_ref()
                    .map(|s| s.scoped_name.clone())
                    .unwrap_or_else(|| rel.callee_name.clone()),
                target_kind: target_sk.as_ref().map(|s| s.kind.to_string()),
                relation_type: rel.relation_type,
                is_external: rel.is_external,
            });
        }
    }
    relations.sort();

    // Collect dependency edges from the graph via get_all_files + get_dependencies.
    let all_files = index.dependency_graph.get_all_files();
    for file in &all_files {
        for dep in index.dependency_graph.get_dependencies(file) {
            dependencies.push(CanonicalDependency {
                from: file.clone(),
                to: dep,
            });
        }
    }
    dependencies.sort();
    dependencies.dedup();

    RelationSnapshot {
        epoch,
        entities,
        relations,
        dependencies,
        fingerprint: index.compute_fingerprint(),
        parser_version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

// ---------------------------------------------------------------------------
// Invariant assertion helpers
// ---------------------------------------------------------------------------

/// Check that the active epoch has a matching manifest fingerprint.
pub fn check_fingerprint_consistency(snapshot: &RelationSnapshot, loaded_fp: &str) -> bool {
    snapshot.fingerprint == loaded_fp
}

/// Verify that every caller/target with a known symbol key resolves to an entity
/// in the same epoch (i.e. no dangling references).
pub fn check_no_dangling_references(snapshot: &RelationSnapshot) -> Vec<String> {
    let entity_set: std::collections::HashSet<_> = snapshot
        .entities
        .iter()
        .map(|e| format!("{}:{}:{}", e.file_path, e.scoped_name, e.kind))
        .collect();

    let mut dangling = Vec::new();
    for rel in &snapshot.relations {
        if !rel.is_external {
            if let Some(ref tf) = rel.target_file {
                let key = format!(
                    "{}:{}:{}",
                    tf,
                    rel.target_scoped_name,
                    rel.target_kind.as_deref().unwrap_or("")
                );
                if !entity_set.contains(&key) {
                    dangling.push(format!(
                        "Unresolved target: {} calls {} at {}:{:?}",
                        rel.caller_scoped_name, rel.target_scoped_name, tf, rel.target_kind
                    ));
                }
            }
        }
    }
    dangling
}

/// Assert that two snapshots are equivalent, printing a diff on failure.
#[macro_export]
macro_rules! assert_relation_snapshot_eq {
    ($left:expr, $right:expr) => {{
        let left: &RelationSnapshot = &$left;
        let right: &RelationSnapshot = &$right;
        if left.entities != right.entities {
            panic!(
                "Entity mismatch:\nleft:  {:#?}\nright: {:#?}",
                left.entities, right.entities
            );
        }
        if left.relations != right.relations {
            panic!(
                "Relation mismatch:\nleft:  {:#?}\nright: {:#?}",
                left.relations, right.relations
            );
        }
        if left.dependencies != right.dependencies {
            panic!(
                "Dependency mismatch:\nleft:  {:#?}\nright: {:#?}",
                left.dependencies, right.dependencies
            );
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_codegraph::RelationIndex;

    #[test]
    fn test_empty_snapshot() {
        let index = RelationIndex::new();
        let snap = snapshot_from_index(&index, 0);
        assert!(snap.entities.is_empty());
        assert!(snap.relations.is_empty());
        assert!(snap.dependencies.is_empty());
        assert!(!snap.fingerprint.is_empty());
    }
}
