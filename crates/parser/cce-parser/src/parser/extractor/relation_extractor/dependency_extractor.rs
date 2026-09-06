//! File-level dependency extraction for a single dependency capture.
//!
//! Handles both structural relations (inheritance, implementation, ...),
//! attributed to their owning entity, and file-level dependencies (imports,
//! module links), attributed to the file itself.

use super::entity_index::EntityIndex;
use super::relation_handlers::{determine_dependency_relation_type, normalize_callee_name};
use crate::parser::extractor::utils;
use crate::tree_sitter_query::executor::Capture;
use cce_types::{Relation, RelationTarget};
use std::collections::{HashMap, HashSet};

/// Process a dependency capture and extract a relation.
pub(crate) fn process_dependency_match(
    dep_capture: &Capture,
    file_id: Option<i64>,
    index: &EntityIndex,
) -> Option<Relation> {
    // Create span from capture
    let span = utils::create_span_from_capture(dep_capture);

    // Determine dependency type from capture name
    let relation_type = determine_dependency_relation_type(&dep_capture.name);

    if relation_type.is_structural() {
        let caller_id = index.find_structural_owner(dep_capture.start_byte)?;
        Some(Relation::entity_relation(
            caller_id.0 as i64,
            RelationTarget::unresolved(normalize_callee_name(&dep_capture.text)),
            relation_type,
            span,
        ))
    } else {
        // For file-level dependencies, the caller is the file itself.
        let caller_id = file_id.unwrap_or(0);
        let dst_name = normalize_callee_name(&dep_capture.text);
        if dst_name.is_empty() {
            return None;
        }
        Some(Relation::file_relation(
            caller_id,
            RelationTarget::unresolved(dst_name),
            relation_type,
            span,
        ))
    }
}

/// Remove generic import relations shadowed by specific ones.
///
/// Dependency queries pair a whole-statement pattern (e.g.
/// `dependency.import`) with specific sub-patterns (named, default,
/// namespace, dynamic). Both fire for a single statement such as
/// `import { helper } from "..."`, yielding duplicate edges. When one
/// statement span produces both a generic `ImportStandard` and a more
/// specific import edge for the same target, the generic edge is noise
/// and is dropped. Side-effect imports only match the generic pattern,
/// so they are preserved.
pub(crate) fn deduplicate_generic_import_relations(relations: &mut Vec<Relation>) {
    use cce_types::RelationType;
    if relations.len() <= 1 {
        return;
    }
    let mut groups: HashMap<(usize, usize, String), Vec<usize>> = HashMap::new();
    for (idx, rel) in relations.iter().enumerate() {
        if !rel.relation_type.is_import() {
            continue;
        }
        groups
            .entry((
                rel.span.start_byte,
                rel.span.end_byte,
                rel.dst_name().to_string(),
            ))
            .or_default()
            .push(idx);
    }
    let mut remove: HashSet<usize> = HashSet::new();
    for (_, idxs) in groups {
        if idxs.len() <= 1 {
            continue;
        }
        let has_specific = idxs
            .iter()
            .any(|&i| !matches!(relations[i].relation_type, RelationType::ImportStandard));
        if has_specific {
            for &i in &idxs {
                if matches!(relations[i].relation_type, RelationType::ImportStandard) {
                    remove.insert(i);
                }
            }
        }
    }
    if !remove.is_empty() {
        let mut kept = Vec::with_capacity(relations.len() - remove.len());
        for (idx, rel) in relations.drain(..).enumerate() {
            if !remove.contains(&idx) {
                kept.push(rel);
            }
        }
        *relations = kept;
    }
}
