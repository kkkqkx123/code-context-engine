//! Chunker module - Intelligent text segmentation based on EntityGroup boundaries
//!
//! This module provides Group-level text splitting functionality with:
//! - Respects Group boundaries during splitting (never splits a group across chunks)
//! - Optimal splitting strategy based on GroupType
//! - Multi-level splitting fallback (members -> sentences -> paragraphs -> lines -> tokens)
//! - Automatic overlap generation to maintain context continuity
//! - Cross-Group relation tracking
//! - Cross-group merging of undersized chunks (adjacent groups only; merged
//!   groups are recorded in `ChunkMetadata::merged_group_ids`)
//!
//! # Responsibilities
//!
//! The chunker is responsible for **text segmentation and optimization** - breaking down
//! large converted texts into appropriately-sized chunks for indexing. It focuses on:
//!
//! - **Splitting Strategy**: How to divide text while preserving semantic meaning
//! - **Token Management**: Enforcing token and word count limits
//! - **Context Preservation**: Maintaining context through overlaps and header repetition
//! - **Metadata Generation**: Tracking fragment relationships and split reasons
//!
//! # Pipeline Position
//!
//! ```text
//! EntityGroup → [Converter] → ConversionResult → [Chunker] → ChunkedResult
//! ```
//!
//! The chunker receives `ConversionResult` objects from the converter and produces
//! `ChunkedResult` objects ready for vector database indexing.
//!
//! # Key Types
//!
//! - [`GroupChunker`]: Main chunking orchestrator
//! - [`TextSplitter`]: Implements various splitting strategies
//! - [`ChunkedResult`]: Output containing segmented text with metadata
//! - [`ChunkingConfig`]: Configuration for token limits and splitting behavior
//!
//! # Splitting Strategies
//!
//! The chunker selects strategies based on group type:
//!
//! - **ByMembers**: Split at entity boundaries (classes, functions)
//! - **BySentences**: Split at sentence boundaries (standalone entities)
//! - **ByParagraphs**: Split at paragraph boundaries (modules)
//! - **ByNestedGroups**: Split at nested class/struct boundaries
//! - **ByTokens**: Force token-based splitting (fallback)
//!
//! # Smart Chunking
//!
//! For groups with multiple conversions (e.g., class + methods), the chunker implements
//! smart chunking where:
//! - Header text (class overview) is repeated in each chunk for context
//! - Members are grouped by token budget to avoid oversized chunks
//! - Each chunk maintains self-contained semantic meaning
//!
//! # Usage
//!
//! ```ignore
//! // This example requires pre-converted entity groups as input.
//! // See converter module for how to produce GroupConversions.
//!
//! use crate::ast_to_nl::chunker::{GroupChunker, ChunkingConfig};
//!
//! let config = ChunkingConfig::default();
//! let mut chunker = GroupChunker::new(config);
//! let file_path = "src/main.rs";
//!
//! // let chunks = chunker.chunk_groups(&group_conversions, file_path);
//!
//! // Chunks are now ready for embedding and indexing
//! ```
//!
//! # Assumptions
//!
//! The chunker makes the following assumptions about its input:
//!
//! 1. **Dual-Path Text**: Both `bm25_text` and `embedding_text` should be present and aligned
//! 2. **UTF-8 Valid**: All text must be valid UTF-8
//! 3. **Entity Spans**: If using member-based splitting, entity spans must be valid byte offsets
//! 4. **Non-Empty Content**: Empty bm25_text results in no chunks being generated
//!
//! # Alignment Contract
//!
//! The BM25 and Embedding paths split their texts **independently**: there is no
//! requirement that the two paths produce the same number of chunks, and no
//! chunk-level 1:1 correspondence exists between them (chunk IDs are
//! `{group_id}_{bm25|emb}_{index}`). Cross-path alignment happens at the
//! **entity level**: both paths share the same `content_entity_ids`, so hybrid
//! fusion aligns BM25 hits with embedding hits per entity, not per chunk.

// Allow module with same name as parent (chunker::chunker is intentional for organization)
#![allow(clippy::module_inception)]

pub mod boundary;
pub mod chunk_builder;
pub mod chunker;
pub mod config;
pub mod header;
pub mod header_chunk;
pub mod overlap;
pub mod result;
pub mod source_coverage;
pub mod splitter;
pub mod strategy;
pub mod tracker;

mod merge;
mod segment_limit;
mod strategies;

#[cfg(test)]
mod test;

// Re-export main types
pub use boundary::{ChunkBoundary, NlEntityBoundary, SplitReason};
pub use cce_config::modules::ChunkingConfig;
pub use chunker::GroupChunker;
pub use overlap::OverlapManager;
pub use result::{
    ChunkContentType, ChunkMetadata, ChunkPath, ChunkedResult, CodeSpecificMetadata, GroupRelation,
    GroupRelationType, OverlapRegion, OverlapType, SourceSpanKind,
};
pub use splitter::TextSplitter;
pub use strategies::tokens::split_by_tokens;
pub use strategy::SplitStrategy;
pub use tracker::GroupTracker;
