//! Embedding pure semantic summary generation
//!
//! This module generates pure semantic summaries for embedding-based search,
//! removing all code symbols and focusing on natural language intent.
//!
//! # Output Format
//!
//! Embedding text includes:
//! - Core intent (from docstring or normalized function name)
//! - Parameter semantics (count and description, not names)
//! - Return value semantics (description, not type)
//! - No file paths, module names, or code symbols

pub mod generator;
mod noise_filter;
pub mod templates;
pub mod text_cleaner;

pub use noise_filter::filter_embedding_noise;

#[cfg(test)]
mod test;

// Re-export main types
pub use generator::EmbeddingGenerator;
