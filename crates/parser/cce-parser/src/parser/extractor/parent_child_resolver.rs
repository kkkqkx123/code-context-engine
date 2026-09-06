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

/// Establish parent-child relationships between Go structs and their receiver methods.
///
/// Go methods are top-level declarations with `func (r ReceiverType) Method()` syntax
/// rather than being syntactically nested inside struct bodies. This function uses
/// the `receiver_type` metadata (extracted from the signature by `receiver_extractor`)
/// to link methods to their owning struct/type entity.
///
/// Matching is done by normalizing the receiver type name (stripping pointer `*` and
/// package qualifiers) and comparing against struct/class entity names.
pub fn establish_go_method_relationships(entities: &mut [Entity]) {
    // Collect struct/class names and their IDs for lookup.
    let struct_names: Vec<(String, EntityId)> = entities
        .iter()
        .filter(|e| matches!(e.kind, EntityKind::Struct | EntityKind::Class))
        .map(|e| (normalize_type_name(&e.name), e.id))
        .collect();

    if struct_names.is_empty() {
        return;
    }

    // Build a lookup map from normalized type name to entity ID.
    let name_to_id: std::collections::HashMap<String, EntityId> =
        struct_names.into_iter().collect();

    for entity in entities.iter_mut() {
        if !entity.kind.is_function_like() || entity.parent.is_some() {
            continue;
        }
        if let Some(receiver_type) = entity.metadata.get("receiver_type") {
            let normalized = normalize_type_name(receiver_type);
            if let Some(&struct_id) = name_to_id.get(&normalized) {
                entity.parent = Some(struct_id);
                entity.depth += 1;
            }
        }
    }
}

/// Normalize a Go type name for matching: strip pointer prefix and package qualifier.
///
/// Examples: `*User` → `User`, `pkg.User` → `User`, `User` → `User`.
fn normalize_type_name(name: &str) -> String {
    let name = name.trim().trim_start_matches('*');
    // Strip package qualifier: `pkg.Type` → `Type`
    if let Some(pos) = name.rfind('.') {
        name[pos + 1..].to_string()
    } else {
        name.to_string()
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

    #[test]
    fn test_establish_go_method_relationships() {
        let mut entities = vec![
            make_entity(0, EntityKind::Struct, "User", 0, 100),
            make_entity(1, EntityKind::Struct, "Order", 200, 300),
        ];
        // Methods with receiver_type metadata (as extracted by receiver_extractor)
        let mut method1 = make_entity(2, EntityKind::Method, "Greet", 110, 150);
        method1.set_metadata("receiver_type", "User".to_string());
        let mut method2 = make_entity(3, EntityKind::Method, "String", 110, 150);
        method2.set_metadata("receiver_type", "User".to_string());
        let mut method3 = make_entity(4, EntityKind::Method, "Total", 310, 350);
        method3.set_metadata("receiver_type", "Order".to_string());
        // Method without receiver_type (plain function)
        let method4 = make_entity(5, EntityKind::Function, "helper", 400, 420);
        entities.push(method1);
        entities.push(method2);
        entities.push(method3);
        entities.push(method4);

        establish_go_method_relationships(&mut entities);

        assert_eq!(entities[2].parent, Some(EntityId(0))); // Greet -> User
        assert_eq!(entities[3].parent, Some(EntityId(0))); // String -> User
        assert_eq!(entities[4].parent, Some(EntityId(1))); // Total -> Order
        assert_eq!(entities[5].parent, None); // helper has no receiver
    }

    #[test]
    fn test_establish_go_method_pointer_receiver() {
        let mut entities = vec![make_entity(0, EntityKind::Struct, "MyStruct", 0, 100)];
        let mut method = make_entity(1, EntityKind::Method, "DoStuff", 110, 150);
        method.set_metadata("receiver_type", "*MyStruct"); // pointer receiver
        entities.push(method);

        establish_go_method_relationships(&mut entities);

        assert_eq!(entities[1].parent, Some(EntityId(0)));
    }

    #[test]
    fn test_establish_go_method_qualified_receiver() {
        let mut entities = vec![make_entity(0, EntityKind::Struct, "Foo", 0, 100)];
        let mut method = make_entity(1, EntityKind::Method, "Bar", 110, 150);
        method.set_metadata("receiver_type", "pkg.Foo"); // package-qualified
        entities.push(method);

        establish_go_method_relationships(&mut entities);

        assert_eq!(entities[1].parent, Some(EntityId(0)));
    }

    #[test]
    fn test_establish_go_method_skips_already_parented() {
        let mut entities = vec![
            make_entity(0, EntityKind::Struct, "User", 0, 100),
            make_entity(1, EntityKind::InherentImpl, "impl", 0, 100),
        ];
        let mut method = make_entity(2, EntityKind::Method, "Greet", 10, 50);
        method.set_metadata("receiver_type", "User");
        method.parent = Some(EntityId(1)); // already has parent
        entities.push(method);

        establish_go_method_relationships(&mut entities);

        // Should keep existing parent, not override
        assert_eq!(entities[2].parent, Some(EntityId(1)));
    }

    #[test]
    fn test_normalize_type_name() {
        assert_eq!(normalize_type_name("User"), "User");
        assert_eq!(normalize_type_name("*User"), "User");
        assert_eq!(normalize_type_name("pkg.User"), "User");
        assert_eq!(normalize_type_name("*pkg.User"), "User");
        assert_eq!(normalize_type_name("  *pkg.User  "), "User");
    }
}
