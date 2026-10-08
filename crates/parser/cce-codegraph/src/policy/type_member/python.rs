use cce_types::entity::{Entity, EntityId, EntityKind};
use cce_types::language::Language;
use std::collections::HashMap;

use super::common::simple_name;
use crate::symbol_table::type_index::{MemberEntry, TypeEntry, TypeKey, TypeMemberIndex};

fn is_python_type_kind(kind: EntityKind) -> bool {
    kind == EntityKind::Class
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
    // first pass types
    for entity in entities {
        if is_python_type_kind(entity.kind) {
            let simple = simple_name(&entity.name).to_string();
            let qualified = if module_path.is_empty() {
                simple.clone()
            } else {
                format!("{}.{}", module_path.replace("::", "."), simple)
            };
            let key = TypeKey::new(qualified, simple, file_path.to_string());
            let vis = crate::policy::detect_entity_visibility(entity, &Language::Python);
            let entry = TypeEntry::new(entity.id, key.clone(), entity.kind, language, vis);
            type_index.insert_type(key, entry);
        }
    }
    // map id -> key for quickly finding owner
    let mut id_to_key: HashMap<EntityId, TypeKey> = HashMap::new();
    for e in type_index.all_types() {
        id_to_key.insert(e.entity_id, e.key.clone());
    }

    for entity in entities {
        if let Some(parent_id) = entity.parent {
            if let Some(parent) = by_id.get(&parent_id) {
                if is_python_type_kind(parent.kind) {
                    let key = id_to_key.get(&parent_id).cloned().unwrap_or_else(|| {
                        let simple = simple_name(&parent.name).to_string();
                        let qualified = if module_path.is_empty() {
                            simple.clone()
                        } else {
                            format!("{}.{}", module_path.replace("::", "."), simple)
                        };
                        TypeKey::new(qualified, simple, file_path.to_string())
                    });
                    let is_ctor = entity.name == "__init__";
                    let kind = if is_ctor {
                        EntityKind::Constructor
                    } else if entity.kind == EntityKind::Method
                        || entity.kind == EntityKind::Function
                    {
                        EntityKind::Method
                    } else {
                        entity.kind
                    };
                    let vis = crate::policy::detect_entity_visibility(entity, &Language::Python);
                    let modifiers_lower: Vec<String> = entity
                        .modifiers
                        .iter()
                        .map(|m| m.to_ascii_lowercase())
                        .collect();
                    let meta_has = |k: &str| {
                        entity.metadata.contains_key(k) || modifiers_lower.iter().any(|m| m == k)
                    };
                    let is_static = meta_has("staticmethod");
                    let is_associated = meta_has("classmethod");
                    let is_property = meta_has("property");
                    let final_kind = if is_property {
                        EntityKind::Property
                    } else {
                        kind
                    };
                    let module_path_opt = if module_path.is_empty() {
                        None
                    } else {
                        Some(module_path.replace("::", "."))
                    };
                    let member = MemberEntry {
                        entity_id: entity.id,
                        name: entity.name.clone(),
                        kind: final_kind,
                        visibility: vis,
                        is_static,
                        is_associated,
                        span: entity.span,
                        file_path: file_path.to_string(),
                        module_path: module_path_opt,
                        package: package.to_string(),
                    };
                    // ensure type exists
                    if type_index.get_type_by_key(&key).is_none() {
                        let vis_parent =
                            crate::policy::detect_entity_visibility(parent, &Language::Python);
                        let entry = TypeEntry::new(
                            parent.id,
                            key.clone(),
                            parent.kind,
                            language,
                            vis_parent,
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
    use cce_types::entity::EntityKind;

    #[test]
    fn python_class_methods() {
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
            modifiers: vec!["staticmethod".to_string()],
            ..Default::default()
        };
        let ctor = Entity {
            id: EntityId(3),
            kind: EntityKind::Method,
            name: "__init__".to_string(),
            parent: Some(EntityId(1)),
            ..Default::default()
        };
        let entities = vec![class, method, ctor];
        let mut idx = TypeMemberIndex::new();
        build_type_index(
            &entities,
            "myapp.mod",
            "myapp/mod.py",
            "myapp",
            Language::Python,
            &mut idx,
        );
        let foo_members = idx.get_members("myapp.mod.Foo", "m").unwrap();
        assert!(foo_members[0].is_static);
        assert_eq!(idx.get_type("myapp.mod.Foo").unwrap().constructors.len(), 1);
    }
}
