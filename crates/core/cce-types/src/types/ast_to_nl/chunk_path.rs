//! Retrieval path discriminator for chunked content
//!
//! This module defines `ChunkPath`, which is NOT a file path but a discriminator
//! for the retrieval pipeline a chunk belongs to. It determines whether a chunk
//! should be indexed in the BM25 full-text search engine or the vector embedding
//! store. The name "ChunkPath" is historical and may be confusing; a more
//! accurate name would be `RetrievalPipeline` or `ChunkDestination`.

use serde::{Deserialize, Serialize};

/// Retrieval pipeline discriminator for chunks
///
/// Despite the name, this is NOT a file path. It identifies which retrieval
/// pipeline a chunk belongs to:
/// - `Bm25`: Chunks go to the BM25 full-text search index
/// - `Embedding`: Chunks go to the vector embedding store (Qdrant)
///
/// This discriminator is used in chunk ID generation and storage routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChunkPath {
    /// BM25 full-text search pipeline
    Bm25,
    /// Embedding vector search pipeline
    Embedding,
}

impl ChunkPath {
    /// Get string representation for chunk_id generation
    pub fn as_str(&self) -> &'static str {
        match self {
            ChunkPath::Bm25 => "bm25",
            ChunkPath::Embedding => "emb",
        }
    }
}

impl std::fmt::Display for ChunkPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
