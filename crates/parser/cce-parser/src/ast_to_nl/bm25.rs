//! BM25 index text generation
//!
//! This module generates BM25-optimized text that keeps source identifiers
//! verbatim so the shared tokenizer can produce whole-identifier and split
//! forms for exact-spelling and fuzzy recall.
//!
//! # Output Format
//!
//! BM25 text for an entity is an identity line plus real content:
//! - Identity: signature (or `name (kind).`) with the declaration name written
//!   once, qualified as `parent.member` by the converter
//! - Doc comment: original sentences with comment markers stripped
//! - Behavior facts and uncovered body: de-commented source text appended by
//!   the index enrichment pass (never a natural-language restatement)
//!
//! Parameters, return types, and call targets appear only in this content,
//! never in the boosted keyword field.

pub mod generator;
pub mod keyword_extractor;
pub mod templates;

#[cfg(test)]
mod test;

// Re-export main types
pub use generator::Bm25Generator;
pub use keyword_extractor::KeywordExtractor;
// Shared text utilities (MixedTokenizer / Bm25TextCleaner) live in
// cce_text; the parser and infrastructure consume them directly.
