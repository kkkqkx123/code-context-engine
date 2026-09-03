//! Parent-child relationship establishment for entities
//!
//! This module establishes hierarchical relationships between entities based on
//! span containment. Tree-sitter returns flat matches without nesting information,
//! so we manually establish:
//! - impl block -> method relationships
//! - module -> child entity relationships
//! - struct/class -> field relationships
//! - class -> method relationships

use cce_types::{Entity, EntityId, EntityKind};

/// Establish parent-child relationships between impl blocks and their methods.
///
/// Tree-sitter returns flat matches without nesting information, so we need
/// to manually establish impl -> method relationships based on span containment.
///
/// This function:
/// 1. Finds all impl blocks (InherentImpl and TraitImpl)
/// 2. For each function-like entity, checks if it's contained within an impl block
/// 3. Sets the function's parent to the impl block's ID
pub fn establish_impl_method_relationships(entities: &mut [Entity]) {
    let impl_blocks: Vec<(EntityId, std::ops::Range<usize>)> = entities
        .iter()
        .filter(|e| matches!(e.kind, EntityKind::InherentImpl | EntityKind::TraitImpl))
        .map(|e| (e.id, e.span.start_byte..e.span.end_byte))
        .collect();

    if impl_blocks.is_empty() {
        return;
    }

    for entity in entities.iter_mut() {
        if entity.kind.is_function_like() && entity.parent.is_none() {
            let entity_range = entity.span.start_byte..entity.span.end_byte;

            let mut best_impl_id = None;
            let mut best_impl_size = usize::MAX;

            for (impl_id, impl_range) in &impl_blocks {
                if impl_range.start <= entity_range.start
                    && entity_range.end <= impl_range.end
                    && impl_range.len() < best_impl_size
                {
                    best_impl_id = Some(*impl_id);
                    best_impl_size = impl_range.len();
                }
            }

            if let Some(impl_id) = best_impl_id {
                entity.parent = Some(impl_id);
                entity.depth += 1;
            }
        }
    }
}

/// Establish parent-child relationships between modules and their child entities.
///
/// Tree-sitter returns flat matches without nesting information, so we need
/// to manually establish module -> child relationships based on span containment.
///
/// This function:
/// 1. Takes module spans collected during first pass
/// 2. For each entity, checks if it's contained within a module
/// 3. Sets the entity's parent to the module's ID (if not already set)
pub fn establish_module_entity_relationships(
    entities: &mut [Entity],
    module_spans: &[(EntityId, std::ops::Range<usize>)],
) {
    if module_spans.is_empty() {
        return;
    }

    for entity in entities.iter_mut() {
        if entity.parent.is_none() && !entity.kind.is_module_like() {
            let entity_range = entity.span.start_byte..entity.span.end_byte;

            let mut best_module_id = None;
            let mut best_module_size = usize::MAX;

            for (module_id, module_range) in module_spans {
                if entity.id == *module_id {
                    continue;
                }

                if module_range.start <= entity_range.start
                    && entity_range.end <= module_range.end
                    && module_range.len() < best_module_size
                {
                    best_module_id = Some(*module_id);
                    best_module_size = module_range.len();
                }
            }

            if let Some(module_id) = best_module_id {
                entity.parent = Some(module_id);
                entity.depth += 1;
            }
        }
    }
}

/// Establish parent-child relationships between structs/classes and their fields.
///
/// Tree-sitter returns flat matches without nesting information, so we need
/// to manually link field entities to their parent container via span containment.
///
/// This function:
/// 1. Collects all container entities (Struct, Class, Enum, Trait, Interface)
/// 2. For each field entity, finds the smallest container that fully contains it
/// 3. Sets the field's parent to the container's ID
pub fn establish_struct_field_relationships(entities: &mut [Entity]) {
    let containers: Vec<(EntityId, std::ops::Range<usize>)> = entities
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                EntityKind::Struct
                    | EntityKind::Class
                    | EntityKind::Enum
                    | EntityKind::Trait
                    | EntityKind::Interface
            )
        })
        .map(|e| (e.id, e.span.start_byte..e.span.end_byte))
        .collect();

    if containers.is_empty() {
        return;
    }

    for entity in entities.iter_mut() {
        if matches!(entity.kind, EntityKind::Field | EntityKind::EnumVariant)
            && entity.parent.is_none()
        {
            let entity_range = entity.span.start_byte..entity.span.end_byte;

            let mut best_container_id = None;
            let mut best_container_size = usize::MAX;

            for (container_id, container_range) in &containers {
                if container_range.start <= entity_range.start
                    && entity_range.end <= container_range.end
                    && container_range.len() < best_container_size
                {
                    best_container_id = Some(*container_id);
                    best_container_size = container_range.len();
                }
            }

            if let Some(container_id) = best_container_id {
                entity.parent = Some(container_id);
                entity.depth += 1;
            }
        }
    }
}

/// Establish container -> function-like method relationships based on span containment.
///
/// This handles languages where methods are syntactically nested inside
/// class/struct bodies (e.g., Python, Ruby, JavaScript) rather than in
/// separate impl blocks (Rust).
///
/// Only sets parent on entities that don't already have a parent
/// (preserving relationships set by `establish_impl_method_relationships`).
pub fn establish_class_method_relationships(entities: &mut [Entity]) {
    let containers: Vec<(EntityId, std::ops::Range<usize>)> = entities
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                EntityKind::Class | EntityKind::Struct | EntityKind::Trait | EntityKind::Interface
            )
        })
        .map(|e| (e.id, e.span.start_byte..e.span.end_byte))
        .collect();

    if containers.is_empty() {
        return;
    }

    for entity in entities.iter_mut() {
        if entity.kind.is_function_like() && entity.parent.is_none() {
            let entity_range = entity.span.start_byte..entity.span.end_byte;

            let mut best_container_id = None;
            let mut best_container_size = usize::MAX;

            for (container_id, container_range) in &containers {
                if container_range.start <= entity_range.start
                    && entity_range.end <= container_range.end
                    && container_range.len() < best_container_size
                {
                    best_container_id = Some(*container_id);
                    best_container_size = container_range.len();
                }
            }

            if let Some(container_id) = best_container_id {
                entity.parent = Some(container_id);
                entity.depth += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;

    fn make_entity(id: u64, kind: EntityKind, name: &str, start: usize, end: usize) -> Entity {
        Entity::new(
            EntityId(id),
            kind,
            name.to_string(),
            Span::new(start, end, 0, 0, 0, 0),
        )
    }

    #[test]
    fn test_establish_impl_method_relationships() {
        let mut entities = vec![
            make_entity(0, EntityKind::InherentImpl, "MyStruct", 0, 100),
            make_entity(1, EntityKind::Method, "new", 10, 50),
            make_entity(2, EntityKind::Method, "get", 51, 90),
        ];

        establish_impl_method_relationships(&mut entities);

        assert_eq!(entities[1].parent, Some(EntityId(0)));
        assert_eq!(entities[2].parent, Some(EntityId(0)));
    }

    #[test]
    fn test_establish_module_entity_relationships() {
        let mut entities = vec![
            make_entity(0, EntityKind::Module, "mymodule", 0, 100),
            make_entity(1, EntityKind::Function, "foo", 10, 50),
            make_entity(2, EntityKind::Struct, "Bar", 51, 90),
        ];
        let module_spans = vec![(EntityId(0), 0..100)];

        establish_module_entity_relationships(&mut entities, &module_spans);

        assert_eq!(entities[1].parent, Some(EntityId(0)));
        assert_eq!(entities[2].parent, Some(EntityId(0)));
    }

    #[test]
    fn test_establish_struct_field_relationships() {
        let mut entities = vec![
            make_entity(0, EntityKind::Struct, "MyStruct", 0, 100),
            make_entity(1, EntityKind::Field, "x", 10, 30),
            make_entity(2, EntityKind::Field, "y", 31, 50),
        ];

        establish_struct_field_relationships(&mut entities);

        assert_eq!(entities[1].parent, Some(EntityId(0)));
        assert_eq!(entities[2].parent, Some(EntityId(0)));
    }

    #[test]
    fn test_establish_class_method_relationships() {
        let mut entities = vec![
            make_entity(0, EntityKind::Class, "MyClass", 0, 100),
            make_entity(1, EntityKind::Method, "__init__", 10, 50),
            make_entity(2, EntityKind::Method, "do_something", 51, 90),
        ];

        establish_class_method_relationships(&mut entities);

        assert_eq!(entities[1].parent, Some(EntityId(0)));
        assert_eq!(entities[2].parent, Some(EntityId(0)));
    }

    #[test]
    fn test_class_method_relationships_skips_existing_parent() {
        let mut entities = vec![
            make_entity(0, EntityKind::InherentImpl, "MyStruct", 0, 100),
            make_entity(1, EntityKind::Class, "MyClass", 0, 100),
            make_entity(2, EntityKind::Method, "new", 10, 50),
        ];
        entities[2].parent = Some(EntityId(0));

        establish_class_method_relationships(&mut entities);

        assert_eq!(entities[2].parent, Some(EntityId(0)));
    }
}
