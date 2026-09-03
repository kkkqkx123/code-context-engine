//! Built-in grouping pipeline stages
//!
//! This module provides helper functions for the built-in grouping pipeline:
//! - Group hierarchy resolution
//! - Import-only group detection and removal
//! - Low-value entity filtering
//! - Group name collision assertion

use cce_types::entity::{Entity, EntityId, EntityKind};

use super::types::EntityGroup;

/// Resolve parent-child relationships between entity groups based on entity hierarchy.
///
/// For each group whose header entity is module-like and has children (from fill_children),
/// find the corresponding child groups and set parent_group_id on child groups.
///
/// This enables hierarchical description generation where parent groups reference their
/// children and vice versa, while maintaining independent retrievability for all groups.
/// The inverse mapping (parent -> children) is derived from parent_group_id during enrichment.
pub fn resolve_group_hierarchy(groups: &mut [EntityGroup], entities: &[Entity]) -> usize {
    // Build map: entity_id -> index in groups vec
    let mut entity_to_group: std::collections::HashMap<EntityId, usize> =
        std::collections::HashMap::new();
    for (i, group) in groups.iter().enumerate() {
        if let Some(header_id) = group.header_id {
            entity_to_group.insert(header_id, i);
        }
    }

    let mut links: Vec<(usize, String)> = Vec::new();

    // Collect links first to avoid borrow conflicts
    for (i, group) in groups.iter().enumerate() {
        if let Some(header_id) = group.header_id {
            if let Some(entity) = entities.iter().find(|e| e.id == header_id) {
                if !entity.children.is_empty() {
                    for &child_id in &entity.children {
                        if let Some(&child_idx) = entity_to_group.get(&child_id) {
                            if i != child_idx && groups[child_idx].parent_group_id.is_none() {
                                links.push((child_idx, groups[i].group_id.to_string()));
                            }
                        }
                    }
                }
            }
        }
    }

    // Apply collected links
    for (child_idx, parent_id) in &links {
        groups[*child_idx].parent_group_id = Some(parent_id.clone().into());
    }

    links.len()
}

/// Whether a group is import-only: its header (and therefore, by the
/// small-fragment-merger boundary rule, every member) is an import-like
/// entity.
///
/// Import-like entities (import/require/include/export) carry no retrieval
/// value for vector/BM25 search; they are collected at the file level
/// (summary) and in the relationship index instead. Dropping import-only
/// groups before conversion/chunking keeps import text out of retrieval
/// chunks without losing any non-import content, because the merger never
/// mixes import-like and non-import entities in one group.
pub fn is_import_only_group(group: &EntityGroup) -> bool {
    let header_import_like = group
        .header
        .as_ref()
        .is_some_and(|h| h.kind.is_import_like());
    if !header_import_like {
        return false;
    }
    group.members.iter().all(|m| m.kind.is_import_like())
}

/// Remove import-only groups from a group list and return the number dropped.
///
/// Callers must refresh their `output_groups` stat from the new length.
pub fn drop_import_only_groups(groups: &mut Vec<EntityGroup>) -> usize {
    let before = groups.len();
    groups.retain(|g| !is_import_only_group(g));
    before - groups.len()
}

/// Check if an entity is low-value and should be skipped from standalone group creation.
///
/// Low-value entities are those that only have entity annotations or metadata
/// without meaningful content (no doc_comment, no children, tiny span).
/// Examples: "drop entity.", "init entity. Attributes: cold."
///
/// Also skips placeholder entities that have no semantic value:
/// - Zero/negative-width phantom nodes from tree-sitter error recovery
/// - Zero-variant enums (e.g. `enum Void {}` - used as never type)
/// - Empty/stub functions (e.g. `fn _dummy() {}`)
/// - Empty structs without doc_comment (marker types)
pub fn should_skip_low_value_entity(entity: &Entity) -> bool {
    // Skip phantom nodes from tree-sitter error recovery with invalid spans:
    // - zero or negative byte range (end_byte <= start_byte)
    // - reversed line positions (end_row < start_row)
    if entity.span.end_byte <= entity.span.start_byte
        || entity.span.end_position.row < entity.span.start_position.row
    {
        return true;
    }

    // Skip zero-variant enums (placeholder/never types like `enum Void {}`).
    // These are used as companion types for infallible closures or type-level markers.
    // Zero-variant enums always have very small spans (< 80 bytes) and no doc comments.
    if matches!(entity.kind, EntityKind::Enum)
        && entity.doc_comment.is_none()
        && entity.span.len() < 80
    {
        return true;
    }

    // Skip empty stub functions/methods (tiny span, name starts with '_').
    // These are often dummy functions to suppress unused-import warnings
    // (e.g., `fn _dummy() {}`), or marker/unimplemented functions.
    // Doc comments on these are typically about test cases (compile_fail),
    // not about the entity's behavior, so we filter regardless.
    if matches!(entity.kind, EntityKind::Function | EntityKind::Method)
        && entity.span.len() < 80
        && entity.name.starts_with('_')
    {
        return true;
    }

    // Skip compile-time assertion constants (name starts with '_').
    // In Rust, `const _FOO: () = assert!(...)` is exclusively used for
    // compile-time assertions. These have no runtime value and are never
    // meaningful as retrieval targets, regardless of span size.
    if matches!(entity.kind, EntityKind::Constant) && entity.name.starts_with('_') {
        return true;
    }

    false
}

/// Assert that no same-named top-level group nests a function-like group.
///
/// The grouper partitions entities into exclusive groups; a function-like
/// group (method/function) whose span nests inside a same-named group means
/// the grouper emitted both an impl block and its method as top-level groups
/// (e.g. `impl Clone` and its `fn clone` member). The chunker must not rely on
/// span containment to silently drop content, so this invariant is asserted
/// here in debug builds.
///
/// Allowed nesting patterns (intentional):
/// - struct fields inside their struct (e.g. `Paths` containing field `paths`)
/// - empty impl blocks inside a same-named module
/// - call-merger fragment groups inside a same-named container
pub fn assert_no_same_name_nested_groups(groups: &[EntityGroup], file_path: &str) {
    for (i, a) in groups.iter().enumerate() {
        if a.span.is_empty() {
            continue;
        }
        let Some(a_name) = a.header.as_ref().map(|h| h.name.as_str()) else {
            continue;
        };
        for b in groups.iter().skip(i + 1) {
            if b.span.is_empty() {
                continue;
            }
            let Some(b_name) = b.header.as_ref().map(|h| h.name.as_str()) else {
                continue;
            };
            if !a_name.eq_ignore_ascii_case(b_name) {
                continue;
            }
            let inner = if a.span.contains(&b.span) {
                b
            } else if b.span.contains(&a.span) {
                a
            } else {
                continue;
            };
            let inner_is_function = inner
                .header
                .as_ref()
                .is_some_and(|h| h.kind.is_function_like());
            let outer = if inner.group_id == a.group_id { b } else { a };
            debug_assert!(
                !inner_is_function,
                "file {}: function-like group {} (name {:?}, kind {:?}) nests inside \
                 same-named group {} (kind {:?}); impl-block methods must be group members, \
                 not top-level groups",
                file_path,
                inner.group_id,
                a_name,
                inner.group_type,
                outer.group_id,
                outer.group_type
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;
    use cce_types::entity::EntityId;
    use cce_types::language::Language;

    fn import_entity(id: u64, name: &str) -> Entity {
        Entity::new(
            EntityId(id),
            EntityKind::Import,
            name.to_string(),
            Span::new(
                id as usize * 10,
                id as usize * 10 + 10,
                id as usize,
                0,
                id as usize,
                10,
            ),
        )
    }

    #[test]
    fn test_skip_zero_width_entity() {
        let entity = Entity::new(
            EntityId(1),
            EntityKind::Enum,
            "Void".to_string(),
            Span::new(100, 100, 10, 0, 10, 0),
        );
        assert!(
            should_skip_low_value_entity(&entity),
            "zero-width entity (end_byte == start_byte) should be skipped"
        );
    }

    #[test]
    fn test_skip_negative_width_entity() {
        let entity = Entity::new(
            EntityId(2),
            EntityKind::Function,
            "_dummy".to_string(),
            Span::new(200, 150, 5, 0, 3, 0),
        );
        assert!(
            should_skip_low_value_entity(&entity),
            "negative-width entity (end_byte < start_byte) should be skipped"
        );
    }

    #[test]
    fn test_skip_reversed_row_entity() {
        // Valid bytes but reversed rows — tree-sitter phantom pattern
        let entity = Entity::new(
            EntityId(5),
            EntityKind::Function,
            "_dummy".to_string(),
            Span::new(100, 115, 497, 0, 496, 0),
        );
        assert!(
            should_skip_low_value_entity(&entity),
            "entity with valid bytes but end_row < start_row should be skipped"
        );
    }

    #[test]
    fn test_keep_valid_row_entity() {
        let entity = Entity::new(
            EntityId(6),
            EntityKind::Function,
            "real_func".to_string(),
            Span::new(100, 150, 5, 0, 6, 0),
        );
        assert!(
            !should_skip_low_value_entity(&entity),
            "entity with consistent positions should not be skipped"
        );
    }

    #[test]
    fn test_keep_valid_entity() {
        let entity = Entity::new(
            EntityId(3),
            EntityKind::Function,
            "real_func".to_string(),
            Span::new(0, 200, 0, 0, 5, 0),
        );
        assert!(
            !should_skip_low_value_entity(&entity),
            "valid entity with positive span should not be skipped"
        );
    }

    #[test]
    fn test_skip_underscore_method_with_doc_comment() {
        // _dummy methods (e.g., inside impl blocks) should be filtered
        // even when they have doc comments (compile_fail test cases).
        let entity = Entity::new(
            EntityId(7),
            EntityKind::Method,
            "_dummy".to_string(),
            Span::new(100, 115, 496, 0, 496, 15),
        );
        let mut entity = entity;
        entity.doc_comment = Some("```compile_fail\nstruct S;\n```".to_string());
        assert!(
            should_skip_low_value_entity(&entity),
            "_dummy method with small span should be skipped even with doc_comment"
        );
    }

    #[test]
    fn test_skip_underscore_function_with_doc_comment() {
        // _dummy functions should be filtered even with doc comments.
        let entity = Entity::new(
            EntityId(8),
            EntityKind::Function,
            "_dummy".to_string(),
            Span::new(100, 115, 496, 0, 496, 15),
        );
        let mut entity = entity;
        entity.doc_comment = Some("```compile_fail\nstruct S;\n```".to_string());
        assert!(
            should_skip_low_value_entity(&entity),
            "_dummy function with small span should be skipped even with doc_comment"
        );
    }

    #[test]
    fn test_skip_underscore_constant() {
        // _-prefixed constants (compile-time assertions) should be filtered
        // regardless of span size.
        // Small span case:
        let small = Entity::new(
            EntityId(9),
            EntityKind::Constant,
            "_ALIGNMENT_COMPATIBLE".to_string(),
            Span::new(100, 120, 77, 0, 77, 20),
        );
        assert!(
            should_skip_low_value_entity(&small),
            "_-prefixed constant should be skipped (small span)"
        );
        // Large span case (e.g., multiline assertion):
        let large = Entity::new(
            EntityId(10),
            EntityKind::Constant,
            "_ALIGNMENT_COMPATIBLE".to_string(),
            Span::new(100, 250, 77, 0, 78, 80),
        );
        assert!(
            should_skip_low_value_entity(&large),
            "_-prefixed constant should be skipped even with large span"
        );
    }

    #[test]
    fn test_skip_zero_variant_enum() {
        let entity = Entity::new(
            EntityId(4),
            EntityKind::Enum,
            "Void".to_string(),
            Span::new(50, 70, 1, 0, 1, 20),
        );
        assert!(
            should_skip_low_value_entity(&entity),
            "zero-variant enum under 80 bytes with no doc_comment should be skipped"
        );
    }

    #[test]
    fn test_import_only_group_definition() {
        // A group is import-only when its header is import-like and
        // every member is import-like. Such groups are dropped before
        // conversion/chunking because they carry no retrieval value.
        let group = EntityGroup::from_entity(import_entity(1, "use std::fmt;"), Language::Rust);
        assert!(
            is_import_only_group(&group),
            "a standalone import group must be import-only"
        );

        // Import header with import members stays import-only.
        let mut group_with_members =
            EntityGroup::from_entity(import_entity(2, "use std::io;"), Language::Rust);
        group_with_members
            .members
            .push(cce_types::entity::GroupedEntity {
                id: EntityId(3),
                kind: EntityKind::Export,
                name: "pub use crate::x".to_string(),
                ..Default::default()
            });
        assert!(
            is_import_only_group(&group_with_members),
            "import header + import members must be import-only"
        );

        // An import header that absorbed a non-import member must NOT be
        // dropped — dropping it would remove real content from retrieval.
        let mut mixed = EntityGroup::from_entity(import_entity(4, "use std::fmt;"), Language::Rust);
        mixed.members.push(cce_types::entity::GroupedEntity {
            id: EntityId(5),
            kind: EntityKind::Struct,
            name: "Config".to_string(),
            ..Default::default()
        });
        assert!(
            !is_import_only_group(&mixed),
            "a group mixing imports with real entities must never be dropped"
        );

        // Non-import headers are never import-only.
        let regular = EntityGroup::from_entity(
            Entity::new(
                EntityId(6),
                EntityKind::Struct,
                "Config".to_string(),
                Span::default(),
            ),
            Language::Rust,
        );
        assert!(
            !is_import_only_group(&regular),
            "a regular group must never be treated as import-only"
        );
    }

    #[test]
    fn test_resolve_group_hierarchy() {
        use cce_types::entity::EntityKind;

        let parent = Entity {
            id: EntityId(0),
            kind: EntityKind::Module,
            name: "parent".to_string(),
            children: vec![EntityId(1), EntityId(2)],
            ..Default::default()
        };
        let child1 = Entity::new(
            EntityId(1),
            EntityKind::Function,
            "child1".to_string(),
            Span::new(10, 20, 0, 0, 0, 0),
        );
        let child2 = Entity::new(
            EntityId(2),
            EntityKind::Function,
            "child2".to_string(),
            Span::new(30, 40, 0, 0, 0, 0),
        );

        let entities = vec![parent.clone(), child1.clone(), child2.clone()];
        let mut groups = vec![
            EntityGroup::from_entity(parent, Language::Rust),
            EntityGroup::from_entity(child1, Language::Rust),
            EntityGroup::from_entity(child2, Language::Rust),
        ];

        let links = resolve_group_hierarchy(&mut groups, &entities);
        assert_eq!(links, 2);
        assert_eq!(groups[1].parent_group_id.as_deref(), Some("group_0"));
        assert_eq!(groups[2].parent_group_id.as_deref(), Some("group_0"));
    }
}
