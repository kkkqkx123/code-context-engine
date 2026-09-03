//! Entity deduplication logic
//!
//! Handles two stages of span-based deduplication:
//! - Same-span selection: keeps the most specific entity when tree-sitter
//!   returns multiple captures for the same byte range.
//! - Contained-entity removal: removes implementation-detail noise (Variables)
//!   and duplicate decorated definitions.

use cce_types::{Entity, EntityKind};
use std::collections::{HashMap, HashSet};

pub(crate) fn deduplicate_entities_by_span(entities: &mut Vec<Entity>) {
    if entities.len() <= 1 {
        return;
    }

    entities.sort_by_key(|e| (e.span.start_byte, e.span.end_byte));

    let mut to_remove = Vec::new();
    let mut i = 0;
    while i < entities.len() {
        let mut j = i + 1;
        while j < entities.len()
            && entities[j].span.start_byte == entities[i].span.start_byte
            && entities[j].span.end_byte == entities[i].span.end_byte
        {
            j += 1;
        }

        if j > i + 1 {
            let best_idx = select_best_entity(&entities[i..j]);
            for (k, entity) in entities.iter().enumerate().take(j).skip(i) {
                if k != i + best_idx {
                    to_remove.push(entity.id);
                }
            }
        }

        i = j;
    }

    if !to_remove.is_empty() {
        let remove_set: HashSet<_> = to_remove.into_iter().collect();
        entities.retain(|e| !remove_set.contains(&e.id));
    }
}

/// Remove entities whose span is fully contained within their parent and
/// are pure implementation detail noise.
///
/// Only removes `Variable` — these are never meaningful as standalone
/// retrieval units and would pollute the index if kept. Everything else
/// (Field, Constant, etc.) is preserved.
///
/// Also removes duplicate entities created by tree-sitter queries matching
/// both (`decorated_definition`) and (`function_definition`) for the same
/// decorated function. The parent (broader span) is kept.
pub(crate) fn deduplicate_contained_entities(entities: &mut Vec<Entity>) {
    if entities.len() <= 1 {
        return;
    }

    let mut to_remove: HashSet<cce_types::EntityId> = HashSet::new();
    let mut doc_propagations: HashMap<cce_types::EntityId, String> = HashMap::new();

    for i in 0..entities.len() {
        if to_remove.contains(&entities[i].id) {
            continue;
        }
        for j in 0..entities.len() {
            if i == j || to_remove.contains(&entities[j].id) {
                continue;
            }

            let parent = &entities[i];
            let child = &entities[j];

            // Parent must fully contain child span
            if !parent.span.contains(&child.span) {
                continue;
            }

            // Same span → already handled by deduplicate_entities_by_span
            if parent.span.start_byte == child.span.start_byte
                && parent.span.end_byte == child.span.end_byte
            {
                continue;
            }

            // Only remove truly low-value implementation detail entities:
            // local variables.
            if matches!(child.kind, EntityKind::Variable) {
                to_remove.insert(child.id);
            }

            // Duplicate entity: same name and kind, child span fully inside
            // parent span. This happens when the entity query matches both
            // (decorated_definition) and (function_definition) for the same
            // decorated function. Keep the parent (which includes the decorator).
            if parent.kind == child.kind && parent.name == child.name {
                doc_propagations
                    .entry(parent.id)
                    .or_insert_with(|| child.doc_comment.clone().unwrap_or_default());
                to_remove.insert(child.id);
            }
        }
    }

    // Apply doc_comment propagations after collecting
    for entity in entities.iter_mut() {
        if let Some(doc) = doc_propagations.remove(&entity.id) {
            if entity.doc_comment.is_none() && !doc.is_empty() {
                entity.doc_comment = Some(doc);
            }
        }
    }

    if !to_remove.is_empty() {
        entities.retain(|e| !to_remove.contains(&e.id));
    }
}

fn select_best_entity(group: &[Entity]) -> usize {
    let priority = |kind: &EntityKind| -> u8 {
        match kind {
            EntityKind::TestCase | EntityKind::TestSuite => 11,
            EntityKind::Function => 10,
            EntityKind::Struct | EntityKind::Enum | EntityKind::Trait | EntityKind::Class => 8,
            EntityKind::Module | EntityKind::Namespace | EntityKind::Package => 5,
            EntityKind::InherentImpl => 4,
            EntityKind::TraitImpl => 3,
            EntityKind::Method => 2,
            _ => 0,
        }
    };

    let mut best = 0;
    let mut best_score = priority(&group[0].kind);
    for (i, entity) in group.iter().enumerate().skip(1) {
        let score = priority(&entity.kind);
        if score > best_score || (score == best_score && entity.doc_comment.is_some()) {
            best = i;
            best_score = score;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::{EntityId, Span};

    #[test]
    fn test_deduplicate_contained_entities() {
        let mut entities = vec![
            Entity::new(
                EntityId(0),
                EntityKind::Struct,
                "OnceCell".to_string(),
                Span::new(0, 100, 0, 0, 5, 10),
            ),
            Entity::new(
                EntityId(1),
                EntityKind::Variable,
                "local_var".to_string(),
                Span::new(30, 80, 2, 0, 4, 10),
            ),
        ];

        deduplicate_contained_entities(&mut entities);

        let names: Vec<_> = entities.iter().map(|e| e.name.as_str()).collect();
        assert!(
            !names.contains(&"local_var"),
            "Contained variable should be removed"
        );
        assert!(
            names.contains(&"OnceCell"),
            "Parent struct OnceCell should be kept"
        );
    }

    #[test]
    fn test_deduplicate_contained_entities_preserves_functions() {
        let mut entities = vec![
            Entity::new(
                EntityId(0),
                EntityKind::InherentImpl,
                "OnceCell".to_string(),
                Span::new(0, 100, 0, 0, 5, 10),
            ),
            Entity::new(
                EntityId(1),
                EntityKind::Method,
                "new".to_string(),
                Span::new(10, 50, 1, 0, 3, 10),
            ),
        ];

        deduplicate_contained_entities(&mut entities);

        let names: Vec<_> = entities.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"new"),
            "Contained method should be preserved (weight > 3)"
        );
        assert!(names.contains(&"OnceCell"), "Parent impl should be kept");
    }

    #[test]
    fn test_deduplicate_entities_by_span_same_span_different_kind() {
        let mut entities = vec![
            Entity::new(
                EntityId(0),
                EntityKind::Function,
                "foo".to_string(),
                Span::new(0, 50, 0, 0, 0, 5),
            ),
            Entity::new(
                EntityId(1),
                EntityKind::TestCase,
                "foo".to_string(),
                Span::new(0, 50, 0, 0, 0, 5),
            ),
        ];

        deduplicate_entities_by_span(&mut entities);

        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].kind, EntityKind::TestCase);
    }

    #[test]
    fn test_deduplicate_entities_by_span_different_spans() {
        let mut entities = vec![
            Entity::new(
                EntityId(0),
                EntityKind::Struct,
                "OnceCell".to_string(),
                Span::new(0, 100, 0, 0, 5, 10),
            ),
            Entity::new(
                EntityId(1),
                EntityKind::Function,
                "new".to_string(),
                Span::new(10, 20, 0, 10, 0, 2),
            ),
        ];

        deduplicate_entities_by_span(&mut entities);

        assert_eq!(entities.len(), 2);
    }
}
