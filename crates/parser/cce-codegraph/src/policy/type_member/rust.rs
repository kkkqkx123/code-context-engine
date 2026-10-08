use cce_types::entity::{Entity, EntityId, EntityKind};
use cce_types::language::Language;
use std::collections::HashMap;

use super::common::{simple_name, strip_generics};
use crate::symbol_table::type_index::{MemberEntry, TypeEntry, TypeKey, TypeMemberIndex};

fn is_rust_type_kind(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Struct
            | EntityKind::Enum
            | EntityKind::Union
            | EntityKind::Trait
            | EntityKind::TypeAlias
    )
}

fn build_qualified(module_path: &str, simple: &str) -> String {
    if module_path.is_empty() {
        simple.to_string()
    } else {
        format!("{}::{}", module_path, simple)
    }
}

pub fn build_type_index(
    entities: &[Entity],
    module_path: &str,
    file_path: &str,
    package: &str,
    language: Language,
    type_index: &mut TypeMemberIndex,
) {
    // Map id -> entity for parent lookup
    let by_id: HashMap<EntityId, &Entity> = entities.iter().map(|e| (e.id, e)).collect();

    // First pass: insert real type definitions
    for entity in entities {
        if is_rust_type_kind(entity.kind) && entity.parent.is_none() {
            let simple = simple_name(&entity.name).to_string();
            let qualified = build_qualified(module_path, &simple);
            let key = TypeKey::new(qualified, simple, file_path.to_string());
            let vis = crate::policy::detect_entity_visibility(entity, &Language::Rust);
            let entry = TypeEntry::new(entity.id, key.clone(), entity.kind, language, vis);
            type_index.insert_type(key, entry);
        }
    }

    // Second pass: handle impl members
    // Need to find impl blocks
    for entity in entities {
        if entity.kind.is_function_like() {
            if let Some(parent_id) = entity.parent {
                if let Some(parent) = by_id.get(&parent_id) {
                    if parent.kind.is_impl_block() {
                        let impl_for = parent
                            .metadata
                            .get("impl_for_type")
                            .cloned()
                            .unwrap_or_else(|| parent.name.clone());
                        let simple = simple_name(&impl_for).to_string();
                        let stripped = strip_generics(&simple).to_string();
                        // Try to find existing type entry with same simple name in this module
                        // Search types with simple == stripped
                        let qualified = build_qualified(module_path, &stripped);
                        let key = TypeKey::new(
                            qualified.clone(),
                            stripped.clone(),
                            file_path.to_string(),
                        );
                        // Ensure placeholder or real entry exists
                        if type_index.get_type_by_key(&key).is_none() {
                            // check if there is a real type elsewhere with same simple but different qualified?
                            // fallback to simple search: find any type with simple == stripped
                            // Prefer non-placeholder (real) types over placeholders
                            let candidates: Vec<_> = type_index
                                .get_type_by_simple(&stripped)
                                .into_iter()
                                .filter(|e| e.key.simple == stripped)
                                .collect();
                            let found_key = candidates
                                .iter()
                                .find(|e| !e.is_placeholder)
                                .or(candidates.first())
                                .map(|e| e.key.clone());
                            if let Some(existing) = found_key {
                                insert_rust_member(
                                    entity, &existing, file_path, package, type_index,
                                );
                            } else {
                                // placeholder
                                let ph = type_index.upsert_type_placeholder(key.clone(), language);
                                // if placeholder was just created, keep it; otherwise reuse
                                let _ = ph;
                                insert_rust_member(entity, &key, file_path, package, type_index);
                            }
                        } else {
                            insert_rust_member(entity, &key, file_path, package, type_index);
                        }
                    }
                }
            }
        }
    }
}

fn insert_rust_member(
    entity: &Entity,
    key: &TypeKey,
    file_path: &str,
    package: &str,
    type_index: &mut TypeMemberIndex,
) {
    let vis = crate::policy::detect_entity_visibility(entity, &cce_types::language::Language::Rust);
    let is_assoc = !super::common::has_self_param(&entity.parameters);
    let kind = if is_assoc {
        EntityKind::Function
    } else {
        EntityKind::Method
    };
    // For impl trait members, kind remains Method/Function but is_associated distinguishes
    let is_static = false;
    // Use the target type's module path (from key.qualified) rather than the
    // current file's module path, so that cross-file impl members get the
    // correct module context for visibility checks.
    let module_path = key.qualified.rsplit_once("::").map(|(m, _)| m.to_string());
    let member = MemberEntry {
        entity_id: entity.id,
        name: entity.name.clone(),
        kind,
        visibility: vis,
        is_static,
        is_associated: is_assoc,
        span: entity.span,
        file_path: file_path.to_string(),
        module_path,
        package: package.to_string(),
    };
    let _ = type_index.insert_member(key, member);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entity(id: u64, kind: EntityKind, name: &str, parent: Option<EntityId>) -> Entity {
        Entity {
            id: EntityId(id),
            kind,
            name: name.to_string(),
            parent,
            ..Default::default()
        }
    }

    #[test]
    fn rust_struct_and_impl_member() {
        let module_path = "a";
        let file_path = "src/a.rs";
        let package = "pkg";
        let mut idx = TypeMemberIndex::new();
        let s = make_entity(1, EntityKind::Struct, "Foo", None);
        let impl_entity = Entity {
            id: EntityId(2),
            kind: EntityKind::InherentImpl,
            name: "Foo".to_string(),
            metadata: {
                let mut m = std::collections::HashMap::new();
                m.insert("impl_for_type".to_string(), "Foo".to_string());
                m
            },
            ..Default::default()
        };
        let method = Entity {
            id: EntityId(3),
            kind: EntityKind::Function,
            name: "bar".to_string(),
            parent: Some(EntityId(2)),
            parameters: vec![("self".to_string(), None)],
            ..Default::default()
        };
        let associated = Entity {
            id: EntityId(4),
            kind: EntityKind::Function,
            name: "new".to_string(),
            parent: Some(EntityId(2)),
            parameters: vec![],
            ..Default::default()
        };
        let entities = vec![s, impl_entity, method, associated];
        build_type_index(
            &entities,
            module_path,
            file_path,
            package,
            Language::Rust,
            &mut idx,
        );
        assert!(idx.get_type("a::Foo").is_some());
        let members = idx.get_members("a::Foo", "bar").unwrap();
        assert!(!members[0].is_associated);
        let assoc = idx.get_members("a::Foo", "new").unwrap();
        assert!(assoc[0].is_associated);
    }

    #[test]
    fn rust_placeholder_for_cross_file_impl() {
        let mut idx = TypeMemberIndex::new();
        let impl_entity = Entity {
            id: EntityId(2),
            kind: EntityKind::InherentImpl,
            name: "Foo".to_string(),
            metadata: {
                let mut m = std::collections::HashMap::new();
                m.insert("impl_for_type".to_string(), "Foo".to_string());
                m
            },
            ..Default::default()
        };
        let method = Entity {
            id: EntityId(3),
            kind: EntityKind::Function,
            name: "bar".to_string(),
            parent: Some(EntityId(2)),
            parameters: vec![("self".to_string(), None)],
            ..Default::default()
        };
        let entities = vec![impl_entity, method];
        build_type_index(&entities, "b", "src/b.rs", "pkg", Language::Rust, &mut idx);
        // placeholder should exist
        assert!(idx.get_type("b::Foo").unwrap().is_placeholder);
    }
}
