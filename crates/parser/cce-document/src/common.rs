//! Common utilities for document processing
//!
//! This module provides shared traits, types, and functions to reduce code duplication
//! across different document type processors (JSON, XML, TOML, YAML, Markdown).

pub mod chunker;
pub mod code_block_embedding;
pub mod group;
pub mod node;
pub mod summarizer;
pub mod token_split;
pub mod types;

pub use chunker::{GenericChunker, TwoTierParams, two_tier_chunking};
pub use code_block_embedding::code_block_embedding;
pub use group::GenericGroup;
pub use node::DocumentNode;
pub use types::MergingConfig;
