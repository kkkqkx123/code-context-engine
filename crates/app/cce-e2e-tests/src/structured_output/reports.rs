//! Per-file and per-directory report rendering.
//!
//! Provides `render_file_report` (symbols + relations + type inference for a
//! single source file) and `render_directory_report` (subtree overview).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write as _;

use cce_codegraph::RelationIndex;
use cce_codegraph::index::{EntityIndexOps, RelationQueryOps};
use cce_codegraph::type_inference::types::ScopedTypeContext;
use cce_types::{Entity, EntityId, EntityKind, ParsedFile};

use crate::review_filter::ReviewFilterOptions;

use super::types::{
    escape_md, is_noise_module_target, is_string_literal_ref, local_value_names,
    normalize_nullable_display, span_str, span_str_from_span, visibility_of,
};

/// Render a per-file report that colocates symbols, relations, and type
/// inference for a single source file.
///
/// `filter` drops relation edges that point into excluded files; the report
/// itself is only rendered for files that passed the filter.
pub fn render_file_report(
    file_path: &str,
    entities: &[(EntityId, Entity)],
    index: &RelationIndex,
    parsed_file: Option<&ParsedFile>,
    global_id_to_entity: &BTreeMap<EntityId, Entity>,
    inferred: Option<&ScopedTypeContext>,
    filter: &ReviewFilterOptions,
) -> String {
    let mut out = String::new();
    writeln!(out, "# File: {file_path}").expect("write");
    writeln!(out).expect("write");
    writeln!(
        out,
        "_Entities: {} | Language: {}_",
        entities.len(),
        parsed_file
            .map(|pf| pf.language.to_string())
            .or_else(|| index
                .file_records()
                .read()
                .get(file_path)
                .map(|r| r.info.language.clone()))
            .unwrap_or_else(|| "unknown".to_string())
    )
    .expect("write");
    writeln!(out).expect("write");

    let lang = parsed_file
        .map(|pf| pf.language.to_string())
        .or_else(|| {
            index
                .file_records()
                .read()
                .get(file_path)
                .map(|r| r.info.language.clone())
        })
        .unwrap_or_default();

    // Build parent -> children for this file only
    let mut parent_to_children: BTreeMap<EntityId, Vec<&Entity>> = BTreeMap::new();
    for (_, e) in entities {
        if let Some(pid) = e.parent {
            parent_to_children.entry(pid).or_default().push(e);
        }
    }
    let mut by_kind: BTreeMap<String, Vec<&Entity>> = BTreeMap::new();
    for (_, e) in entities {
        by_kind.entry(e.kind.to_string()).or_default().push(e);
    }
    for list in by_kind.values_mut() {
        list.sort_by_key(|e| e.span.start_position.row);
    }

    if entities.is_empty() {
        writeln!(out, "_No entities in this file_").expect("write");
        writeln!(out).expect("write");
    } else {
        // Type definitions
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
                writeln!(out, "## {}s", super::types::capitalize(kind)).expect("write");
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

        for kind in ["inherent_impl", "trait_impl"] {
            if let Some(list) = by_kind.get(kind) {
                writeln!(out, "## {}s", super::types::capitalize(kind)).expect("write");
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

        let callable_kinds = ["function", "method", "constructor", "operator"];
        let has_callable = callable_kinds.iter().any(|k| by_kind.contains_key(*k));
        if has_callable {
            writeln!(out, "## Functions / Methods").expect("write");
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

        let var_kinds = ["field", "variable", "constant", "property", "enum_variant"];
        let has_vars = var_kinds.iter().any(|k| by_kind.contains_key(*k));
        if has_vars {
            writeln!(out, "## Variables / Fields / Constants").expect("write");
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
                            .and_then(|pid| global_id_to_entity.get(&pid).map(|p| p.name.clone()))
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
            writeln!(out, "## Other Entities").expect("write");
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

    // ---- Relations colocated with this file ----
    writeln!(out, "## Relations").expect("write");
    writeln!(out).expect("write");

    let mut id_to_info: BTreeMap<EntityId, (String, String, String)> = BTreeMap::new();
    for entry in index.function_index().iter() {
        let id = *entry.key();
        let e = entry.value();
        let file = index.get_file_path_by_entity(id).unwrap_or_default();
        id_to_info.insert(id, (e.name.clone(), file, e.kind.to_string()));
    }

    let mut internal_calls: Vec<(String, String, String, String)> = Vec::new();
    let mut external_calls: Vec<(String, String, String)> = Vec::new();
    let mut hierarchy_rels: Vec<(String, String, String)> = Vec::new();

    for entry in index.resolved_relation_index().iter() {
        let caller_id = *entry.key();
        let caller_file = index.get_file_path_by_entity(caller_id).unwrap_or_default();
        if caller_file != file_path {
            continue;
        }
        let caller_name = id_to_info
            .get(&caller_id)
            .map(|(n, _, _)| n.clone())
            .unwrap_or_else(|| format!("<?>:{}", caller_id.0));
        for rel in entry.value().iter() {
            if rel.relation_type.is_call() {
                let callee_name = if let Some(cid) = rel.callee_id {
                    id_to_info
                        .get(&cid)
                        .map(|(n, _, _)| n.clone())
                        .unwrap_or_else(|| rel.callee_name.clone())
                } else {
                    rel.callee_name.clone()
                };
                let callee_file = rel
                    .callee_id
                    .and_then(|cid| id_to_info.get(&cid).map(|(_, f, _)| f.clone()))
                    .unwrap_or_default();
                let span = span_str_from_span(&rel.span);
                if rel.is_external {
                    external_calls.push((caller_name.clone(), callee_name, span));
                } else {
                    internal_calls.push((caller_name.clone(), callee_name, callee_file, span));
                }
            } else if rel.relation_type.is_structural()
                || rel.relation_type.is_reference()
                || rel.relation_type.is_dependency()
                || matches!(
                    rel.relation_type,
                    cce_types::RelationType::Inheritance
                        | cce_types::RelationType::Implementation
                        | cce_types::RelationType::TraitBound
                        | cce_types::RelationType::ImplAssociation
                )
            {
                let callee = rel
                    .callee_id
                    .and_then(|cid| id_to_info.get(&cid).map(|(n, _, _)| n.clone()))
                    .unwrap_or_else(|| rel.callee_name.clone());
                hierarchy_rels.push((
                    id_to_info
                        .get(&rel.caller)
                        .map(|(n, _, _)| n.clone())
                        .unwrap_or_else(|| rel.caller.0.to_string()),
                    callee,
                    rel.relation_type.to_string(),
                ));
            }
        }
    }
    if let Some(rels) = index.file_relations(file_path) {
        for rel in rels.iter() {
            if rel.relation_type.is_call() {
                let callee_name = if let Some(cid) = rel.callee_id {
                    id_to_info
                        .get(&cid)
                        .map(|(n, _, _)| n.clone())
                        .unwrap_or_else(|| rel.callee_name.clone())
                } else {
                    rel.callee_name.clone()
                };
                let callee_file = rel
                    .callee_id
                    .and_then(|cid| id_to_info.get(&cid).map(|(_, f, _)| f.clone()))
                    .unwrap_or_default();
                let span = span_str_from_span(&rel.span);
                if rel.is_external {
                    external_calls.push((file_path.to_string(), callee_name, span));
                } else {
                    internal_calls.push((file_path.to_string(), callee_name, callee_file, span));
                }
            } else if rel.relation_type.is_structural()
                || rel.relation_type.is_reference()
                || rel.relation_type.is_dependency()
            {
                let callee = rel
                    .callee_id
                    .and_then(|cid| id_to_info.get(&cid).map(|(n, _, _)| n.clone()))
                    .unwrap_or_else(|| rel.callee_name.clone());
                hierarchy_rels.push((file_path.to_string(), callee, rel.relation_type.to_string()));
            }
        }
    }
    // Edges into filtered-out files are dropped from the report.
    internal_calls.retain(|(_, _, callee_file, _)| !filter.is_excluded_path(callee_file));
    // Deduplicate calls: entity callers win over file callers.
    internal_calls.sort_by(|a, b| {
        (a.1.clone(), a.3.clone(), (a.0 == file_path)).cmp(&(
            b.1.clone(),
            b.3.clone(),
            (b.0 == file_path),
        ))
    });
    {
        let mut seen: HashSet<(String, String)> = HashSet::new();
        internal_calls.retain(|(_, callee, _, span)| {
            let key = (callee.clone(), span.clone());
            if seen.contains(&key) {
                return false;
            }
            seen.insert(key);
            true
        });
        internal_calls.sort();
        internal_calls.dedup();
    }
    external_calls.sort_by(|a, b| {
        (a.1.clone(), a.2.clone(), (a.0 == file_path)).cmp(&(
            b.1.clone(),
            b.2.clone(),
            (b.0 == file_path),
        ))
    });
    {
        let mut seen: HashSet<(String, String)> = HashSet::new();
        external_calls.retain(|(_, callee, span)| {
            let key = (callee.clone(), span.clone());
            if seen.contains(&key) {
                return false;
            }
            seen.insert(key);
            true
        });
        external_calls.sort();
        external_calls.dedup();
    }
    let local_names = local_value_names(entities);
    hierarchy_rels.retain(|(_, callee, kind)| {
        !is_string_literal_ref(callee) && !is_noise_module_target(callee, kind, &local_names)
    });
    hierarchy_rels.sort();
    hierarchy_rels.dedup();

    writeln!(out, "### Internal Calls ({})", internal_calls.len()).expect("write");
    writeln!(out).expect("write");
    if internal_calls.is_empty() {
        writeln!(out, "_No internal calls_").expect("write");
        writeln!(out).expect("write");
    } else {
        writeln!(out, "| Caller | Callee | Callee File | Span |").expect("write");
        writeln!(out, "|--------|--------|-------------|------|").expect("write");
        for (caller, callee, callee_file, span) in &internal_calls {
            writeln!(
                out,
                "| {} | {} | {} | {} |",
                escape_md(caller),
                escape_md(callee),
                escape_md(callee_file),
                escape_md(span)
            )
            .expect("write");
        }
        writeln!(out).expect("write");
    }

    writeln!(out, "### External Calls ({})", external_calls.len()).expect("write");
    writeln!(out).expect("write");
    if external_calls.is_empty() {
        writeln!(out, "_No external calls_").expect("write");
        writeln!(out).expect("write");
    } else {
        writeln!(out, "| Caller | Callee | Span |").expect("write");
        writeln!(out, "|--------|--------|------|").expect("write");
        for (caller, callee, span) in &external_calls {
            writeln!(
                out,
                "| {} | {} | {} |",
                escape_md(caller),
                escape_md(callee),
                escape_md(span)
            )
            .expect("write");
        }
        writeln!(out).expect("write");
    }

    writeln!(out, "### Type Relationships ({})", hierarchy_rels.len()).expect("write");
    writeln!(out).expect("write");
    if hierarchy_rels.is_empty() {
        writeln!(out, "_No type relationships_").expect("write");
        writeln!(out).expect("write");
    } else {
        writeln!(out, "| Source | Target | Kind |").expect("write");
        writeln!(out, "|--------|--------|------|").expect("write");
        for (src, target, kind) in &hierarchy_rels {
            writeln!(
                out,
                "| {} | {} | {} |",
                escape_md(src),
                escape_md(target),
                escape_md(kind)
            )
            .expect("write");
        }
        writeln!(out).expect("write");
    }

    // Contains
    writeln!(out, "### Contains (Parent → Child)").expect("write");
    writeln!(out).expect("write");
    let mut contains: Vec<(String, String, String)> = Vec::new();
    for (_, e) in entities {
        if let Some(pid) = e.parent {
            if let Some(parent) = global_id_to_entity.get(&pid) {
                let parent_file = index.get_file_path_by_entity(pid).unwrap_or_default();
                if parent_file == file_path {
                    contains.push((parent.name.clone(), e.name.clone(), e.kind.to_string()));
                }
            }
        }
    }
    contains.sort();
    if contains.is_empty() {
        writeln!(out, "_No parent-child relations_").expect("write");
        writeln!(out).expect("write");
    } else {
        writeln!(out, "| Parent | Child | Kind |").expect("write");
        writeln!(out, "|--------|-------|------|").expect("write");
        for (parent, child, kind) in &contains {
            writeln!(
                out,
                "| {} | {} | {} |",
                escape_md(parent),
                escape_md(child),
                escape_md(kind)
            )
            .expect("write");
        }
        writeln!(out).expect("write");
    }

    // File dependencies (outgoing from this file)
    writeln!(out, "### File Dependencies").expect("write");
    writeln!(out).expect("write");
    let deps = index.dependency_graph.get_dependencies(file_path);
    if deps.is_empty() {
        writeln!(out, "_No file dependencies_").expect("write");
        writeln!(out).expect("write");
    } else {
        let mut sorted = deps.clone();
        sorted.sort();
        sorted.dedup();
        writeln!(out, "| From | To |").expect("write");
        writeln!(out, "|------|----|").expect("write");
        for to in sorted {
            writeln!(out, "| {} | {} |", escape_md(file_path), escape_md(&to)).expect("write");
        }
        writeln!(out).expect("write");
    }

    // Module metadata (exports/imports for this file)
    writeln!(out, "### Module Metadata").expect("write");
    writeln!(out).expect("write");
    if let Some(rec) = index.file_records().read().get(file_path) {
        writeln!(out, "#### Exports ({})", rec.exports.len()).expect("write");
        writeln!(out).expect("write");
        if rec.exports.is_empty() {
            writeln!(out, "_No exports_").expect("write");
            writeln!(out).expect("write");
        } else {
            for ex in rec.exports.iter() {
                writeln!(
                    out,
                    "- {} ({:?})",
                    escape_md(&ex.function_name),
                    ex.export_type
                )
                .expect("write");
            }
            writeln!(out).expect("write");
        }
        writeln!(
            out,
            "#### Imports ({})",
            rec.imports.standardized_imports.len()
        )
        .expect("write");
        writeln!(out).expect("write");
        if rec.imports.standardized_imports.is_empty() {
            writeln!(out, "_No imports_").expect("write");
            writeln!(out).expect("write");
        } else {
            for imp in &rec.imports.standardized_imports {
                writeln!(out, "- {:?} -> {}", imp.kind, escape_md(&imp.source)).expect("write");
            }
            writeln!(out).expect("write");
        }
    } else {
        writeln!(out, "_No module metadata_").expect("write");
        writeln!(out).expect("write");
    }

    // Type inference for this single file
    writeln!(out, "## Type Inference").expect("write");
    writeln!(out).expect("write");
    let merged_owned: Option<ScopedTypeContext> = inferred.cloned().or_else(|| {
        parsed_file.map(|pf| {
            let ctx = cce_codegraph::type_inference::TypeInferenceEngine::infer_types(
                pf,
                &cce_codegraph::type_inference::traits::InferenceContext::default(),
            );
            let ctx_two = cce_codegraph::type_inference::TypeInferenceEngine::infer_types_two_pass(
                pf,
                &cce_codegraph::type_inference::traits::InferenceContext::default(),
            );
            let mut merged = ctx.clone();
            merged.merge_from(&ctx_two);
            merged
        })
    });
    if let Some(merged) = merged_owned.as_ref() {
        if merged.is_empty() {
            writeln!(out, "_No inferred types_").expect("write");
            writeln!(out).expect("write");
        } else {
            // Canonical nullable display keeps `str | None` and
            // `Optional[str]` (or `String | null` and `String?`) on one
            // spelling in this report, mirroring the aggregated export.
            let report_lang = parsed_file
                .map(|pf| pf.language)
                .unwrap_or(cce_types::language::Language::Unknown);
            writeln!(out, "### Variables").expect("write");
            writeln!(out).expect("write");
            writeln!(
                out,
                "| Variable | Inferred Type | Origin | Priority | Shape | Span |"
            )
            .expect("write");
            writeln!(
                out,
                "|----------|---------------|--------|----------|-------|------|"
            )
            .expect("write");
            let mut var_count = 0usize;
            for frame in merged.frames_iter() {
                for (name, binding) in &frame.bindings {
                    let origin = binding
                        .origin
                        .map(|o| format!("{:?}", o))
                        .unwrap_or_else(|| "-".to_string());
                    let priority = binding
                        .origin
                        .map(|o| {
                            cce_codegraph::type_inference::types::origin_priority(Some(o))
                                .to_string()
                        })
                        .unwrap_or_else(|| "0".to_string());
                    let shape = binding
                        .shape
                        .as_ref()
                        .map(|s| normalize_nullable_display(report_lang, &s.to_type_string()))
                        .unwrap_or_else(|| "-".to_string());
                    writeln!(
                        out,
                        "| {} | {} | {} | {} | {} | {} |",
                        escape_md(name),
                        escape_md(&normalize_nullable_display(report_lang, &binding.type_name)),
                        escape_md(&origin),
                        priority,
                        escape_md(&shape),
                        span_str_from_span(&binding.span)
                    )
                    .expect("write");
                    var_count += 1;
                }
            }
            if var_count == 0 {
                writeln!(out, "| - | - | - | - | - | - |").expect("write");
                // Distinguish "nothing to bind" (no variable entities) from
                // inference failure, mirroring the aggregated export.
                let has_variable_entities = parsed_file.is_some_and(|pf| {
                    pf.entities
                        .iter()
                        .any(|e| matches!(e.kind, cce_types::EntityKind::Variable))
                }) || entities
                    .iter()
                    .any(|(_, e)| matches!(e.kind, cce_types::EntityKind::Variable));
                if !has_variable_entities {
                    writeln!(
                        out,
                        "_No variable entities: all values are annotated parameters or returns._"
                    )
                    .expect("write");
                }
            }
            writeln!(out).expect("write");

            writeln!(out, "### Function Returns").expect("write");
            writeln!(out).expect("write");
            writeln!(
                out,
                "| Function (EntityId) | Return Type | Origin | Shape |"
            )
            .expect("write");
            writeln!(out, "|-----|-------------|--------|-------|").expect("write");
            let mut ret_count = 0usize;
            for (eid, binding) in merged.return_types_iter() {
                let shape = binding
                    .shape
                    .as_ref()
                    .map(|s| normalize_nullable_display(report_lang, &s.to_type_string()))
                    .unwrap_or_else(|| "-".to_string());
                let origin = binding
                    .origin
                    .map(|o| format!("{:?}", o))
                    .unwrap_or_else(|| "-".to_string());
                // Return bindings are keyed by parsed-file-local IDs; resolve
                // against the parsed file first so index-space entities with
                // colliding numbers cannot misattribute names.
                let func_name = super::types::resolve_inferred_return_name(
                    eid,
                    parsed_file,
                    entities,
                    global_id_to_entity,
                );
                writeln!(
                    out,
                    "| {} ({}) | {} | {} | {} |",
                    escape_md(&func_name),
                    eid.0,
                    escape_md(&normalize_nullable_display(report_lang, &binding.type_name)),
                    escape_md(&origin),
                    escape_md(&shape)
                )
                .expect("write");
                ret_count += 1;
            }
            if ret_count == 0 {
                writeln!(out, "| - | - | - | - |").expect("write");
            }
            writeln!(out).expect("write");

            writeln!(out, "### Control-Flow Narrowing").expect("write");
            writeln!(out).expect("write");
            writeln!(out, "| Variable | Narrowed Type | Origin | Span |").expect("write");
            writeln!(out, "|----------|---------------|--------|------|").expect("write");
            let mut narrow_count = 0usize;
            for frame in merged.frames_iter() {
                for (name, list) in &frame.narrowed {
                    for binding in list {
                        let origin = binding
                            .origin
                            .map(|o| format!("{:?}", o))
                            .unwrap_or_else(|| "-".to_string());
                        writeln!(
                            out,
                            "| {} | {} | {} | {} |",
                            escape_md(name),
                            escape_md(&normalize_nullable_display(report_lang, &binding.type_name)),
                            escape_md(&origin),
                            span_str_from_span(&binding.span)
                        )
                        .expect("write");
                        narrow_count += 1;
                    }
                }
            }
            if narrow_count == 0 {
                writeln!(out, "| - | - | - | - |").expect("write");
                // Distinguish "nothing to narrow" (no control-flow facts)
                // from conservative no-guess narrowing.
                let has_facts = parsed_file.is_some_and(|pf| !pf.control_flow.is_empty());
                if !has_facts {
                    writeln!(out, "_No control-flow facts._").expect("write");
                } else {
                    writeln!(
                        out,
                        "_No narrowable conditions: guards carry no supported type tests._"
                    )
                    .expect("write");
                }
            }
            writeln!(out).expect("write");

            writeln!(out, "### Type Shapes (distinct)").expect("write");
            writeln!(out).expect("write");
            let mut shapes: Vec<String> = merged
                .frames_iter()
                .flat_map(|f| {
                    f.bindings.values().filter_map(|b| {
                        b.shape
                            .as_ref()
                            .map(|s| normalize_nullable_display(report_lang, &s.to_type_string()))
                    })
                })
                .collect();
            shapes.extend(merged.return_types_iter().filter_map(|(_, b)| {
                b.shape
                    .as_ref()
                    .map(|s| normalize_nullable_display(report_lang, &s.to_type_string()))
            }));
            shapes.sort();
            shapes.dedup();
            if shapes.is_empty() {
                writeln!(out, "_No complex shapes_").expect("write");
                writeln!(out).expect("write");
            } else {
                writeln!(out, "| Shape |").expect("write");
                writeln!(out, "|-------|").expect("write");
                for s in shapes {
                    writeln!(out, "| {} |", escape_md(&s)).expect("write");
                }
                writeln!(out).expect("write");
            }
        }
    } else {
        writeln!(out, "_No parsed file available for type inference_").expect("write");
        writeln!(out).expect("write");
    }

    out
}

/// Render a per-directory overview file.
///
/// The file lives sibling to the directory (e.g. `crates/cli/src.dir.txt`)
/// so it does not clash with a normal source file.
pub fn render_directory_report(
    dir_path: &str,
    ordered: &BTreeMap<String, Vec<(EntityId, Entity)>>,
    index: &RelationIndex,
) -> String {
    let mut out = String::new();
    writeln!(out, "# Directory: {dir_path}").expect("write");
    writeln!(out).expect("write");

    let prefix = format!("{dir_path}/");
    let mut subtree_files: Vec<String> = Vec::new();
    for fp in ordered.keys() {
        if fp == dir_path || fp.starts_with(&prefix) {
            subtree_files.push(fp.clone());
        }
    }
    for fp in index.file_records().read().keys() {
        if (fp == dir_path || fp.starts_with(&prefix)) && !subtree_files.contains(fp) {
            subtree_files.push(fp.clone());
        }
    }
    subtree_files.sort();
    let subtree_entity_count: usize = subtree_files
        .iter()
        .filter_map(|f| ordered.get(f))
        .map(|v| v.len())
        .sum();

    writeln!(
        out,
        "_Files in subtree: {} | Entities: {}_",
        subtree_files.len(),
        subtree_entity_count
    )
    .expect("write");
    writeln!(out).expect("write");

    // Direct children files and subdirectories
    let mut direct_files: Vec<(String, usize)> = Vec::new();
    let mut direct_subdirs: BTreeSet<String> = BTreeSet::new();
    for fp in &subtree_files {
        let rel = if fp == dir_path {
            ""
        } else if let Some(stripped) = fp.strip_prefix(&prefix) {
            stripped
        } else {
            continue;
        };
        if rel.is_empty() {
            continue;
        }
        if let Some((first, rest)) = rel.split_once('/') {
            direct_subdirs.insert(first.to_string());
            let _ = rest;
        } else {
            let cnt = ordered.get(fp).map(|v| v.len()).unwrap_or(0);
            direct_files.push((fp.clone(), cnt));
        }
    }

    writeln!(out, "## Direct Files ({})", direct_files.len()).expect("write");
    writeln!(out).expect("write");
    if direct_files.is_empty() {
        writeln!(out, "_No direct files_").expect("write");
        writeln!(out).expect("write");
    } else {
        direct_files.sort();
        writeln!(out, "| File | Entities |").expect("write");
        writeln!(out, "|------|----------|").expect("write");
        for (f, cnt) in &direct_files {
            writeln!(out, "| {} | {} |", escape_md(f), cnt).expect("write");
        }
        writeln!(out).expect("write");
    }

    writeln!(out, "## Direct Subdirectories ({})", direct_subdirs.len()).expect("write");
    writeln!(out).expect("write");
    if direct_subdirs.is_empty() {
        writeln!(out, "_No subdirectories_").expect("write");
        writeln!(out).expect("write");
    } else {
        writeln!(
            out,
            "| Directory | Files (recursive) | Entities (recursive) |"
        )
        .expect("write");
        writeln!(
            out,
            "|-----------|-------------------|----------------------|"
        )
        .expect("write");
        let mut subdir_sorted: Vec<String> = direct_subdirs.into_iter().collect();
        subdir_sorted.sort();
        for sub in subdir_sorted {
            let sub_prefix = format!("{dir_path}/{sub}/");
            let mut cnt_files = 0usize;
            let mut cnt_entities = 0usize;
            for fp in &subtree_files {
                if fp.starts_with(&sub_prefix) || fp == &format!("{dir_path}/{sub}") {
                    cnt_files += 1;
                    if let Some(v) = ordered.get(fp) {
                        cnt_entities += v.len();
                    }
                }
            }
            writeln!(
                out,
                "| {} | {} | {} |",
                escape_md(&sub),
                cnt_files,
                cnt_entities
            )
            .expect("write");
        }
        writeln!(out).expect("write");
    }

    writeln!(out, "## All Files in Subtree").expect("write");
    writeln!(out).expect("write");
    if subtree_files.is_empty() {
        writeln!(out, "_No files_").expect("write");
        writeln!(out).expect("write");
    } else {
        writeln!(out, "| File | Entities |").expect("write");
        writeln!(out, "|------|----------|").expect("write");
        for f in &subtree_files {
            let cnt = ordered.get(f).map(|v| v.len()).unwrap_or(0);
            writeln!(out, "| {} | {} |", escape_md(f), cnt).expect("write");
        }
        writeln!(out).expect("write");
    }

    // Entities by kind aggregated for subtree
    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
    for fp in &subtree_files {
        if let Some(ents) = ordered.get(fp) {
            for (_, e) in ents {
                *by_kind.entry(e.kind.to_string()).or_default() += 1;
            }
        }
    }
    writeln!(out, "## Entities by Kind (subtree)").expect("write");
    writeln!(out).expect("write");
    if by_kind.is_empty() {
        writeln!(out, "_No entities_").expect("write");
        writeln!(out).expect("write");
    } else {
        writeln!(out, "| Kind | Count |").expect("write");
        writeln!(out, "|------|-------|").expect("write");
        let mut sorted: Vec<(&String, &usize)> = by_kind.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (k, c) in sorted {
            writeln!(out, "| {} | {} |", k, c).expect("write");
        }
        writeln!(out).expect("write");
    }

    // File dependencies within subtree
    writeln!(out, "## File Dependencies (outgoing from subtree)").expect("write");
    writeln!(out).expect("write");
    let mut deps: Vec<(String, String)> = Vec::new();
    for fp in &subtree_files {
        for to in index.dependency_graph.get_dependencies(fp) {
            deps.push((fp.clone(), to));
        }
    }
    deps.sort();
    deps.dedup();
    if deps.is_empty() {
        writeln!(out, "_No file dependencies_").expect("write");
        writeln!(out).expect("write");
    } else {
        writeln!(out, "| From | To |").expect("write");
        writeln!(out, "|------|----|").expect("write");
        for (from, to) in deps {
            writeln!(out, "| {} | {} |", escape_md(&from), escape_md(&to)).expect("write");
        }
        writeln!(out).expect("write");
    }

    out
}
