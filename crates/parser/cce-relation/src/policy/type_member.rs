pub mod common;
pub mod go;
pub mod java;
pub mod javascript;
pub mod python;
pub mod rust;

use crate::symbol_table::type_index::TypeMemberIndex;
use cce_types::entity::Entity;
use cce_types::language::Language;

pub fn build_type_index_for_file(
    entities: &[Entity],
    module_path: &str,
    file_path: &str,
    package: &str,
    language: Language,
    index: &mut TypeMemberIndex,
) {
    match language {
        Language::Rust => {
            rust::build_type_index(entities, module_path, file_path, package, language, index)
        }
        Language::Python => {
            python::build_type_index(entities, module_path, file_path, package, language, index)
        }
        Language::Go => {
            go::build_type_index(entities, module_path, file_path, package, language, index)
        }
        Language::Java | Language::Kotlin | Language::Scala => {
            java::build_type_index(entities, module_path, file_path, package, language, index)
        }
        Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
            javascript::build_type_index(entities, module_path, file_path, package, language, index)
        }
        Language::CSharp => {
            // C# shares Java logic for type/member
            java::build_type_index(entities, module_path, file_path, package, language, index)
        }
        Language::Cpp => {
            java::build_type_index(entities, module_path, file_path, package, language, index)
        }
        Language::Dart => {
            // Dart similar to Java
            java::build_type_index(entities, module_path, file_path, package, language, index)
        }
        _ => {
            // generic fallback: use parent-based indexing
            generic_build(entities, module_path, file_path, package, language, index)
        }
    }
}

fn generic_build(
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
        if common::is_type_definition_kind(e.kind) && e.parent.is_none() {
            let simple = common::simple_name(&e.name).to_string();
            let qualified = if module_path.is_empty() {
                simple.clone()
            } else {
                format!("{}::{}", module_path, simple)
            };
            let key = TypeKey::new(qualified, simple, file_path.to_string());
            let vis = crate::policy::detect_entity_visibility(e, &language);
            let entry = TypeEntry::new(e.id, key.clone(), e.kind, language, vis);
            index.insert_type(key, entry);
        }
    }
    let mut id_to_key: HashMap<cce_types::entity::EntityId, TypeKey> = HashMap::new();
    for e in index.all_types() {
        id_to_key.insert(e.entity_id, e.key.clone());
    }
    for e in entities {
        if let Some(pid) = e.parent {
            if let Some(parent) = by_id.get(&pid) {
                if common::is_type_definition_kind(parent.kind) {
                    let key = match id_to_key.get(&pid) {
                        Some(k) => k.clone(),
                        None => {
                            let simple = common::simple_name(&parent.name).to_string();
                            let qualified = if module_path.is_empty() {
                                simple.clone()
                            } else {
                                format!("{}::{}", module_path, simple)
                            };
                            TypeKey::new(qualified, simple, file_path.to_string())
                        }
                    };
                    if common::is_type_definition_kind(e.kind) {
                        continue;
                    }
                    let vis = crate::policy::detect_entity_visibility(e, &language);
                    let is_static = common::is_static_modifiers(&e.modifiers);
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
                        let entry = TypeEntry::new(
                            parent.id,
                            key.clone(),
                            parent.kind,
                            language,
                            parent_vis,
                        );
                        index.insert_type(key.clone(), entry);
                        id_to_key.insert(pid, key.clone());
                    }
                    let _ = index.insert_member(&key, member);
                }
            }
        }
    }
}

/// Helpers used by builder and resolver
pub fn strip_generics(s: &str) -> &str {
    common::strip_generics(s)
}

pub fn simple_name(s: &str) -> &str {
    common::simple_name(s)
}
