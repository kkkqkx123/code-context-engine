//! Common types for document processing
//!
//! This module provides shared type definitions to reduce code duplication
//! across different document type processors.

/// Check if a word is a common English stopword
///
/// Used by keyword extraction to filter out low-value terms.
pub fn is_stopword(word: &str) -> bool {
    matches!(
        word.to_lowercase().as_str(),
        "the"
            | "a"
            | "an"
            | "is"
            | "are"
            | "was"
            | "were"
            | "be"
            | "been"
            | "being"
            | "have"
            | "has"
            | "had"
            | "do"
            | "does"
            | "did"
            | "will"
            | "would"
            | "could"
            | "should"
            | "may"
            | "might"
            | "shall"
            | "can"
            | "need"
            | "dare"
            | "ought"
            | "used"
            | "to"
            | "of"
            | "in"
            | "for"
            | "on"
            | "with"
            | "at"
            | "by"
            | "from"
            | "as"
            | "into"
            | "through"
            | "during"
            | "before"
            | "after"
            | "above"
            | "below"
            | "between"
            | "out"
            | "off"
            | "over"
            | "under"
            | "again"
            | "further"
            | "then"
            | "once"
            | "here"
            | "there"
            | "when"
            | "where"
            | "why"
            | "how"
            | "all"
            | "each"
            | "every"
            | "both"
            | "few"
            | "more"
            | "most"
            | "other"
            | "some"
            | "such"
            | "no"
            | "nor"
            | "not"
            | "only"
            | "own"
            | "same"
            | "so"
            | "than"
            | "too"
            | "very"
            | "just"
            | "because"
            | "but"
            | "and"
            | "or"
            | "if"
            | "while"
            | "about"
            | "up"
            | "it"
            | "its"
            | "this"
            | "that"
            | "these"
            | "those"
            | "i"
            | "me"
            | "my"
            | "we"
            | "our"
            | "you"
            | "your"
            | "he"
            | "him"
            | "his"
            | "she"
            | "her"
            | "they"
            | "them"
            | "their"
            | "what"
            | "which"
            | "who"
            | "whom"
            | "also"
            | "get"
            | "got"
            | "use"
            | "using"
    )
}

/// Smart merging configuration
#[derive(Debug, Clone)]
pub struct MergingConfig {
    /// Enable smart merging strategy
    pub enable_smart_merging: bool,
    /// Minimum chunk tokens threshold for merging
    pub min_chunk_tokens: usize,
    /// Maximum merge expansion factor relative to max_tokens
    pub max_merge_expansion_factor: f32,
    /// Enable key-based association merging (for structured data)
    pub enable_key_based_association: bool,
}

impl Default for MergingConfig {
    fn default() -> Self {
        Self {
            enable_smart_merging: true,
            min_chunk_tokens: 20,
            max_merge_expansion_factor: 1.5,
            enable_key_based_association: false,
        }
    }
}
