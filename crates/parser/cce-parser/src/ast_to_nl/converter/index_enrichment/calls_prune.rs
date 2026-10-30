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
/// bare `Option`/`Result`/iterator plumbing needs this list. Entries already
/// covered by the detector's macro table (`vec!`, `format!`, `print!`, …)
/// are intentionally absent: the detector prunes them first, so the table
/// stays minimal and only covers the detector's blind spot.
const RUST_BARE_METHODS: &[&str] = &[
    "Ok",
    "Err",
    "Some",
    "None",
    "write",
    "writeln",
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
pub(crate) fn bare_boilerplate_methods(language: Language) -> &'static [&'static str] {
    // Remapped plugin languages inherit their host grammar's table.
    match cce_types::language::effective_language(&language) {
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

/// Whether `name` is boilerplate plumbing by name alone (no body context).
///
/// Used by group-level aggregation, which has no surrounding text to judge
/// restatement against: only language-dispatched stdlib and bare tables
/// apply. Plugin bare lists are handled later by line pruning, which sees
/// the full text.
pub(crate) fn is_boilerplate_name(name: &str, language: Language) -> bool {
    let name = name.trim().trim_end_matches('.').trim();
    if name.is_empty() {
        return true;
    }
    if COMMON_BARE_METHODS.contains(&name) {
        return true;
    }
    if StdlibDetector::is_stdlib_call(name, &language) {
        return true;
    }
    bare_boilerplate_methods(language).contains(&name)
}

/// Whether `line` (already trimmed of leading whitespace) opens a `calls:`
/// line, regardless of `calls:`/`Calls:` casing.
fn strip_calls_prefix(trimmed: &str) -> Option<&str> {
    trimmed
        .get(.."calls:".len())
        .filter(|head| head.eq_ignore_ascii_case("calls:"))
        .map(|_| &trimmed["calls:".len()..])
}

/// Whether `text` contains any `calls:` line, regardless of casing.
fn contains_calls_line(text: &str) -> bool {
    text.lines()
        .any(|line| strip_calls_prefix(line.trim_start()).is_some())
}

/// Whether `line` (already trimmed) is a curated header aggregate
/// (`calls: internal [..]; external [..].`) rather than a plain
/// `calls: foo, bar.` list. Aggregates are built from already-filtered
/// member calls, so the generic list pruner must leave them alone.
fn is_aggregate_calls_line(rest: &str) -> bool {
    rest.contains('[')
}

/// Per-run observability for `calls:` pruning.
///
/// Returned by [`prune_redundant_calls_lines`] so tests can assert exact
/// behavior and the converter can log per-file totals. No global metrics
/// registry is involved: enrichment runs per entity without access to the
/// orchestrator's registry, so aggregation stops at a `tracing::debug!`
/// per file in the group converter.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct CallsPruneStats {
    /// Entities visited (one `prune_redundant_calls_lines` call each).
    pub entities_visited: usize,
    /// `calls:` lines scanned (aggregates skipped, not counted).
    pub lines_scanned: usize,
    /// Total callees seen across scanned lines.
    pub callees_total: usize,
    /// Callees dropped (restated or boilerplate).
    pub callees_pruned: usize,
    /// `calls:` lines removed entirely.
    pub lines_removed: usize,
    /// `calls:` lines retained by the sole-signal guard.
    pub lines_guard_retained: usize,
}

impl std::ops::AddAssign for CallsPruneStats {
    fn add_assign(&mut self, other: Self) {
        self.entities_visited += other.entities_visited;
        self.lines_scanned += other.lines_scanned;
        self.callees_total += other.callees_total;
        self.callees_pruned += other.callees_pruned;
        self.lines_removed += other.lines_removed;
        self.lines_guard_retained += other.lines_guard_retained;
    }
}

/// Why a callee is dropped (or kept).
///
/// The split between [`CalleeFate::Stdlib`] and [`CalleeFate::BareTable`]
/// is deliberate: detector/COMMON hits are high-confidence stdlib plumbing
/// and prune even as the sole callee, while bare-table hits are
/// receiver-dependent guesses that must not delete an entity's only signal
/// (see the sole-signal guard in [`prune_redundant_calls_lines`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CalleeFate {
    /// Distinctive: resolved internal target or unknown name.
    Keep,
    /// Restates surrounding text (verbatim, same-line head-tail, token).
    PruneRestated,
    /// High-confidence stdlib (COMMON, detector, plugin stdlib).
    PruneStdlib,
    /// Low-confidence bare-table guess (built-in or plugin bare list).
    PruneBareTable,
}

/// Precomputed per-entity views for redundancy checks.
///
/// Built once per `calls:`-bearing entity: `body_lines` are the non-`calls:`
/// lines, `global_tokens` their identifier set, `line_tokens` the per-line
/// sets for the same-line head-tail rule. No joined `full_text` is kept:
/// verbatim matching iterates lines, avoiding one allocation per entity.
struct PruneCtx<'a> {
    body_lines: Vec<&'a str>,
    global_tokens: std::collections::HashSet<&'a str>,
    line_tokens: Vec<std::collections::HashSet<&'a str>>,
    language: Language,
    plugin_bare: Option<&'a [String]>,
    plugin_registry: Option<&'a cce_plugin::PluginRegistry>,
    plugin_file_path: Option<&'a str>,
    /// Normalized same-file resolved callee names for this entity
    /// (`RESOLVED_CALLS` metadata). Resolution trumps every name rule.
    resolved: Option<&'a std::collections::HashSet<String>>,
}

fn tokenize(line: &str) -> impl Iterator<Item = &str> {
    line.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|t| !t.is_empty())
}

/// Whether a qualified callee occurs verbatim in any body line.
fn occurs_verbatim(body_lines: &[&str], callee: &str) -> bool {
    body_lines.iter().any(|l| l.contains(callee))
}

/// Whether head and tail co-occur as tokens on one body line.
///
/// Same-line co-occurrence is required (not whole-text): `Candidate::new`
/// must not prune when `Candidate` and an unrelated `new` merely appear
/// pages apart. The receiver spelling may differ (`self.map` vs `items.map`
/// both state `map` beside a receiver), but both parts must share a line.
fn head_tail_same_line(line_tokens: &[std::collections::HashSet<&str>], head: &str, tail: &str) -> bool {
    line_tokens
        .iter()
        .any(|tokens| tokens.contains(head) && tokens.contains(tail))
}

fn plugin_stdlib_hit(
    registry: Option<&cce_plugin::PluginRegistry>,
    file_path: Option<&str>,
    language: Language,
    callee: &str,
) -> bool {
    if !matches!(language, Language::Custom(_)) {
        return false;
    }
    let Some(registry) = registry else {
        return false;
    };
    let language_str = language.to_string();
    crate::plugin::heuristics::classify_stdlib_call(
        registry,
        callee,
        None,
        file_path,
        Some(language_str.as_str()),
    )
    .is_some()
}

/// Classify one callee against the precomputed context.
///
/// Order: resolved internal (keep) → restatement → high-confidence stdlib
/// → low-confidence bare table → keep. Bare names use token membership
/// only, so `log` never matches `catalog`; qualified names may additionally
/// match verbatim (their `./::` delimiters make accidental containment
/// unlikely) or by same-line head-tail co-occurrence.
fn classify_callee(callee: &str, ctx: &PruneCtx) -> CalleeFate {
    let callee = callee.trim().trim_end_matches('.').trim();
    if callee.is_empty() {
        return CalleeFate::PruneRestated;
    }
    if ctx
        .resolved
        .is_some_and(|set| set.contains(callee))
    {
        return CalleeFate::Keep;
    }
    let qualified = callee.contains("::") || callee.contains('.');
    if qualified {
        if occurs_verbatim(&ctx.body_lines, callee) {
            return CalleeFate::PruneRestated;
        }
        let head = callee.split([':', '.']).next().unwrap_or("").trim();
        let tail = callee.rsplit([':', '.']).next().unwrap_or("").trim();
        if !head.is_empty()
            && head != tail
            && head_tail_same_line(&ctx.line_tokens, head, tail)
        {
            return CalleeFate::PruneRestated;
        }
        if StdlibDetector::is_stdlib_call(callee, &ctx.language) {
            return CalleeFate::PruneStdlib;
        }
        // Custom languages: consult plugin stdlib when built-ins miss,
        // mirroring relation extraction. Built-in languages never reach plugins.
        if plugin_stdlib_hit(
            ctx.plugin_registry,
            ctx.plugin_file_path,
            ctx.language,
            callee,
        ) {
            return CalleeFate::PruneStdlib;
        }
        return CalleeFate::Keep;
    }
    if ctx.global_tokens.contains(callee) {
        return CalleeFate::PruneRestated;
    }
    if COMMON_BARE_METHODS.contains(&callee) {
        return CalleeFate::PruneStdlib;
    }
    if StdlibDetector::is_stdlib_call(callee, &ctx.language) {
        return CalleeFate::PruneStdlib;
    }
    if bare_boilerplate_methods(ctx.language).contains(&callee) {
        return CalleeFate::PruneBareTable;
    }
    // Plugin-contributed bare names (custom languages only; built-in
    // tables stay pure). The converter resolves them per file, so this
    // layer only applies the list it is given.
    if ctx
        .plugin_bare
        .is_some_and(|list| list.iter().any(|name| name == callee))
    {
        return CalleeFate::PruneBareTable;
    }
    if plugin_stdlib_hit(
        ctx.plugin_registry,
        ctx.plugin_file_path,
        ctx.language,
        callee,
    ) {
        return CalleeFate::PruneStdlib;
    }
    CalleeFate::Keep
}

/// Drop `calls:` callees the embedding text already states.
///
/// Operates on the embedding text only, after the behavior sidecar is
/// appended, so redundancy is judged against the complete text the
/// retriever will read (identity, documentation and raw body).
/// Matching is case-insensitive (`calls:`/`Calls:`) and output is
/// normalized to lowercase `calls:`. Curated header aggregates
/// (`calls: internal [..]; external [..].`) are left untouched.
///
/// Sole-signal guard: a line emptied only by low-confidence bare-table
/// drops is retained verbatim (normalized casing). High-confidence drops
/// (restated or stdlib) still delete the line: the body already states
/// restated callees, and stdlib plumbing carries no topic signal.
pub(super) fn prune_redundant_calls_lines(
    conversion: &mut ConversionResult,
    language: Language,
    plugin_bare: Option<&[String]>,
    plugin_registry: Option<&cce_plugin::PluginRegistry>,
    resolved: Option<&std::collections::HashSet<String>>,
) -> CallsPruneStats {
    let mut stats = CallsPruneStats::default();
    stats.entities_visited = 1;
    let Some(text) = conversion.embedding_text.as_mut() else {
        return stats;
    };
    if !contains_calls_line(text) {
        return stats;
    }
    // `file_path` on the conversion drives plugin routing; fall back to
    // unfiltered matching when it is empty (unit tests).
    let file_path_opt = (!conversion.file_path.is_empty()).then_some(conversion.file_path.as_str());
    let mut body_lines: Vec<&str> = Vec::new();
    for line in text.lines() {
        if strip_calls_prefix(line.trim_start()).is_none() {
            body_lines.push(line);
        }
    }
    let global_tokens: std::collections::HashSet<&str> =
        body_lines.iter().flat_map(|l| tokenize(l)).collect();
    let line_tokens: Vec<std::collections::HashSet<&str>> =
        body_lines.iter().map(|l| tokenize(l).collect()).collect();
    let ctx = PruneCtx {
        body_lines,
        global_tokens,
        line_tokens,
        language,
        plugin_bare,
        plugin_registry,
        plugin_file_path: file_path_opt,
        resolved,
    };
    let mut kept_lines: Vec<String> = Vec::new();
    let mut changed = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = strip_calls_prefix(trimmed) else {
            kept_lines.push(line.to_string());
            continue;
        };
        if is_aggregate_calls_line(rest) {
            kept_lines.push(line.to_string());
            continue;
        }
        let indent_len = line.len() - trimmed.len();
        let indent: String = line.chars().take(indent_len).collect();
        let original: Vec<&str> = rest
            .trim_end_matches('.')
            .split(',')
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .collect();
        stats.lines_scanned += 1;
        stats.callees_total += original.len();
        let mut kept: Vec<&str> = Vec::with_capacity(original.len());
        let mut saw_restated_or_stdlib_drop = false;
        for c in original.iter().copied() {
            match classify_callee(c, &ctx) {
                CalleeFate::Keep => kept.push(c),
                CalleeFate::PruneRestated | CalleeFate::PruneStdlib => {
                    saw_restated_or_stdlib_drop = true;
                }
                CalleeFate::PruneBareTable => {}
            }
        }
        stats.callees_pruned += original.len() - kept.len();
        if kept.is_empty() && !saw_restated_or_stdlib_drop {
            // Sole-signal guard: everything dropped was a low-confidence
            // bare-table guess with no restatement and no stdlib hit.
            // Retain the line (normalized casing) instead of deleting the
            // entity's only recorded calls.
            stats.lines_guard_retained += 1;
            let normalized = format!("{}calls: {}.", indent, original.join(", "));
            if normalized != line {
                changed = true;
            }
            kept_lines.push(normalized);
            continue;
        }
        if kept.len() != original.len() {
            changed = true;
        }
        if kept.is_empty() {
            stats.lines_removed += 1;
            continue;
        }
        let normalized = format!("{}calls: {}.", indent, kept.join(", "));
        if normalized != line {
            changed = true;
        }
        kept_lines.push(normalized);
    }
    if changed {
        *text = kept_lines.join("\n");
    }
    stats
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
        prune_redundant_calls_lines(&mut conversion, language, None, None, None);
        conversion
            .embedding_text
            .expect("embedding text survives pruning")
    }

    fn prune_case_with_plugin_bare(text: &str, language: Language, bare: &[String]) -> String {
        let mut conversion = ConversionResult::embedding_only(
            EntityId(1),
            EntityKind::Function,
            "demo".to_string(),
            "demo.tplx".to_string(),
            text.to_string(),
        );
        prune_redundant_calls_lines(&mut conversion, language, Some(bare), None, None);
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
        let text = "function demo.\ncalls: append, build_index.\nDoes things.\nlet x = 1;";
        let out = prune_case_for_language(text, Language::Python);
        assert!(
            out.contains("calls: build_index."),
            "python append pruned beside distinctive: {out:?}"
        );
        assert!(!out.contains("append,"), "bare append dropped: {out:?}");
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
    fn prune_applies_plugin_bare_list_for_custom_language() {
        // A plugin-contributed bare name prunes even though built-in
        // tables know nothing about the custom language.
        let text = "function demo.\ncalls: tpl_emit, tpl_build.\nDoes things.\nlet x = 1;";
        let bare = ["tpl_emit".to_string()];
        let out = prune_case_with_plugin_bare(text, Language::Custom(7), &bare);
        assert!(
            out.contains("calls: tpl_build."),
            "plugin bare name pruned beside distinctive: {out:?}"
        );
        // Without the plugin list the same callee is distinctive.
        let kept = prune_case_for_language(text, Language::Custom(7));
        assert!(
            kept.contains("tpl_emit"),
            "no plugin answer keeps callee: {kept:?}"
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

    /// Plugin contract: a remapped language prunes like its host; a native
    /// plugin language keeps the conservative fallback (no bare table, no
    /// detector hit, so an unstated distinctive callee survives).
    #[test]
    fn prune_custom_language_inheritance_and_fallback() {
        use cce_types::language::{
            clear_plugin_languages, register_plugin_language, register_remap_plugin_language,
        };

        // Shares the process-global table with tree_sitter_init tests.
        let _guard = crate::tree_sitter_init::GLOBAL_TABLE_LOCK
            .lock()
            .expect("test lock poisoned");
        clear_plugin_languages();
        let remapped = register_remap_plugin_language(
            "sync-prune-remap",
            &["syncpr".to_string()],
            &Language::Rust.to_string(),
        );
        let native = register_plugin_language("sync-prune-native", &["syncpn".to_string()]);
        let text = "function demo.\ncalls: unwrap, build_from_hir.\nDoes things.\nlet x = 1;";
        let remapped_out = prune_case_for_language(text, Language::Custom(remapped));
        assert!(
            remapped_out.contains("calls: build_from_hir."),
            "remapped language must prune like its host: {remapped_out:?}"
        );
        assert!(
            !remapped_out.contains("unwrap,"),
            "remapped bare boilerplate dropped: {remapped_out:?}"
        );
        assert!(
            prune_case_for_language(text, Language::Custom(native)).contains("unwrap"),
            "native plugin language must keep conservative fallback"
        );
        clear_plugin_languages();
    }

    /// Sync guard: every parsable language must have a conscious bare-table
    /// decision. `C` is covered by libc-signature detection; `Dart` keeps an
    /// empty table because its detector answers bare receiver-dependent names
    /// itself (`is_common_method` covers `map`/`where`/`toString` — see
    /// `dart.rs::test_is_stdlib_call`); document languages carry no prunable
    /// bare calls.
    #[test]
    fn prune_bare_table_covers_all_parsable_languages() {
        let expects_entries: &[Language] = &[
            Language::Rust,
            Language::Python,
            Language::JavaScript,
            Language::TypeScript,
            Language::Jsx,
            Language::Tsx,
            Language::Vue,
            Language::Svelte,
            Language::Ejs,
            Language::Go,
            Language::Java,
            Language::Kotlin,
            Language::Scala,
            Language::Cpp,
            Language::CSharp,
            Language::Ruby,
            Language::Php,
            Language::Lua,
            Language::Bash,
        ];
        let expects_empty: &[Language] =
            &[Language::C, Language::Dart, Language::Html, Language::Css];
        for language in Language::ast_parsable_languages() {
            let table = bare_boilerplate_methods(*language);
            if expects_empty.contains(language) {
                assert!(
                    table.is_empty(),
                    "{language:?} must keep an empty bare table (detector covers it)"
                );
            } else {
                assert!(
                    expects_entries.contains(language),
                    "{language:?} is parsable but has no bare-table decision: \
                     add it to expects_entries or expects_empty"
                );
                assert!(
                    !table.is_empty(),
                    "{language:?} listed as covered but its bare table is empty"
                );
            }
        }
    }

    #[test]
    fn prune_covers_go_and_java_stdlib() {
        let go = prune_case_for_language(
            "function demo.\ncalls: Printf, build_index.\nDoes things.\nlet x = 1;",
            Language::Go,
        );
        assert!(
            go.contains("calls: build_index."),
            "go fmt helper pruned beside distinctive: {go:?}"
        );
        let java = prune_case_for_language(
            "function demo.\ncalls: toString, build_index.\nDoes things.\nlet x = 1;",
            Language::Java,
        );
        assert!(
            java.contains("calls: build_index."),
            "java noise pruned beside distinctive: {java:?}"
        );
    }

    #[test]
    fn prune_matches_calls_prefix_case_insensitively() {
        // Plugins and BM25 templates may emit `Calls:`; embedding output
        // normalizes to lowercase `calls:`.
        let out = prune_case_for_language(
            "function demo.\nCalls: vec, push.\nDoes things.\nlet x = 1;",
            Language::Rust,
        );
        assert!(
            !out.contains("calls:") && !out.contains("Calls:"),
            "capital prefix pruned: {out:?}"
        );
        let kept = prune_case_for_language(
            "function demo.\nCalls: build_from_hir.\nBuilds.\nlet x = 1;",
            Language::Rust,
        );
        assert!(
            kept.contains("calls: build_from_hir."),
            "distinctive capital line normalizes to lowercase: {kept:?}"
        );
    }

    #[test]
    fn prune_bare_name_does_not_match_substring() {
        // `log` must not match `catalog`; bare names use token membership.
        let out = prune_case_for_language(
            "function demo.\ncalls: log.\nDoes things.\nlet catalog = 1;",
            Language::Rust,
        );
        assert!(
            out.contains("log"),
            "substring must not prune bare callee: {out:?}"
        );
    }

    #[test]
    fn prune_drops_dot_qualified_stdlib_per_language() {
        // CALL_PATHS now preserves dotted qualifiers, so the shared
        // detector sees them whole.
        let go = prune_case_for_language(
            "function demo.\ncalls: fmt.Println.\nDoes things.\nlet x = 1;",
            Language::Go,
        );
        assert!(!go.contains("calls:"), "qualified Go stdlib pruned: {go:?}");
        let py = prune_case_for_language(
            "function demo.\ncalls: os.path.join.\nDoes things.\nlet x = 1;",
            Language::Python,
        );
        assert!(
            !py.contains("calls:"),
            "qualified Python stdlib pruned: {py:?}"
        );
        let js = prune_case_for_language(
            "function demo.\ncalls: console.log.\nDoes things.\nlet x = 1;",
            Language::JavaScript,
        );
        assert!(!js.contains("calls:"), "qualified JS stdlib pruned: {js:?}");
        // Same spelling in another language stays distinctive.
        let rust = prune_case_for_language(
            "function demo.\ncalls: console.log.\nDoes things.\nlet x = 1;",
            Language::Rust,
        );
        assert!(
            rust.contains("console.log"),
            "cross-language kept: {rust:?}"
        );
    }

    #[test]
    fn prune_skips_header_aggregate_format() {
        // Curated `internal [..]; external [..]` summaries are built from
        // already-filtered calls and must survive verbatim.
        let text = "function demo.\ncalls: internal [a, b]; external [c].\nBody.\nlet x = 1;";
        let out = prune_case(text);
        assert!(
            out.contains("internal [a, b]"),
            "aggregate format untouched: {out:?}"
        );
    }

    #[test]
    fn prune_retains_sole_bare_table_signal() {
        // A line emptied only by low-confidence bare-table guesses is
        // retained: deleting it would erase the entity's only recorded
        // calls. High-confidence stdlib/COMMON drops still delete.
        let out = prune_case_for_language(
            "function demo.\ncalls: map.\nDoes things.\nlet x = 1;",
            Language::Rust,
        );
        assert!(
            out.contains("calls: map."),
            "sole bare-table signal retained: {out:?}"
        );
        let stdlib_only = prune_case_for_language(
            "function demo.\ncalls: panic.\nDoes things.\nx();",
            Language::Rust,
        );
        assert!(
            !stdlib_only.contains("calls:"),
            "sole stdlib still pruned: {stdlib_only:?}"
        );
    }

    #[test]
    fn prune_drops_bare_beside_distinctive() {
        let out = prune_case_for_language(
            "function demo.\ncalls: map, build_from_hir.\nDoes things.\nlet x = 1;",
            Language::Rust,
        );
        assert!(
            out.contains("calls: build_from_hir."),
            "distinctive kept, bare dropped: {out:?}"
        );
        assert!(!out.contains("map,"), "bare guess dropped: {out:?}");
    }

    #[test]
    fn prune_head_tail_requires_same_line() {
        // Head and tail on one line prune; scattered across lines they do not.
        let same_line = prune_case(
            "function demo.\ncalls: Candidate::new.\nReturns true.\nlet x = make(Candidate, new);",
        );
        assert!(
            !same_line.contains("calls:"),
            "same-line head-tail pruned: {same_line:?}"
        );
        let scattered = prune_case(
            "function demo.\ncalls: Candidate::new.\nReturns Candidate.\nlet y = new;\nlet x = 1;",
        );
        assert!(
            scattered.contains("Candidate::new"),
            "scattered head-tail retained: {scattered:?}"
        );
    }

    #[test]
    fn prune_resolved_internal_survives_stdlib_name() {
        // A same-file resolved target is distinctive by definition, even
        // when its spelling collides with stdlib (`print`).
        let mut conversion = ConversionResult::embedding_only(
            EntityId(1),
            EntityKind::Function,
            "demo".to_string(),
            "demo.py".to_string(),
            "function demo.\ncalls: print.\nDoes things.\nlet x = 1;".to_string(),
        );
        let resolved: std::collections::HashSet<String> =
            ["print".to_string()].into_iter().collect();
        let stats = prune_redundant_calls_lines(
            &mut conversion,
            Language::Python,
            None,
            None,
            Some(&resolved),
        );
        let out = conversion.embedding_text.expect("text survives");
        assert!(out.contains("calls: print."), "resolved kept: {out:?}");
        assert_eq!(stats.callees_pruned, 0);
    }

    #[test]
    fn prune_reports_stats() {
        let mut conversion = ConversionResult::embedding_only(
            EntityId(1),
            EntityKind::Function,
            "demo".to_string(),
            "demo.rs".to_string(),
            "function demo.\ncalls: build_from_hir, Ok.\nBuilds.\nOk(output)".to_string(),
        );
        let stats =
            prune_redundant_calls_lines(&mut conversion, Language::Rust, None, None, None);
        assert_eq!(stats.entities_visited, 1);
        assert_eq!(stats.lines_scanned, 1);
        assert_eq!(stats.callees_total, 2);
        assert_eq!(stats.callees_pruned, 1);
        assert_eq!(stats.lines_removed, 0);
        assert_eq!(stats.lines_guard_retained, 0);
    }
}
