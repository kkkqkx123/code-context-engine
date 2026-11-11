//! File-level dependency extraction for a single dependency capture.
//!
//! Handles both structural relations (inheritance, implementation, ...),
//! attributed to their owning entity, and file-level dependencies (imports,
//! module links), attributed to the file itself.

use super::entity_index::EntityIndex;
use super::relation_handlers::{determine_dependency_relation_type, normalize_callee_name};
use crate::parser::extractor::utils;
use crate::parser::stdlib::StdlibDetector;
use crate::tree_sitter_query::executor::{Capture, QueryMatch};
use cce_types::language::Language;
use cce_types::{Relation, RelationTarget};
use std::collections::{HashMap, HashSet};

/// Process a dependency capture and extract a relation.
///
/// Every dependency target is a *reference* — an import path, a type name or a
/// supertype — so each edge carries the stdlib category of its target. The
/// resolver then applies the shared `filter_stdlib_calls` switch to all of
/// them, which is what makes `use std::collections::HashMap` and `Vec<T>` field
/// types disappear from the relation graph instead of lingering as permanent
/// unresolved edges.
pub(crate) fn process_dependency_match(
    dep_capture: &Capture,
    file_id: Option<i64>,
    index: &EntityIndex,
    language: &Language,
) -> Option<Relation> {
    // Create span from capture
    let span = utils::create_span_from_capture(dep_capture);

    // Determine dependency type from capture name
    let relation_type = determine_dependency_relation_type(&dep_capture.name);

    if relation_type.is_structural() {
        let caller_id = index.find_structural_owner(dep_capture.start_byte)?;
        let dst_name = normalize_callee_name(&dep_capture.text);
        Some(
            Relation::entity_relation(
                caller_id.0 as i64,
                RelationTarget::unresolved(dst_name.clone()),
                relation_type,
                span,
            )
            .with_stdlib_category(StdlibDetector::ref_category(&dst_name, language)),
        )
    } else {
        // For file-level dependencies, the caller is the file itself.
        let caller_id = file_id.unwrap_or(0);
        let dst_name = normalize_callee_name(&dep_capture.text);
        if dst_name.is_empty() {
            return None;
        }
        Some(
            Relation::file_relation(
                caller_id,
                RelationTarget::unresolved(dst_name.clone()),
                relation_type,
                span,
            )
            .with_stdlib_category(StdlibDetector::ref_category(&dst_name, language)),
        )
    }
}

/// Compose the full path for Rust scoped-use list items.
///
/// A `use prefix::{item}` match carries the list prefix and the item as
/// separate captures because no single AST node spans `prefix::item`, so
/// generic single-capture selection cannot produce the item target. When a
/// match holds list-item captures, compose the target here and synthesize
/// a capture carrying it; `self` items resolve to the prefix itself since
/// `use prefix::{self}` re-imports the prefix module. The synthesized
/// capture keeps the `use` category so relation typing is unchanged, and
/// its span is the item span so per-site grouping stays precise.
/// Returns `None` for matches without list-item captures.
pub(crate) fn compose_rust_use_list_target(mat: &QueryMatch) -> Option<Capture> {
    let prefix = mat
        .captures
        .iter()
        .find(|c| c.name == "dependency.use.list.path")?;
    let named = |name: &str| mat.captures.iter().find(|c| c.name == name);
    let (item_text, item_capture) = named("dependency.use.list.item")
        .map(|c| (c.text.clone(), c))
        .or_else(|| named("dependency.use.list.item.path").map(|c| (c.text.clone(), c)))?;
    let target = if item_text == "self" {
        prefix.text.clone()
    } else {
        format!("{}::{}", prefix.text, item_text)
    };
    Some(Capture {
        name: "dependency.use.list.item".to_string(),
        text: target,
        start_byte: item_capture.start_byte,
        end_byte: item_capture.end_byte,
        start_point: item_capture.start_point,
        end_point: item_capture.end_point,
    })
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
