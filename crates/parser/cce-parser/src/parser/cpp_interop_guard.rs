//! Neutralization of the C++ interop guard before AST parsing.
//!
//! Real-world C and C++ sources — especially public headers — are written
//! for consumption from both languages:
//!
//! ```c
//! #ifdef __cplusplus
//! extern "C" {
//! #endif
//!
//! /* declarations */
//!
//! #ifdef __cplusplus
//! }
//! #endif
//! #endif /* !_foo_h_ */
//! ```
//!
//! The pinned tree-sitter C/C++ grammars cannot parse this shape. They treat
//! `extern "C" {` as a `linkage_specification` whose body is a brace-delimited
//! declaration list, and inside such a list a stray `#endif` is consumed by
//! the generic `preproc_call` rule instead of terminating the surrounding
//! conditional. From there on the conditional nesting is off by one: a
//! synthetic `MISSING #endif` node is injected and the `#ifdef __cplusplus`
//! node swallows the remainder of the file, so the warning fires and every
//! macro-conditional span downstream is wrong.
//!
//! A C compiler never sees the guard body in a C translation unit
//! (`__cplusplus` is undefined), and a C++ compiler sees only declarations
//! between the braces. Both readings agree that the two brace lines carry no
//! declarations, so this module builds a *shadow* copy of the source in
//! which exactly those two lines are replaced by spaces of identical byte
//! length:
//!
//! - byte offsets, line numbers and columns are unchanged, so every span
//!   taken from the shadow tree indexes identically into the original
//!   source, and text extraction keeps the original bytes;
//! - all conditional directives stay in place, so the preprocessor
//!   structure and its macro dependencies remain intact.
//!
//! The rewrite is all-or-nothing: it only triggers when a complete
//! guard pair is present, so an unrecognized or unbalanced file is handed
//! to tree-sitter exactly as written.

use std::borrow::Cow;

use cce_types::language::Language;

/// Byte content a guard line is replaced by: same byte length, no tokens.
fn blank(line: &str) -> String {
    " ".repeat(line.len())
}

/// Whether `line` is a conditional that opens on `__cplusplus` being defined,
/// in either spelling used in the wild: `#ifdef __cplusplus` and
/// `#if defined(__cplusplus)`.
fn opens_on_cplusplus(line: &str) -> bool {
    let Some(rest) = trim_line(line).trim_start().strip_prefix('#') else {
        return false;
    };
    let rest = rest.trim_start();
    if let Some(after) = rest.strip_prefix("ifdef") {
        return after.trim() == "__cplusplus";
    }
    if let Some(after) = rest.strip_prefix("if") {
        return is_defined_cplusplus(after.trim());
    }
    false
}

/// Whether `expr` is a condition that reduces to `__cplusplus` being defined,
/// i.e. `defined(__cplusplus)` or `defined __cplusplus`.
fn is_defined_cplusplus(expr: &str) -> bool {
    let Some(rest) = expr.strip_prefix("defined") else {
        return false;
    };
    let rest = rest.trim_start();
    let inner = match rest.strip_prefix('(') {
        Some(inner) => match inner.strip_suffix(')') {
            Some(inner) => inner,
            None => return false,
        },
        None => rest,
    };
    inner.trim() == "__cplusplus"
}

/// Strip an optional trailing carriage return (CRLF sources).
fn trim_line(line: &str) -> &str {
    line.strip_suffix('\r').unwrap_or(line)
}

/// Whether `line` is `extern "C" {` — the guard body opener.
fn is_extern_c_brace(line: &str) -> bool {
    let Some(rest) = trim_line(line).trim().strip_prefix("extern") else {
        return false;
    };
    let Some(rest) = rest.trim_start().strip_prefix('"') else {
        return false;
    };
    let Some(rest) = rest.strip_prefix('C') else {
        return false;
    };
    let Some(rest) = rest.strip_prefix('"') else {
        return false;
    };
    rest.trim() == "{"
}

/// Whether `line` is a lone `}` — the guard body closer.
fn is_lone_brace(line: &str) -> bool {
    trim_line(line).trim() == "}"
}

/// Whether `line` is an `#endif` directive.
fn is_endif(line: &str) -> bool {
    let Some(rest) = trim_line(line).trim_start().strip_prefix('#') else {
        return false;
    };
    match rest.trim_start().strip_prefix("endif") {
        Some(rest) => rest.chars().all(char::is_whitespace),
        None => false,
    }
}

/// A guard pair located in the source, as line indices.
struct GuardPair {
    /// Line holding `extern "C" {`.
    open: usize,
    /// Line holding the matching `}`.
    close: usize,
}

/// Whether the three lines starting at `index` open a guard.
fn is_opener(lines: &[&str], index: usize) -> bool {
    opens_on_cplusplus(lines[index])
        && is_extern_c_brace(lines[index + 1])
        && is_endif(lines[index + 2])
}

/// Whether the three lines starting at `index` close a guard.
fn is_closer(lines: &[&str], index: usize) -> bool {
    opens_on_cplusplus(lines[index])
        && is_lone_brace(lines[index + 1])
        && is_endif(lines[index + 2])
}

/// Locate the first guard pair at or after line `from`.
///
/// A pair is a `<conditional on __cplusplus> / extern "C" { / #endif`
/// opener followed later by a `<conditional on __cplusplus> / } / #endif`
/// closer. Both halves must be present: blanking an opener without its
/// closer would strand a `}` at top level, which is a hard parse error
/// rather than a degraded tree.
fn find_pair(lines: &[&str], from: usize) -> Option<GuardPair> {
    let mut open = from;
    loop {
        if open + 3 > lines.len() {
            return None;
        }
        if is_opener(lines, open) {
            break;
        }
        open += 1;
    }

    let mut close = open + 3;
    loop {
        if close + 3 > lines.len() {
            return None;
        }
        if is_closer(lines, close) {
            return Some(GuardPair { open, close });
        }
        close += 1;
    }
}

/// Every guard pair in the source, in source order.
fn guard_pairs(lines: &[&str]) -> Vec<GuardPair> {
    let mut pairs = Vec::new();
    let mut from = 0;
    while let Some(pair) = find_pair(lines, from) {
        from = pair.close + 3;
        pairs.push(pair);
    }
    pairs
}

/// Build the shadow source used for AST parsing.
///
/// Returns `None` when no complete guard pair exists, in which case the
/// original source is parsed as-is. The returned string always has the same
/// byte length as `source`.
pub fn shadow_for_parse<'a>(source: &'a str, language: &Language) -> Option<Cow<'a, str>> {
    if !matches!(language, Language::C | Language::Cpp) {
        return None;
    }
    // The guard is `extern "C"`, so a source without a linkage spelling
    // cannot contain one.
    if !source.contains("extern") {
        return None;
    }

    let lines: Vec<&str> = source.split('\n').collect();
    let pairs = guard_pairs(&lines);
    if pairs.is_empty() {
        return None;
    }

    let mut shadow: Vec<String> = lines.iter().map(|line| (*line).to_string()).collect();
    for pair in &pairs {
        shadow[pair.open + 1] = blank(lines[pair.open + 1]);
        shadow[pair.close + 1] = blank(lines[pair.close + 1]);
    }

    let joined = shadow.join("\n");
    debug_assert_eq!(joined.len(), source.len());
    Some(Cow::Owned(joined))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shadow(src: &str) -> Option<String> {
        shadow_for_parse(src, &Language::C).map(|cow| cow.into_owned())
    }

    #[test]
    fn returns_none_without_guard() {
        let src = "int main(void) { return 0; }\n";
        assert!(shadow(src).is_none());
    }

    #[test]
    fn returns_none_when_opener_has_no_closer() {
        let src = "#ifdef __cplusplus\nextern \"C\" {\n#endif\n\nint a;\n\n#endif\n";
        assert!(shadow(src).is_none());
    }

    #[test]
    fn returns_none_when_closer_has_no_opener() {
        let src = "#ifdef __cplusplus\n}\n#endif\n\nint a;\n";
        assert!(shadow(src).is_none());
    }

    #[test]
    fn blanks_both_brace_lines_and_keeps_directives() {
        let src = concat!(
            "#ifndef _foo_h_\n",
            "#define _foo_h_\n",
            "\n",
            "#ifdef __cplusplus\n",
            "extern \"C\" {\n",
            "#endif\n",
            "\n",
            "int foo(void);\n",
            "\n",
            "#ifdef __cplusplus\n",
            "}\n",
            "#endif\n",
            "#endif\n",
        );
        let out = shadow(src).expect("guard present");
        assert_eq!(out.len(), src.len(), "byte length must be preserved");

        let out_lines: Vec<&str> = out.split('\n').collect();
        assert_eq!(out_lines[3], "#ifdef __cplusplus");
        assert_eq!(out_lines[4].len(), 12, "opener blanked");
        assert!(out_lines[4].bytes().all(|b| b == b' '));
        assert_eq!(out_lines[5], "#endif");
        assert_eq!(out_lines[10], " ", "closer blanked");
        assert_eq!(out_lines[6], "");
        assert_eq!(out_lines[7], "int foo(void);");
        assert!(out.contains("#endif"), "directives are untouched");
    }

    #[test]
    fn accepts_if_defined_spelling() {
        let src = concat!(
            "#if defined(__cplusplus)\n",
            "extern \"C\" {\n",
            "#endif\n",
            "int foo(void);\n",
            "#if defined(__cplusplus)\n",
            "}\n",
            "#endif\n",
        );
        let out = shadow(src).expect("guard present");
        assert_eq!(out.len(), src.len());
        assert!(!out.contains("extern \"C\""));
        assert!(!out.contains('}'), "closer blanked too");
    }

    #[test]
    fn handles_crlf_sources() {
        let src = "#ifdef __cplusplus\r\nextern \"C\" {\r\n#endif\r\nint foo(void);\r\n#ifdef __cplusplus\r\n}\r\n#endif\r\n";
        let out = shadow(src).expect("guard present");
        assert_eq!(out.len(), src.len());
        let out_lines: Vec<&str> = out.split('\n').collect();
        assert_eq!(out_lines[1].len(), 13, "opener blanked with its CR");
        assert!(out_lines[1].bytes().all(|b| b == b' '));
        assert_eq!(out_lines[5].len(), 2, "closer blanked with its CR");
        assert!(out_lines[5].bytes().all(|b| b == b' '));
    }

    #[test]
    fn handles_multiple_guard_pairs() {
        let src = concat!(
            "#ifdef __cplusplus\n",
            "extern \"C\" {\n",
            "#endif\n",
            "int a;\n",
            "#ifdef __cplusplus\n",
            "}\n",
            "#endif\n",
            "int b;\n",
            "#ifdef __cplusplus\n",
            "extern \"C\" {\n",
            "#endif\n",
            "int c;\n",
            "#ifdef __cplusplus\n",
            "}\n",
            "#endif\n",
        );
        let out = shadow(src).expect("guards present");
        assert_eq!(out.len(), src.len());
        assert!(!out.contains("extern \"C\""));
        assert!(!out.trim_end().ends_with('}'));
    }

    #[test]
    fn leaves_struct_braces_before_endif_alone() {
        let src = concat!(
            "#ifdef _WIN32\n",
            "struct foo\n",
            "{\n",
            "    int a;\n",
            "};\n",
            "#endif\n",
        );
        assert!(shadow(src).is_none());
    }

    #[test]
    fn ignores_non_c_languages() {
        let src =
            "#ifdef __cplusplus\nextern \"C\" {\n#endif\nint a;\n#ifdef __cplusplus\n}\n#endif\n";
        assert!(shadow_for_parse(src, &Language::Rust).is_none());
        assert!(shadow_for_parse(src, &Language::Unknown).is_none());
    }

    #[test]
    fn ignores_unrelated_extern_linkage() {
        let src = "extern \"C\" int foo(void);\nextern \"C\" {\nint bar;\n}\n";
        assert!(shadow(src).is_none());
    }

    #[test]
    fn accepts_defined_without_parens_and_leading_indent() {
        let src = concat!(
            "  #if defined __cplusplus\n",
            "  extern \"C\" {\n",
            "#endif\n",
            "int foo(void);\n",
            "#ifdef __cplusplus\n",
            "  }\n",
            "#endif\n",
        );
        let out = shadow(src).expect("guard present");
        assert_eq!(out.len(), src.len());
        assert!(!out.contains("extern"));
        assert!(!out.contains('}'));
    }

    #[test]
    fn leaves_unrecognized_opener_untouched() {
        // A trailing comment after the brace is not the canonical shape, so
        // the file is handed to the grammar exactly as written.
        let src = concat!(
            "#ifdef __cplusplus\n",
            "extern \"C\" { /* wraps the header */\n",
            "#endif\n",
            "int foo(void);\n",
            "#ifdef __cplusplus\n",
            "}\n",
            "#endif\n",
        );
        assert!(shadow(src).is_none());
    }
}
