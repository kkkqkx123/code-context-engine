//! Read-only diagnostics collected from tree-sitter syntax trees.
//!
//! These helpers only classify or report parser outcomes. They never repair
//! the AST, change entity spans, or suppress the degraded status of a file.
//!
//! The coarse tags are intentionally broad triage labels rather than precise
//! compiler diagnostics.

use tree_sitter::Tree;

/// A syntax error region reported by tree-sitter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxDiagnostic {
    /// Start byte offset in the source passed to the parser.
    pub start_byte: usize,
    /// End byte offset (exclusive) in the source passed to the parser.
    pub end_byte: usize,
    /// Whether this is a synthetic missing token rather than an ERROR node.
    pub is_missing: bool,
}

/// Syntax-level diagnostic summary for one parse invocation.
#[derive(Debug, Clone, Default)]
pub struct ParseDiagnostics {
    /// Whether tree-sitter reported syntax errors.
    pub has_syntax_errors: bool,
    /// Collected ERROR and missing-token spans, innermost-first where useful.
    pub syntax_error_spans: Vec<SyntaxDiagnostic>,
    /// Coarse triage tag for degraded files, mainly for reporting.
    pub coarse_tag: Option<&'static str>,
}

/// Collect all ERROR and missing-token regions from `tree`.
pub fn collect_syntax_diagnostics(tree: &Tree) -> Vec<SyntaxDiagnostic> {
    let mut out = Vec::new();
    collect_recursive(tree.root_node(), &mut out);
    out
}

fn collect_recursive(node: tree_sitter::Node, out: &mut Vec<SyntaxDiagnostic>) {
    if node.kind() == "ERROR" {
        out.push(SyntaxDiagnostic {
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            is_missing: false,
        });
    }
    if node.is_missing() {
        out.push(SyntaxDiagnostic {
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            is_missing: true,
        });
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_recursive(child, out);
    }
}

/// Coarse degraded triage label for C/C++ parses.
///
/// This is a reporting aid only. It must not be used to alter entity counts.
pub fn coarse_tag_for_c_cpp(
    source: &str,
    has_guard_pair: bool,
    has_syntax_errors: bool,
) -> Option<&'static str> {
    if !has_syntax_errors {
        return None;
    }
    if has_guard_pair {
        return Some("extern-c-guard-neighboring-errors");
    }
    if source.contains("DEF_FUN_") || source.contains("SDP_UTIL_") {
        return Some("file-scope-macro");
    }
    if source.contains("list_entry(") {
        return Some("macro-type-arg");
    }
    if source.contains("#if") || source.contains("#else") || source.contains("#endif") {
        return Some("preproc-boundary");
    }
    Some("syntax-error")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::AstParser;
    use cce_types::language::Language;

    #[test]
    fn collects_errors_from_malformed_c() {
        let mut parser = AstParser::new();
        let (tree, _) = parser
            .parse_with_tree("int @x = 1;\n", &Language::C)
            .expect("parse");
        let diagnostics = collect_syntax_diagnostics(&tree);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics.iter().any(|d| !d.is_missing));
    }

    #[test]
    fn keeps_degraded_for_type_as_macro_argument() {
        let mut parser = AstParser::new();
        let (tree, _) = parser
            .parse_with_tree(
                "void f() { list_entry(p, struct X_t, link); }\n",
                &Language::C,
            )
            .expect("parse");
        let diagnostics = collect_syntax_diagnostics(&tree);
        assert!(
            !diagnostics.is_empty(),
            "type-as-macro-argument must stay a real syntax signal"
        );
        assert_eq!(
            coarse_tag_for_c_cpp(
                "void f() { list_entry(p, struct X_t, link); }\n",
                false,
                true
            ),
            Some("macro-type-arg")
        );
    }

    #[test]
    fn keeps_degraded_for_file_scope_macro_call() {
        let mut parser = AstParser::new();
        let (tree, _) = parser
            .parse_with_tree("DEF_FUN_(name, const char* f);\n", &Language::C)
            .expect("parse");
        let diagnostics = collect_syntax_diagnostics(&tree);
        assert!(
            !diagnostics.is_empty(),
            "file-scope macro call must stay a real syntax signal"
        );
        assert_eq!(
            coarse_tag_for_c_cpp("DEF_FUN_(name, const char* f);\n", false, true),
            Some("file-scope-macro")
        );
    }

    #[test]
    fn collects_no_errors_from_extern_c_guarded_header() {
        use crate::parser::cpp_interop_guard::shadow_for_parse;
        let src = "#ifndef _X_H_\n#define _X_H_\n\n#ifdef __cplusplus\nextern \"C\" {\n#endif\n\nint foo(void);\n\n#ifdef __cplusplus\n}\n#endif\n#endif\n";
        let shadow = shadow_for_parse(src, &Language::C);
        assert!(shadow.is_some(), "fixture should contain a guard pair");
        let mut parser = AstParser::new();
        let (tree, _) = parser
            .parse_with_tree(shadow.as_ref().unwrap().as_ref(), &Language::C)
            .expect("parse");
        let diagnostics = collect_syntax_diagnostics(&tree);
        assert!(
            diagnostics.is_empty(),
            "neutralized guard must not report syntax errors: {diagnostics:?}"
        );
    }
}
