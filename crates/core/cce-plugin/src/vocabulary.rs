//! Domain terminology (glossary) support for the `Vocabulary` capability.
//!
//! Domain terminology is not part of the generic engine: a plugin declares a
//! table of identifier tokens and their human-readable expansions through the
//! additive `Vocabulary` capability. The host owns the matching and the
//! rendering so every plugin contributes the same shape of output:
//!
//! - matching happens on identifier tokens and on their underscore and
//!   camelCase segments, so `stts` matches `parse_stts` while `ts` never
//!   matches `stts`;
//! - matching is case-insensitive, matching the index tokenizer that
//!   lowercases every token, so a source identifier written `STTS` matches
//!   the term `stts`;
//! - the tables of all matching plugins are merged and de-duplicated by
//!   token and scope, keeping the first occurrence;
//! - an entry may carry a scope: the path patterns that select the files the
//!   entry applies to. A table whose entries carry no scope applies to every
//!   file of its provider set; a scoped table is resolved per file. The
//!   entries that share a token are decided by the scope priority, then by
//!   how narrow the matching scope is, then by the order the tables were
//!   merged in. The scope patterns speak the same path pattern language as
//!   the plugin-level file filter, so both routing levels select the same
//!   files for the same pattern;
//! - the matched terms are rendered as a single `Glossary: ...` line whose
//!   entries are separated by semicolons.
//!
//! This module is the single implementation of that contract. The AST-to-NL
//! pipeline and the document pipeline both build on it.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::CodePlugin;
use crate::path_pattern::PathPattern;
use cce_types::plugin::{MAX_MERGED_VOCABULARY_ENTRIES, VocabularyEntry, VocabularyScope};

/// A merged glossary table collected from the matching `Vocabulary` plugins.
#[derive(Debug, Default, Clone)]
pub struct VocabularyTable {
    /// Entries in first-occurrence order, de-duplicated by match key and scope.
    entries: Vec<VocabularyEntry>,
    /// The compiled scope of each entry, aligned with `entries`.
    scopes: Vec<Option<CompiledScope>>,
    /// The (match key, scope key) pairs of `entries`, for de-duplication.
    keys: HashSet<(String, String)>,
}

impl VocabularyTable {
    /// Build a table from raw entries.
    ///
    /// Entries are de-duplicated by match key and scope (first occurrence
    /// wins) and the total number of entries is capped. An entry whose scope
    /// cannot be satisfied is dropped.
    pub fn from_entries(entries: impl IntoIterator<Item = VocabularyEntry>) -> Self {
        let mut table = Self::default();
        for entry in entries {
            table.push(entry);
        }
        table
    }

    /// Append an entry, ignoring duplicates and unusable entries.
    ///
    /// Returns `true` when the entry was accepted, `false` when it was
    /// dropped as unusable, a duplicate, or over the merged limit.
    fn push(&mut self, entry: VocabularyEntry) -> bool {
        let Some(entry) = entry.normalized() else {
            return false;
        };
        if self.entries.len() >= MAX_MERGED_VOCABULARY_ENTRIES {
            return false;
        }
        let scope = match &entry.scope {
            None => None,
            Some(scope) => {
                let Some(compiled) = CompiledScope::compile(scope) else {
                    return false;
                };
                Some(compiled)
            }
        };
        let key = (entry.match_key(), entry.scope_key());
        if !self.keys.insert(key) {
            return false;
        }
        self.entries.push(entry);
        self.scopes.push(scope);
        true
    }

    /// Whether the table is at the merged limit and cannot take more entries.
    pub fn is_full(&self) -> bool {
        self.entries.len() >= MAX_MERGED_VOCABULARY_ENTRIES
    }

    /// Whether the table carries no entry.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The merged entries, in first-occurrence order.
    pub fn entries(&self) -> &[VocabularyEntry] {
        &self.entries
    }

    /// Whether any entry of the table restricts the files it applies to.
    ///
    /// A table without a restricting scope applies to every file of its
    /// provider set, so its applicable entries do not depend on the file.
    pub fn has_scopes(&self) -> bool {
        self.scopes
            .iter()
            .any(|scope| scope.as_ref().is_some_and(|scope| scope.restricts()))
    }

    /// The entries that apply to `file_path`, one per token.
    ///
    /// The result holds the indexes of the winning entries in table order, so
    /// the rendered glossary line stays stable across entities.
    fn resolve(&self, file_path: &str) -> Vec<u32> {
        let mut winners: HashMap<String, (usize, (i32, Specificity))> = HashMap::new();
        for (index, entry) in self.entries.iter().enumerate() {
            let (priority, specificity) = match &self.scopes[index] {
                None => (0, Specificity::GLOBAL),
                Some(scope) => match scope.applies(file_path) {
                    None => continue,
                    Some(specificity) => (scope.priority, specificity),
                },
            };
            let rank = (priority, specificity);
            // A strictly better rank replaces the winner; a tie keeps the
            // entry that came first, which is the entry of the plugin with
            // the higher priority.
            let key = entry.match_key();
            let better = match winners.get(&key) {
                None => true,
                Some((_, current)) => rank > *current,
            };
            if better {
                winners.insert(key, (index, rank));
            }
        }
        let mut selected: Vec<usize> = winners.into_values().map(|(index, _)| index).collect();
        selected.sort_unstable();
        selected.into_iter().map(|index| index as u32).collect()
    }
}

/// The glossary entries that apply to one file.
///
/// The view is the file-resolved form of a merged table: it holds the entries
/// whose scope matches the file, one per token. It borrows the merged table
/// instead of copying it, so a view is cheap to keep around.
#[derive(Debug, Clone)]
pub struct FileVocabulary {
    /// The merged table the view resolves.
    table: Arc<VocabularyTable>,
    /// The indexes of the entries that apply, in table order.
    indexes: Vec<u32>,
}

impl FileVocabulary {
    /// A view over every entry of the table.
    pub fn all(table: Arc<VocabularyTable>) -> Self {
        let indexes = (0..table.entries().len() as u32).collect();
        Self { table, indexes }
    }

    /// A view over the entries of the table that apply to `file_path`.
    pub fn resolved(table: Arc<VocabularyTable>, file_path: &str) -> Self {
        let indexes = table.resolve(file_path);
        Self { table, indexes }
    }

    /// Whether the view carries no entry.
    pub fn is_empty(&self) -> bool {
        self.indexes.is_empty()
    }

    /// The applicable entries, in table order.
    pub fn entries(&self) -> impl Iterator<Item = &VocabularyEntry> {
        let entries = self.table.entries();
        self.indexes
            .iter()
            .map(move |index| &entries[*index as usize])
    }

    /// The entries whose token occurs as an identifier token in `texts`.
    ///
    /// The result keeps the table order, so the rendered glossary line is
    /// stable across entities.
    pub fn matched_entries(&self, texts: &[&str]) -> Vec<&VocabularyEntry> {
        if self.indexes.is_empty() {
            return Vec::new();
        }
        let mut present: HashSet<String> = HashSet::new();
        for text in texts {
            for token in identifier_tokens(text) {
                if !present.contains(token.as_str()) {
                    present.insert(token);
                }
            }
        }
        self.entries()
            .filter(|entry| present.contains(&entry.match_key()))
            .collect()
    }

    /// Render the glossary line for the given entries.
    ///
    /// Returns `None` when no entry matched.
    pub fn glossary_line(entries: &[&VocabularyEntry]) -> Option<String> {
        if entries.is_empty() {
            return None;
        }
        let parts: Vec<String> = entries
            .iter()
            .map(|entry| format!("{} = {}", entry.token, entry.expansion))
            .collect();
        Some(format!("Glossary: {}", parts.join("; ")))
    }
}

/// The compiled scope of one glossary entry.
#[derive(Debug, Clone)]
struct CompiledScope {
    /// The patterns that select the files the entry applies to.
    include: Vec<CompiledPattern>,
    /// The patterns that exclude files from the entry.
    exclude: Vec<CompiledPattern>,
    /// The priority of the scope among entries that share a token.
    priority: i32,
}

impl CompiledScope {
    /// Compile the scope of an entry.
    ///
    /// Returns `None` when the scope expresses a constraint that no pattern
    /// can satisfy: such an entry is dropped instead of being applied to
    /// files it was never meant for.
    fn compile(scope: &VocabularyScope) -> Option<Self> {
        let include = compile_patterns(&scope.include);
        let exclude = compile_patterns(&scope.exclude);
        if include.is_empty() && exclude.is_empty() && !scope.is_unrestricted() {
            return None;
        }
        Some(Self {
            include,
            exclude,
            priority: scope.priority,
        })
    }

    /// Whether the scope restricts the files the entry applies to.
    ///
    /// A scope without patterns applies everywhere; it only carries the
    /// priority that decides between entries sharing a token.
    fn restricts(&self) -> bool {
        !self.include.is_empty() || !self.exclude.is_empty()
    }

    /// The specificity of the scope for `path`, or `None` when the scope does
    /// not apply to the file.
    fn applies(&self, path: &str) -> Option<Specificity> {
        if self.exclude.iter().any(|pattern| pattern.matches(path)) {
            return None;
        }
        if self.include.is_empty() {
            return Some(if self.exclude.is_empty() {
                Specificity::GLOBAL
            } else {
                Specificity::EXCLUDE_ONLY
            });
        }
        self.include
            .iter()
            .filter(|pattern| pattern.matches(path))
            .map(|pattern| pattern.specificity)
            .max()
    }
}

/// One compiled path pattern of a scope.
#[derive(Debug, Clone)]
struct CompiledPattern {
    /// The matcher of the pattern.
    pattern: PathPattern,
    /// How narrow the pattern is.
    specificity: Specificity,
}

impl CompiledPattern {
    /// Whether the pattern selects `path`.
    fn matches(&self, path: &str) -> bool {
        self.pattern.matches(path)
    }
}

/// Compile the patterns of one scope list.
///
/// A pattern that is not a valid glob is dropped: the scope then either
/// selects fewer files than the author wrote, or none at all.
fn compile_patterns(patterns: &[String]) -> Vec<CompiledPattern> {
    patterns
        .iter()
        .filter_map(|pattern| {
            let specificity = pattern_specificity(pattern);
            match PathPattern::compile(pattern) {
                Some(compiled) => Some(CompiledPattern {
                    pattern: compiled,
                    specificity,
                }),
                None => {
                    tracing::warn!(
                        pattern = %pattern,
                        "Dropping invalid vocabulary scope pattern"
                    );
                    None
                }
            }
        })
        .collect()
}

/// How narrow a scope or a path pattern is.
///
/// The literal path components come first and the literal characters second,
/// so a deeper path beats a shallower one and a longer literal beats a
/// shorter one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Specificity {
    /// The number of path components that carry no wildcard.
    components: i32,
    /// The number of characters that are no wildcard.
    chars: i32,
}

impl Specificity {
    /// The specificity of an entry that no scope restricts.
    const GLOBAL: Specificity = Specificity {
        components: -1,
        chars: -1,
    };
    /// The specificity of a scope that only excludes paths.
    const EXCLUDE_ONLY: Specificity = Specificity {
        components: 0,
        chars: 0,
    };
}

/// The specificity of one path pattern.
fn pattern_specificity(pattern: &str) -> Specificity {
    let components = pattern
        .split('/')
        .filter(|component| {
            !component.is_empty() && !component.contains(['*', '?', '[', ']', '{', '}'])
        })
        .count() as i32;
    let chars = pattern
        .chars()
        .filter(|character| !matches!(character, '*' | '?' | '[' | ']' | '{' | '}'))
        .count() as i32;
    Specificity { components, chars }
}

/// The identity of a provider set, used as the cache key of the merged table.
///
/// The vocabulary interface takes no file argument, so one provider set
/// yields the same table for every file it routes to. The identity therefore
/// describes the providers themselves plus the registry revision, which keeps
/// the number of cached tables independent of the number of indexed files.
pub fn provider_identity(providers: &[&Arc<dyn CodePlugin>], revision: u64) -> String {
    let rows: Vec<String> = providers
        .iter()
        .map(|plugin| {
            let meta = plugin.metadata();
            format!("{}\u{1f}{}", meta.id, meta.version)
        })
        .collect();
    format!("{}\u{1e}{}", revision, rows.join("\u{1f}"))
}

/// Merge the terminology tables of the given providers.
///
/// A failing plugin is logged and skipped, the remaining tables still
/// contribute.
pub fn merge(providers: &[&Arc<dyn CodePlugin>]) -> VocabularyTable {
    let mut table = VocabularyTable::default();
    let mut dropped = 0usize;
    for plugin in providers {
        match plugin.vocabulary() {
            Ok(entries) => {
                for entry in entries {
                    if table.is_full() {
                        dropped += 1;
                        continue;
                    }
                    table.push(entry);
                }
            }
            Err(e) => {
                tracing::warn!(
                    plugin_id = %plugin.metadata().id,
                    error = %e,
                    "Plugin vocabulary lookup failed"
                );
            }
        }
    }
    if dropped > 0 {
        tracing::warn!(
            limit = MAX_MERGED_VOCABULARY_ENTRIES,
            dropped,
            "Merged vocabulary table reached the host limit; the remaining entries are dropped"
        );
    }
    table
}

/// The identifier tokens of `text`, lowercased.
///
/// A token is a maximal sequence of alphanumeric characters and underscores,
/// plus each of its underscore and camelCase separated segments. Matching a
/// term against `parse_stts` therefore hits `stts`, while `ts` never matches
/// `stts`: a segment boundary is a real identifier boundary.
fn identifier_tokens(text: &str) -> impl Iterator<Item = String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|run| !run.is_empty())
        .flat_map(|run| {
            let mut tokens = Vec::new();
            tokens.push(run.to_lowercase());
            tokens.extend(
                split_identifier_segments(run)
                    .into_iter()
                    .map(str::to_lowercase),
            );
            tokens
        })
}

/// The underscore and camelCase separated segments of an identifier run.
///
/// Digits never start a new segment, so `co64` stays one token.
fn split_identifier_segments(run: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    for part in run.split('_') {
        if part.is_empty() {
            continue;
        }
        let chars: Vec<(usize, char)> = part.char_indices().collect();
        let mut start = 0usize;
        for index in 1..chars.len() {
            let previous = chars[index - 1].1;
            let current = chars[index].1;
            let next = chars.get(index + 1).map(|(_, c)| *c);
            let boundary = (previous.is_lowercase() && current.is_uppercase())
                || (previous.is_uppercase()
                    && current.is_uppercase()
                    && next.is_some_and(|c| c.is_lowercase()));
            if boundary {
                segments.push(&part[start..chars[index].0]);
                start = chars[index].0;
            }
        }
        segments.push(&part[start..]);
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::plugin::MAX_VOCABULARY_EXPANSION_LENGTH;

    fn entry(token: &str, expansion: &str) -> VocabularyEntry {
        VocabularyEntry::new(token, expansion)
    }

    fn scoped(token: &str, expansion: &str, include: &[&str]) -> VocabularyEntry {
        VocabularyEntry::with_scope(
            token,
            expansion,
            VocabularyScope {
                include: include.iter().map(|p| p.to_string()).collect(),
                exclude: Vec::new(),
                priority: 0,
            },
        )
    }

    /// The expansions of the entries that apply to `path`.
    fn expansions(table: &VocabularyTable, path: &str) -> Vec<(String, String)> {
        let view = FileVocabulary::resolved(Arc::new(table.clone()), path);
        view.matched_entries(&["stts track"])
            .into_iter()
            .map(|entry| (entry.token.clone(), entry.expansion.clone()))
            .collect()
    }

    #[test]
    fn test_matches_identifier_token_only() {
        let table = VocabularyTable::from_entries([entry("stts", "sync time-to-sample")]);
        let view = FileVocabulary::all(Arc::new(table));
        assert_eq!(view.matched_entries(&["stts"]).len(), 1);
        assert!(
            view.matched_entries(&["stts_box"]).len() == 1,
            "a compound identifier must match through its segment"
        );
        assert!(
            view.matched_entries(&["mystts"]).is_empty(),
            "a substring must not match"
        );
    }

    #[test]
    fn test_matches_identifier_segments() {
        let table = VocabularyTable::from_entries([
            entry("stts", "sync time-to-sample"),
            entry("h264", "H.264 video"),
        ]);
        let view = FileVocabulary::all(Arc::new(table));
        let matched = view.matched_entries(&["parse_stts", "decode_h264_frame"]);
        assert_eq!(matched.len(), 2);
        assert!(
            view.matched_entries(&["sttsCount"])
                .iter()
                .any(|e| e.token == "stts")
        );
    }

    #[test]
    fn test_short_token_does_not_match_inside_a_segment() {
        let table = VocabularyTable::from_entries([entry("ts", "timestamp")]);
        let view = FileVocabulary::all(Arc::new(table));
        assert!(view.matched_entries(&["stts"]).is_empty());
        assert!(view.matched_entries(&["timestamp"]).is_empty());
        assert_eq!(view.matched_entries(&["ts"]).len(), 1);
    }

    #[test]
    fn test_digit_sequences_stay_one_segment() {
        let table = VocabularyTable::from_entries([entry("co64", "64-bit chunk offset box")]);
        let view = FileVocabulary::all(Arc::new(table));
        assert_eq!(view.matched_entries(&["co64_offset"]).len(), 1);
    }

    #[test]
    fn test_matching_is_case_insensitive() {
        let table = VocabularyTable::from_entries([entry("stts", "sync time-to-sample")]);
        let view = FileVocabulary::all(Arc::new(table));
        assert_eq!(view.matched_entries(&["stts"]).len(), 1);
        assert_eq!(view.matched_entries(&["STTS"]).len(), 1);
        assert_eq!(view.matched_entries(&["Stts"]).len(), 1);
        assert_eq!(view.matched_entries(&["parse_STTS_box"]).len(), 1);
    }

    #[test]
    fn test_case_variants_share_one_entry() {
        let table = VocabularyTable::from_entries([
            entry("stts", "sync time-to-sample"),
            entry("STTS", "duplicate meaning"),
        ]);
        assert_eq!(table.entries().len(), 1);
        assert_eq!(table.entries()[0].expansion, "sync time-to-sample");
    }

    #[test]
    fn test_matches_across_name_signature_and_body() {
        let table = VocabularyTable::from_entries([
            entry("stts", "sync time-to-sample"),
            entry("sdp", "session description protocol"),
        ]);
        let view = FileVocabulary::all(Arc::new(table));
        let matched = view.matched_entries(&["parse", "fn parse(sdp: &str)", "let x = 1;"]);
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].token, "sdp");
    }

    #[test]
    fn test_deduplicates_by_token_keeping_first_occurrence() {
        let table = VocabularyTable::from_entries([
            entry("stts", "first meaning"),
            entry("sdp", "session description protocol"),
            entry("stts", "second meaning"),
        ]);
        assert_eq!(table.entries().len(), 2);
        assert_eq!(table.entries()[0].expansion, "first meaning");
    }

    #[test]
    fn test_empty_entries_are_ignored() {
        let table = VocabularyTable::from_entries([entry("", "no token"), entry("token", "   ")]);
        assert!(table.is_empty());
        let view = FileVocabulary::all(Arc::new(table));
        assert!(view.matched_entries(&["token"]).is_empty());
    }

    #[test]
    fn test_merged_entry_count_is_capped() {
        let entries: Vec<VocabularyEntry> = (0..MAX_MERGED_VOCABULARY_ENTRIES + 10)
            .map(|i| entry(&format!("t{i}"), "meaning"))
            .collect();
        let table = VocabularyTable::from_entries(entries);
        assert_eq!(table.entries().len(), MAX_MERGED_VOCABULARY_ENTRIES);
    }

    #[test]
    fn test_over_long_expansion_is_truncated() {
        let long = "x".repeat(MAX_VOCABULARY_EXPANSION_LENGTH + 50);
        let table = VocabularyTable::from_entries([entry("token", &long)]);
        assert_eq!(
            table.entries()[0].expansion.chars().count(),
            MAX_VOCABULARY_EXPANSION_LENGTH
        );
    }

    #[test]
    fn test_glossary_line_format() {
        let table = VocabularyTable::from_entries([
            entry("stts", "sync time-to-sample"),
            entry("sdp", "session description protocol"),
        ]);
        let view = FileVocabulary::all(Arc::new(table));
        let matched = view.matched_entries(&["stts and sdp"]);
        let line = FileVocabulary::glossary_line(&matched).expect("glossary line");
        assert_eq!(
            line,
            "Glossary: stts = sync time-to-sample; sdp = session description protocol"
        );
    }

    #[test]
    fn test_glossary_line_keeps_the_plugin_spelling() {
        let table = VocabularyTable::from_entries([entry("avcC", "AVC decoder configuration")]);
        let view = FileVocabulary::all(Arc::new(table));
        let matched = view.matched_entries(&["avcc"]);
        let line = FileVocabulary::glossary_line(&matched).expect("glossary line");
        assert_eq!(line, "Glossary: avcC = AVC decoder configuration");
    }

    #[test]
    fn test_glossary_line_is_absent_without_matches() {
        let table = VocabularyTable::from_entries([entry("stts", "sync time-to-sample")]);
        let view = FileVocabulary::all(Arc::new(table));
        let matched = view.matched_entries(&["nothing here"]);
        assert!(FileVocabulary::glossary_line(&matched).is_none());
    }

    #[test]
    fn test_identifier_tokens_split_on_punctuation() {
        let tokens: Vec<String> = identifier_tokens("fn parse(sdp: &str) -> bool").collect();
        assert!(tokens.contains(&"fn".to_string()));
        assert!(tokens.contains(&"parse".to_string()));
        assert!(tokens.contains(&"sdp".to_string()));
        assert!(tokens.contains(&"str".to_string()));
        assert!(tokens.contains(&"bool".to_string()));
    }

    #[test]
    fn test_identifier_segments_split_camel_case() {
        let segments = split_identifier_segments("parseSttsBox");
        assert_eq!(segments, vec!["parse", "Stts", "Box"]);
        assert_eq!(
            split_identifier_segments("HTTPResponse"),
            vec!["HTTP", "Response"]
        );
        assert_eq!(split_identifier_segments("stts"), vec!["stts"]);
    }

    // ── Scopes ────────────────────────────────────────────────────────

    #[test]
    fn test_scoped_entry_applies_only_inside_its_scope() {
        let table = VocabularyTable::from_entries([scoped(
            "track",
            "media track",
            &["packages/player/**"],
        )]);
        assert!(table.has_scopes());
        assert_eq!(
            expansions(&table, "packages/player/src/track.c"),
            vec![("track".to_string(), "media track".to_string())]
        );
        assert!(
            expansions(&table, "packages/recorder/src/track.c").is_empty(),
            "a file outside the scope must not receive the entry"
        );
    }

    #[test]
    fn test_unscoped_entry_applies_to_every_file() {
        let table = VocabularyTable::from_entries([entry("track", "media track")]);
        assert!(!table.has_scopes());
        assert_eq!(expansions(&table, "packages/recorder/src/track.c").len(), 1);
        assert_eq!(expansions(&table, "a/b/c.c").len(), 1);
    }

    #[test]
    fn test_same_token_carries_a_different_meaning_per_package() {
        let table = VocabularyTable::from_entries([
            scoped("track", "media track", &["packages/player/**"]),
            scoped("track", "training track", &["packages/recorder/**"]),
        ]);
        assert_eq!(
            table.entries().len(),
            2,
            "both scopes must survive the merge"
        );
        assert_eq!(
            expansions(&table, "packages/player/a.c"),
            vec![("track".to_string(), "media track".to_string())]
        );
        assert_eq!(
            expansions(&table, "packages/recorder/a.c"),
            vec![("track".to_string(), "training track".to_string())]
        );
    }

    #[test]
    fn test_the_narrower_scope_wins() {
        let table = VocabularyTable::from_entries([
            scoped("track", "broad meaning", &["packages/**"]),
            scoped("track", "narrow meaning", &["packages/player/src/**"]),
        ]);
        assert_eq!(
            expansions(&table, "packages/player/src/a.c"),
            vec![("track".to_string(), "narrow meaning".to_string())]
        );
        assert_eq!(
            expansions(&table, "packages/player/test/a.c"),
            vec![("track".to_string(), "broad meaning".to_string())]
        );
    }

    #[test]
    fn test_the_scope_priority_wins_over_the_specificity() {
        let mut entry = scoped("track", "high priority", &["packages/**"]);
        entry.scope.as_mut().expect("scope").priority = 10;
        let table = VocabularyTable::from_entries([
            entry,
            scoped("track", "narrow meaning", &["packages/player/src/**"]),
        ]);
        assert_eq!(
            expansions(&table, "packages/player/src/a.c"),
            vec![("track".to_string(), "high priority".to_string())]
        );
    }

    #[test]
    fn test_the_exclude_pattern_removes_files() {
        let table = VocabularyTable::from_entries([VocabularyEntry::with_scope(
            "track",
            "media track",
            VocabularyScope {
                include: vec!["packages/**".to_string()],
                exclude: vec!["packages/player/tests/**".to_string()],
                priority: 0,
            },
        )]);
        assert_eq!(expansions(&table, "packages/player/src/a.c").len(), 1);
        assert!(
            expansions(&table, "packages/player/tests/a.c").is_empty(),
            "an excluded file must not receive the entry"
        );
    }

    #[test]
    fn test_an_exclude_only_scope_applies_everywhere_else() {
        let table = VocabularyTable::from_entries([VocabularyEntry::with_scope(
            "track",
            "media track",
            VocabularyScope {
                include: Vec::new(),
                exclude: vec!["**/tests/**".to_string()],
                priority: 0,
            },
        )]);
        assert_eq!(expansions(&table, "src/a.c").len(), 1);
        assert!(expansions(&table, "src/tests/a.c").is_empty());
    }

    #[test]
    fn test_an_unusable_scope_drops_the_entry() {
        let table = VocabularyTable::from_entries([VocabularyEntry::with_scope(
            "track",
            "media track",
            VocabularyScope {
                include: vec!["[".to_string()],
                exclude: Vec::new(),
                priority: 0,
            },
        )]);
        assert!(
            table.is_empty(),
            "an entry whose scope can never be satisfied must be dropped"
        );
    }

    #[test]
    fn test_an_empty_scope_keeps_the_entry_unrestricted() {
        let table = VocabularyTable::from_entries([VocabularyEntry::with_scope(
            "track",
            "media track",
            VocabularyScope::default(),
        )]);
        assert_eq!(table.entries().len(), 1);
        assert_eq!(expansions(&table, "any/file.c").len(), 1);
    }

    #[test]
    fn test_the_same_token_and_scope_are_deduplicated() {
        let table = VocabularyTable::from_entries([
            scoped("track", "first meaning", &["packages/player/**"]),
            scoped("track", "second meaning", &["packages/player/**"]),
        ]);
        assert_eq!(table.entries().len(), 1);
        assert_eq!(table.entries()[0].expansion, "first meaning");
    }

    #[test]
    fn test_a_suffix_pattern_selects_the_file_extension() {
        let table = VocabularyTable::from_entries([scoped("track", "media track", &["*.c"])]);
        assert_eq!(expansions(&table, "packages/player/src/a.c").len(), 1);
        assert!(expansions(&table, "packages/player/src/a.rs").is_empty());
    }
}
