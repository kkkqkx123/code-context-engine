//! BM25 hybrid enhanced text generation
//!
//! This module generates BM25-optimized text that preserves key entities
//! (function names, types, file paths) while adding natural language descriptions.
//!
//! # Output Format
//!
//! BM25 text includes:
//! - Original function/class names (preserved so the tokenizer can produce
//!   whole-identifier tokens for spelling-accurate recall)
//! - Normalized names (for fuzzy matching)
//! - Parameters and types (for API documentation)
//! - File path and module context (for navigation)
//! - Keywords (for keyword search)

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
