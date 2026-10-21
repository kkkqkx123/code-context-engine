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
//!   token, keeping the first occurrence;
//! - the matched terms are rendered as a single `Glossary: ...` line whose
//!   entries are separated by semicolons.
//!
//! This module is the single implementation of that contract. The AST-to-NL
//! pipeline and the document pipeline both build on it.

use std::collections::HashSet;
use std::sync::Arc;

use crate::{CodePlugin, PluginRegistry};
use cce_types::plugin::{MAX_MERGED_VOCABULARY_ENTRIES, VocabularyEntry};

/// A merged glossary table collected from the matching `Vocabulary` plugins.
#[derive(Debug, Default, Clone)]
pub struct VocabularyTable {
    /// Entries in first-occurrence order, de-duplicated by match key.
    entries: Vec<VocabularyEntry>,
    /// The match keys of `entries`, for whole-token lookup.
    keys: HashSet<String>,
}

impl VocabularyTable {
    /// Build a table from raw entries.
    ///
    /// Entries are de-duplicated by match key (first occurrence wins) and the
    /// total number of entries is capped.
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
        if !self.keys.insert(entry.match_key()) {
            return false;
        }
        self.entries.push(entry);
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

    /// The entries whose token occurs as an identifier token in `texts`.
    ///
    /// The result keeps the table order, so the rendered glossary line is
    /// stable across entities.
    pub fn matched_entries<'a>(&'a self, texts: &[&str]) -> Vec<&'a VocabularyEntry> {
        if self.entries.is_empty() {
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
        self.entries
            .iter()
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

/// The plugins that contribute a terminology table to a file.
///
/// The plugins are ordered by descending effective priority (ties by
/// registration order), which is the order their tables are merged in.
pub fn providers<'a>(
    registry: &'a PluginRegistry,
    file_path: &str,
    language: &str,
) -> Vec<&'a Arc<dyn CodePlugin>> {
    registry.get_vocabulary_providers(Some(file_path), Some(language))
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

    #[test]
    fn test_matches_identifier_token_only() {
        let table = VocabularyTable::from_entries([entry("stts", "sync time-to-sample")]);
        assert_eq!(table.matched_entries(&["stts"]).len(), 1);
        assert!(
            table.matched_entries(&["stts_box"]).len() == 1,
            "a compound identifier must match through its segment"
        );
        assert!(
            table.matched_entries(&["mystts"]).is_empty(),
            "a substring must not match"
        );
    }

    #[test]
    fn test_matches_identifier_segments() {
        let table = VocabularyTable::from_entries([
            entry("stts", "sync time-to-sample"),
            entry("h264", "H.264 video"),
        ]);
        let matched = table.matched_entries(&["parse_stts", "decode_h264_frame"]);
        assert_eq!(matched.len(), 2);
        assert!(
            table
                .matched_entries(&["sttsCount"])
                .iter()
                .any(|e| e.token == "stts")
        );
    }

    #[test]
    fn test_short_token_does_not_match_inside_a_segment() {
        let table = VocabularyTable::from_entries([entry("ts", "timestamp")]);
        assert!(table.matched_entries(&["stts"]).is_empty());
        assert!(table.matched_entries(&["timestamp"]).is_empty());
        assert_eq!(table.matched_entries(&["ts"]).len(), 1);
    }

    #[test]
    fn test_digit_sequences_stay_one_segment() {
        let table = VocabularyTable::from_entries([entry("co64", "64-bit chunk offset box")]);
        assert_eq!(table.matched_entries(&["co64_offset"]).len(), 1);
    }

    #[test]
    fn test_matching_is_case_insensitive() {
        let table = VocabularyTable::from_entries([entry("stts", "sync time-to-sample")]);
        assert_eq!(table.matched_entries(&["stts"]).len(), 1);
        assert_eq!(table.matched_entries(&["STTS"]).len(), 1);
        assert_eq!(table.matched_entries(&["Stts"]).len(), 1);
        assert_eq!(table.matched_entries(&["parse_STTS_box"]).len(), 1);
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
        let matched = table.matched_entries(&["parse", "fn parse(sdp: &str)", "let x = 1;"]);
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
        let matched = table.matched_entries(&["stts and sdp"]);
        let line = VocabularyTable::glossary_line(&matched).expect("glossary line");
        assert_eq!(
            line,
            "Glossary: stts = sync time-to-sample; sdp = session description protocol"
        );
    }

    #[test]
    fn test_glossary_line_keeps_the_plugin_spelling() {
        let table = VocabularyTable::from_entries([entry("avcC", "AVC decoder configuration")]);
        let matched = table.matched_entries(&["avcc"]);
        let line = VocabularyTable::glossary_line(&matched).expect("glossary line");
        assert_eq!(line, "Glossary: avcC = AVC decoder configuration");
    }

    #[test]
    fn test_glossary_line_is_absent_without_matches() {
        let table = VocabularyTable::from_entries([entry("stts", "sync time-to-sample")]);
        let matched = table.matched_entries(&["nothing here"]);
        assert!(VocabularyTable::glossary_line(&matched).is_none());
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
}
