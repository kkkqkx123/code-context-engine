//! Main AST to Natural Language converter
//!
//! This module provides the main converter that orchestrates BM25 and Embedding
//! generators to produce dual-path output for code search.
//!
//! # Responsibilities
//!
//! The converter is responsible for **semantic transformation** - converting structured
//! code entities (AST) into natural language representations. It focuses on:
//!
//! - **Content Generation**: What to say about each entity
//! - **Output Mode Handling**: Generating BM25 (keyword-rich) or Embedding (semantic) text
//!
//! # Pipeline Position
//!
//! ```text
//! EntityGroup → [Converter] → ConversionResult → [Chunker] → ChunkedResult
//! ```
//!
//! The converter outputs `ConversionResult` objects containing natural language text,
//! which are then passed to the chunker for segmentation.
//!
//! # Key Types
//!
//! - [`AstToNlConverter`]: Main converter orchestrator
//! - [`GroupConversions`]: Groups with their associated conversions (header + members)
//! - [`ConversionResult`]: Output containing bm25_text and/or embedding_text
//!
//! # Usage
//!
//! ```no_run
//! use cce_parser::ast_to_nl::converter::AstToNlConverter;
//! use cce_config::AstToNlConfig;
//!
//! let config = AstToNlConfig::default();
//! let converter = AstToNlConverter::with_config(&config);
//!
//! // Convert entity groups to natural language
//! // let group_conversions = converter.convert_entity_groups(&groups, file_path, None);
//!
//! // Pass results to chunker for segmentation
//! // let chunks = chunker.chunk_groups(&group_conversions, file_path);
//! ```

mod entity_converter;
pub mod group_converter;
mod index_enrichment;

#[cfg(test)]
mod test;

pub use group_converter::GroupConversions;

use crate::ast_to_nl::ConversionRequest;
use crate::ast_to_nl::bm25::generator::Bm25Generator;
use crate::ast_to_nl::embedding::generator::EmbeddingGenerator;
use crate::ast_to_nl::embedding::text_cleaner::EmbeddingTextCleaner;
use crate::plugin::PluginRegistry;
use cce_config::AstToNlConfig;
use cce_text::Bm25TextCleaner;
use cce_types::OutputMode;

/// AST to Natural Language converter
///
/// Supports dual-path output:
/// - BM25: Hybrid enhanced text with key entities preserved
/// - Embedding: Pure semantic summary without code symbols
pub struct AstToNlConverter {
    config: AstToNlConfig,
    bm25_generator: Bm25Generator,
    embedding_generator: EmbeddingGenerator,
    plugin_registry: Option<std::sync::Arc<PluginRegistry>>,
    pub(crate) bm25_cleaner: Bm25TextCleaner,
    pub(crate) embedding_cleaner: EmbeddingTextCleaner,
}

impl AstToNlConverter {
    /// Create a new converter with configuration
    pub fn with_config(config: &AstToNlConfig) -> Self {
        Self {
            config: config.clone(),
            bm25_generator: Bm25Generator::with_config(&config.bm25),
            embedding_generator: EmbeddingGenerator::with_config(&config.embedding),
            plugin_registry: None,
            bm25_cleaner: Bm25TextCleaner::with_config(config.text_cleaner.clone()),
            embedding_cleaner: EmbeddingTextCleaner::new(),
        }
    }

    /// Set plugin registry
    pub fn with_plugin_registry(mut self, plugin_registry: std::sync::Arc<PluginRegistry>) -> Self {
        self.plugin_registry = Some(plugin_registry);
        self
    }

    /// Create a new converter with default configuration
    pub fn new() -> Self {
        Self::with_config(&AstToNlConfig::default())
    }

    /// Get the output mode based on request and config
    fn resolve_mode(&self, request: Option<&ConversionRequest>) -> OutputMode {
        request
            .and_then(|r| r.force_mode)
            .unwrap_or(self.config.default_mode)
    }
}

impl Default for AstToNlConverter {
    fn default() -> Self {
        Self::new()
    }
}
