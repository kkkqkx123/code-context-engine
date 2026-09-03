use cce_types::entity::{Entity, EntityId, EntityKind};
use cce_types::language::Language;
use std::collections::HashMap;

use super::common::simple_name;
use crate::symbol_table::type_index::{MemberEntry, TypeEntry, TypeKey, TypeMemberIndex};

fn is_java_type_kind(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Class
            | EntityKind::Interface
            | EntityKind::Enum
            | EntityKind::Struct
            | EntityKind::TypeAlias
    )
}

pub fn build_type_index(
    entities: &[Entity],
    module_path: &str,
    file_path: &str,
    package: &str,
    language: Language,
    type_index: &mut TypeMemberIndex,
) {
    let by_id: HashMap<EntityId, &Entity> = entities.iter().map(|e| (e.id, e)).collect();
    // first pass: create entries for all types, with qualified computed
    // For Java, qualified is package + outer inner concatenation
    // We'll build a temporary map from entity id to its qualified
    let mut id_to_qualified: HashMap<EntityId, String> = HashMap::new();
    for entity in entities {
        if is_java_type_kind(entity.kind) {
            let qualified = if let Some(parent_id) = entity.parent {
                if let Some(parent) = by_id.get(&parent_id) {
                    if is_java_type_kind(parent.kind) {
                        // inner class: parent qualified + "." + simple
                        let parent_qualified =
                            parent_qualified(entity, &by_id, module_path, package);
                        format!("{}.{}", parent_qualified, simple_name(&entity.name))
                    } else {
                        top_qualified(&entity.name, module_path, package)
                    }
                } else {
                    top_qualified(&entity.name, module_path, package)
                }
            } else {
                top_qualified(&entity.name, module_path, package)
            };
            id_to_qualified.insert(entity.id, qualified.clone());
            let simple = simple_name(&entity.name).to_string();
            let key = TypeKey::new(qualified, simple, file_path.to_string());
            let vis = crate::policy::detect_entity_visibility(entity, &language);
            let entry = TypeEntry::new(entity.id, key.clone(), entity.kind, language, vis);
            type_index.insert_type(key, entry);
        }
    }

    // helper to get key for type id
    let get_key = |id: EntityId| -> Option<TypeKey> {
        let q = id_to_qualified.get(&id)?;
        let s = simple_name(by_id.get(&id)?.name.as_str()).to_string();
        Some(TypeKey::new(q.clone(), s, file_path.to_string()))
    };

    for entity in entities {
        if let Some(parent_id) = entity.parent {
            if let Some(parent) = by_id.get(&parent_id) {
                if is_java_type_kind(parent.kind) {
                    if let Some(key) = get_key(parent_id) {
                        // skip types themselves (already inserted)
                        if is_java_type_kind(entity.kind) {
                            continue;
                        }
                        let is_ctor =
                            entity.kind == EntityKind::Constructor || entity.name == parent.name;
                        let is_field = matches!(
                            entity.kind,
                            EntityKind::Field
                                | EntityKind::Property
                                | EntityKind::Variable
                                | EntityKind::Constant
                        );
                        let kind = if is_ctor {
                            EntityKind::Constructor
                        } else if is_field {
                            EntityKind::Field
                        } else {
                            entity.kind
                        };
                        let vis = crate::policy::detect_entity_visibility(entity, &language);
                        let is_static = entity
                            .modifiers
                            .iter()
                            .any(|m| m.eq_ignore_ascii_case("static"));
                        let member = MemberEntry {
                            entity_id: entity.id,
                            name: entity.name.clone(),
                            kind,
                            visibility: vis,
                            is_static,
                            is_associated: is_static,
                            span: entity.span,
                            file_path: file_path.to_string(),
                            module_path: if module_path.is_empty() {
                                None
                            } else {
                                Some(module_path.to_string())
                            },
                            package: package.to_string(),
                        };
                        // ensure type exists (already)
                        let _ = type_index.insert_member(&key, member);
                    }
                }
            }
        }
    }
}

fn top_qualified(name: &str, module_path: &str, package: &str) -> String {
    let simple = simple_name(name);
    if !package.is_empty() {
        format!("{}.{}", package, simple)
    } else if !module_path.is_empty() && module_path.contains('.') {
        format!("{}.{}", module_path, simple)
    } else if !module_path.is_empty() {
        // fallback: module_path may be file path like com/example/Foo -> already package?
        format!("{}.{}", module_path.replace('/', "."), simple)
    } else {
        simple.to_string()
    }
}

fn parent_qualified(
    entity: &Entity,
    by_id: &HashMap<EntityId, &Entity>,
    module_path: &str,
    package: &str,
) -> String {
    if let Some(parent_id) = entity.parent {
        if let Some(parent) = by_id.get(&parent_id) {
            return top_qualified(&parent.name, module_path, package);
        }
    }
    package.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn java_top_qualified() {
        assert_eq!(
            top_qualified("Foo", "com.example", "com.example"),
            "com.example.Foo"
        );
        assert_eq!(top_qualified("Bar", "", ""), "Bar");
    }

    #[test]
    fn java_build_index() {
        let class = Entity {
            id: EntityId(1),
            kind: EntityKind::Class,
            name: "Foo".to_string(),
            ..Default::default()
        };
        let method = Entity {
            id: EntityId(2),
            kind: EntityKind::Method,
            name: "m".to_string(),
            parent: Some(EntityId(1)),
            modifiers: vec!["public".to_string()],
            ..Default::default()
        };
        let field = Entity {
            id: EntityId(3),
            kind: EntityKind::Field,
            name: "x".to_string(),
            parent: Some(EntityId(1)),
            ..Default::default()
        };
        let inner = Entity {
            id: EntityId(4),
            kind: EntityKind::Class,
            name: "Inner".to_string(),
            parent: Some(EntityId(1)),
            ..Default::default()
        };
        let inner_method = Entity {
            id: EntityId(5),
            kind: EntityKind::Method,
            name: "innerM".to_string(),
            parent: Some(EntityId(4)),
            ..Default::default()
        };
        let entities = vec![class, method, field, inner, inner_method];
        let mut idx = TypeMemberIndex::new();
        build_type_index(
            &entities,
            "com.example.Foo",
            "src/com/example/Foo.java",
            "com.example",
            Language::Java,
            &mut idx,
        );
        assert!(idx.get_type("com.example.Foo").is_some());
        assert!(idx.get_members("com.example.Foo", "m").is_some());
        // field is stored separately in fields map
        assert!(
            idx.get_type("com.example.Foo")
                .unwrap()
                .fields
                .contains_key("x")
        );
        assert!(
            idx.get_type("com.example.Inner").is_some()
                || idx.get_type("com.example.Foo.Inner").is_some()
        );
    }
}
