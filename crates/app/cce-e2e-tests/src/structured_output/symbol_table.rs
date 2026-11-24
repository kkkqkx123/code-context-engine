//! Symbol table rendering for structured output.
//!
//! Provides `render_symbol_table` for the aggregated markdown symbol table
//! and `collect_symbols` for JSON export.

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;

use cce_codegraph::RelationIndex;
use cce_codegraph::index::{EntityIndexOps, FileIndexOps};
use cce_types::{Entity, EntityId, EntityKind};

use super::types::{
    SymbolEntry, capitalize, escape_md, file_language, ordered_files, span_str, visibility_of,
};

/// Render a full symbol table as markdown.
///
/// Groups entities per file and per kind category, showing fields for type
/// definitions and signatures for callables. Visibility and span are derived
/// from the entity metadata so the output can be audited without re-parsing.
pub fn render_symbol_table(index: &RelationIndex, project_name: &str) -> String {
    let mut out = String::new();
    writeln!(out, "# Symbol Table for {project_name}").expect("write");
    writeln!(out).expect("write");
    writeln!(
        out,
        "_Generated from the relation index ({} entities, {} files)_",
        index.function_count(),
        index.file_count()
    )
    .expect("write");
    writeln!(out).expect("write");

    let files = ordered_files(index);
    if files.is_empty() {
        writeln!(out, "(no entities)").expect("write");
        return out;
    }

    // Build parent -> children index for field resolution
    let mut id_to_entity: BTreeMap<EntityId, Entity> = BTreeMap::new();
    for entities in files.values() {
        for (id, e) in entities {
            id_to_entity.insert(*id, e.clone());
        }
    }
    let mut parent_to_children: BTreeMap<EntityId, Vec<Entity>> = BTreeMap::new();
    for entities in files.values() {
        for (_, e) in entities {
            if let Some(pid) = e.parent {
                parent_to_children.entry(pid).or_default().push(e.clone());
            }
        }
    }

    writeln!(out, "## File Overview").expect("write");
    writeln!(out).expect("write");
    writeln!(out, "| File | Entities |").expect("write");
    writeln!(out, "|------|----------|").expect("write");
    for (file, entities) in &files {
        writeln!(out, "| {} | {} |", file, entities.len()).expect("write");
    }
    writeln!(out).expect("write");

    for (file, entities) in &files {
        writeln!(out, "## File: {file}").expect("write");
        writeln!(out).expect("write");

        let lang = file_language(index, file);

        // Group by kind domain for readable sections
        let mut by_kind: BTreeMap<String, Vec<&Entity>> = BTreeMap::new();
        for (_, e) in entities {
            by_kind.entry(e.kind.to_string()).or_default().push(e);
        }

        // Emit type definitions first (struct/class/enum/trait etc.)
        let type_kinds = [
            "struct",
            "class",
            "enum",
            "trait",
            "interface",
            "type_alias",
            "union",
        ];
        for kind in type_kinds {
            if let Some(list) = by_kind.get(kind) {
                writeln!(out, "### {}s", capitalize(kind)).expect("write");
                writeln!(out).expect("write");
                writeln!(out, "| Name | Signature | Fields | Visibility | Span |").expect("write");
                writeln!(out, "|------|-----------|--------|------------|------|").expect("write");
                for e in list {
                    let fields = parent_to_children
                        .get(&e.id)
                        .map(|children| {
                            children
                                .iter()
                                .filter(|c| {
                                    c.kind == EntityKind::Field
                                        || c.kind == EntityKind::Property
                                        || c.kind == EntityKind::Variable
                                })
                                .map(|c| {
                                    let ty = c
                                        .metadata
                                        .get("field_type")
                                        .or_else(|| c.metadata.get("type_annotation"))
                                        .or_else(|| c.metadata.get("variable_type"))
                                        .cloned()
                                        .unwrap_or_else(|| "-".to_string());
                                    if ty == "-" {
                                        c.name.clone()
                                    } else {
                                        format!("{}: {}", c.name, ty)
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();
                    let fields = if fields.is_empty() {
                        "-".to_string()
                    } else {
                        fields
                    };
                    let sig = if e.signature.is_empty() {
                        e.name.clone()
                    } else {
                        e.signature.clone()
                    };
                    writeln!(
                        out,
                        "| {} | {} | {} | {} | {} |",
                        escape_md(&e.name),
                        escape_md(&sig),
                        escape_md(&fields),
                        escape_md(&visibility_of(e, &lang)),
                        span_str(e)
                    )
                    .expect("write");
                }
                writeln!(out).expect("write");
            }
        }

        // Inherent / trait impl blocks
        for kind in ["inherent_impl", "trait_impl"] {
            if let Some(list) = by_kind.get(kind) {
                writeln!(out, "### {}s", capitalize(kind)).expect("write");
                writeln!(out).expect("write");
                writeln!(out, "| Name | Signature | Visibility | Span |").expect("write");
                writeln!(out, "|------|-----------|------------|------|").expect("write");
                for e in list {
                    let sig = if e.signature.is_empty() {
                        e.name.clone()
                    } else {
                        e.signature.clone()
                    };
                    writeln!(
                        out,
                        "| {} | {} | {} | {} |",
                        escape_md(&e.name),
                        escape_md(&sig),
                        escape_md(&visibility_of(e, &lang)),
                        span_str(e)
                    )
                    .expect("write");
                }
                writeln!(out).expect("write");
            }
        }

        // Callables: function / method / constructor / operator
        let callable_kinds = ["function", "method", "constructor", "operator"];
        let mut has_callable = false;
        for kind in callable_kinds {
            if by_kind.contains_key(kind) {
                has_callable = true;
                break;
            }
        }
        if has_callable {
            writeln!(out, "### Functions / Methods").expect("write");
            writeln!(out).expect("write");
            writeln!(
                out,
                "| Name | Kind | Parameters | Return Type | Visibility | Span |"
            )
            .expect("write");
            writeln!(
                out,
                "|------|------|------------|-------------|------------|------|"
            )
            .expect("write");
            for kind in callable_kinds {
                if let Some(list) = by_kind.get(kind) {
                    for e in list {
                        let params = if e.parameters.is_empty() {
                            "-".to_string()
                        } else {
                            e.parameters
                                .iter()
                                .map(|(n, t)| {
                                    if let Some(ty) = t {
                                        format!("{n}: {ty}")
                                    } else {
                                        n.clone()
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join(", ")
                        };
                        let ret = e.return_type.as_deref().unwrap_or("-");
                        writeln!(
                            out,
                            "| {} | {} | {} | {} | {} | {} |",
                            escape_md(&e.name),
                            kind,
                            escape_md(&params),
                            escape_md(ret),
                            escape_md(&visibility_of(e, &lang)),
                            span_str(e)
                        )
                        .expect("write");
                    }
                }
            }
            writeln!(out).expect("write");
        }

        // Variables / fields / constants that are not part of a parent struct
        let var_kinds = ["field", "variable", "constant", "property", "enum_variant"];
        let mut has_vars = false;
        for kind in var_kinds {
            if by_kind.contains_key(kind) {
                has_vars = true;
                break;
            }
        }
        if has_vars {
            writeln!(out, "### Variables / Fields / Constants").expect("write");
            writeln!(out).expect("write");
            writeln!(out, "| Name | Kind | Type | Visibility | Span | Parent |").expect("write");
            writeln!(out, "|------|------|------|------------|------|--------|").expect("write");
            for kind in var_kinds {
                if let Some(list) = by_kind.get(kind) {
                    for e in list {
                        let ty = e
                            .metadata
                            .get("field_type")
                            .or_else(|| e.metadata.get("type_annotation"))
                            .or_else(|| e.metadata.get("variable_type"))
                            .or_else(|| e.metadata.get("literal_type"))
                            .or_else(|| e.metadata.get("constructor_type"))
                            .cloned()
                            .unwrap_or_else(|| "-".to_string());
                        let parent_name = e
                            .parent
                            .and_then(|pid| id_to_entity.get(&pid).map(|p| p.name.clone()))
                            .unwrap_or_else(|| "-".to_string());
                        writeln!(
                            out,
                            "| {} | {} | {} | {} | {} | {} |",
                            escape_md(&e.name),
                            kind,
                            escape_md(&ty),
                            escape_md(&visibility_of(e, &lang)),
                            span_str(e),
                            escape_md(&parent_name)
                        )
                        .expect("write");
                    }
                }
            }
            writeln!(out).expect("write");
        }

        // Fallback for any remaining kinds
        let covered: HashSet<&str> = [
            "struct",
            "class",
            "enum",
            "trait",
            "interface",
            "type_alias",
            "union",
            "inherent_impl",
            "trait_impl",
            "function",
            "method",
            "constructor",
            "operator",
            "field",
            "variable",
            "constant",
            "property",
            "enum_variant",
        ]
        .into_iter()
        .collect();
        let mut others: Vec<(&String, &Vec<&Entity>)> = by_kind
            .iter()
            .filter(|(k, _)| !covered.contains(k.as_str()))
            .collect();
        if !others.is_empty() {
            others.sort_by(|a, b| a.0.cmp(b.0));
            writeln!(out, "### Other Entities").expect("write");
            writeln!(out).expect("write");
            writeln!(out, "| Name | Kind | Signature | Visibility | Span |").expect("write");
            writeln!(out, "|------|------|-----------|------------|------|").expect("write");
            for (kind, list) in others {
                for e in list {
                    let sig = if e.signature.is_empty() {
                        e.name.clone()
                    } else {
                        e.signature.clone()
                    };
                    writeln!(
                        out,
                        "| {} | {} | {} | {} | {} |",
                        escape_md(&e.name),
                        escape_md(kind),
                        escape_md(&sig),
                        escape_md(&visibility_of(e, &lang)),
                        span_str(e)
                    )
                    .expect("write");
                }
            }
            writeln!(out).expect("write");
        }
    }

    out
}

/// Collect symbol entries for JSON export.
pub fn collect_symbols(index: &RelationIndex) -> Vec<SymbolEntry> {
    let mut entries = Vec::new();
    let files = ordered_files(index);
    let mut id_to_parent: BTreeMap<EntityId, String> = BTreeMap::new();
    for entities in files.values() {
        for (id, e) in entities {
            if let Some(pid) = e.parent {
                let parent_name = files
                    .values()
                    .flat_map(|v| v.iter())
                    .find(|(eid, _)| *eid == pid)
                    .map(|(_, pe)| pe.name.clone())
                    .unwrap_or_else(|| pid.0.to_string());
                id_to_parent.insert(*id, parent_name);
            }
        }
    }
    for (file, entities) in files {
        let lang = file_language(index, &file);
        for (id, e) in entities {
            entries.push(SymbolEntry {
                name: e.name.clone(),
                kind: e.kind.to_string(),
                signature: e.signature.clone(),
                file_path: file.clone(),
                visibility: visibility_of(&e, &lang),
                span: span_str(&e),
                parent: id_to_parent.get(&id).cloned(),
                return_type: e.return_type.clone(),
                parameters: e.parameters.clone(),
                doc_present: e.doc_comment.is_some(),
            });
        }
    }
    entries.sort_by(|a, b| {
        a.file_path
            .cmp(&b.file_path)
            .then_with(|| a.name.cmp(&b.name))
    });
    entries
}
