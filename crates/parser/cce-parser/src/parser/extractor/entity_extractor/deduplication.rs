//! Entity deduplication logic
//!
//! Handles two stages of span-based deduplication:
//! - Same-span selection: keeps the most specific entity when tree-sitter
//!   returns multiple captures for the same byte range.
//! - Contained-entity removal: removes bare implementation-detail noise
//!   (untyped local variables) and duplicate decorated definitions, while
//!   preserving type-bearing locals for inference.

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
/// Removes contained `Variable` entities that carry no type information —
/// bare locals are never meaningful as standalone retrieval units and would
/// pollute the index if kept. Variables that do carry type information
/// (annotation, constructor call, literal, or call target) are preserved so
/// type inference keeps working on them; the grouper already skips variable
/// entities when forming standalone groups, so retention does not leak into
/// retrieval. Everything else (Field, Constant, etc.) is preserved.
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
            // bare local variables. Typed locals are kept for inference.
            if matches!(child.kind, EntityKind::Variable) && !variable_carries_type_info(child) {
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

            // Decorated method duality: the generic function patterns match
            // the inner `function_definition` while the class-method patterns
            // match the outer `decorated_definition` for the same decorated
            // method. Both survive span dedup (different spans), so drop the
            // inner Function in favor of the outer Method.
            if parent.kind == EntityKind::Method
                && child.kind == EntityKind::Function
                && parent.name == child.name
            {
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

/// Whether a variable entity carries type information worth preserving.
///
/// Any of the parser-produced type keys (annotation, constructor call,
/// literal, call target, destructuring source, or legacy inference keys)
/// counts; bare locals without them remain eligible for contained-entity
/// removal. `source_type` records destructuring provenance (tuple unpacking,
/// loop/except/with/case bindings) and feeds positional element mapping.
pub(crate) fn variable_carries_type_info(entity: &Entity) -> bool {
    const TYPE_KEYS: &[&str] = &[
        "type_annotation",
        "constructor_type",
        "literal_type",
        "call_target",
        "explicit_type",
        "var_type",
        "inferred_type",
        "source_type",
    ];
    TYPE_KEYS
        .iter()
        .any(|key| entity.metadata.contains_key(*key))
}

fn select_best_entity(group: &[Entity]) -> usize {
    let priority = |kind: &EntityKind| -> u8 {
        match kind {
            EntityKind::TestCase | EntityKind::TestSuite => 12,
            EntityKind::Method => 11,
            EntityKind::Function => 10,
            EntityKind::Class
            | EntityKind::Struct
            | EntityKind::Enum
            | EntityKind::Trait
            | EntityKind::Interface => 9,
            EntityKind::TypeAlias => 8,
            EntityKind::Module | EntityKind::Namespace | EntityKind::Package => 5,
            EntityKind::InherentImpl => 4,
            EntityKind::TraitImpl => 3,
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
            "Contained bare variable should be removed"
        );
        assert!(
            names.contains(&"OnceCell"),
            "Parent struct OnceCell should be kept"
        );
    }

    #[test]
    fn test_deduplicate_contained_entities_keeps_typed_variable() {
        let parent = Entity::new(
            EntityId(0),
            EntityKind::Function,
            "main".to_string(),
            Span::new(0, 100, 0, 0, 5, 10),
        );
        let mut typed = Entity::new(
            EntityId(1),
            EntityKind::Variable,
            "id".to_string(),
            Span::new(30, 80, 2, 0, 4, 10),
        );
        typed.set_metadata("type_annotation", "String".to_string());
        let mut constructed = Entity::new(
            EntityId(2),
            EntityKind::Variable,
            "user".to_string(),
            Span::new(30, 80, 2, 0, 4, 10),
        );
        constructed.set_metadata("constructor_type", "User".to_string());
        let mut entities = vec![parent, typed, constructed];

        deduplicate_contained_entities(&mut entities);

        let names: Vec<_> = entities.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"id"),
            "Contained variable with type annotation should be kept"
        );
        assert!(
            names.contains(&"user"),
            "Contained variable with constructor type should be kept"
        );
        // Typed locals must not leak their types onto the parent.
        let main = entities.iter().find(|e| e.name == "main").unwrap();
        assert!(!main.metadata.contains_key("type_annotation"));
        assert!(!main.metadata.contains_key("constructor_type"));
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
