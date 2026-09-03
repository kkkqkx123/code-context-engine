//! Plugin document format pipeline
//!
//! Processes a [`PluginDocument`] produced by the `FormatParse` capability
//! into standard [`ChunkedResult`]s (`ChunkContentType::Document`).
//!
//! The pipeline is deliberately simple: each plugin entity becomes its own
//! document group whose NL text is derived from the entity's name, signature,
//! doc comment, and (recursively) its children. Chunking reuses the document
//! pipeline's `two_tier_chunking` so the storage layer and alignment semantics
//! are identical to built-in formats.

use cce_config::modules::ChunkingConfig;
use cce_types::ast_to_nl::options::OutputMode;
use cce_types::{ChunkedResult, PluginDocument, PluginEntity};
use cce_utils::token_estimation::TokenEstimator;

use crate::common::chunker::{TwoTierParams, two_tier_chunking};
use crate::types::DocumentClassification;
use cce_types::GroupType;

/// Pipeline that turns a plugin-parsed document into chunks.
#[derive(Debug, Clone)]
pub struct PluginDocumentPipeline {
    config: ChunkingConfig,
    estimator: TokenEstimator,
}

impl PluginDocumentPipeline {
    /// Create a new pipeline for the given chunking config.
    pub fn new(config: ChunkingConfig) -> Self {
        Self {
            config,
            estimator: TokenEstimator::default(),
        }
    }

    /// Process a plugin document into chunks.
    ///
    /// The classification is derived once from the unified detection chain so
    /// a plugin handling `.proto` files produces `Schema`-labelled chunks
    /// exactly like the built-in pipelines would.
    pub fn process(
        &self,
        doc: &PluginDocument,
        file_path: &str,
        output_mode: OutputMode,
    ) -> Vec<ChunkedResult> {
        let classification = DocumentClassification::detect(file_path);
        let mut chunks = Vec::new();
        for entity in &doc.entities {
            chunks.extend(self.chunk_entity(entity, file_path, output_mode, &classification));
        }
        chunks
    }

    fn chunk_entity(
        &self,
        entity: &PluginEntity,
        file_path: &str,
        output_mode: OutputMode,
        classification: &DocumentClassification,
    ) -> Vec<ChunkedResult> {
        let text = Self::entity_text(entity);
        let group_id = format!("plugin_{}_{}", entity.kind, entity.id);
        let span = entity.span.unwrap_or_default();
        two_tier_chunking(TwoTierParams {
            embedding_text: &text,
            bm25_text: &text,
            source_span: span,
            source_group_id: &group_id,
            file_path,
            config: &self.config,
            estimator: &self.estimator,
            group_type: GroupType::Standalone,
            bm25_title: if entity.name.is_empty() {
                None
            } else {
                Some(entity.name.clone())
            },
            output_mode,
            content_type: classification.payload().clone(),
            file_category: classification.category(),
        })
    }

    /// Derive the NL text for an entity (name + signature + doc comment + children).
    fn entity_text(entity: &PluginEntity) -> String {
        let mut parts = Vec::new();
        if !entity.name.is_empty() {
            parts.push(entity.name.clone());
        }
        if let Some(sig) = &entity.signature {
            parts.push(sig.clone());
        }
        if let Some(doc) = &entity.doc_comment {
            parts.push(doc.clone());
        }
        for child in &entity.children {
            parts.push(Self::entity_text(child));
        }
        parts
            .into_iter()
            .filter(|p| !p.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}
