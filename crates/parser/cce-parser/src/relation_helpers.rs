//! Relation helpers for `cce-parser` internal use.
//!
//! The authoritative visibility determination lives in `cce-relation::policy`
//! (per-language modules under `crates/parser/cce_relation/src/policy/`).
//! This module provides a lightweight 3-value re-implementation to avoid a
//! circular dependency between `cce-parser` and `cce-relation`. The dispatch
//! structure mirrors the authoritative policy but collapses to
//! `Public/Package/Private`. Keep both sides in sync when adding a language.

use cce_types::entity::{Entity, EntityId};
use cce_types::import::{ReexportRecord, StandardizedImportTable};
use cce_types::language::Language;
use cce_types::{ImportTable, ParseError};
use tree_sitter::Tree;

use crate::parser::extractor::create_extractor_with_registry;

mod common;
mod dart;
mod go;
mod javascript;
mod jvm;
mod python;
mod rust;

/// Visibility of an entity (3-value collapsed model).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Package,
    Private,
}

fn visibility_from_signal(signal: &str, language: &Language) -> Option<Visibility> {
    match language {
        Language::Rust => rust::visibility_from_signal(signal),
        Language::Go => go::visibility_from_signal(signal),
        Language::Python => python::visibility_from_signal(signal),
        Language::Dart => dart::visibility_from_signal(signal),
        Language::Java | Language::Kotlin | Language::Scala | Language::CSharp => {
            jvm::visibility_from_signal(signal)
        }
        Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
            javascript::visibility_from_signal(signal)
        }
        _ => {
            let t = signal.to_lowercase();
            let t = t.trim();
            if t == "pub" || t == "public" || t == "export" || t == "exported" {
                return Some(Visibility::Public);
            }
            if t == "pub(crate)" || t == "crate" || t == "internal" || t == "package" {
                return Some(Visibility::Package);
            }
            if t == "pub(super)"
                || t == "super"
                || t == "protected"
                || t == "protected internal"
                || t == "private protected"
            {
                return Some(Visibility::Package);
            }
            if t == "pub(self)" || t == "self" || t == "private" {
                return Some(Visibility::Private);
            }
            if t.starts_with("pub(in") {
                return Some(Visibility::Package);
            }
            if t.starts_with("friend") {
                return Some(Visibility::Private);
            }
            None
        }
    }
}

fn visibility_from_name(name: &str, language: &Language) -> Option<Visibility> {
    match language {
        Language::Go => go::visibility_from_name(name),
        Language::Python => python::visibility_from_name(name),
        Language::Dart => dart::visibility_from_name(name),
        Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
            javascript::visibility_from_name(name)
        }
        _ => None,
    }
}

pub fn detect_entity_visibility(entity: &Entity, language: &Language) -> Visibility {
    for modifier in &entity.modifiers {
        if let Some(vis) = visibility_from_signal(&modifier.to_lowercase(), language) {
            return vis;
        }
    }
    if let Some(signal) = entity.metadata.get("visibility") {
        if let Some(vis) = visibility_from_signal(&signal.to_lowercase(), language) {
            return vis;
        }
    }
    if *language == Language::Python {
        if let Some(flag) = entity.metadata.get("is_exported_by_all") {
            if flag == "true" {
                return Visibility::Public;
            } else if flag == "false" {
                return Visibility::Private;
            }
        }
    }
    if let Some(vis) = visibility_from_name(&entity.name, language) {
        match language {
            Language::Go | Language::Python | Language::Dart => return vis,
            Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx
                if vis == Visibility::Private =>
            {
                return vis;
            }
            _ => {}
        }
    }
    match language {
        Language::Rust => rust::default_visibility(),
        Language::Go => go::default_visibility(&entity.name),
        Language::Python => python::default_visibility(&entity.name),
        Language::Dart => dart::default_visibility(&entity.name),
        Language::Java | Language::Kotlin | Language::Scala | Language::CSharp => {
            jvm::default_visibility()
        }
        Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
            javascript::default_visibility()
        }
        _ => Visibility::Public,
    }
}

/// Export info (subset of cce_relation::index::core::ExportInfo).
#[derive(Debug, Clone)]
pub struct ExportInfo {
    pub function_id: EntityId,
    pub function_name: String,
    pub export_type: ExportType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportType {
    Named,
    Default,
}

pub fn extract_exports_from_entities(entities: &[Entity], language: &Language) -> Vec<ExportInfo> {
    let mut exports = Vec::new();
    for entity in entities {
        if entity.depth > 0 || entity.parent.is_some() {
            continue;
        }
        let is_export = !matches!(
            detect_entity_visibility(entity, language),
            Visibility::Private
        );
        if is_export {
            let export_type = if entity
                .metadata
                .get("is_default")
                .is_some_and(|v| v == "true")
            {
                ExportType::Default
            } else {
                ExportType::Named
            };
            exports.push(ExportInfo {
                function_id: entity.id,
                function_name: entity.name.clone(),
                export_type,
            });
        }
    }
    exports
}

pub fn extract_imports(
    tree: &Tree,
    source: &str,
    language: &Language,
    _context: Option<()>,
) -> Result<ImportTable, ParseError> {
    let language_str = language.to_string();
    let Some(extractor) = create_extractor_with_registry(*language, None, "", &language_str) else {
        if matches!(language, Language::Custom(_)) {
            return Err(ParseError::ast_parsing(format!(
                "No extractor available for language: {}",
                language
            )));
        }
        return Ok(ImportTable::from_standardized(
            &StandardizedImportTable::new(""),
        ));
    };

    let standardized_imports = extractor.extract_imports(tree, source);
    let mut std_table = StandardizedImportTable::new("");
    for import in standardized_imports {
        std_table.add_import(import);
    }
    Ok(ImportTable::from_standardized(&std_table))
}

pub fn extract_reexports(tree: &Tree, source: &str, language: &Language) -> Vec<ReexportRecord> {
    let language_str = language.to_string();
    let Some(extractor) = create_extractor_with_registry(*language, None, "", &language_str) else {
        return Vec::new();
    };
    extractor
        .extract_exports(tree, source)
        .into_iter()
        .filter(|export| export.is_reexport && export.target.name != "*")
        .filter_map(|export| {
            let source_module = export.target.source_module.as_deref()?;
            let (original_module, original_name) = match export.target.original_name.as_deref() {
                Some(original) if original.contains("::") => {
                    let (module, name) = original.rsplit_once("::")?;
                    (module.to_string(), name.to_string())
                }
                Some(original) => (source_module.to_string(), original.to_string()),
                None => match source_module.rsplit_once("::") {
                    Some((module, name)) if name == export.target.name => {
                        (module.to_string(), name.to_string())
                    }
                    _ => (source_module.to_string(), export.target.name.clone()),
                },
            };
            if export.target.name.is_empty()
                || original_module.is_empty()
                || original_name.is_empty()
            {
                return None;
            }
            Some(ReexportRecord::new(
                export.target.name.clone(),
                original_module,
                original_name,
            ))
        })
        .collect()
}
