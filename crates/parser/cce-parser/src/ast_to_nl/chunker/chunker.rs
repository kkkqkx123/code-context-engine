//! Group-level chunker
//!
//! Main chunking implementation for entity groups.
//!
//! # Input Contract
//!
//! This module expects `ConversionResult` objects from the converter with:
//! - Valid UTF-8 text in both `bm25_text` and `embedding_text` fields
//! - Aligned dual-path content (same semantic meaning, different representations)
//! - Accurate word counts when provided
//!
//! # Output Guarantees
//!
//! The chunker guarantees:
//! - All chunks respect configured token and word limits
//! - UTF-8 safety in all text operations
//! - Proper fragment metadata for multi-chunk entities
//! - Context preservation through overlaps and header repetition
//!
//! # Policy vs. Mechanism
//!
//! - **Policy** (when to split): Decided by `GroupChunker::chunk_group`
//! - **Mechanism** (how to split): Delegated to `TextSplitter`
//!
//! This separation allows flexible policy decisions while keeping splitting logic reusable.

use crate::grouper::EntityGroup;
use cce_config::modules::GlossaryScope;
use cce_plugin::CodePlugin;
use cce_plugin::vocabulary::{FileVocabulary, InjectionLimits, VocabularyStats, inject_chunks};
use cce_types::ConversionResult;
use cce_types::ParseError;
use cce_types::Span;
use cce_types::entity::EntityId;
use cce_utils::token_estimation::TokenEstimator;
use std::sync::Arc;

/// Output of one chunking unit (a single text path, a group, or a whole file).
///
/// `dropped_blank_segments` counts segments the splitter produced as
/// whitespace-only and had to drop: content loss that must surface as a
/// degraded file rather than stay invisible behind a warn log.
///
/// `vocabulary` counts the domain terminology the chunker placed into the
/// chunks and the terminology that did not fit into the reserved room.
#[derive(Debug, Default)]
pub struct ChunkOutput {
    pub chunks: Vec<ChunkedResult>,
    pub dropped_blank_segments: usize,
    pub vocabulary: VocabularyStats,
}

impl ChunkOutput {
    pub(crate) fn with_chunks(chunks: Vec<ChunkedResult>) -> Self {
        Self {
            chunks,
            dropped_blank_segments: 0,
            vocabulary: VocabularyStats::default(),
        }
    }

    pub(crate) fn absorb(&mut self, other: ChunkOutput) {
        self.chunks.extend(other.chunks);
        self.dropped_blank_segments += other.dropped_blank_segments;
        self.vocabulary.absorb(other.vocabulary);
    }
}

/// (Before-builtin, below-builtin fallback) override-tier chunks.
type OverridePlugins = (Vec<Arc<dyn CodePlugin>>, Vec<Arc<dyn CodePlugin>>);

use super::boundary::{NlEntityBoundary, SplitReason};
use super::chunk_builder::{
    ChunkBuilder, SegmentHeaderContext, SingleChunkContext, UnsplitContext,
};
use super::config::ChunkingConfig;
use super::overlap::OverlapManager;
use super::result::{ChunkPath, ChunkedResult};
use super::splitter::TextSplitter;
use super::strategy::SplitStrategy;
use super::tracker::GroupTracker;

/// Embedding-path chunk wrapper.
///
/// Type-system guarantee: only embedding chunks can enter cross-group merge
/// and overlap passes. BM25 chunks are excluded at compile time.
#[derive(Debug, Clone)]
pub struct EmbeddingChunk(pub ChunkedResult);

impl EmbeddingChunk {
    /// Create a new embedding chunk wrapper
    pub fn new(chunk: ChunkedResult) -> Self {
        debug_assert_eq!(chunk.path, ChunkPath::Embedding);
        Self(chunk)
    }

    /// Unwrap the inner chunk
    pub fn into_inner(self) -> ChunkedResult {
        self.0
    }

    /// Get a reference to the inner chunk
    pub fn inner(&self) -> &ChunkedResult {
        &self.0
    }
}

/// BM25-path chunk wrapper.
///
/// Type-system guarantee: BM25 chunks never enter cross-group merge or overlap
/// passes. They are kept separate from embedding chunks at compile time.
#[derive(Debug, Clone)]
pub struct Bm25Chunk(pub ChunkedResult);

impl Bm25Chunk {
    /// Create a new BM25 chunk wrapper
    pub fn new(chunk: ChunkedResult) -> Self {
        debug_assert_eq!(chunk.path, ChunkPath::Bm25);
        Self(chunk)
    }

    /// Unwrap the inner chunk
    pub fn into_inner(self) -> ChunkedResult {
        self.0
    }

    /// Get a reference to the inner chunk
    pub fn inner(&self) -> &ChunkedResult {
        &self.0
    }
}

/// Group-level chunker
pub struct GroupChunker {
    config: ChunkingConfig,
    estimator: TokenEstimator,
    overlap_manager: OverlapManager,
    /// Plugin registry for the `Chunk` override capability and the
    /// `Vocabulary` terminology table.
    plugin_registry: Option<std::sync::Arc<crate::plugin::PluginRegistry>>,
}

impl GroupChunker {
    /// Create new GroupChunker
    pub fn new(config: ChunkingConfig) -> Self {
        Self {
            overlap_manager: OverlapManager::new(config.clone()),
            config,
            estimator: TokenEstimator::default(),
            plugin_registry: None,
        }
    }

    /// Attach a plugin registry so `Chunk`-capability plugins can override
    /// the built-in chunking and `Vocabulary` plugins can contribute
    /// terminology.
    pub fn with_plugin_registry(
        mut self,
        plugin_registry: std::sync::Arc<crate::plugin::PluginRegistry>,
    ) -> Self {
        self.plugin_registry = Some(plugin_registry);
        self
    }

    /// The terminology that applies to one file.
    ///
    /// Returns `None` when no plugin contributes a terminology table to the
    /// file, or when the table that applies to the file carries no entry.
    fn terminology(&self, file_path: &str, language: Option<&str>) -> Option<Arc<FileVocabulary>> {
        let registry = self.plugin_registry.as_ref()?;
        let vocabulary = registry.vocabulary_for(file_path, language)?;
        if vocabulary.is_empty() {
            return None;
        }
        Some(vocabulary)
    }

    /// The configuration used to split one group.
    ///
    /// Files that carry a terminology table reserve room for the injected
    /// expansions. Every group additionally reserves the decoration its
    /// chunks will carry (identity lines, topic lines, synthetic block
    /// heads), so the splitter works on a body limit and the assembly does
    /// not push the final chunk over the pre-injection limit.
    fn group_config(
        &self,
        group: &EntityGroup,
        file_path: &str,
        language: Option<&str>,
    ) -> (ChunkingConfig, usize, usize) {
        let has_vocabulary = self.terminology(file_path, language).is_some();
        let embedding_decoration = ChunkBuilder::decoration_allowance(group, ChunkPath::Embedding);
        let bm25_decoration = ChunkBuilder::decoration_allowance(group, ChunkPath::Bm25);
        let config = self
            .config
            .body_config(has_vocabulary, embedding_decoration, bm25_decoration);
        let embedding_limit = self
            .config
            .pre_injection_limit(ChunkPath::Embedding, has_vocabulary);
        let bm25_limit = self
            .config
            .pre_injection_limit(ChunkPath::Bm25, has_vocabulary);
        (config, embedding_limit, bm25_limit)
    }

    /// Inject the domain terminology into every chunk that uses a term.
    ///
    /// The chunk is the unit that is actually retrieved: a BM25 chunk is one
    /// document and an embedding chunk is one vector. Matching the chunk's
    /// own text therefore puts exactly the expansions the chunk needs in
    /// front of the retriever, and no chunk carries the terms of its
    /// neighbours.
    ///
    /// The expansions are placed at the first occurrence of the term, never
    /// in place of the term itself. The reserved room bounds how much the
    /// injection may add, so the chunk keeps its budget.
    ///
    /// The chunk scope expands a term in every chunk that mentions it. The
    /// entity scope expands it once per entity, in the chunk that holds the
    /// first occurrence, so the expansion does not repeat across the chunks
    /// of one entity.
    fn inject_vocabulary(
        &self,
        chunks: &mut [ChunkedResult],
        vocabulary: &FileVocabulary,
    ) -> VocabularyStats {
        let per_entity = self.config.glossary_scope == GlossaryScope::Entity;
        inject_chunks(
            chunks,
            vocabulary,
            per_entity,
            self.config.glossary_max_entries,
            InjectionLimits {
                bm25: self.config.limit(ChunkPath::Bm25),
                embedding: self.config.limit(ChunkPath::Embedding),
            },
        )
    }

    /// Process entity group with independent dual-path splitting
    ///
    /// Produces two independent sets of chunks:
    /// - **BM25 path**: splits `bm25_text` by `max_bm25_terms` using entity-aware strategies
    /// - **Embedding path**: splits `embedding_text` by `max_tokens` using token-aware splitting
    ///
    /// Each path uses its own text and limits, producing separate chunks with
    /// path-specific `chunk_id` format: `{groupId}_{path}_{index}`.
    pub fn chunk_group(
        &self,
        group: &EntityGroup,
        conversion: &ConversionResult,
        file_path: &str,
    ) -> Result<ChunkOutput, ParseError> {
        let mut tracker = GroupTracker::new();
        tracker.record_group(group);

        let bm25_text = conversion.bm25_text.clone().unwrap_or_default();
        let embedding_text = conversion.embedding_text.clone().unwrap_or_default();

        let mut all_chunks = ChunkOutput::default();

        let language = group.language.to_string();
        let (config, embedding_limit, bm25_limit) =
            self.group_config(group, file_path, Some(&language));
        let splitter = TextSplitter::new(config.clone());
        let infra = ChunkInfrastructure {
            config: &config,
            estimator: &self.estimator,
            splitter: &splitter,
            pre_injection_embedding: embedding_limit,
            pre_injection_bm25: bm25_limit,
        };

        if !bm25_text.trim().is_empty() {
            // The text belongs to the converted entity alone, so its region
            // is the whole text. Splitting it at name occurrences would hand
            // the text after an incidental mention to whichever entity owns
            // that name.
            let nl_boundaries = super::boundary::compute_nl_entity_boundaries(
                &bm25_text,
                Some(conversion.entity_id),
                &[],
                ChunkPath::Bm25,
            );
            let input = ChunkInput {
                infra: &infra,
                group,
                text: &bm25_text,
                file_path,
                keywords: &conversion.keywords,
                path: ChunkPath::Bm25,
                strategy: SplitStrategy::ByNlEntityBoundaries,
                nl_boundaries: Some(nl_boundaries.as_slice()),
                tracker: &tracker,
                header_mode: None,
            };
            all_chunks.absorb(chunk_single_path(input)?);
        }

        let bm25_count = all_chunks.chunks.len();

        if !embedding_text.trim().is_empty() {
            let nl_boundaries = super::boundary::compute_nl_entity_boundaries(
                &embedding_text,
                Some(conversion.entity_id),
                &[],
                ChunkPath::Embedding,
            );
            let input = ChunkInput {
                infra: &infra,
                group,
                text: &embedding_text,
                file_path,
                keywords: &conversion.keywords,
                path: ChunkPath::Embedding,
                strategy: SplitStrategy::ByNlEntityBoundaries,
                nl_boundaries: Some(nl_boundaries.as_slice()),
                tracker: &tracker,
                header_mode: None,
            };
            all_chunks.absorb(chunk_single_path(input)?);
        }

        // The expansions are placed before the overlap, so the overlap a
        // chunk carries points into the text the retriever sees.
        if let Some(vocabulary) = self.terminology(file_path, Some(&language)) {
            let stats = self.inject_vocabulary(&mut all_chunks.chunks, &vocabulary);
            all_chunks.vocabulary.absorb(stats);
        }
        enforce_hard_limit(&mut all_chunks.chunks, &self.config)?;

        let total = all_chunks.chunks.len();
        if bm25_count < total {
            self.overlap_manager
                .apply_overlap(&mut all_chunks.chunks[bm25_count..], ChunkPath::Embedding);
        }

        add_relations(&tracker, &mut all_chunks.chunks);

        Ok(all_chunks)
    }

    /// Process entity group with multiple conversions
    ///
    /// This method handles the case where a group produces multiple conversion results
    /// (e.g., class description + method descriptions). It implements smart chunking:
    /// - Header (group overview) is included only in the first chunk for context
    /// - Members are grouped by token budget to avoid oversized chunks
    pub fn chunk_group_with_conversions(
        &self,
        group: &EntityGroup,
        group_conversions: &crate::ast_to_nl::converter::GroupConversions,
        file_path: &str,
    ) -> Result<ChunkOutput, ParseError> {
        let language = group.language.to_string();
        let (config, embedding_limit, bm25_limit) =
            self.group_config(group, file_path, Some(&language));
        let splitter = TextSplitter::new(config.clone());
        let infra = ChunkInfrastructure {
            config: &config,
            estimator: &self.estimator,
            splitter: &splitter,
            pre_injection_embedding: embedding_limit,
            pre_injection_bm25: bm25_limit,
        };
        let mut tracker = GroupTracker::new();
        let mut output = crate::ast_to_nl::chunker::header_chunk::chunk_group_with_conversions(
            &infra,
            &mut tracker,
            group,
            group_conversions,
            file_path,
        )?;
        // The header path owns no injection of its own: the single-group
        // entry point must place expansions exactly like `chunk_group` and
        // `chunk_groups_builtin` do — after splitting, before overlap — or a
        // caller that chunks one group at a time silently loses terminology.
        if let Some(vocabulary) = self.terminology(file_path, Some(&language)) {
            let stats = self.inject_vocabulary(&mut output.chunks, &vocabulary);
            output.vocabulary.absorb(stats);
        }
        enforce_hard_limit(&mut output.chunks, &self.config)?;
        let first_emb = output
            .chunks
            .iter()
            .position(|c| c.path == ChunkPath::Embedding)
            .unwrap_or(output.chunks.len());
        if first_emb < output.chunks.len() {
            self.overlap_manager
                .apply_overlap(&mut output.chunks[first_emb..], ChunkPath::Embedding);
        }
        Ok(output)
    }

    /// Batch process groups
    ///
    /// Each group may have multiple conversion results (e.g., class + methods).
    /// This method properly handles the one-to-many relationship.
    ///
    /// After chunking all groups, performs a cross-group merge pass to combine
    /// small chunks with adjacent ones, producing more uniformly sized output.
    pub fn chunk_groups(
        &self,
        group_conversions: &[crate::ast_to_nl::converter::GroupConversions],
        file_path: &str,
    ) -> Result<ChunkOutput, ParseError> {
        // Plugin `Chunk` override: the first matching plugin replaces the
        // built-in chunker entirely. The returned chunks must follow the
        // standard `ChunkedResult` shape.
        //
        // Three-tier order: override-tier plugins (priority >= 0) → built-in
        // chunker → below-builtin fallback plugins (negative priority, only
        // when the built-in produced no chunks).
        let language = group_conversions
            .first()
            .map(|gc| gc.group.language.to_string());
        let override_plugins: Option<OverridePlugins> =
            self.plugin_registry.as_ref().map(|registry| {
                let (above, below) = registry.get_override_plugins(
                    cce_plugin::PluginCapability::Chunk,
                    Some(file_path),
                    language.as_deref(),
                );
                (
                    above.into_iter().cloned().collect(),
                    below.into_iter().cloned().collect(),
                )
            });

        let mut output = if let Some((above, below)) = override_plugins {
            if let Some(chunks) = self.try_plugin_chunk(group_conversions, file_path, &above) {
                let mut output = ChunkOutput::with_chunks(chunks);
                self.inject_into_output(&mut output, file_path, language.as_deref());
                output
            } else {
                let builtin = self.chunk_groups_builtin(group_conversions, file_path)?;
                if builtin.chunks.is_empty() {
                    match self.try_plugin_chunk(group_conversions, file_path, &below) {
                        Some(chunks) => {
                            let mut output = ChunkOutput::with_chunks(chunks);
                            self.inject_into_output(&mut output, file_path, language.as_deref());
                            output
                        }
                        None => builtin,
                    }
                } else {
                    builtin
                }
            }
        } else {
            self.chunk_groups_builtin(group_conversions, file_path)?
        };
        enforce_hard_limit(&mut output.chunks, &self.config)?;

        Ok(output)
    }

    /// Inject the domain terminology into the chunks of an output.
    ///
    /// The built-in chunker places the expansions itself, between the merge
    /// and the overlap pass. A plugin that produced the chunks instead owns
    /// the whole text, so the host places the expansions here.
    fn inject_into_output(
        &self,
        output: &mut ChunkOutput,
        file_path: &str,
        language: Option<&str>,
    ) {
        let Some(table) = self.terminology(file_path, language) else {
            return;
        };
        let stats = self.inject_vocabulary(&mut output.chunks, &table);
        output.vocabulary.absorb(stats);
    }

    /// Chunk groups with the built-in chunker (merge + overlap passes).
    fn chunk_groups_builtin(
        &self,
        group_conversions: &[crate::ast_to_nl::converter::GroupConversions],
        file_path: &str,
    ) -> Result<ChunkOutput, ParseError> {
        let language = group_conversions
            .first()
            .map(|gc| gc.group.language.to_string());
        let has_vocabulary = self.terminology(file_path, language.as_deref()).is_some();
        let merge_config = self.config.pre_injection_config(has_vocabulary);
        let mut tracker = GroupTracker::new();
        let mut all_chunks = ChunkOutput::default();
        let groups: Vec<EntityGroup> = group_conversions
            .iter()
            .map(|gc| gc.group.clone())
            .collect();

        // Pre-register every group identity before chunking: parent
        // qualification must not depend on the accidental processing order
        // of sibling groups. Registration alone creates no relations.
        for group in &groups {
            tracker.register_identity(group);
        }

        for gc in group_conversions {
            let (config, embedding_limit, bm25_limit) =
                self.group_config(&gc.group, file_path, language.as_deref());
            let splitter = TextSplitter::new(config.clone());
            let infra = ChunkInfrastructure {
                config: &config,
                estimator: &self.estimator,
                splitter: &splitter,
                pre_injection_embedding: embedding_limit,
                pre_injection_bm25: bm25_limit,
            };
            let chunks = crate::ast_to_nl::chunker::header_chunk::chunk_group_with_conversions(
                &infra,
                &mut tracker,
                &gc.group,
                gc,
                file_path,
            )?;
            all_chunks.absorb(chunks);
        }

        let group_spans: std::collections::HashMap<String, Span> = groups
            .iter()
            .map(|g| (g.group_id.to_string(), g.span))
            .collect();

        let emb_chunks: Vec<EmbeddingChunk> = all_chunks
            .chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Embedding)
            .cloned()
            .map(EmbeddingChunk::new)
            .collect();
        let bm25_chunks: Vec<Bm25Chunk> = all_chunks
            .chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Bm25)
            .cloned()
            .map(Bm25Chunk::new)
            .collect();
        let dropped_blank_segments = all_chunks.dropped_blank_segments;

        // Cross-group merging and word overlap stay on the embedding path only.
        // Merging BM25 chunks across groups would mix independent entities
        // into one block, and overlap would repeat content across blocks.
        // The type system guarantees Bm25Chunk cannot enter this path.
        let emb_chunks_inner: Vec<ChunkedResult> =
            emb_chunks.into_iter().map(|c| c.into_inner()).collect();
        let merged_emb_chunks = super::merge::merge_small_chunks_cross_group(
            emb_chunks_inner,
            &group_spans,
            &merge_config,
        );

        let mut emb_results: Vec<ChunkedResult> = merged_emb_chunks;
        let mut bm25_results: Vec<ChunkedResult> =
            bm25_chunks.into_iter().map(|c| c.into_inner()).collect();

        // The expansions are placed before the overlap, so the overlap a
        // chunk carries points into the text the retriever sees.
        let mut vocabulary = VocabularyStats::default();
        if let Some(table) = self.terminology(file_path, language.as_deref()) {
            vocabulary = self.inject_vocabulary(&mut emb_results, &table);
            vocabulary.absorb(self.inject_vocabulary(&mut bm25_results, &table));
        }

        let mut emb_chunks: Vec<EmbeddingChunk> =
            emb_results.into_iter().map(EmbeddingChunk::new).collect();
        self.overlap_manager.apply_overlap_emb(&mut emb_chunks);

        let total_chunks = emb_chunks.len() + bm25_results.len();

        let mut result = Vec::with_capacity(total_chunks);
        result.extend(emb_chunks.into_iter().map(|c| c.into_inner()));
        result.extend(bm25_results);
        Ok(ChunkOutput {
            chunks: result,
            dropped_blank_segments,
            vocabulary,
        })
    }

    /// Try to satisfy `chunk_groups` via a `Chunk`-capability plugin.
    ///
    /// Returns `Some` only when a plugin matched the file (by pattern) and
    /// returned a non-empty chunk list.
    fn try_plugin_chunk(
        &self,
        group_conversions: &[crate::ast_to_nl::converter::GroupConversions],
        file_path: &str,
        plugins: &[Arc<dyn CodePlugin>],
    ) -> Option<Vec<ChunkedResult>> {
        use tracing::warn;

        if plugins.is_empty() {
            return None;
        }

        let conversions: Vec<crate::ast_to_nl::converter::GroupConversions> =
            group_conversions.to_vec();
        for plugin in plugins {
            match plugin.chunk(conversions.clone(), file_path) {
                Ok(Some(chunks)) if !chunks.is_empty() => return Some(chunks),
                Ok(_) => {}
                Err(e) => {
                    warn!(
                        plugin = %plugin.metadata().id,
                        file_path = %file_path,
                        error = %e,
                        "Chunk plugin failed; falling back to built-in chunker"
                    );
                }
            }
        }
        None
    }

    /// Get config
    pub fn config(&self) -> &ChunkingConfig {
        &self.config
    }
}

impl Default for GroupChunker {
    fn default() -> Self {
        Self::new(ChunkingConfig::default())
    }
}

/// Shared infrastructure for chunking operations.
///
/// Groups the three core services that are always passed together
/// through the chunking pipeline.
pub struct ChunkInfrastructure<'a> {
    pub config: &'a ChunkingConfig,
    pub estimator: &'a TokenEstimator,
    pub splitter: &'a TextSplitter,
    /// The limit a chunk must satisfy before terminology injection.
    pub pre_injection_embedding: usize,
    /// The limit a BM25 chunk must satisfy before terminology injection.
    pub pre_injection_bm25: usize,
}

impl ChunkInfrastructure<'_> {
    fn pre_injection_limit(&self, path: ChunkPath) -> usize {
        match path {
            ChunkPath::Bm25 => self.pre_injection_bm25,
            ChunkPath::Embedding => self.pre_injection_embedding,
        }
    }
}

/// Input parameters for chunking a single text path.
pub struct ChunkInput<'a> {
    pub infra: &'a ChunkInfrastructure<'a>,
    pub group: &'a EntityGroup,
    pub text: &'a str,
    pub file_path: &'a str,
    pub keywords: &'a [String],
    pub path: ChunkPath,
    pub strategy: SplitStrategy,
    pub nl_boundaries: Option<&'a [NlEntityBoundary]>,
    /// File-level group history for cross-group parent qualification.
    /// The plain single-path flow must use the caller-owned tracker, never
    /// a fresh empty one, or standalone child groups lose their parent.
    pub tracker: &'a GroupTracker,
    /// Header-specific parameters; `None` for the plain single-path flow.
    pub header_mode: Option<HeaderPathParams<'a>>,
}

/// Header-path parameters for `chunk_single_path`.
///
/// When present, `text` is a header + member combination and the chunk is
/// built with header semantics: the header entity is context (excluded from
/// content attribution), it joins the source coverage of the first chunk
/// only, and the unsplit fallback carries member-group metadata.
pub struct HeaderPathParams<'a> {
    pub tracker: &'a GroupTracker,
    pub header_entity_id: Option<EntityId>,
    /// Entity ids of the members that contributed text, in assembly order.
    pub member_entity_ids: &'a [EntityId],
    pub chunk_index: usize,
    pub total_chunks: usize,
    pub include_header_in_first_coverage: bool,
    /// Topic lines of the entities this batch covers, keyed by entity id.
    pub topic_lines: &'a [(EntityId, Vec<String>)],
}

/// Chunk a single text path (BM25 or Embedding) independently
///
/// Unified orchestration for both the plain single-path flow and the
/// header-path flow (header + members): check limits, split, enforce segment
/// limits, then build chunks.
pub fn chunk_single_path(input: ChunkInput) -> Result<ChunkOutput, ParseError> {
    let ChunkInput {
        infra,
        group,
        text,
        file_path,
        keywords,
        path,
        strategy,
        nl_boundaries,
        tracker,
        header_mode,
    } = input;
    let word_count = infra.config.cost(text, ChunkPath::Bm25);

    let needs_split =
        infra.config.limit(path) > 0 && infra.config.cost(text, path) > infra.config.limit(path);

    if !needs_split {
        // The plain path shares the file-level tracker; the header path
        // shares the tracker carried by its header parameters.
        let plain_tracker: &GroupTracker = tracker;
        let active: &GroupTracker = header_mode.as_ref().map_or(plain_tracker, |h| h.tracker);
        let builder = ChunkBuilder::new();
        let chunks = if let Some(header) = &header_mode {
            vec![build_unsplit_header_chunk(
                &builder, active, header, group, file_path, path, text, word_count, keywords,
            )]
        } else {
            vec![builder.from_single_text(
                active,
                SingleChunkContext {
                    group,
                    file_path,
                    path,
                    text,
                    keywords,
                },
            )]
        };
        return Ok(ChunkOutput::with_chunks(enforce_pre_injection_limit(
            chunks,
            infra.pre_injection_limit(path),
            path,
            infra.config,
            infra.estimator,
        )?));
    }

    let segments = if let Some(nl) = nl_boundaries {
        infra
            .splitter
            .split_with_nl_boundaries(text, group, strategy, path, nl)?
    } else {
        infra.splitter.split(text, group, strategy, path)?
    };

    let segments = super::segment_limit::enforce_segment_max_limit(
        segments,
        path,
        infra,
        group,
        nl_boundaries.unwrap_or_default(),
    )?;

    // Over-limit text must always produce at least one segment; empty output
    // means content was silently lost, which the splitter invariants forbid.
    // This is a deterministic splitter bug for the given content, so it is
    // reported as a permanent `CodeSplitting` failure instead of a panic
    // (which would bypass error classification and be retried as transient).
    if segments.is_empty() {
        return Err(ParseError::code_splitting(format!(
            "over-limit text produced no segments (path={path}, strategy={strategy:?}, group={})",
            group.group_id
        )));
    }

    // A chunk text must carry real content: whitespace-only text would be
    // rejected by embedding providers with an opaque 400 and pollutes the
    // index. Callers gate on `trim().is_empty()`; this catches segments that
    // are still blank after splitting (a splitter/converter bug). The check
    // is enforced in release builds too: blank segments must never reach
    // storage, and the drop count is reported so the file is marked degraded.
    let blank_count = segments.iter().filter(|s| s.text.trim().is_empty()).count();
    let segments = if blank_count > 0 {
        tracing::warn!(
            path = ?path,
            group = %group.group_id,
            blank = blank_count,
            "Dropping blank segments produced by splitting (splitter/converter bug)"
        );
        segments
            .into_iter()
            .filter(|s| !s.text.trim().is_empty())
            .collect::<Vec<_>>()
    } else {
        segments
    };
    if segments.is_empty() {
        return Ok(ChunkOutput {
            chunks: Vec::new(),
            dropped_blank_segments: blank_count,
            vocabulary: VocabularyStats::default(),
        });
    }

    let active: &GroupTracker = header_mode.as_ref().map_or(tracker, |h| h.tracker);
    let builder = ChunkBuilder::new();

    let header_ctx = header_mode.as_ref().map(|h| SegmentHeaderContext {
        header_entity_id: h.header_entity_id,
        include_header_in_first_coverage: h.include_header_in_first_coverage,
        topic_lines: h.topic_lines,
    });

    let chunks = builder.from_segments(
        active,
        &segments,
        path,
        group,
        file_path,
        keywords,
        nl_boundaries.unwrap_or_default(),
        header_ctx,
    );
    Ok(ChunkOutput {
        chunks: enforce_pre_injection_limit(
            chunks,
            infra.pre_injection_limit(path),
            path,
            infra.config,
            infra.estimator,
        )?,
        dropped_blank_segments: blank_count,
        vocabulary: VocabularyStats::default(),
    })
}

/// Enforce the pre-injection limit after decoration has been appended.
///
/// The splitter works on the body limit, so the identity and topic lines
/// added during assembly normally fit the remaining room. A signature that
/// is longer than the allowance, or a configuration whose allowance is
/// exhausted, can still produce an over-limit chunk. The final text is
/// then split at token/term granularity: correctness of the hard limit is
/// not negotiable, and a safe chunk with a less natural boundary is better
/// than an over-limit chunk that the embedding provider rejects.
fn enforce_pre_injection_limit(
    chunks: Vec<ChunkedResult>,
    limit: usize,
    path: ChunkPath,
    config: &ChunkingConfig,
    estimator: &TokenEstimator,
) -> Result<Vec<ChunkedResult>, ParseError> {
    if limit == 0 || limit == usize::MAX {
        return Ok(chunks);
    }
    let unit = config.unit(path);
    let mut split_config = config.clone();
    match path {
        ChunkPath::Bm25 => split_config.max_bm25_terms = limit,
        ChunkPath::Embedding => split_config.max_tokens = limit,
    }

    let mut output = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        if unit.cost(&chunk.text) <= limit {
            output.push(chunk);
            continue;
        }
        let boundaries = super::strategies::tokens::split_by_tokens_in_range(
            &chunk.text,
            0..chunk.text.len(),
            path,
            &split_config,
            estimator,
        );
        super::splitter::validate_partition(&boundaries, &chunk.text, 0..chunk.text.len())?;
        if boundaries.is_empty() {
            return Err(ParseError::code_splitting(format!(
                "over-limit chunk produced no replacement pieces (path={path}, chunk={})",
                chunk.chunk_id
            )));
        }
        for boundary in boundaries {
            let mut piece = chunk.clone();
            piece.text = chunk.text[boundary.start_byte..boundary.end_byte].to_string();
            piece.token_count = unit.cost(&piece.text);
            if piece.metadata.bm25_word_count.is_some() {
                piece.metadata.bm25_word_count = Some(config.cost(&piece.text, ChunkPath::Bm25));
            }
            if piece.token_count > limit {
                return Err(ParseError::code_splitting(format!(
                    "pre-injection split could not fit the limit (path={path}, chunk={}, cost={}, limit={limit})",
                    piece.chunk_id, piece.token_count
                )));
            }
            piece.prev_overlap = None;
            piece.next_overlap = None;
            output.push(piece);
        }
    }

    let total = output.len();
    for (index, chunk) in output.iter_mut().enumerate() {
        chunk.chunk_index = index;
        chunk.total_chunks = total;
        chunk.chunk_id = format!("{}_{}_{}", chunk.source_group_id, path, index);
        if let Some(code) = chunk.metadata.as_code_mut() {
            code.is_fragment = total > 1;
            code.fragment_index = (total > 1).then_some(index);
            code.total_fragments = (total > 1).then_some(total);
        }
    }
    Ok(output)
}

/// Reject any chunk that still exceeds its configured hard limit.
///
/// Injection is bounded by the absolute final text, so this is a guard
/// against a plugin-produced chunk, an incorrect budget, or a future
/// assembly path that bypasses the pre-injection repair. It runs in release
/// builds as well; a silent overflow is not an acceptable degradation.
fn enforce_hard_limit(
    chunks: &mut [ChunkedResult],
    config: &ChunkingConfig,
) -> Result<(), ParseError> {
    for chunk in chunks {
        let limit = config.limit(chunk.path);
        let actual = config.cost(&chunk.text, chunk.path);
        if limit > 0 && actual > limit {
            return Err(ParseError::code_splitting(format!(
                "chunk exceeds the hard limit (path={}, chunk={}, cost={}, limit={limit})",
                chunk.path, chunk.chunk_id, actual
            )));
        }
        chunk.token_count = actual;
        if chunk.metadata.bm25_word_count.is_some() {
            chunk.metadata.bm25_word_count = Some(config.cost(&chunk.text, ChunkPath::Bm25));
        }
    }
    Ok(())
}

/// Build an unsplit chunk with header semantics (header + members).
#[allow(clippy::too_many_arguments)]
fn build_unsplit_header_chunk(
    builder: &ChunkBuilder,
    tracker: &GroupTracker,
    header: &HeaderPathParams,
    group: &EntityGroup,
    file_path: &str,
    path: ChunkPath,
    text: &str,
    word_count: usize,
    keywords: &[String],
) -> ChunkedResult {
    let content_entity_ids = if header.chunk_index == 0 {
        let mut ids: Vec<EntityId> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for id in header
            .header_entity_id
            .into_iter()
            .chain(header.member_entity_ids.iter().copied())
        {
            if seen.insert(id) {
                ids.push(id);
            }
        }
        ids
    } else {
        header.member_entity_ids.to_vec()
    };
    let context_entity_ids = header.header_entity_id.into_iter().collect();
    let chunk_id = format!("{}_{}_{}", group.group_id, path, header.chunk_index);
    let split_reason = if header.total_chunks > 1 {
        SplitReason::MemberBoundary
    } else {
        SplitReason::NotSplit
    };
    builder.from_unsplit(UnsplitContext {
        group,
        file_path,
        path,
        chunk_id,
        chunk_index: header.chunk_index,
        total_chunks: header.total_chunks,
        text: text.to_string(),
        word_count,
        end_byte: text.len(),
        content_entity_ids,
        context_entity_ids,
        keywords: keywords.to_vec(),
        parent_qualifier: ChunkBuilder::cross_group_parent(group, tracker),
        split_reason,
        related_groups: tracker.get_related_groups(&group.group_id),
    })
}

/// Add relations to chunks
pub fn add_relations(tracker: &GroupTracker, chunks: &mut [ChunkedResult]) {
    if let Some(first) = chunks.first() {
        let group_id = &first.source_group_id;
        let relations = tracker.get_related_groups(group_id);

        for chunk in chunks {
            chunk.related_groups = relations.clone();
        }
    }
}

/// Compute NL entity boundaries for a header + member group.
///
/// The combined text layout is: `header + "\n\n" + member1 + "\n\n" + member2 + ...`
/// This function returns per-entity byte ranges within that combined text,
/// which the splitter can use to split at entity boundaries.
pub fn compute_nl_boundaries_for_group(
    header_text: &str,
    header_entity_id: Option<cce_types::entity::EntityId>,
    members: &[ConversionResult],
    path: ChunkPath,
) -> Vec<NlEntityBoundary> {
    super::boundary::compute_nl_entity_boundaries(header_text, header_entity_id, members, path)
}
