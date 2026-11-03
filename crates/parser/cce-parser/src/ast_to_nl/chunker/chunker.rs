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
use cce_utils::token_estimation::TokenEstimator;
use std::sync::Arc;

// Re-export moved items so existing call sites keep their paths.
pub use super::single_path::{
    ChunkInfrastructure, ChunkInput, HeaderPathParams, add_relations, chunk_single_path,
};

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

use super::boundary::NlEntityBoundary;
use super::chunk_builder::ChunkBuilder;
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

    /// The measured terminology reserve for one group and one path.
    ///
    /// The group text and the vocabulary table are both known before
    /// splitting, so the expansion cost of the terms the group actually
    /// mentions is summed instead of using the proportional estimate. The
    /// sum is capped to a single chunk worth of injections: at most
    /// `glossary_max_entries` terms can land in one chunk, so the most
    /// expensive terms bound the room any chunk of the group needs. Groups
    /// that mention few terms keep more body room; the residual unevenness
    /// across chunks is digested by the exact capping inside the injection.
    pub(crate) fn measured_terminology_reserve(
        &self,
        file_path: &str,
        language: Option<&str>,
        texts: &[&str],
        path: ChunkPath,
    ) -> usize {
        let Some(vocabulary) = self.terminology(file_path, language) else {
            return 0;
        };
        if texts.is_empty() {
            return 0;
        }
        let matched = vocabulary.matched_entries(texts);
        if matched.is_empty() {
            return 0;
        }
        let unit = self.config.unit(path);
        let mut costs: Vec<usize> = matched
            .iter()
            .map(|entry| {
                let block = match path {
                    ChunkPath::Bm25 => format!(" [{} = {}]", entry.token, entry.expansion),
                    ChunkPath::Embedding => format!(" ({})", entry.expansion),
                };
                unit.cost(&block)
            })
            .collect();
        costs.sort_unstable_by(|a, b| b.cmp(a));
        let take = self.config.glossary_max_entries.max(1).min(costs.len());
        let sum: usize = costs.iter().take(take).sum();
        let limit = self.config.limit(path);
        if limit == 0 {
            0
        } else {
            sum.min(limit.saturating_sub(1))
        }
    }

    /// The largest measured terminology reserve across a batch, per path.
    ///
    /// The cross-group merge config is shared by every group of the file, so
    /// it keeps the most conservative room any group needs. The merge inputs
    /// are already verified chunks and the merge ceiling never exceeds the
    /// pre-injection limit, so the merge itself needs no repair.
    pub(crate) fn batch_measured_reserves(
        &self,
        group_conversions: &[crate::ast_to_nl::converter::GroupConversions],
        file_path: &str,
        language: Option<&str>,
    ) -> (usize, usize) {
        let Some(vocabulary) = self.terminology(file_path, language) else {
            return (0, 0);
        };
        let mut emb_max = 0usize;
        let mut bm25_max = 0usize;
        for gc in group_conversions {
            let mut texts: Vec<&str> = Vec::new();
            if let Some(header) = gc.header_conversion.as_ref() {
                if let Some(text) = header.bm25_text.as_deref() {
                    texts.push(text);
                }
                if let Some(text) = header.embedding_text.as_deref() {
                    texts.push(text);
                }
            }
            for member in &gc.member_conversions {
                if let Some(text) = member.bm25_text.as_deref() {
                    texts.push(text);
                }
                if let Some(text) = member.embedding_text.as_deref() {
                    texts.push(text);
                }
            }
            let matched = vocabulary.matched_entries(&texts);
            if matched.is_empty() {
                continue;
            }
            for path in [ChunkPath::Embedding, ChunkPath::Bm25] {
                let unit = self.config.unit(path);
                let mut costs: Vec<usize> = matched
                    .iter()
                    .map(|entry| {
                        let block = match path {
                            ChunkPath::Bm25 => {
                                format!(" [{} = {}]", entry.token, entry.expansion)
                            }
                            ChunkPath::Embedding => format!(" ({})", entry.expansion),
                        };
                        unit.cost(&block)
                    })
                    .collect();
                costs.sort_unstable_by(|a, b| b.cmp(a));
                let take = self.config.glossary_max_entries.max(1).min(costs.len());
                let sum: usize = costs.iter().take(take).sum();
                let limit = self.config.limit(path);
                let capped = if limit == 0 {
                    0
                } else {
                    sum.min(limit.saturating_sub(1))
                };
                match path {
                    ChunkPath::Embedding => emb_max = emb_max.max(capped),
                    ChunkPath::Bm25 => bm25_max = bm25_max.max(capped),
                }
            }
        }
        (emb_max, bm25_max)
    }

    /// The configuration used to split one group.
    ///
    /// Files that carry a terminology table reserve the measured expansion
    /// cost of the terms the group actually mentions instead of the
    /// proportional estimate. Every group additionally reserves the measured
    /// decoration its chunks will carry, so the splitter works on a body
    /// limit and the assembly does not push the final chunk over the
    /// pre-injection limit. The first pass uses the per-group measured
    /// decoration; the rare parent-qualified overflow is covered by the
    /// bounded repair pass instead of by a permanent conservative reserve.
    fn group_config(
        &self,
        group: &EntityGroup,
        file_path: &str,
        language: Option<&str>,
        texts: &[&str],
    ) -> Result<
        (
            ChunkingConfig,
            cce_config::modules::ChunkBudget,
            cce_config::modules::ChunkBudget,
        ),
        ParseError,
    > {
        use cce_config::modules::ChunkBudget;
        let emb_reserve =
            self.measured_terminology_reserve(file_path, language, texts, ChunkPath::Embedding);
        let bm25_reserve =
            self.measured_terminology_reserve(file_path, language, texts, ChunkPath::Bm25);
        let embedding_decoration = ChunkBuilder::decoration_allowance(group, ChunkPath::Embedding);
        let bm25_decoration = ChunkBuilder::decoration_allowance(group, ChunkPath::Bm25);
        let emb_budget: ChunkBudget = self.config.budget_with_reserve(
            ChunkPath::Embedding,
            emb_reserve,
            embedding_decoration,
        );
        let bm25_budget: ChunkBudget =
            self.config
                .budget_with_reserve(ChunkPath::Bm25, bm25_reserve, bm25_decoration);
        for (budget, name) in [(&emb_budget, "embedding"), (&bm25_budget, "bm25")] {
            if budget.hard_limit > 0
                && budget.pre_injection_limit != usize::MAX
                && budget.decoration_cost >= budget.pre_injection_limit
            {
                tracing::warn!(
                    file = file_path,
                    group = group.group_id.as_str(),
                    path = name,
                    decoration = budget.decoration_cost,
                    pre = budget.pre_injection_limit,
                    "decoration reaches the pre-injection budget; keeping a minimal body and relying on hard-limit truncation"
                );
            }
        }
        let config = self.config.clone();
        Ok((config, emb_budget, bm25_budget))
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
        let group_texts = [bm25_text.as_str(), embedding_text.as_str()];
        let (config, emb_budget, bm25_budget) =
            self.group_config(group, file_path, Some(&language), &group_texts)?;
        let splitter = TextSplitter::with_budgets(config.clone(), emb_budget, bm25_budget);
        let infra = super::single_path::ChunkInfrastructure {
            config: &config,
            estimator: &self.estimator,
            splitter: &splitter,
            pre_injection_embedding: emb_budget.pre_injection_limit,
            pre_injection_bm25: bm25_budget.pre_injection_limit,
            emb_budget,
            bm25_budget,
            has_vocabulary: emb_budget.reserve > 0 || bm25_budget.reserve > 0,
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
            let input = super::single_path::ChunkInput {
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
            all_chunks.absorb(super::single_path::chunk_single_path(input)?);
        }

        if !embedding_text.trim().is_empty() {
            let nl_boundaries = super::boundary::compute_nl_entity_boundaries(
                &embedding_text,
                Some(conversion.entity_id),
                &[],
                ChunkPath::Embedding,
            );
            let input = super::single_path::ChunkInput {
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
            all_chunks.absorb(super::single_path::chunk_single_path(input)?);
        }

        // The expansions are placed before the overlap, so the overlap a
        // chunk carries points into the text the retriever sees.
        if let Some(vocabulary) = self.terminology(file_path, Some(&language)) {
            let stats = self.inject_vocabulary(&mut all_chunks.chunks, &vocabulary);
            all_chunks.vocabulary.absorb(stats);
        }
        super::limits::enforce_hard_limit(&mut all_chunks.chunks, &self.config, file_path)?;

        let first_emb = all_chunks
            .chunks
            .iter()
            .position(|c| c.path == ChunkPath::Embedding)
            .unwrap_or(all_chunks.chunks.len());
        if first_emb < all_chunks.chunks.len() {
            self.overlap_manager
                .apply_overlap(&mut all_chunks.chunks[first_emb..], ChunkPath::Embedding);
        }

        super::single_path::add_relations(&tracker, &mut all_chunks.chunks);

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
        let mut conversion_texts: Vec<&str> = Vec::new();
        if let Some(header) = group_conversions.header_conversion.as_ref() {
            if let Some(text) = header.bm25_text.as_deref() {
                conversion_texts.push(text);
            }
            if let Some(text) = header.embedding_text.as_deref() {
                conversion_texts.push(text);
            }
        }
        for member in &group_conversions.member_conversions {
            if let Some(text) = member.bm25_text.as_deref() {
                conversion_texts.push(text);
            }
            if let Some(text) = member.embedding_text.as_deref() {
                conversion_texts.push(text);
            }
        }
        let (config, emb_budget, bm25_budget) =
            self.group_config(group, file_path, Some(&language), &conversion_texts)?;
        let splitter = TextSplitter::with_budgets(config.clone(), emb_budget, bm25_budget);
        let infra = super::single_path::ChunkInfrastructure {
            config: &config,
            estimator: &self.estimator,
            splitter: &splitter,
            pre_injection_embedding: emb_budget.pre_injection_limit,
            pre_injection_bm25: bm25_budget.pre_injection_limit,
            emb_budget,
            bm25_budget,
            has_vocabulary: emb_budget.reserve > 0 || bm25_budget.reserve > 0,
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
        super::limits::enforce_hard_limit(&mut output.chunks, &self.config, file_path)?;
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
        super::limits::enforce_hard_limit(&mut output.chunks, &self.config, file_path)?;

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
            let mut conversion_texts: Vec<&str> = Vec::new();
            if let Some(header) = gc.header_conversion.as_ref() {
                if let Some(text) = header.bm25_text.as_deref() {
                    conversion_texts.push(text);
                }
                if let Some(text) = header.embedding_text.as_deref() {
                    conversion_texts.push(text);
                }
            }
            for member in &gc.member_conversions {
                if let Some(text) = member.bm25_text.as_deref() {
                    conversion_texts.push(text);
                }
                if let Some(text) = member.embedding_text.as_deref() {
                    conversion_texts.push(text);
                }
            }
            let (config, emb_budget, bm25_budget) =
                self.group_config(&gc.group, file_path, language.as_deref(), &conversion_texts)?;
            let splitter = TextSplitter::with_budgets(config.clone(), emb_budget, bm25_budget);
            let infra = super::single_path::ChunkInfrastructure {
                config: &config,
                estimator: &self.estimator,
                splitter: &splitter,
                pre_injection_embedding: emb_budget.pre_injection_limit,
                pre_injection_bm25: bm25_budget.pre_injection_limit,
                emb_budget,
                bm25_budget,
                has_vocabulary: emb_budget.reserve > 0 || bm25_budget.reserve > 0,
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

        // Cross-group merging rescues undersized neighbours on both paths.
        // BM25 tiny residues (single-line splinters, bare fields) are as poor
        // retrieval units as their embedding counterparts, so they rejoin a
        // same-kind neighbour within the merge ceiling. The shared kind and
        // test guards still forbid mixing unrelated entities, and overlap
        // stays embedding-only so BM25 documents never repeat content.
        let emb_chunks_inner: Vec<ChunkedResult> =
            emb_chunks.into_iter().map(|c| c.into_inner()).collect();
        let bm25_inner: Vec<ChunkedResult> =
            bm25_chunks.into_iter().map(|c| c.into_inner()).collect();
        let (emb_merge_reserve, bm25_merge_reserve) =
            self.batch_measured_reserves(group_conversions, file_path, language.as_deref());
        let mut merge_config = self.config.clone();
        if self.config.limit(ChunkPath::Embedding) > 0 {
            merge_config.max_tokens = self
                .config
                .pre_injection_limit_with_reserve(ChunkPath::Embedding, emb_merge_reserve);
        }
        if self.config.limit(ChunkPath::Bm25) > 0 {
            merge_config.max_bm25_terms = self
                .config
                .pre_injection_limit_with_reserve(ChunkPath::Bm25, bm25_merge_reserve);
        }
        let merged_emb_chunks = super::merge::merge_small_chunks_cross_group(
            emb_chunks_inner,
            &group_spans,
            &merge_config,
        );
        let merged_bm25_chunks =
            super::merge::merge_small_chunks_cross_group(bm25_inner, &group_spans, &merge_config);

        let mut emb_results: Vec<ChunkedResult> = merged_emb_chunks;
        let mut bm25_results: Vec<ChunkedResult> = merged_bm25_chunks;

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
        // Overview cards give split members a single-hit unit: the head of
        // the first fragment over the union of the fragments' spans.
        // Plugin outputs bypass the built-in chunker, so they own their
        // whole text and never receive cards.
        super::overview::synthesize_overviews(&mut result);
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
