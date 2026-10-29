//! Pruning of redundant `calls:` lines in index-only embedding text.
//!
//! The template emits a `calls:` line naming an entity's call targets, but
//! the behavior sidecar appended during enrichment already states most of
//! those calls verbatim. Restating them in a dedicated line only duplicates
//! the identifiers and adds a shared common component across every callable
//! chunk, pulling unrelated vectors toward each other.
//!
//! A callee is dropped when the surrounding text already names it
//! (language-agnostic verbatim or head-plus-tail match), or when it is
//! standard library plumbing (language-dispatched). The keyword index path
//! is untouched: only the embedding text is pruned.

use cce_stdlib::StdlibDetector;
use cce_types::ast_to_nl::ConversionResult;
use cce_types::language::Language;

/// Bare method names that carry no subject of their own.
///
/// Name-only standard library detection cannot resolve receiver-dependent
/// methods (`unwrap`, `toString`, `push_back`): without the receiver type
/// they are indistinguishable from user code. This table lists the
/// high-frequency, low-signal spellings per language that are dropped even
/// when the surrounding text does not restate them. Everything resolvable
/// by name (qualified paths, builtin functions) is handled by the shared
/// standard library detector instead, so these tables stay small and only
/// cover the detector's blind spot. Languages fully covered by the detector
/// keep an empty table.
const COMMON_BARE_METHODS: &[&str] = &["assert", "panic", "todo", "unimplemented", "unreachable"];

/// Rust receiver-dependent trait and collection methods.
///
/// The detector resolves `Vec::new` style paths and macros by name, but
/// bare `Option`/`Result`/iterator plumbing needs this list.
const RUST_BARE_METHODS: &[&str] = &[
    "Ok",
    "Err",
    "Some",
    "None",
    "vec",
    "format",
    "write",
    "writeln",
    "print",
    "println",
    "eprint",
    "eprintln",
    "clone",
    "cloned",
    "to_string",
    "to_owned",
    "as_ref",
    "as_mut",
    "len",
    "is_empty",
    "is_some",
    "is_none",
    "is_ok",
    "is_err",
    "unwrap",
    "expect",
    "push",
    "pop",
    "iter",
    "iter_mut",
    "into",
    "from",
    "map",
    "and_then",
    "assert_eq",
    "assert_ne",
    "debug_assert",
];

/// Python container and attribute methods not resolvable by bare name.
const PYTHON_BARE_METHODS: &[&str] = &[
    "append", "extend", "get", "set", "items", "keys", "values", "update", "pop", "remove",
];

/// JavaScript family collection and console methods used without qualifier.
const JAVASCRIPT_BARE_METHODS: &[&str] = &[
    "log", "warn", "error", "require", "push", "pop", "map", "filter", "forEach", "toString",
];

/// Go `fmt` package methods used without qualifier.
const GO_BARE_METHODS: &[&str] = &["Print", "Printf", "Println", "Sprintf", "Errorf"];

/// Java object and collection methods used without qualifier.
const JAVA_BARE_METHODS: &[&str] = &[
    "toString", "equals", "hashCode", "print", "println", "printf", "length", "size", "get", "set",
    "add",
];

/// Kotlin scope functions and builders used without qualifier.
const KOTLIN_BARE_METHODS: &[&str] = &[
    "let", "run", "with", "apply", "also", "println", "print", "TODO", "require", "check",
];

/// Scala collection combinators used without qualifier.
const SCALA_BARE_METHODS: &[&str] = &[
    "map",
    "flatMap",
    "filter",
    "foreach",
    "toString",
    "println",
    "print",
    "getOrElse",
];

/// C++ container methods not resolvable by bare name.
const CPP_BARE_METHODS: &[&str] = &["push_back", "emplace_back", "toString"];

/// C# console and collection methods used without qualifier.
const CSHARP_BARE_METHODS: &[&str] = &[
    "Write",
    "WriteLine",
    "ReadLine",
    "ToString",
    "Equals",
    "Add",
    "Remove",
    "Contains",
];

/// Ruby kernel and collection methods used without qualifier.
const RUBY_BARE_METHODS: &[&str] = &["puts", "to_s", "each", "map", "require", "raise"];

/// PHP output and inspection helpers used without qualifier.
const PHP_BARE_METHODS: &[&str] = &["echo", "count", "isset", "empty", "print_r", "var_dump"];

/// Lua iteration and conversion helpers used without qualifier.
const LUA_BARE_METHODS: &[&str] = &[
    "print", "require", "pairs", "ipairs", "tostring", "tonumber",
];

/// Shell output and control builtins appearing as call targets.
const BASH_BARE_METHODS: &[&str] = &["echo", "cd", "exit", "return", "set", "export", "printf"];

/// Bare boilerplate method names for a language.
///
/// Languages whose standard library calls are fully resolvable by name
/// (C, Dart and document-like languages) use an empty table: the shared
/// detector already covers them.
fn bare_boilerplate_methods(language: Language) -> &'static [&'static str] {
    match language {
        Language::Rust => RUST_BARE_METHODS,
        Language::Python => PYTHON_BARE_METHODS,
        Language::JavaScript
        | Language::TypeScript
        | Language::Jsx
        | Language::Tsx
        | Language::Vue
        | Language::Svelte
        | Language::Ejs => JAVASCRIPT_BARE_METHODS,
        Language::Go => GO_BARE_METHODS,
        Language::Java => JAVA_BARE_METHODS,
        Language::Kotlin => KOTLIN_BARE_METHODS,
        Language::Scala => SCALA_BARE_METHODS,
        Language::Cpp => CPP_BARE_METHODS,
        Language::CSharp => CSHARP_BARE_METHODS,
        Language::Ruby => RUBY_BARE_METHODS,
        Language::Php => PHP_BARE_METHODS,
        Language::Lua => LUA_BARE_METHODS,
        Language::Bash => BASH_BARE_METHODS,
        _ => &[],
    }
}

/// Whether a call target restates what the surrounding text already names.
///
/// A qualified target is redundant when it occurs verbatim, or when both
/// its head qualifier and its final segment occur as identifier tokens
/// (the receiver is spelled differently but both parts are stated). A bare
/// target is redundant when it occurs as an identifier token, when the
/// shared standard library detector recognizes it for the entity's own
/// language, or when it belongs to the language's bare boilerplate set.
fn callee_is_redundant(
    callee: &str,
    full_text: &str,
    tokens: &std::collections::HashSet<&str>,
    language: Language,
) -> bool {
    let callee = callee.trim().trim_end_matches('.').trim();
    if callee.is_empty() {
        return true;
    }
    if full_text.contains(callee) {
        return true;
    }
    let qualified = callee.contains("::") || callee.contains('.');
    if qualified {
        let head = callee.split([':', '.']).next().unwrap_or("").trim();
        let tail = callee.rsplit([':', '.']).next().unwrap_or("").trim();
        if !head.is_empty() && head != tail && tokens.contains(head) && tokens.contains(tail) {
            return true;
        }
        return StdlibDetector::is_stdlib_call(callee, &language);
    }
    if tokens.contains(callee) {
        return true;
    }
    if COMMON_BARE_METHODS.contains(&callee) {
        return true;
    }
    if StdlibDetector::is_stdlib_call(callee, &language) {
        return true;
    }
    bare_boilerplate_methods(language).contains(&callee)
}

/// Drop `calls:` callees the embedding text already states.
///
/// Operates on the embedding text only, after the behavior sidecar is
/// appended, so redundancy is judged against the complete text the
/// retriever will read (identity, documentation and raw body).
pub(super) fn prune_redundant_calls_lines(conversion: &mut ConversionResult, language: Language) {
    let Some(text) = conversion.embedding_text.as_mut() else {
        return;
    };
    if !text.contains("calls:") {
        return;
    }
    let mut body_lines: Vec<&str> = Vec::new();
    for line in text.lines() {
        if !line.trim_start().starts_with("calls:") {
            body_lines.push(line);
        }
    }
    let full_text = body_lines.join("\n");
    let tokens: std::collections::HashSet<&str> = full_text
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|t| !t.is_empty())
        .collect();
    let mut kept_lines: Vec<String> = Vec::new();
    let mut changed = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("calls:") else {
            kept_lines.push(line.to_string());
            continue;
        };
        let indent_len = line.len() - trimmed.len();
        let indent: String = line.chars().take(indent_len).collect();
        let original: Vec<&str> = rest
            .trim_end_matches('.')
            .split(',')
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .collect();
        let kept: Vec<&str> = original
            .iter()
            .copied()
            .filter(|c| !callee_is_redundant(c, &full_text, &tokens, language))
            .collect();
        if kept.len() != original.len() {
            changed = true;
        }
        if kept.is_empty() {
            continue;
        }
        kept_lines.push(format!("{}calls: {}.", indent, kept.join(", ")));
    }
    if changed {
        *text = kept_lines.join("\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::{EntityId, EntityKind};

    fn prune_case(text: &str) -> String {
        prune_case_for_language(text, Language::Rust)
    }

    fn prune_case_for_language(text: &str, language: Language) -> String {
        let mut conversion = ConversionResult::embedding_only(
            EntityId(1),
            EntityKind::Function,
            "demo".to_string(),
            "demo.rs".to_string(),
            text.to_string(),
        );
        prune_redundant_calls_lines(&mut conversion, language);
        conversion
            .embedding_text
            .expect("embedding text survives pruning")
    }

    #[test]
    fn prune_drops_callees_stated_by_body() {
        let out = prune_case(
            "function demo.\n\
             calls: Candidate::new, as_ref, is_match_candidate.\n\
             Returns true.\n\
             self.is_match_candidate(&Candidate::new(path.as_ref()))",
        );
        assert!(!out.contains("calls:"), "all callees restated: {out:?}");
        assert!(out.contains("self.is_match_candidate"));
    }

    #[test]
    fn prune_drops_boilerplate_only_line() {
        let out = prune_case("function demo.\ncalls: Ok.\nReturns a match.\nOk(self.find())");
        assert!(!out.contains("calls:"), "boilerplate restated: {out:?}");
    }

    #[test]
    fn prune_drops_boilerplate_absent_from_body() {
        let out = prune_case("function demo.\ncalls: vec, push.\nDoes things.\nlet x = 1;");
        assert!(!out.contains("calls:"), "bare boilerplate dropped: {out:?}");
    }

    #[test]
    fn prune_keeps_distinctive_unstated_callee() {
        let out =
            prune_case("function demo.\ncalls: build_from_hir, Ok.\nBuilds a regex.\nOk(output)");
        assert!(
            out.contains("calls: build_from_hir."),
            "keeps novel callee: {out:?}"
        );
        assert!(!out.contains("Ok,"));
    }

    #[test]
    fn prune_dispatches_boilerplate_by_language() {
        let text = "function demo.\ncalls: vec.\nDoes things.\nlet x = 1;";
        let rust = prune_case_for_language(text, Language::Rust);
        assert!(
            !rust.contains("calls:"),
            "rust vec is boilerplate: {rust:?}"
        );
        let python = prune_case_for_language(text, Language::Python);
        assert!(
            python.contains("calls: vec."),
            "vec is meaningful outside Rust: {python:?}"
        );
    }

    #[test]
    fn prune_applies_python_boilerplate() {
        let text = "function demo.\ncalls: append.\nDoes things.\nlet x = 1;";
        let out = prune_case_for_language(text, Language::Python);
        assert!(
            !out.contains("calls:"),
            "python append is boilerplate: {out:?}"
        );
        let rust = prune_case_for_language(
            "function demo.\ncalls: append.\nDoes things.\nlet x = 1;",
            Language::Rust,
        );
        assert!(
            rust.contains("calls: append."),
            "append is meaningful in Rust: {rust:?}"
        );
    }

    #[test]
    fn prune_unknown_language_keeps_language_specific_boilerplate() {
        let text = "function demo.\ncalls: vec.\nDoes things.\nlet x = 1;";
        let out = prune_case_for_language(text, Language::Unknown);
        assert!(
            out.contains("calls: vec."),
            "unknown language stays conservative: {out:?}"
        );
        let common = prune_case_for_language(
            "function demo.\ncalls: panic.\nDoes things.\nx();",
            Language::Unknown,
        );
        assert!(
            !common.contains("calls:"),
            "common boilerplate still pruned: {common:?}"
        );
    }

    #[test]
    fn prune_drops_detector_covered_stdlib_per_language() {
        let text = "function demo.\ncalls: printf.\nDoes things.\nlet x = 1;";
        let out = prune_case_for_language(text, Language::C);
        assert!(
            !out.contains("calls:"),
            "detector-covered C stdlib pruned: {out:?}"
        );
        let rust = prune_case_for_language(text, Language::Rust);
        assert!(
            rust.contains("calls: printf."),
            "printf is meaningful in Rust: {rust:?}"
        );
    }

    #[test]
    fn prune_drops_qualified_stdlib_path_per_language() {
        let text = "function demo.\ncalls: Vec::new.\nDoes things.\nlet x = 1;";
        let out = prune_case_for_language(text, Language::Rust);
        assert!(
            !out.contains("calls:"),
            "qualified Rust stdlib pruned via detector: {out:?}"
        );
    }

    #[test]
    fn prune_covers_go_and_java_stdlib() {
        let go = prune_case_for_language(
            "function demo.\ncalls: Printf.\nDoes things.\nlet x = 1;",
            Language::Go,
        );
        assert!(!go.contains("calls:"), "go fmt helper pruned: {go:?}");
        let java = prune_case_for_language(
            "function demo.\ncalls: toString.\nDoes things.\nlet x = 1;",
            Language::Java,
        );
        assert!(!java.contains("calls:"), "java noise pruned: {java:?}");
    }
}
