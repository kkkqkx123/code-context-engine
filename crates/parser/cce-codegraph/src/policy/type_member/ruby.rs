use cce_types::entity::{Entity, EntityKind};
use cce_types::language::Language;

use crate::symbol_table::type_index::TypeMemberIndex;

fn is_ruby_type_kind(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Class | EntityKind::Module | EntityKind::TypeAlias
    )
}

/// Build type index for Ruby.
pub fn build_type_index(
    entities: &[Entity],
    module_path: &str,
    file_path: &str,
    package: &str,
    language: Language,
    index: &mut TypeMemberIndex,
) {
    use crate::symbol_table::type_index::{MemberEntry, TypeEntry, TypeKey};
    use std::collections::HashMap;
    let by_id: HashMap<_, _> = entities.iter().map(|e| (e.id, e)).collect();
    for e in entities {
        if is_ruby_type_kind(e.kind) && e.parent.is_none() {
            let simple = super::common::simple_name(&e.name).to_string();
            let qualified = if module_path.is_empty() {
                simple.clone()
            } else {
                format!("{module_path}::{simple}")
            };
            let key = TypeKey::new(qualified, simple, file_path.to_string());
            let vis = crate::policy::detect_entity_visibility(e, &language);
            index.insert_type(
                key.clone(),
                TypeEntry::new(e.id, key, e.kind, language, vis),
            );
        }
    }
    let mut id_to_key: HashMap<cce_types::entity::EntityId, TypeKey> = HashMap::new();
    for e in index.all_types() {
        id_to_key.insert(e.entity_id, e.key.clone());
    }
    for e in entities {
        if let Some(pid) = e.parent {
            if let Some(parent) = by_id.get(&pid) {
                if is_ruby_type_kind(parent.kind) {
                    let key = match id_to_key.get(&pid) {
                        Some(k) => k.clone(),
                        None => {
                            let simple = super::common::simple_name(&parent.name).to_string();
                            let qualified = if module_path.is_empty() {
                                simple.clone()
                            } else {
                                format!("{module_path}::{simple}")
                            };
                            TypeKey::new(qualified, simple, file_path.to_string())
                        }
                    };
                    if is_ruby_type_kind(e.kind) {
                        continue;
                    }
                    let vis = crate::policy::detect_entity_visibility(e, &language);
                    let is_static = super::common::is_static_modifiers(&e.modifiers);
                    let member = MemberEntry {
                        entity_id: e.id,
                        name: e.name.clone(),
                        kind: e.kind,
                        visibility: vis,
                        is_static,
                        is_associated: is_static,
                        span: e.span,
                        file_path: file_path.to_string(),
                        module_path: if module_path.is_empty() {
                            None
                        } else {
                            Some(module_path.to_string())
                        },
                        package: package.to_string(),
                    };
                    if index.get_type_by_key(&key).is_none() {
                        let parent_vis = crate::policy::detect_entity_visibility(parent, &language);
                        index.insert_type(
                            key.clone(),
                            TypeEntry::new(
                                parent.id,
                                key.clone(),
                                parent.kind,
                                language,
                                parent_vis,
                            ),
                        );
                        id_to_key.insert(pid, key.clone());
                    }
                    let _ = index.insert_member(&key, member);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::entity::EntityId;

    #[test]
    fn ruby_class_method_indexed() {
        let parent = Entity {
            id: EntityId(1),
            name: "User".to_string(),
            kind: EntityKind::Class,
            parent: None,
            ..Default::default()
        };
        let method = Entity {
            id: EntityId(2),
            name: "save".to_string(),
            kind: EntityKind::Method,
            parent: Some(EntityId(1)),
            ..Default::default()
        };
        let mut index = TypeMemberIndex::new();
        build_type_index(
            &[parent, method],
            "lib/user",
            "/project/lib/user.rb",
            "pkg",
            Language::Ruby,
            &mut index,
        );
        assert_eq!(index.all_types().len(), 1);
    }
}
