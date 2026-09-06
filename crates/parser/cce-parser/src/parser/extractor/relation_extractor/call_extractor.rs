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

/// Remove duplicate call edges for the same call site.
///
/// Overlapping call patterns (e.g. Go's generic `call.callback` matching any
/// `f(x)` alongside the precise `call.direct`) emit two edges with identical
/// caller, callee and span. Keep one edge per
/// `(caller, callee, span)`, preferring the more specific relation type so the
/// call graph has no duplicate rows.
pub(crate) fn deduplicate_call_relations(relations: &mut Vec<Relation>) {
    use std::collections::{HashMap, HashSet};
    if relations.len() <= 1 {
        return;
    }
    fn priority(ty: &RelationType) -> u8 {
        match ty {
            RelationType::CallbackCall => 1,
            RelationType::DirectCall
            | RelationType::InstanceMethodCall
            | RelationType::StaticMethodCall
            | RelationType::ChainedMethodCall
            | RelationType::ConstructorCall
            | RelationType::GenericCall
            | RelationType::MacroCall
            | RelationType::GoroutineCall
            | RelationType::DeferredCall
            | RelationType::AsyncCall
            | RelationType::HigherOrderCall
            | RelationType::PointerCall => 3,
            _ => 2,
        }
    }
    let mut best: HashMap<(i64, String, usize, usize), usize> = HashMap::new();
    for (idx, rel) in relations.iter().enumerate() {
        if !rel.relation_type.is_call() {
            continue;
        }
        let key = (
            rel.caller_id,
            rel.dst_name().to_string(),
            rel.span.start_byte,
            rel.span.end_byte,
        );
        match best.get(&key) {
            Some(&prev) => {
                if priority(&rel.relation_type) > priority(&relations[prev].relation_type) {
                    best.insert(key, idx);
                }
            }
            None => {
                best.insert(key, idx);
            }
        }
    }
    if best.len()
        == relations
            .iter()
            .filter(|r| r.relation_type.is_call())
            .count()
    {
        return;
    }
    let keep: HashSet<usize> = best.into_values().collect();
    let mut kept = Vec::with_capacity(relations.len());
    for (idx, rel) in relations.drain(..).enumerate() {
        if !rel.relation_type.is_call() || keep.contains(&idx) {
            kept.push(rel);
        }
    }
    *relations = kept;
}

/// Extract calls nested inside Rust macro arguments (`println!("{}", run())`).
///
/// tree-sitter-rust leaves macro bodies as opaque `token_tree` nodes, so a
/// call in a formatting-macro argument is invisible to the call query and
/// the `main → run` edge is lost. This pass re-scans each macro's token
/// tree for `ident(` shapes and emits a `DirectCall` edge for each one that
/// names a known same-file function. Gating on the file's function names
/// keeps the textual scan precise: unknown identifiers (macros, keywords,
/// external calls) produce no edge.
pub(crate) fn extract_macro_inner_calls(
    matches: &[QueryMatch],
    index: &EntityIndex,
    language: &Language,
    file_id: Option<i64>,
    tree: &Tree,
    source: &str,
    function_names: &std::collections::HashSet<String>,
) -> Vec<Relation> {
    if *language != Language::Rust || function_names.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for mat in matches {
        let Some(callee_capture) = mat.captures.iter().find(|c| {
            c.name.contains(".macro") && (c.name.ends_with(".name") || c.name.ends_with(".macro"))
        }) else {
            continue;
        };
        let Some(token_tree) = find_macro_token_tree(tree, callee_capture.start_byte) else {
            continue;
        };
        let macro_name = normalize_callee_name(&callee_capture.text);
        let call_start = mat.captures.first().map(|c| c.start_byte).unwrap_or(0);
        let caller = index.find_caller_by_type(call_start, RelationType::DirectCall);
        let tree_text = source
            .get(token_tree.start_byte()..token_tree.end_byte())
            .unwrap_or_default();
        for (name, offset) in scan_call_idents(tree_text) {
            if name == macro_name || !function_names.contains(&name) {
                continue;
            }
            let abs = token_tree.start_byte() + offset;
            let span = cce_types::Span::from_byte_range(source, abs, abs + name.len());
            let Some(span) = span else { continue };
            let target = RelationTarget::unresolved(name.clone());
            let rel = match caller {
                Some(caller_id) => Relation::new(caller_id, target, RelationType::DirectCall, span),
                None => Relation::file_relation(
                    file_id.unwrap_or(0),
                    target,
                    RelationType::DirectCall,
                    span,
                ),
            };
            out.push(rel);
        }
    }
    out
}

/// Locate the `token_tree` of the macro invocation enclosing `byte_offset`.
fn find_macro_token_tree(tree: &Tree, byte_offset: usize) -> Option<tree_sitter::Node<'_>> {
    let mut node = tree
        .root_node()
        .descendant_for_byte_range(byte_offset, byte_offset)?;
    loop {
        if node.kind() == "macro_invocation" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "token_tree" {
                    return Some(child);
                }
            }
            return None;
        }
        node = node.parent()?;
    }
}

/// Scan text for `ident(` call shapes, skipping keywords, string literals,
/// comments and macro invocations (`ident!`).
fn scan_call_idents(text: &str) -> Vec<(String, usize)> {
    const KEYWORDS: &[&str] = &[
        "as", "async", "await", "break", "const", "continue", "crate", "do", "dyn", "else", "enum",
        "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "macro", "match", "mod",
        "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super",
        "trait", "true", "try", "type", "typeof", "unsafe", "use", "where", "while", "yield",
    ];
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = quote {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' => {
                // Rust raw strings (`r"..."`, `r#"..."#`) start with `r`;
                // the quote still terminates the literal for scan purposes.
                quote = Some(b'"');
                i += 1;
            }
            b'\'' => {
                // Lifetimes (`'a`) vs char literals (`'x'`): only treat as a
                // literal when a closing quote follows within 3 bytes.
                let is_char = bytes.get(i + 2).is_some_and(|c| *c == b'\'')
                    || bytes.get(i + 3).is_some_and(|c| *c == b'\'');
                if is_char {
                    quote = Some(b'\'');
                }
                i += 1;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            }
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let name = &text[start..i];
                let mut j = i;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                // `ident!` is a nested macro, not a call; `ident::`/`ident.`
                // are paths handled by the qualified-call patterns.
                if bytes.get(j) == Some(&b'(') && !KEYWORDS.contains(&name) {
                    out.push((name.to_string(), start));
                }
            }
            _ => {
                i += 1;
            }
        }
    }
    out
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
