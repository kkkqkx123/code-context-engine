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
/// covered by the detector (`vec!`, `format!`, `print!`, …, plus the bare
/// `write`/`writeln` spellings the macro table answers) are intentionally
/// absent: the detector prunes them first, so the table stays minimal and
/// only covers the detector's blind spot.
const RUST_BARE_METHODS: &[&str] = &[
    "Ok",
    "Err",
    "Some",
    "None",
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
///
/// Bare `set` (the builtin type) is answered by the detector and stays out.
const PYTHON_BARE_METHODS: &[&str] = &[
    "append", "extend", "get", "items", "keys", "values", "update", "pop", "remove",
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
///
/// Only `print`/`println` remain: the detector answers the scope functions
/// (`let`/`run`/`with`/`apply`/`also`) and `TODO`/`require`/`check`, which
/// therefore stay out (see the overlap audit lock below).
const KOTLIN_BARE_METHODS: &[&str] = &["println", "print"];

/// Scala collection combinators used without qualifier.
///
/// Empty: the detector answers every former entry (`map`/`flatMap`/
/// `filter`/`foreach`/`getOrElse`, `print`/`println`, `toString`), so no
/// blind spot remains. Kept as an explicit empty table so the sync guard
/// records the conscious decision.
const SCALA_BARE_METHODS: &[&str] = &[];

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
///
/// Empty: the detector answers every former entry (`puts`, `to_s`, `each`,
/// `map`, `require`, `raise`), so no blind spot remains. Kept as an
/// explicit empty table so the sync guard records the conscious decision.
const RUBY_BARE_METHODS: &[&str] = &[];

/// PHP output and inspection helpers used without qualifier.
///
/// Empty: the detector answers every former entry (`echo`, `count`,
/// `isset`, `empty`, `print_r`, `var_dump`), so no blind spot remains.
/// Kept as an explicit empty table so the sync guard records the
/// conscious decision.
const PHP_BARE_METHODS: &[&str] = &[];

/// Lua iteration and conversion helpers used without qualifier.
///
/// Empty: the detector answers every former entry (`print`, `require`,
/// `pairs`/`ipairs`, `tostring`/`tonumber`), so no blind spot remains.
/// Kept as an explicit empty table so the sync guard records the
/// conscious decision.
const LUA_BARE_METHODS: &[&str] = &[];

/// Shell output and control builtins appearing as call targets.
///
/// Empty: the detector answers every former entry (`echo`, `printf`, `cd`,
/// `exit`, `return`, `set`, `export`), so no blind spot remains. Kept as
/// an explicit empty table so the sync guard records the conscious decision.
const BASH_BARE_METHODS: &[&str] = &[];

/// Bare boilerplate method names for a language.
///
/// Languages whose standard library calls are fully resolvable by name
/// (C, Dart, Scala, Ruby, Php, Lua, Bash and document-like languages)
/// use an empty table: the shared detector already covers them.
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

/// Normalize one callee spelling for comparison.
///
/// Trims surrounding whitespace and trailing statement punctuation so
/// metadata (`CALL_PATHS`, `RESOLVED_CALLS`) and template `calls:` lines
/// compare equal despite trailing `.` or macro `!` differences.
pub(crate) fn normalize_callee(name: &str) -> &str {
    name.trim().trim_end_matches(['.', '!']).trim()
}

/// Split a comma-joined call list without breaking generic arguments.
///
/// `HashMap<String, i32>::new` keeps its inner comma: only top-level
/// commas (outside `<...>` nesting) separate entries. Each entry is
/// normalized and empty entries are dropped.
pub(crate) fn split_call_list(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut depth: usize = 0;
    for ch in raw.chars() {
        match ch {
            '<' => {
                depth = depth.saturating_add(1);
                current.push(ch);
            }
            '>' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                let normalized = normalize_callee(&current);
                if !normalized.is_empty() {
                    out.push(normalized.to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let normalized = normalize_callee(&current);
    if !normalized.is_empty() {
        out.push(normalized.to_string());
    }
    out
}

/// Parse a stored call list into a set for membership checks.
pub(crate) fn parse_call_set(raw: &str) -> std::collections::HashSet<String> {
    split_call_list(raw).into_iter().collect()
}

/// Whether `name` is boilerplate plumbing by name alone (no body context).
///
/// Used by group-level aggregation, which has no surrounding text to judge
/// restatement against: only language-dispatched stdlib and bare tables
/// apply. Plugin bare lists are handled later by line pruning, which sees
/// the full text.
pub(crate) fn is_boilerplate_name(name: &str, language: Language) -> bool {
    let name = normalize_callee(name);
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

/// Plugin-aware variant of [`is_boilerplate_name`] for group aggregation.
///
/// Aggregation builds curated header summaries that later pruning skips,
/// so plugin-contributed bare names must be filtered here as well;
/// otherwise they would leak into the aggregate permanently. Built-in
/// languages never consult plugins, keeping their tables pure.
pub(crate) fn is_boilerplate_name_with_plugin(
    name: &str,
    language: Language,
    plugin_bare: Option<&[String]>,
    plugin_registry: Option<&cce_plugin::PluginRegistry>,
    file_path: Option<&str>,
) -> bool {
    if is_boilerplate_name(name, language) {
        return true;
    }
    let normalized = normalize_callee(name);
    if normalized.is_empty() {
        return true;
    }
    if plugin_bare.is_some_and(|list| {
        list.iter()
            .any(|entry| normalize_callee(entry) == normalized)
    }) {
        return true;
    }
    plugin_stdlib_hit(plugin_registry, file_path, language, normalized)
}

/// Whether a callee belongs to the named group by exact qualifier segment.
///
/// Substring matching misclassifies longer names sharing a prefix
/// (`::Group` matches `::GroupExtra::foo`); exact segment comparison
/// covers both `::` and `.` qualification without false positives.
pub(crate) fn is_local_callee(callee: &str, group_name: &str) -> bool {
    if group_name.is_empty() {
        return false;
    }
    callee
        .split([':', '.'])
        .filter(|segment| !segment.is_empty())
        .any(|segment| segment == group_name)
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
pub struct CallsPruneStats {
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
    /// Restates surrounding text (verbatim, split qualifier, token).
    PruneRestated,
    /// High-confidence stdlib (COMMON, detector, plugin stdlib).
    PruneStdlib,
    /// Low-confidence bare-table guess (built-in or plugin bare list).
    PruneBareTable,
}

/// Precomputed per-entity views for redundancy checks.
///
/// Built once per `calls:`-bearing entity: `body_lines` are the non-`calls:`
/// lines, `line_tokens` their per-line identifier sets for the split
/// qualifier rule and the call-shaped bare restatement check. No joined
/// `full_text` is kept: verbatim matching iterates lines, avoiding one
/// allocation per entity.
struct PruneCtx<'a> {
    body_lines: Vec<&'a str>,
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

/// Whether a qualifier is split across two adjacent lines.
///
/// Same-line token co-occurrence is deliberately not treated as
/// restatement: a shared prefix with opposite tails (`Status::enabled`
/// next to an unrelated `disabled`) would otherwise prune the one signal
/// that distinguishes the two. Only an explicit line break at qualifier
/// punctuation counts, either a head line ending in `::` handing off to a
/// tail line starting with the tail identifier, or a tail line starting
/// in qualifier punctuation and carrying the separator-attached tail.
fn head_tail_nearby(
    body_lines: &[&str],
    line_tokens: &[std::collections::HashSet<&str>],
    head: &str,
    tail: &str,
) -> bool {
    for (lines, tokens) in body_lines.windows(2).zip(line_tokens.windows(2)) {
        if !(tokens[0].contains(head) && tokens[1].contains(tail)) {
            continue;
        }
        let head_line = lines[0].trim_end();
        let tail_line = lines[1].trim_start();
        // Head split (`Candidate::` then `new(...)`): sentence periods must
        // not qualify, so only `::` breaks the head line. The tail line
        // must start with the tail identifier itself.
        if head_line.ends_with("::") {
            let tail_first = tokenize(tail_line).next().unwrap_or_default();
            if tail_first == tail {
                return true;
            }
        }
        // Tail continuation (`.join` on its own line): the tail line must
        // carry the separator attached to the tail, not merely start with
        // unrelated punctuation.
        if tail_line.starts_with("::") || tail_line.starts_with('.') {
            let scoped_colon = format!("::{tail}");
            let scoped_dot = format!(".{tail}");
            if tail_line.contains(&scoped_colon) || tail_line.contains(&scoped_dot) {
                return true;
            }
        }
    }
    false
}

/// Whether a bare callee is restated by a call-shaped body line.
///
/// Bare names must appear beside an opening parenthesis to count as
/// restated. A plain assignment (`let map = 1;`) shares the token but
/// carries no call signal, so it must not prune `calls: map`.
fn is_bare_restated(
    body_lines: &[&str],
    line_tokens: &[std::collections::HashSet<&str>],
    callee: &str,
) -> bool {
    body_lines
        .iter()
        .zip(line_tokens.iter())
        .any(|(line, tokens)| tokens.contains(callee) && line.contains('('))
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
/// → low-confidence bare table → keep. Bare names require call-shaped
/// restatement, so `log` never matches `catalog` and plain assignments do
/// not count; qualified names may additionally match verbatim (their
/// `./::` delimiters make accidental containment unlikely) or by an
/// explicit qualifier split across adjacent lines.
fn classify_callee(callee: &str, ctx: &PruneCtx) -> CalleeFate {
    let callee = normalize_callee(callee);
    if callee.is_empty() {
        return CalleeFate::PruneRestated;
    }
    if ctx.resolved.is_some_and(|set| set.contains(callee)) {
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
            && head_tail_nearby(&ctx.body_lines, &ctx.line_tokens, head, tail)
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
    if is_bare_restated(&ctx.body_lines, &ctx.line_tokens, callee) {
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
        .is_some_and(|list| list.iter().any(|name| normalize_callee(name) == callee))
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
    let mut stats = CallsPruneStats {
        entities_visited: 1,
        ..Default::default()
    };
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
    let line_tokens: Vec<std::collections::HashSet<&str>> =
        body_lines.iter().map(|l| tokenize(l).collect()).collect();
    let ctx = PruneCtx {
        body_lines,
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
        let original: Vec<String> = split_call_list(rest);
        stats.lines_scanned += 1;
        stats.callees_total += original.len();
        let mut kept: Vec<String> = Vec::with_capacity(original.len());
        let mut saw_restated_or_stdlib_drop = false;
        for c in original.iter().map(String::as_str) {
            match classify_callee(c, &ctx) {
                CalleeFate::Keep => kept.push(c.to_string()),
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
            Language::Cpp,
            Language::CSharp,
        ];
        // C is covered by libc-signature detection; Dart answers bare
        // receiver-dependent names itself (`is_common_method` covers
        // `map`/`where`/`toString`); Scala, Ruby, Php, Lua and Bash lost
        // their last bare entries to the overlap audit below (every former
        // entry is detector-covered); Kotlin keeps only `print`/`println`
        // (its scope functions are detector-covered); document languages
        // carry no prunable bare calls.
        let expects_empty: &[Language] = &[
            Language::C,
            Language::Dart,
            Language::Scala,
            Language::Ruby,
            Language::Php,
            Language::Lua,
            Language::Bash,
            Language::Html,
            Language::Css,
        ];
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

    /// Sync guard: bare tables must cover the detector's blind spot, not
    /// restate it. `classify_callee` (and `is_boilerplate_name`) consult the
    /// shared detector before the bare table, so a detector-covered entry is
    /// unreachable dead weight: removing it changes no fate (`PruneStdlib`
    /// either way). This test turns red when a table gains — or a detector
    /// learns — an overlapping spelling; delete the entry instead of
    /// updating the test. `COMMON_BARE_METHODS` is exempt: it is consulted
    /// first by design, so overlaps there are harmless either way.
    #[test]
    fn prune_bare_tables_have_no_detector_overlap() {
        for language in Language::ast_parsable_languages() {
            for entry in bare_boilerplate_methods(*language) {
                assert!(
                    !StdlibDetector::is_stdlib_call(entry, language),
                    "{entry} in {language:?} is already covered by the detector; \
                     remove it from the bare table"
                );
            }
        }
    }

    /// Consumer-side lock for the overlap audit above.
    ///
    /// These bare spellings were deleted from the per-language tables
    /// because the detector answers them (same `PruneStdlib` fate either
    /// way). If a detector ever stops covering one of them, pruning would
    /// silently flip to `Keep` — this test turns that red instead, forcing
    /// a conscious decision (re-add to the bare table or accept the keep).
    /// It mirrors the `cce-stdlib` sentinel tests from the consumer side.
    #[test]
    fn prune_detector_covers_audited_spellings() {
        let cases: &[(Language, &[&str])] = &[
            (Language::Rust, &["write", "writeln"]),
            (Language::Python, &["set"]),
            (
                Language::Kotlin,
                &[
                    "let", "run", "with", "apply", "also", "TODO", "require", "check",
                ],
            ),
            (
                Language::Scala,
                &[
                    "map",
                    "flatMap",
                    "filter",
                    "foreach",
                    "toString",
                    "println",
                    "print",
                    "getOrElse",
                ],
            ),
            (
                Language::Ruby,
                &["puts", "to_s", "each", "map", "require", "raise"],
            ),
            (
                Language::Php,
                &["echo", "count", "isset", "empty", "print_r", "var_dump"],
            ),
            (
                Language::Lua,
                &[
                    "print", "require", "pairs", "ipairs", "tostring", "tonumber",
                ],
            ),
            (
                Language::Bash,
                &["echo", "cd", "exit", "return", "set", "export", "printf"],
            ),
        ];
        for (language, names) in cases {
            for name in *names {
                assert!(
                    StdlibDetector::is_stdlib_call(name, language),
                    "detector lost coverage of {name} in {language:?}; \
                     re-add it to the bare table or accept the behavior change"
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
    fn prune_bare_assignment_does_not_count_as_restatement() {
        // `let map = 1;` shares the token but carries no call signal, so
        // the bare guess must fall back to the sole-signal guard instead of
        // vanishing as restated.
        let out = prune_case_for_language(
            "function demo.\ncalls: map.\nDoes things.\nlet map = 1;",
            Language::Rust,
        );
        assert!(
            out.contains("calls: map."),
            "assignment must not restate bare callee: {out:?}"
        );
    }

    #[test]
    fn prune_distinctive_assignment_does_not_count_as_restatement() {
        // A distinctive callee sharing a variable name keeps its signal when
        // the name only appears in a declaration without call syntax.
        let out = prune_case_for_language(
            "function demo.\ncalls: build_index.\nDoes things.\nlet build_index = 1;",
            Language::Rust,
        );
        assert!(
            out.contains("calls: build_index."),
            "assignment must not restate distinctive callee: {out:?}"
        );
    }

    #[test]
    fn prune_bare_call_shape_counts_as_restatement() {
        // The same token beside a parenthesis is call evidence and prunes.
        let out = prune_case_for_language(
            "function demo.\ncalls: build_index.\nDoes things.\nbuild_index(1);",
            Language::Rust,
        );
        assert!(
            !out.contains("calls:"),
            "call-shaped line restates callee: {out:?}"
        );
    }

    #[test]
    fn prune_head_tail_split_qualifier_prunes() {
        // A qualifier split across adjacent lines at punctuation still
        // states the call.
        let out = prune_case(
            "function demo.\ncalls: Candidate::new.\nReturns true.\nlet x = Candidate::\nnew(path);",
        );
        assert!(!out.contains("calls:"), "split qualifier prunes: {out:?}");
    }

    #[test]
    fn split_call_list_keeps_generic_comma() {
        let parts = split_call_list("HashMap<String, i32>::new, build_index.");
        assert_eq!(
            parts,
            vec![
                "HashMap<String, i32>::new".to_string(),
                "build_index".to_string()
            ]
        );
    }

    #[test]
    fn local_callee_requires_exact_segment() {
        assert!(is_local_callee("Group::method", "Group"));
        assert!(is_local_callee("Group.method", "Group"));
        assert!(!is_local_callee("GroupExtra::method", "Group"));
        assert!(!is_local_callee("Other::method", "Group"));
        assert!(!is_local_callee("anything", ""));
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
        // Same-line token co-occurrence without separator attachment is not
        // restatement: `make(Candidate, new)` never states `Candidate::new`.
        let same_line = prune_case(
            "function demo.\ncalls: Candidate::new.\nReturns true.\nlet x = make(Candidate, new);",
        );
        assert!(
            same_line.contains("Candidate::new"),
            "separator-free co-occurrence retained: {same_line:?}"
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
    fn prune_opposite_tail_with_shared_head_is_kept() {
        // Shared prefix with opposite tails must not prune: the tail is the
        // only signal distinguishing `enable` from `disable`.
        let out = prune_case(
            "function demo.\ncalls: Status::disabled.\nReturns true.\nStatus::enabled(x);\nlet ok = disabled;",
        );
        assert!(
            out.contains("Status::disabled"),
            "opposite tail retained: {out:?}"
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
        let stats = prune_redundant_calls_lines(&mut conversion, Language::Rust, None, None, None);
        assert_eq!(stats.entities_visited, 1);
        assert_eq!(stats.lines_scanned, 1);
        assert_eq!(stats.callees_total, 2);
        assert_eq!(stats.callees_pruned, 1);
        assert_eq!(stats.lines_removed, 0);
        assert_eq!(stats.lines_guard_retained, 0);
    }
}
