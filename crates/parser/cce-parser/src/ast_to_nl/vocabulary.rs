//! Domain vocabulary (glossary) support for the AST→NL pipeline.
//!
//! Domain terminology is not part of the generic engine: a plugin may
//! declare a table of identifier tokens and their human-readable
//! expansions through the additive `Vocabulary` capability. The host owns
//! the matching and the rendering so every plugin contributes the same
//! shape of output:
//!
//! - matching is restricted to whole identifier tokens, so a short token
//!   (`ts`) never matches a longer identifier (`timestamp`);
//! - the tables of all matching plugins are merged and de-duplicated by
//!   token, keeping the first occurrence;
//! - an entity contributes at most one `Glossary: ...` line, whose entries
//!   are separated by semicolons.

use std::collections::HashSet;

use cce_plugin::PluginRegistry;
use cce_types::plugin::VocabularyEntry;

/// Maximum number of glossary entries the host keeps for one file.
///
/// The limit bounds the index growth a misbehaving plugin can cause; the
/// entries beyond it are dropped.
pub const MAX_VOCABULARY_ENTRIES: usize = 256;

/// Maximum length (in characters) of a single glossary expansion.
pub const MAX_EXPANSION_LENGTH: usize = 200;

/// A merged glossary table collected from the matching `Vocabulary` plugins.
#[derive(Debug, Default, Clone)]
pub struct VocabularyTable {
    /// Entries in first-occurrence order, de-duplicated by token.
    entries: Vec<VocabularyEntry>,
    /// The tokens of `entries`, for whole-token lookup.
    tokens: HashSet<String>,
}

impl VocabularyTable {
    /// Build a table from raw entries.
    ///
    /// Entries are de-duplicated by token (first occurrence wins) and the
    /// total number of entries is capped.
    pub fn from_entries(entries: impl IntoIterator<Item = VocabularyEntry>) -> Self {
        let mut table = Self::default();
        for entry in entries {
            table.push(entry);
        }
        table
    }

    /// Append an entry, ignoring duplicates and over-long expansions.
    ///
    /// Returns `true` when the entry was accepted.
    fn push(&mut self, mut entry: VocabularyEntry) -> bool {
        if entry.token.is_empty() || entry.expansion.trim().is_empty() {
            return false;
        }
        if self.entries.len() >= MAX_VOCABULARY_ENTRIES {
            return false;
        }
        if entry.expansion.chars().count() > MAX_EXPANSION_LENGTH {
            entry.expansion = entry.expansion.chars().take(MAX_EXPANSION_LENGTH).collect();
        }
        if !self.tokens.insert(entry.token.clone()) {
            return false;
        }
        self.entries.push(entry);
        true
    }

    /// Whether the table carries no entry.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The merged entries, in first-occurrence order.
    pub fn entries(&self) -> &[VocabularyEntry] {
        &self.entries
    }

    /// The entries whose token occurs as a whole identifier in `texts`.
    ///
    /// The result keeps the table order, so the rendered glossary line is
    /// stable across entities.
    pub fn matched_entries<'a>(&'a self, texts: &[&str]) -> Vec<&'a VocabularyEntry> {
        if self.entries.is_empty() {
            return Vec::new();
        }
        let mut present: HashSet<&str> = HashSet::new();
        for text in texts {
            present.extend(identifier_runs(text));
        }
        self.entries
            .iter()
            .filter(|entry| present.contains(entry.token.as_str()))
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

/// Collect and merge the terminology tables of all matching plugins.
///
/// Plugins are consulted in registry priority order; a failing plugin is
/// logged and skipped, the remaining tables still contribute.
pub fn collect(registry: &PluginRegistry, file_path: &str, language: &str) -> VocabularyTable {
    let mut table = VocabularyTable::default();
    for plugin in registry.get_vocabulary_providers(Some(file_path), Some(language)) {
        match plugin.vocabulary() {
            Ok(entries) => {
                for entry in entries {
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
    table
}

/// The identifier-like runs of `text`.
///
/// A run is a maximal sequence of alphanumeric characters and underscores,
/// which is the granularity at which identifiers are compared.
fn identifier_runs(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|run| !run.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(token: &str, expansion: &str) -> VocabularyEntry {
        VocabularyEntry::new(token, expansion)
    }

    #[test]
    fn test_matches_whole_identifier_only() {
        let table = VocabularyTable::from_entries([entry("stts", "sync time-to-sample")]);
        assert_eq!(table.matched_entries(&["stts"]).len(), 1);
        assert!(
            table.matched_entries(&["stts_box"]).is_empty(),
            "a longer identifier must not match a short token"
        );
        assert!(
            table.matched_entries(&["mystts"]).is_empty(),
            "a substring must not match"
        );
    }

    #[test]
    fn test_matching_is_case_sensitive() {
        let table = VocabularyTable::from_entries([entry("stts", "sync time-to-sample")]);
        assert_eq!(table.matched_entries(&["stts"]).len(), 1);
        assert!(table.matched_entries(&["STTS"]).is_empty());
        assert!(table.matched_entries(&["Stts"]).is_empty());
    }

    #[test]
    fn test_matches_across_name_signature_and_body() {
        let table = VocabularyTable::from_entries([
            entry("stts", "sync time-to-sample"),
            entry("sdp", "session description protocol"),
        ]);
        let matched = table.matched_entries(&["parse_stts", "fn parse(sdp: &str)", "let x = 1;"]);
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
        assert!(table.matched_entries(&["token"]).is_empty());
    }

    #[test]
    fn test_entry_count_is_capped() {
        let entries: Vec<VocabularyEntry> = (0..MAX_VOCABULARY_ENTRIES + 10)
            .map(|i| entry(&format!("t{i}"), "meaning"))
            .collect();
        let table = VocabularyTable::from_entries(entries);
        assert_eq!(table.entries().len(), MAX_VOCABULARY_ENTRIES);
    }

    #[test]
    fn test_over_long_expansion_is_truncated() {
        let long = "x".repeat(MAX_EXPANSION_LENGTH + 50);
        let table = VocabularyTable::from_entries([entry("token", &long)]);
        assert_eq!(
            table.entries()[0].expansion.chars().count(),
            MAX_EXPANSION_LENGTH
        );
    }

    #[test]
    fn test_glossary_line_format() {
        let table = VocabularyTable::from_entries([
            entry("stts", "sync time-to-sample"),
            entry("sdp", "session description protocol"),
        ]);
        let matched = table.matched_entries(&["stts and sdp"]);
        let line = VocabularyTable::glossary_line(&matched).expect("glossary line");
        assert_eq!(
            line,
            "Glossary: stts = sync time-to-sample; sdp = session description protocol"
        );
    }

    #[test]
    fn test_glossary_line_is_absent_without_matches() {
        let table = VocabularyTable::from_entries([entry("stts", "sync time-to-sample")]);
        let matched = table.matched_entries(&["nothing here"]);
        assert!(VocabularyTable::glossary_line(&matched).is_none());
    }

    #[test]
    fn test_identifier_runs_split_on_punctuation() {
        let runs: Vec<&str> = identifier_runs("fn parse(sdp: &str) -> bool").collect();
        assert_eq!(runs, vec!["fn", "parse", "sdp", "str", "bool"]);
    }
}
