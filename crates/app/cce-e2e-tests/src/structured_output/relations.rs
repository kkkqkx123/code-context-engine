//! Relations rendering for structured output.
//!
//! Provides `render_relations` for the aggregated markdown relations report
//! and `collect_relations` for JSON export.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use cce_codegraph::RelationIndex;
use cce_codegraph::index::{EntityIndexOps, RelationQueryOps};
use cce_types::{EntityId, RelationType};

use super::types::{RelationEntry, escape_md, is_string_literal_ref, span_str_from_span};

/// Render call graph, type relationships and file dependencies as markdown.
pub fn render_relations(index: &RelationIndex, project_name: &str) -> String {
    let mut out = String::new();
    writeln!(out, "# Relations for {project_name}").expect("write");
    writeln!(out).expect("write");
    writeln!(
        out,
        "_Resolved relations: {}, file-level relations: {}, dependencies: {}_",
        index.resolved_relation_count(),
        index
            .file_records()
            .read()
            .keys()
            .filter_map(|k| index.file_relations(k))
            .map(|v| v.len())
            .sum::<usize>(),
        index.dependency_graph.get_all_files().len()
    )
    .expect("write");
    writeln!(out).expect("write");

    // Build entity id -> (name, file) map for quick lookup
    let mut id_to_info: BTreeMap<EntityId, (String, String, String)> = BTreeMap::new();
    for entry in index.function_index().iter() {
        let id = *entry.key();
        let e = entry.value();
        let file = index.get_file_path_by_entity(id).unwrap_or_default();
        id_to_info.insert(id, (e.name.clone(), file, e.kind.to_string()));
    }

    // ---- Direct / method calls ----
    writeln!(out, "## Call Graph").expect("write");
    writeln!(out).expect("write");
    let mut internal_calls: Vec<(String, String, String, String, String, String)> = Vec::new();
    let mut external_calls: Vec<(String, String, String, String)> = Vec::new();
    for entry in index.resolved_relation_index().iter() {
        let caller_id = *entry.key();
        let caller_info = id_to_info.get(&caller_id);
        let caller_name = caller_info
            .map(|(n, _, _)| n.clone())
            .unwrap_or_else(|| format!("<?>:{}", caller_id.0));
        let caller_file = caller_info.map(|(_, f, _)| f.clone()).unwrap_or_default();
        for rel in entry.value().iter() {
            if !rel.relation_type.is_call() {
                continue;
            }
            let callee_file = rel
                .callee_id
                .and_then(|cid| id_to_info.get(&cid).map(|(_, f, _)| f.clone()));
            let callee_name = if let Some(cid) = rel.callee_id {
                id_to_info
                    .get(&cid)
                    .map(|(n, _, _)| n.clone())
                    .unwrap_or_else(|| rel.callee_name.clone())
            } else {
                rel.callee_name.clone()
            };
            let span = span_str_from_span(&rel.span);
            if rel.is_external {
                external_calls.push((
                    caller_name.clone(),
                    callee_name,
                    rel.relation_type.to_string(),
                    span,
                ));
            } else {
                let callee_kind = rel
                    .callee_id
                    .and_then(|cid| id_to_info.get(&cid).map(|(_, _, k)| k.clone()))
                    .unwrap_or_default();
                internal_calls.push((
                    caller_name.clone(),
                    caller_file.clone(),
                    callee_name,
                    callee_file.clone().unwrap_or_default(),
                    rel.relation_type.to_string(),
                    span,
                ));
                let _ = callee_kind;
            }
        }
    }
    // Include file-level relations as well
    for file_path in index
        .file_records()
        .read()
        .keys()
        .cloned()
        .collect::<Vec<_>>()
    {
        if let Some(rels) = index.file_relations(&file_path) {
            let caller_file = file_path.clone();
            for rel in rels.iter() {
                if !rel.relation_type.is_call() {
                    continue;
                }
                let callee_file = rel
                    .callee_id
                    .and_then(|cid| id_to_info.get(&cid).map(|(_, f, _)| f.clone()));
                let callee_name = if let Some(cid) = rel.callee_id {
                    id_to_info
                        .get(&cid)
                        .map(|(n, _, _)| n.clone())
                        .unwrap_or_else(|| rel.callee_name.clone())
                } else {
                    rel.callee_name.clone()
                };
                let span = span_str_from_span(&rel.span);
                if rel.is_external {
                    external_calls.push((
                        caller_file.clone(),
                        callee_name,
                        rel.relation_type.to_string(),
                        span,
                    ));
                } else {
                    internal_calls.push((
                        caller_file.clone(),
                        caller_file.clone(),
                        callee_name,
                        callee_file.clone().unwrap_or_default(),
                        rel.relation_type.to_string(),
                        span,
                    ));
                }
            }
        }
    }
    internal_calls.sort();
    internal_calls.dedup();
    external_calls.sort();
    external_calls.dedup();

    writeln!(out, "### Internal Calls ({})", internal_calls.len()).expect("write");
    writeln!(out).expect("write");
    if internal_calls.is_empty() {
        writeln!(out, "_No internal calls_").expect("write");
        writeln!(out).expect("write");
    } else {
        writeln!(
            out,
            "| Caller | Caller File | Callee | Callee File | Type | Span |"
        )
        .expect("write");
        writeln!(
            out,
            "|--------|-------------|--------|-------------|------|------|"
        )
        .expect("write");
        for (caller, caller_file, callee, callee_file, ty, span) in &internal_calls {
            writeln!(
                out,
                "| {} | {} | {} | {} | {} | {} |",
                escape_md(caller),
                escape_md(caller_file),
                escape_md(callee),
                escape_md(callee_file),
                escape_md(ty),
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
        writeln!(out, "| Caller | Callee | Type | Span |").expect("write");
        writeln!(out, "|--------|--------|------|------|").expect("write");
        for (caller, callee, ty, span) in &external_calls {
            writeln!(
                out,
                "| {} | {} | {} | {} |",
                escape_md(caller),
                escape_md(callee),
                escape_md(ty),
                escape_md(span)
            )
            .expect("write");
        }
        writeln!(out).expect("write");
    }

    // ---- Type relationships: inheritance, implementation, contains ----
    writeln!(out, "## Type Relationships").expect("write");
    writeln!(out).expect("write");

    let mut hierarchy_rels: Vec<(String, String, String, String)> = Vec::new();
    for entry in index.resolved_relation_index().iter() {
        for rel in entry.value().iter() {
            if rel.relation_type.is_call() {
                continue;
            }
            if rel.relation_type.is_structural()
                || rel.relation_type.is_reference()
                || rel.relation_type.is_dependency()
                || matches!(
                    rel.relation_type,
                    RelationType::Inheritance
                        | RelationType::Implementation
                        | RelationType::TraitBound
                        | RelationType::ImplAssociation
                )
            {
                let caller = id_to_info
                    .get(&rel.caller)
                    .map(|(n, _, _)| n.clone())
                    .unwrap_or_else(|| rel.caller.0.to_string());
                let callee = rel
                    .callee_id
                    .and_then(|cid| id_to_info.get(&cid).map(|(n, _, _)| n.clone()))
                    .unwrap_or_else(|| rel.callee_name.clone());
                let caller_file = id_to_info
                    .get(&rel.caller)
                    .map(|(_, f, _)| f.clone())
                    .unwrap_or_default();
                hierarchy_rels.push((caller, callee, rel.relation_type.to_string(), caller_file));
            }
        }
    }
    for file_path in index
        .file_records()
        .read()
        .keys()
        .cloned()
        .collect::<Vec<_>>()
    {
        if let Some(rels) = index.file_relations(&file_path) {
            for rel in rels.iter() {
                if rel.relation_type.is_call() {
                    continue;
                }
                if rel.relation_type.is_structural()
                    || rel.relation_type.is_reference()
                    || rel.relation_type.is_dependency()
                {
                    let callee = rel
                        .callee_id
                        .and_then(|cid| id_to_info.get(&cid).map(|(n, _, _)| n.clone()))
                        .unwrap_or_else(|| rel.callee_name.clone());
                    let caller = file_path.clone();
                    hierarchy_rels.push((
                        caller.clone(),
                        callee,
                        rel.relation_type.to_string(),
                        caller,
                    ));
                }
            }
        }
    }
    hierarchy_rels.retain(|(_, callee, _, _)| !is_string_literal_ref(callee));
    hierarchy_rels.sort();
    hierarchy_rels.dedup();
    writeln!(
        out,
        "### Inheritance / Implementation ({})",
        hierarchy_rels.len()
    )
    .expect("write");
    writeln!(out).expect("write");
    if hierarchy_rels.is_empty() {
        writeln!(out, "_No hierarchy relations_").expect("write");
        writeln!(out).expect("write");
    } else {
        writeln!(out, "| Type | Target | Kind | File |").expect("write");
        writeln!(out, "|------|--------|------|------|").expect("write");
        for (ty, target, kind, file) in &hierarchy_rels {
            writeln!(
                out,
                "| {} | {} | {} | {} |",
                escape_md(ty),
                escape_md(target),
                escape_md(kind),
                escape_md(file)
            )
            .expect("write");
        }
        writeln!(out).expect("write");
    }

    // Contains (parent -> children)
    writeln!(out, "### Contains (Parent → Child)").expect("write");
    writeln!(out).expect("write");
    let mut contains: Vec<(String, String, String, String)> = Vec::new();
    for entry in index.function_index().iter() {
        let e = entry.value();
        if let Some(pid) = e.parent {
            if let Some(parent) = index.function_index().get(&pid) {
                let parent_file = index.get_file_path_by_entity(pid).unwrap_or_default();
                contains.push((
                    parent.name.clone(),
                    e.name.clone(),
                    e.kind.to_string(),
                    parent_file,
                ));
            }
        }
    }
    contains.sort();
    if contains.is_empty() {
        writeln!(out, "_No parent-child relations_").expect("write");
        writeln!(out).expect("write");
    } else {
        writeln!(out, "| Parent | Child | Kind | File |").expect("write");
        writeln!(out, "|--------|-------|------|------|").expect("write");
        for (parent, child, kind, file) in &contains {
            writeln!(
                out,
                "| {} | {} | {} | {} |",
                escape_md(parent),
                escape_md(child),
                escape_md(kind),
                escape_md(file)
            )
            .expect("write");
        }
        writeln!(out).expect("write");
    }

    // ---- File dependencies ----
    writeln!(out, "## File Dependencies").expect("write");
    writeln!(out).expect("write");
    let all_files = index.dependency_graph.get_all_files();
    if all_files.is_empty() {
        writeln!(out, "_No file dependencies_").expect("write");
        writeln!(out).expect("write");
    } else {
        writeln!(out, "| From | To |").expect("write");
        writeln!(out, "|------|----|").expect("write");
        let mut deps: Vec<(String, String)> = Vec::new();
        for f in all_files {
            for to in index.dependency_graph.get_dependencies(&f) {
                deps.push((f.clone(), to));
            }
        }
        deps.sort();
        deps.dedup();
        for (from, to) in deps {
            writeln!(out, "| {} | {} |", escape_md(&from), escape_md(&to)).expect("write");
        }
        writeln!(out).expect("write");
    }

    // Exports / Imports per file
    writeln!(out, "## Module Metadata").expect("write");
    writeln!(out).expect("write");
    writeln!(out, "### Exports").expect("write");
    writeln!(out).expect("write");
    let mut has_exports = false;
    for entry in index.file_records().read().iter() {
        if !entry.1.exports.is_empty() {
            has_exports = true;
            writeln!(
                out,
                "- **{}** ({} exports):",
                escape_md(entry.0),
                entry.1.exports.len()
            )
            .expect("write");
            for ex in entry.1.exports.iter() {
                writeln!(
                    out,
                    "  - {} ({:?})",
                    escape_md(&ex.function_name),
                    ex.export_type
                )
                .expect("write");
            }
        }
    }
    if !has_exports {
        writeln!(out, "_No exports_").expect("write");
    }
    writeln!(out).expect("write");
    writeln!(out, "### Imports").expect("write");
    writeln!(out).expect("write");
    let mut has_imports = false;
    for entry in index.file_records().read().iter() {
        if !entry.1.imports.standardized_imports.is_empty() {
            has_imports = true;
            writeln!(
                out,
                "- **{}** ({} imports):",
                escape_md(entry.0),
                entry.1.imports.standardized_imports.len()
            )
            .expect("write");
            for imp in &entry.1.imports.standardized_imports {
                writeln!(out, "  - {:?} -> {}", imp.kind, escape_md(&imp.source)).expect("write");
            }
        }
    }
    if !has_imports {
        writeln!(out, "_No imports_").expect("write");
    }
    writeln!(out).expect("write");

    out
}

/// Collect relation entries for JSON.
pub fn collect_relations(index: &RelationIndex) -> Vec<RelationEntry> {
    let mut entries = Vec::new();
    let mut id_to_info: BTreeMap<EntityId, (String, String)> = BTreeMap::new();
    for entry in index.function_index().iter() {
        let id = *entry.key();
        let e = entry.value();
        let file = index.get_file_path_by_entity(id).unwrap_or_default();
        id_to_info.insert(id, (e.name.clone(), file));
    }
    for entry in index.resolved_relation_index().iter() {
        let caller = *entry.key();
        let caller_info = id_to_info.get(&caller);
        let caller_name = caller_info
            .map(|(n, _)| n.clone())
            .unwrap_or_else(|| caller.0.to_string());
        let caller_file = caller_info.map(|(_, f)| f.clone()).unwrap_or_default();
        for rel in entry.value().iter() {
            let callee_file = rel
                .callee_id
                .and_then(|cid| id_to_info.get(&cid).map(|(_, f)| f.clone()));
            let callee_name = rel
                .callee_id
                .and_then(|cid| id_to_info.get(&cid).map(|(n, _)| n.clone()))
                .unwrap_or_else(|| rel.callee_name.clone());
            entries.push(RelationEntry {
                caller: caller_name.clone(),
                caller_file: caller_file.clone(),
                callee: callee_name,
                callee_file,
                relation_type: rel.relation_type.to_string(),
                span: span_str_from_span(&rel.span),
                is_external: rel.is_external,
            });
        }
    }
    entries.sort_by(|a, b| {
        a.caller
            .cmp(&b.caller)
            .then_with(|| a.callee.cmp(&b.callee))
    });
    entries
}
