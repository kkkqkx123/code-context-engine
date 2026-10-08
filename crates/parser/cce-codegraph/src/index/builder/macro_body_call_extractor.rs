//! Macro body call extraction
//!
//! Reads `BehaviorFactKind::MacroBody` facts from `ParsedFile.behavior` and
//! recovers function calls hidden inside `macro_rules!` definition bodies.
//! Macro bodies are flat token trees rather than typed AST nodes, so a light
//! textual scan for `ident(` shapes is used instead of tree-sitter queries.

use std::collections::HashSet;

use cce_types::{BehaviorFactKind, EntityId, ParsedFile, Span};

/// A function call found inside a macro definition body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroBodyCall {
    /// Macro definition entity that owns the body.
    pub caller_entity_id: EntityId,
    /// Callee name (last path segment, e.g. `func` for `mod::func()`).
    pub callee_name: String,
    /// Absolute byte span of the callee name in the source file.
    pub span: Span,
}

/// Rust keywords and contextual names that never denote a call.
const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "do", "dyn", "else", "enum",
    "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "macro", "match", "mod",
    "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait",
    "true", "try", "type", "typeof", "unsafe", "use", "where", "while", "yield",
];

/// Extract calls from one macro body text.
///
/// `base_offset` is the body's absolute start byte in the source file; the
/// returned spans carry absolute byte offsets. Row/column positions are body
/// relative; callers that own the full source should rebuild spans with
/// [`Span::from_byte_range`] (see [`extract_macro_body_calls`]).
pub fn extract_calls_from_macro_body(
    body_text: &str,
    caller_entity_id: EntityId,
    offset: usize,
) -> Vec<MacroBodyCall> {
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (name, rel_start) in scan_call_idents(body_text) {
        if !seen.insert(name.clone()) {
            continue;
        }
        let abs_start = offset.saturating_add(rel_start);
        let abs_end = abs_start.saturating_add(name.len());
        let span = span_in_body(body_text, rel_start, name.len(), abs_start, abs_end);
        out.push(MacroBodyCall {
            caller_entity_id,
            callee_name: name,
            span,
        });
    }
    out
}

/// Extract calls from every `MacroBody` fact in a parsed file.
///
/// Spans are rebuilt against `file.source` so row/column positions are file
/// accurate; facts whose bytes fall outside the source keep body-relative spans.
pub fn extract_macro_body_calls(file: &ParsedFile) -> Vec<MacroBodyCall> {
    if file.behavior.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (entity_id, behavior) in file.behavior.iter() {
        for fact in &behavior.facts {
            if fact.kind != BehaviorFactKind::MacroBody {
                continue;
            }
            for mut call in extract_calls_from_macro_body(&fact.text, entity_id, fact.start_byte) {
                let end = call.span.end_byte;
                if let Some(file_span) =
                    Span::from_byte_range(file.source_str(), call.span.start_byte, end)
                {
                    call.span = file_span;
                }
                out.push(call);
            }
        }
    }
    out
}

/// Build a span with absolute bytes and body-relative row/column positions.
fn span_in_body(
    body_text: &str,
    rel_start: usize,
    len: usize,
    abs_start: usize,
    abs_end: usize,
) -> Span {
    let prefix = &body_text[..rel_start.min(body_text.len())];
    let start_row = prefix.bytes().filter(|b| *b == b'\n').count();
    let start_column = prefix
        .rfind('\n')
        .map_or(rel_start, |index| rel_start - index - 1);
    let name_end = rel_start.saturating_add(len);
    let end_prefix = &body_text[..name_end.min(body_text.len())];
    let end_row = end_prefix.bytes().filter(|b| *b == b'\n').count();
    let end_column = end_prefix
        .rfind('\n')
        .map_or(name_end, |index| name_end - index - 1);
    Span::new(
        abs_start,
        abs_end,
        start_row,
        start_column,
        end_row,
        end_column,
    )
}

/// Scan text for `ident(` call shapes, skipping string literals, comments,
/// keywords and macro invocations (`ident!`).
fn scan_call_idents(text: &str) -> Vec<(String, usize)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = quote {
            if b == b'\\' {
                i = i.saturating_add(2).min(bytes.len());
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
                quote = Some(b'"');
                i += 1;
            }
            b'\'' => {
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
                // A `$`-prefixed fragment (`$x`, `$crate`) is a metavariable,
                // not a plain identifier call target.
                let is_metavariable = start > 0 && bytes[start - 1] == b'$';
                let mut j = i;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                // `ident::` is a path prefix; the trailing segment is matched
                // separately when it is followed by `(`.
                if bytes.get(j) == Some(&b':') && bytes.get(j + 1) == Some(&b':') && {
                    let mut k = j + 2;
                    while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                        k += 1;
                    }
                    k < bytes.len() && (bytes[k].is_ascii_alphabetic() || bytes[k] == b'_')
                } {
                    continue;
                }
                // `ident!` is a nested macro invocation, not a function call.
                if bytes.get(j) == Some(&b'!') {
                    continue;
                }
                if bytes.get(j) == Some(&b'(') && !is_metavariable && !KEYWORDS.contains(&name) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn caller() -> EntityId {
        EntityId(7)
    }

    fn names(calls: &[MacroBodyCall]) -> Vec<&str> {
        calls.iter().map(|c| c.callee_name.as_str()).collect()
    }

    #[test]
    fn extracts_simple_call() {
        let calls = extract_calls_from_macro_body("run()", caller(), 10);
        assert_eq!(names(&calls), vec!["run"]);
        assert_eq!(calls[0].span.start_byte, 10);
        assert_eq!(calls[0].span.end_byte, 13);
    }

    #[test]
    fn extracts_nested_calls() {
        let calls =
            extract_calls_from_macro_body("format!(\"result: {}\", foo(bar()))", caller(), 0);
        assert_eq!(names(&calls), vec!["foo", "bar"]);
    }

    #[test]
    fn deduplicates_repeated_calls() {
        let calls = extract_calls_from_macro_body("vec![compute(1), compute(2)]", caller(), 0);
        assert_eq!(names(&calls), vec!["compute"]);
    }

    #[test]
    fn skips_nested_macro_invocation() {
        let calls = extract_calls_from_macro_body("if ready { proceed!(); }", caller(), 0);
        assert!(calls.is_empty());
    }

    #[test]
    fn skips_string_contents() {
        let calls = extract_calls_from_macro_body("\"not a call()\"", caller(), 0);
        assert!(calls.is_empty());
    }

    #[test]
    fn extracts_method_call_receiver_name() {
        let calls = extract_calls_from_macro_body("obj.method(x)", caller(), 0);
        assert_eq!(names(&calls), vec!["method"]);
    }

    #[test]
    fn extracts_last_path_segment() {
        let calls = extract_calls_from_macro_body("mod::func()", caller(), 0);
        assert_eq!(names(&calls), vec!["func"]);
    }

    #[test]
    fn skips_keywords_and_comments() {
        let calls = extract_calls_from_macro_body("if x { return foo(); } // bar()", caller(), 0);
        assert_eq!(names(&calls), vec!["foo"]);
        let calls = extract_calls_from_macro_body("/* baz() */ foo()", caller(), 0);
        assert_eq!(names(&calls), vec!["foo"]);
    }

    #[test]
    fn skips_metavariables() {
        let calls = extract_calls_from_macro_body("$x($y)", caller(), 0);
        assert!(calls.is_empty());
    }

    #[test]
    fn empty_body_yields_no_calls() {
        assert!(extract_calls_from_macro_body("", caller(), 0).is_empty());
        assert!(extract_calls_from_macro_body("   \n  ", caller(), 0).is_empty());
    }

    #[test]
    fn file_level_extraction_rebases_spans_to_source() {
        use cce_types::{BehaviorFact, BehaviorStore};

        let source = "macro_rules! m { () => { run(); } };";
        let body_start = source.find("run();").expect("fixture contains call");
        let mut file = ParsedFile::new(cce_types::Language::Rust, "m.rs".to_string(), source);
        let mut behavior = BehaviorStore::default();
        behavior.push_fact(
            EntityId(3),
            BehaviorFact::new(
                BehaviorFactKind::MacroBody,
                "run();",
                body_start,
                body_start + 6,
            ),
        );
        file.behavior = behavior;

        let calls = extract_macro_body_calls(&file);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].callee_name, "run");
        assert_eq!(calls[0].caller_entity_id, EntityId(3));
        assert_eq!(calls[0].span.start_byte, body_start);
        assert!(calls[0].span.is_available());
    }

    #[test]
    fn macro_body_calls_enter_relation_graph() {
        use crate::index::builder::IndexBuilder;
        use crate::index::{EntityIndexOps, FileLevelOps};
        use cce_types::{BehaviorFact, BehaviorStore, Entity, EntityKind, Language, RelationType};

        fn test_entity(id: u64, kind: EntityKind, name: &str) -> Entity {
            Entity {
                id: EntityId(id),
                kind,
                name: name.to_string(),
                signature: format!("{name}()"),
                parameters: Vec::new(),
                return_type: None,
                span: Span::default(),
                depth: 0,
                parent: None,
                children: Vec::new(),
                doc_comment: None,
                modifiers: vec!["pub".to_string()],
                attributes: std::collections::HashMap::new(),
                metadata: std::collections::HashMap::new(),
                is_stdlib: false,
                subtype: None,
                stdlib_category: None,
            }
        }

        let source = "pub fn run() {}\nmacro_rules! m { () => { run(); } };";
        let body_start = source.find("run();").expect("fixture contains call");
        let mut file = ParsedFile::new(Language::Rust, "m.rs".to_string(), source);
        file.add_entity(test_entity(0, EntityKind::Macro, "m"));
        file.add_entity(test_entity(1, EntityKind::Function, "run"));
        let mut behavior = BehaviorStore::default();
        behavior.push_fact(
            EntityId(0),
            BehaviorFact::new(
                BehaviorFactKind::MacroBody,
                "run();",
                body_start,
                body_start + 6,
            ),
        );
        file.behavior = behavior;

        let builder = IndexBuilder::new();
        builder.add_parsed_files(&[&file]);
        let index = builder.build();

        let file_relations = index.get_resolved_relations_by_file("m.rs");
        let edges: Vec<_> = file_relations
            .iter()
            .flat_map(|(_, relations)| relations.iter())
            .filter(|relation| relation.callee_name == "run")
            .collect();
        assert_eq!(edges.len(), 1, "macro body call should enter the graph");
        assert_eq!(edges[0].relation_type, RelationType::DirectCall);
        let caller = index
            .get_function_by_entity_id(edges[0].caller)
            .expect("macro caller should be indexed");
        assert_eq!(caller.kind, EntityKind::Macro);
    }

    #[test]
    fn real_parse_macro_body_call_enters_graph() {
        use crate::index::FileLevelOps;
        use crate::index::builder::IndexBuilder;
        use cce_parser::parser::ParseCoordinator;
        use cce_types::{BehaviorFactKind, RelationType};

        let source = "fn run() -> i32 { 1 }\nmacro_rules! m { () => { run(); } };\n";
        let mut coordinator = ParseCoordinator::new();
        let parsed = coordinator
            .parse("m.rs", source)
            .expect("parse should succeed");
        assert!(
            parsed.behavior.iter().any(|(_, behavior)| behavior
                .facts
                .iter()
                .any(|fact| fact.kind == BehaviorFactKind::MacroBody)),
            "macro definition should carry a MacroBody fact"
        );

        let builder = IndexBuilder::new();
        builder.add_parsed_files(&[&parsed]);
        let index = builder.build();

        let file_relations = index.get_resolved_relations_by_file("m.rs");
        let edges: Vec<_> = file_relations
            .iter()
            .flat_map(|(_, relations)| relations.iter())
            .filter(|relation| relation.callee_name == "run")
            .collect();
        assert_eq!(
            edges.len(),
            1,
            "macro body call should enter the graph from a real parse"
        );
        assert_eq!(edges[0].relation_type, RelationType::DirectCall);
    }
}
