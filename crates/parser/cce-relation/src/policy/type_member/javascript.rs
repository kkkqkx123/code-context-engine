use cce_types::entity::{Entity, EntityId, EntityKind};
use cce_types::language::Language;
use std::collections::HashMap;

use super::common::simple_name;
use crate::symbol_table::type_index::{MemberEntry, TypeEntry, TypeKey, TypeMemberIndex};

fn is_js_type_kind(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Class
            | EntityKind::Interface
            | EntityKind::Enum
            | EntityKind::TypeAlias
            | EntityKind::Struct
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
    for entity in entities {
        if is_js_type_kind(entity.kind) {
            let simple = simple_name(&entity.name).to_string();
            let qualified = if module_path.is_empty() {
                simple.clone()
            } else {
                format!("{}.{}", module_path, simple)
            };
            let key = TypeKey::new(qualified, simple, file_path.to_string());
            let vis = crate::policy::detect_entity_visibility(entity, &language);
            let entry = TypeEntry::new(entity.id, key.clone(), entity.kind, language, vis);
            type_index.insert_type(key, entry);
        }
    }
    let mut id_to_key: HashMap<EntityId, TypeKey> = HashMap::new();
    for e in type_index.all_types() {
        id_to_key.insert(e.entity_id, e.key.clone());
    }

    for entity in entities {
        if let Some(parent_id) = entity.parent {
            if let Some(parent) = by_id.get(&parent_id) {
                if is_js_type_kind(parent.kind) {
                    if is_js_type_kind(entity.kind) {
                        continue;
                    }
                    let key = match id_to_key.get(&parent_id) {
                        Some(k) => k.clone(),
                        None => {
                            let simple = simple_name(&parent.name).to_string();
                            let qualified = if module_path.is_empty() {
                                simple.clone()
                            } else {
                                format!("{}.{}", module_path, simple)
                            };
                            TypeKey::new(qualified, simple, file_path.to_string())
                        }
                    };
                    // skip types
                    let is_ctor =
                        entity.name == "constructor" || entity.kind == EntityKind::Constructor;
                    let is_field = matches!(
                        entity.kind,
                        EntityKind::Field | EntityKind::Property | EntityKind::Variable
                    );
                    let kind = if is_ctor {
                        EntityKind::Constructor
                    } else if is_field {
                        EntityKind::Field
                    } else {
                        entity.kind
                    };
                    let mut vis = crate::policy::detect_entity_visibility(entity, &language);
                    // check for # prefix
                    if entity.name.starts_with('#') && vis != crate::symbol::Visibility::Private {
                        vis = crate::symbol::Visibility::Private;
                    }
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
                    if type_index.get_type_by_key(&key).is_none() {
                        let parent_vis = crate::policy::detect_entity_visibility(parent, &language);
                        let entry = TypeEntry::new(
                            parent.id,
                            key.clone(),
                            parent.kind,
                            language,
                            parent_vis,
                        );
                        type_index.insert_type(key.clone(), entry);
                        id_to_key.insert(parent.id, key.clone());
                    }
                    let _ = type_index.insert_member(&key, member);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn js_build() {
        let class = Entity {
            id: EntityId(1),
            kind: EntityKind::Class,
            name: "C".to_string(),
            ..Default::default()
        };
        let method = Entity {
            id: EntityId(2),
            kind: EntityKind::Method,
            name: "m".to_string(),
            parent: Some(EntityId(1)),
            ..Default::default()
        };
        let private = Entity {
            id: EntityId(3),
            kind: EntityKind::Field,
            name: "#p".to_string(),
            parent: Some(EntityId(1)),
            ..Default::default()
        };
        let static_m = Entity {
            id: EntityId(4),
            kind: EntityKind::Method,
            name: "s".to_string(),
            parent: Some(EntityId(1)),
            modifiers: vec!["static".to_string()],
            ..Default::default()
        };
        let ctor = Entity {
            id: EntityId(5),
            kind: EntityKind::Constructor,
            name: "constructor".to_string(),
            parent: Some(EntityId(1)),
            ..Default::default()
        };
        let entities = vec![class, method, private, static_m, ctor];
        let mut idx = TypeMemberIndex::new();
        build_type_index(
            &entities,
            "src/a",
            "src/a.ts",
            "pkg",
            Language::TypeScript,
            &mut idx,
        );
        assert!(idx.get_members("src/a.C", "m").is_some());
        assert!(idx.get_type("src/a.C").unwrap().fields.contains_key("#p"));
        assert!(idx.get_type("src/a.C").unwrap().constructors.len() == 1);
    }
}
