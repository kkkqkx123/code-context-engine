use cce_types::entity::{Entity, EntityId, EntityKind};
use cce_types::language::Language;
use std::collections::HashMap;

use crate::symbol_table::type_index::{MemberEntry, TypeEntry, TypeKey, TypeMemberIndex};

fn is_go_type_kind(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Struct | EntityKind::Interface | EntityKind::TypeAlias
    )
}

fn extract_receiver_type(entity: &Entity) -> Option<String> {
    if let Some(rt) = entity.metadata.get("receiver_type") {
        return Some(normalize_receiver(rt));
    }
    // Fallback only for Go methods (`func (recv Type) Name(...)`). Plain
    // functions must not be misread: their first parameter is not a receiver.
    let rest = entity
        .signature
        .trim_start()
        .strip_prefix("func")?
        .trim_start();
    if !rest.starts_with('(') {
        return None;
    }
    let end = rest.find(')')?;
    let inside = rest[1..end].trim();
    if inside.is_empty() {
        return None;
    }
    let parts: Vec<&str> = inside.split_whitespace().collect();
    let type_part = if parts.len() == 1 { parts[0] } else { parts[1] };
    Some(normalize_receiver(type_part))
}

fn normalize_receiver(s: &str) -> String {
    let t = s.trim().trim_start_matches('*').trim();
    // strip package prefix pkg.Type -> Type
    let simple = t.rsplit('.').next().unwrap_or(t);
    // strip generics if any
    let gener = if let Some(pos) = simple.find('[') {
        &simple[..pos]
    } else {
        simple
    };
    // also strip <T> generics
    let gener = if let Some(pos) = gener.find('<') {
        &gener[..pos]
    } else {
        gener
    };
    gener.trim().to_string()
}

pub fn build_type_index(
    entities: &[Entity],
    module_path: &str,
    file_path: &str,
    package: &str,
    language: Language,
    type_index: &mut TypeMemberIndex,
) {
    // first pass types
    for entity in entities {
        if is_go_type_kind(entity.kind) {
            let simple = entity.name.clone();
            let qualified = if package.is_empty() {
                simple.clone()
            } else {
                format!("{}.{}", package, simple)
            };
            let key = TypeKey::new(qualified, simple, file_path.to_string());
            let vis = crate::policy::detect_entity_visibility(entity, &Language::Go);
            let entry = TypeEntry::new(entity.id, key.clone(), entity.kind, language, vis);
            type_index.insert_type(key, entry);
        }
    }
    // build map simple -> keys for receiver resolution
    // second pass methods with receiver
    for entity in entities {
        if entity.kind.is_function_like() {
            if let Some(receiver) = extract_receiver_type(entity) {
                if receiver.is_empty() {
                    continue;
                }
                // find type entry with simple == receiver
                let candidates = type_index.get_type_by_simple(&receiver);
                let key = if let Some(entry) =
                    candidates.into_iter().find(|e| e.key.simple == receiver)
                {
                    entry.key.clone()
                } else {
                    // placeholder type for receiver not defined in this file but may be in same package
                    let qualified = if package.is_empty() {
                        receiver.clone()
                    } else {
                        format!("{}.{}", package, receiver)
                    };
                    TypeKey::new(qualified, receiver.clone(), file_path.to_string())
                };
                let vis = crate::policy::detect_entity_visibility(entity, &Language::Go);
                let member = MemberEntry {
                    entity_id: entity.id,
                    name: entity.name.clone(),
                    kind: EntityKind::Method,
                    visibility: vis,
                    is_static: false,
                    is_associated: false,
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
                    let placeholder = type_index.upsert_type_placeholder(key.clone(), language);
                    let _ = placeholder;
                }
                let _ = type_index.insert_member(&key, member);
            }
        }
        // interface methods are already parented? But Go interface methods are inside interface body and have parent?
        // If parent linking established, handle via parent fallback as well
        if let Some(parent_id) = entity.parent {
            let by_id: HashMap<EntityId, &Entity> = entities.iter().map(|e| (e.id, e)).collect();
            if let Some(parent) = by_id.get(&parent_id) {
                if parent.kind == EntityKind::Interface {
                    // member already handled via receiver? For interface, method elems are children of interface
                    let simple = parent.name.clone();
                    let qualified = if package.is_empty() {
                        simple.clone()
                    } else {
                        format!("{}.{}", package, simple)
                    };
                    let key = TypeKey::new(qualified, simple, file_path.to_string());
                    if type_index.get_type_by_key(&key).is_none() {
                        let parent_vis =
                            crate::policy::detect_entity_visibility(parent, &Language::Go);
                        let entry = TypeEntry::new(
                            parent.id,
                            key.clone(),
                            parent.kind,
                            language,
                            parent_vis,
                        );
                        type_index.insert_type(key.clone(), entry);
                    }
                    let vis = crate::policy::detect_entity_visibility(entity, &Language::Go);
                    let member = MemberEntry {
                        entity_id: entity.id,
                        name: entity.name.clone(),
                        kind: EntityKind::Method,
                        visibility: vis,
                        is_static: false,
                        is_associated: false,
                        span: entity.span,
                        file_path: file_path.to_string(),
                        module_path: None,
                        package: package.to_string(),
                    };
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
    fn go_receiver_normalizes() {
        assert_eq!(normalize_receiver("*MyStruct"), "MyStruct");
        assert_eq!(normalize_receiver("pkg.Type"), "Type");
        assert_eq!(normalize_receiver("*pkg.Type"), "Type");
        assert_eq!(normalize_receiver("Type[T]"), "Type");
    }

    #[test]
    fn go_extract_receiver_from_signature() {
        let e = Entity {
            signature: "func (s *MyStruct) Method(a int) string".to_string(),
            ..Default::default()
        };
        assert_eq!(extract_receiver_type(&e).unwrap(), "MyStruct");
        let e2 = Entity {
            signature: "func (s MyStruct) ValueReceiver()".to_string(),
            ..Default::default()
        };
        assert_eq!(extract_receiver_type(&e2).unwrap(), "MyStruct");
        let e3 = Entity {
            metadata: {
                let mut m = std::collections::HashMap::new();
                m.insert("receiver_type".to_string(), "*pkg.Foo".to_string());
                m
            },
            ..Default::default()
        };
        assert_eq!(extract_receiver_type(&e3).unwrap(), "Foo");
    }

    #[test]
    fn go_build_index() {
        let s = Entity {
            id: EntityId(1),
            kind: EntityKind::Struct,
            name: "S".to_string(),
            ..Default::default()
        };
        let m1 = Entity {
            id: EntityId(2),
            kind: EntityKind::Method,
            name: "M".to_string(),
            signature: "func (s S) M()".to_string(),
            ..Default::default()
        };
        let m2 = Entity {
            id: EntityId(3),
            kind: EntityKind::Method,
            name: "N".to_string(),
            signature: "func (s *S) N()".to_string(),
            ..Default::default()
        };
        let entities = vec![s, m1, m2];
        let mut idx = TypeMemberIndex::new();
        build_type_index(
            &entities,
            "pkg/utils",
            "pkg/utils/a.go",
            "pkg",
            Language::Go,
            &mut idx,
        );
        assert!(idx.get_members("pkg.S", "M").is_some());
        assert!(idx.get_members("pkg.S", "N").is_some());
    }
}
