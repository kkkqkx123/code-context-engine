//! Call-relation extraction for a single tree-sitter call match.
//!
//! Converts one `QueryMatch` from the call query into a `Relation` with an
//! unresolved callee name. The caller is located via `EntityIndex`; matches
//! outside any entity become file-level relations so module-level calls
//! survive into the relation graph.

use super::entity_index::EntityIndex;
use super::relation_handlers::{
    build_full_callee_name, determine_call_relation_type, find_callee_capture,
    normalize_callee_name,
};
use crate::parser::extractor::utils;
use crate::parser::stdlib::StdlibDetector;
use crate::tree_sitter_query::executor::QueryMatch;
use cce_types::language::Language;
use cce_types::{Relation, RelationTarget, RelationType, StdlibCategory};
use tree_sitter::Tree;

/// Process a call match and extract a relation.
///
/// Calls without a containing entity (module-level statements, top-level
/// script code) are emitted as file-level relations instead of being
/// dropped, so module-level calls survive into the relation graph.
pub(crate) fn process_call_match(
    mat: &QueryMatch,
    index: &EntityIndex,
    language: &Language,
    file_id: Option<i64>,
    tree: &Tree,
    source: &str,
) -> Option<Relation> {
    // Find the callee name
    let callee_capture = find_callee_capture(mat)?;

    // Determine relation type first (needed for caller lookup strategy)
    let relation_type = determine_call_relation_type(&callee_capture.name);

    // Find caller using entity index with relation-type-aware lookup
    let call_start = mat.captures.first().map(|c| c.start_byte).unwrap_or(0);
    let caller_id = index.find_caller_by_type(call_start, relation_type);

    // Create span from capture
    let span = utils::create_span_from_capture(callee_capture);
    let callee_name = build_full_callee_name(mat, language, tree, source)
        .unwrap_or_else(|| normalize_callee_name(&callee_capture.text));

    // Pre-compute argument count using AST when available.
    // This avoids re-scanning source text later in local_call_resolver.
    let argument_count = count_call_arguments(&relation_type, callee_capture, tree);

    // Detect stdlib and set its category. This is the PRIMARY detection point for
    // relations (the single source of truth for RawRelationData.stdlib_category).
    //
    // # Design: Why Detect Here?
    //
    // We detect stdlib at relation extraction time (not just during entity parsing)
    // because:
    // 1. Relations reference targets that may be external (different files/packages)
    // 2. We use is_stdlib_by_type() with RelationType for higher accuracy than
    //    simple name matching
    // 3. This ensures RawRelationData.stdlib_category is always populated
    //
    // For entities that ARE stdlib (detected in mark_stdlib), this detection
    // confirms and provides additional categorization based on relation type.
    //
    // See STDLIB_SUMMARY.md for design rationale and architecture.
    let stdlib_category = classify_stdlib_category(&callee_name, &relation_type, language);

    Some(match caller_id {
        Some(caller_id) => Relation::new(
            caller_id,
            RelationTarget::unresolved(callee_name),
            relation_type,
            span,
        )
        .with_stdlib_category(stdlib_category)
        .with_argument_count(argument_count),
        // Module-level call: no containing entity. Keep the edge as a
        // file-level relation so it survives into the graph instead
        // of being silently dropped.
        None => Relation::file_relation(
            file_id.unwrap_or(0),
            RelationTarget::unresolved(callee_name),
            relation_type,
            span,
        )
        .with_stdlib_category(stdlib_category)
        .with_argument_count(argument_count),
    })
}

/// Count call arguments from the AST for call-type relations.
///
/// Finds the `call_expression` ancestor of the callee capture and counts its
/// arguments. Returns `None` for non-call relation types or when the AST
/// node cannot be located.
fn count_call_arguments(
    relation_type: &RelationType,
    callee_capture: &crate::tree_sitter_query::executor::Capture,
    tree: &Tree,
) -> Option<usize> {
    if !relation_type.is_call() {
        return None;
    }
    // Find the call expression node using the callee capture's byte range.
    // The callee capture is typically a child of the call expression,
    // so we walk up to find the call_expression ancestor.
    let callee_node = tree
        .root_node()
        .descendant_for_byte_range(callee_capture.start_byte, callee_capture.end_byte);
    callee_node.and_then(|node| {
        // Walk up to find the call_expression ancestor
        let mut current = Some(node);
        while let Some(n) = current {
            if n.kind() == "call_expression"
                || n.kind() == "macro_invocation"
                || n.kind() == "function_call"
            {
                return cce_parser_core::count_call_arguments_from_node(n);
            }
            current = n.parent();
        }
        None
    })
}

/// Classify the stdlib category of a callee using its qualified name.
///
/// Detection uses the qualified callee name (e.g. `console.log` derives its
/// category from the `console` prefix) together with the relation type for
/// higher accuracy than plain name matching.
fn classify_stdlib_category(
    callee_name: &str,
    relation_type: &RelationType,
    language: &Language,
) -> Option<StdlibCategory> {
    if !StdlibDetector::is_stdlib_by_type(callee_name, relation_type, language) {
        return None;
    }
    // Get the specific category from the language-specific detector
    match language {
        cce_types::Language::JavaScript | cce_types::Language::TypeScript => {
            crate::parser::stdlib::javascript::JavaScriptStdlibDetector::get_category(callee_name)
        }
        cce_types::Language::Rust => {
            crate::parser::stdlib::rust::RustStdlibDetector::get_category(callee_name)
        }
        cce_types::Language::Python => {
            crate::parser::stdlib::python::PythonStdlibDetector::get_category(callee_name)
        }
        cce_types::Language::Go => {
            crate::parser::stdlib::go::GoStdlibDetector::get_category(callee_name)
        }
        cce_types::Language::Java => {
            crate::parser::stdlib::java::JavaStdlibDetector::get_category(callee_name)
        }
        cce_types::Language::CSharp => {
            crate::parser::stdlib::csharp::CSharpStdlibDetector::get_category(callee_name)
        }
        _ => Some(StdlibCategory::Other),
    }
}
