//! Overlap management for chunks
//!
//! Manages overlap regions between adjacent chunks.

use cce_utils::token_estimation::TokenEstimator;

use super::config::ChunkingConfig;
use super::result::{ChunkPath, ChunkedResult, OverlapRegion, OverlapType};

/// Overlap information with byte range
#[derive(Debug, Clone)]
struct OverlapInfo {
    /// Overlap text
    text: String,
    /// Start byte position
    start_byte: usize,
    /// End byte position
    end_byte: usize,
}

/// Overlap manager
pub struct OverlapManager {
    config: ChunkingConfig,
    estimator: TokenEstimator,
}

impl OverlapManager {
    /// Create new overlap manager
    pub fn new(config: ChunkingConfig) -> Self {
        Self {
            config,
            estimator: TokenEstimator::default(),
        }
    }

    /// Calculate overlap between two chunks
    pub fn calculate_overlap(
        &self,
        _prev_chunk: &str,
        next_chunk: &str,
        path: ChunkPath,
    ) -> Option<OverlapRegion> {
        let info = self.extract_overlap_from_start(next_chunk, path);

        if info.text.is_empty() {
            return None;
        }

        let token_count = self.estimator.estimate_text(&info.text);

        Some(OverlapRegion {
            text: info.text,
            token_count,
            source_chunk_id: String::new(), // Filled by caller
            overlap_type: OverlapType::Previous,
            start_byte: info.start_byte,
            end_byte: info.end_byte,
        })
    }

    /// Extract overlap from start of text
    fn extract_overlap_from_start(&self, text: &str, path: ChunkPath) -> OverlapInfo {
        if text.is_empty() {
            return OverlapInfo {
                text: String::new(),
                start_byte: 0,
                end_byte: 0,
            };
        }

        let target_byte = match path {
            ChunkPath::Bm25 => {
                // Find byte position after overlap_bm25_words words
                let limit = self.config.overlap_bm25_words;
                let word_end = text
                    .split_whitespace()
                    .take(limit)
                    .map(|w| w.len() + 1) // +1 for the space
                    .sum::<usize>()
                    .min(text.len());
                Self::safe_boundary(text, word_end)
            }
            ChunkPath::Embedding => {
                let max_chars = self.config.overlap_tokens * 4; // ~4 chars per token
                if text.len() <= max_chars {
                    return OverlapInfo {
                        text: text.to_string(),
                        start_byte: 0,
                        end_byte: text.len(),
                    };
                }
                max_chars
            }
        };

        if text.len() <= target_byte {
            return OverlapInfo {
                text: text.to_string(),
                start_byte: 0,
                end_byte: text.len(),
            };
        }

        // Find sentence boundary near target_byte
        let search_start = Self::safe_boundary(text, target_byte.saturating_sub(50));
        let search_end = Self::safe_boundary(text, (target_byte + 100).min(text.len()));

        if let Some(pos) = self.find_sentence_boundary(&text[search_start..search_end]) {
            let end = Self::safe_boundary(text, search_start + pos);
            return OverlapInfo {
                text: text[..end].to_string(),
                start_byte: 0,
                end_byte: end,
            };
        }

        // Fallback to safe character boundary
        let end = Self::safe_boundary(text, target_byte);
        OverlapInfo {
            text: text[..end].to_string(),
            start_byte: 0,
            end_byte: end,
        }
    }

    /// Extract overlap from end of text
    fn extract_overlap_from_end(&self, text: &str, path: ChunkPath) -> OverlapInfo {
        if text.is_empty() {
            return OverlapInfo {
                text: String::new(),
                start_byte: 0,
                end_byte: 0,
            };
        }

        let target_byte = match path {
            ChunkPath::Bm25 => {
                // Find byte position overlap_bm25_words words from the end
                let limit = self.config.overlap_bm25_words;
                let words: Vec<&str> = text.split_whitespace().collect();
                if words.len() <= limit {
                    return OverlapInfo {
                        text: text.to_string(),
                        start_byte: 0,
                        end_byte: text.len(),
                    };
                }
                // Byte position after (words.len() - limit) words → overlap is everything after
                let keep_count = words.len() - limit;
                let byte_pos: usize = words
                    .iter()
                    .take(keep_count)
                    .map(|w| w.len() + 1) // +1 for the space
                    .sum::<usize>()
                    .min(text.len());
                Self::safe_boundary(text, byte_pos)
            }
            ChunkPath::Embedding => {
                let max_chars = self.config.overlap_tokens * 4; // ~4 chars per token
                if text.len() <= max_chars {
                    return OverlapInfo {
                        text: text.to_string(),
                        start_byte: 0,
                        end_byte: text.len(),
                    };
                }
                text.len().saturating_sub(max_chars)
            }
        };

        // Find sentence boundary near target_byte
        let search_start = Self::safe_boundary(text, target_byte.saturating_sub(50));
        let search_end = Self::safe_boundary(text, (target_byte + 100).min(text.len()));

        if search_start < search_end && search_end <= text.len() {
            if let Some(pos) = self.find_sentence_boundary(&text[search_start..search_end]) {
                let start = Self::safe_boundary(text, search_start + pos);
                if start < text.len() {
                    return OverlapInfo {
                        text: text[start..].to_string(),
                        start_byte: start,
                        end_byte: text.len(),
                    };
                }
            }
        }

        // Fallback: start at the computed position
        let start = Self::safe_boundary(text, target_byte);
        OverlapInfo {
            text: text[start..].to_string(),
            start_byte: start,
            end_byte: text.len(),
        }
    }

    /// Find safe UTF-8 character boundary
    fn safe_boundary(text: &str, pos: usize) -> usize {
        if pos >= text.len() {
            return text.len();
        }
        if text.is_char_boundary(pos) {
            return pos;
        }
        // Move backward to find safe boundary
        let mut p = pos;
        while p > 0 && !text.is_char_boundary(p) {
            p -= 1;
        }
        p
    }

    /// Find sentence boundary position
    fn find_sentence_boundary(&self, text: &str) -> Option<usize> {
        let sentence_endings = ['.', '!', '?', '。', '！', '？', '\n'];

        for (i, ch) in text.char_indices() {
            if sentence_endings.contains(&ch) {
                let next_pos = i + ch.len_utf8();
                // Skip whitespace
                return Some(next_pos);
            }
        }

        None
    }

    /// Apply overlap to chunks (single-direction: only prev_overlap)
    ///
    /// This simplified approach avoids content duplication by only setting
    /// prev_overlap on each chunk (from the end of the previous chunk).
    pub fn apply_overlap(&self, chunks: &mut [ChunkedResult], path: ChunkPath) {
        if chunks.len() < 2 {
            return;
        }

        for i in 1..chunks.len() {
            // Only set prev_overlap from previous chunk's end
            let prev_text = chunks[i - 1].pure_text();
            let info = self.extract_overlap_from_end(prev_text, path);
            if !info.text.is_empty() {
                let token_count = self.estimator.estimate_text(&info.text);

                // Validate overlap doesn't exceed max ratio
                if self.validate_overlap(chunks[i].token_count, token_count) {
                    let prev_id = chunks[i - 1].chunk_id.clone();

                    chunks[i].prev_overlap = Some(OverlapRegion {
                        text: info.text.clone(),
                        token_count,
                        source_chunk_id: prev_id,
                        overlap_type: OverlapType::Previous,
                        start_byte: info.start_byte,
                        end_byte: info.end_byte,
                    });

                    if let Some(code_meta) = chunks[i].metadata.as_code_mut() {
                        code_meta.has_overlap = true;
                    }
                }
            }
        }

        // Validate final chunks don't exceed token limits with overlap
        for chunk in chunks.iter() {
            if let Some(ref overlap) = chunk.prev_overlap {
                let total_tokens = chunk.token_count + overlap.token_count;
                // Allow some buffer (2x max_tokens) since overlap is additional context
                if total_tokens > self.config.max_tokens * 2 {
                    tracing::warn!(
                        chunk_id = chunk.chunk_id,
                        overlap_source = overlap.source_chunk_id,
                        chunk_tokens = chunk.token_count,
                        overlap_tokens = overlap.token_count,
                        total_tokens = total_tokens,
                        max_allowed = self.config.max_tokens * 2,
                        "Chunk with overlap may exceed model context window"
                    );
                }
            }
        }
    }

    /// Validate overlap doesn't exceed max ratio
    pub fn validate_overlap(&self, chunk_tokens: usize, overlap_tokens: usize) -> bool {
        if chunk_tokens == 0 {
            return true;
        }
        let ratio = overlap_tokens as f32 / chunk_tokens as f32;
        ratio <= self.config.max_overlap_ratio
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast_to_nl::chunker::ChunkPath;

    #[test]
    fn test_extract_overlap_from_start() {
        let config = ChunkingConfig::default();
        let manager = OverlapManager::new(config);

        let text = "First sentence. Second sentence. Third sentence.";
        let overlap = manager.extract_overlap_from_start(text, ChunkPath::Bm25);

        assert!(!overlap.text.is_empty());
        assert!(overlap.text.len() <= text.len());
    }

    #[test]
    fn test_apply_overlap() {
        let config = ChunkingConfig::default();
        let manager = OverlapManager::new(config);

        let mut chunks = vec![
            ChunkedResult::new(
                "chunk_0".to_string(),
                "group_1".to_string(),
                ChunkPath::Bm25,
                0,
                2,
            ),
            ChunkedResult::new(
                "chunk_1".to_string(),
                "group_1".to_string(),
                ChunkPath::Bm25,
                1,
                2,
            ),
        ];
        chunks[0].text = "First chunk with some content.".to_string();
        chunks[1].text = "Second chunk with more content.".to_string();

        manager.apply_overlap(&mut chunks, ChunkPath::Bm25);

        assert!(chunks[1].prev_overlap.is_some());
    }

    #[test]
    fn test_validate_overlap() {
        let config = ChunkingConfig::default();
        let manager = OverlapManager::new(config);

        assert!(manager.validate_overlap(100, 10)); // 10% overlap
        assert!(!manager.validate_overlap(100, 50)); // 50% overlap (exceeds 20%)
    }

    #[test]
    fn test_has_overlap_flag_set() {
        use super::super::result::{ChunkMetadata, CodeSpecificMetadata};
        use cce_types::Span;
        use cce_types::language::Language;

        let config = ChunkingConfig::default();
        let manager = OverlapManager::new(config);

        let mut chunks = vec![
            ChunkedResult::new(
                "chunk_0".to_string(),
                "group_1".to_string(),
                ChunkPath::Bm25,
                0,
                2,
            ),
            ChunkedResult::new(
                "chunk_1".to_string(),
                "group_1".to_string(),
                ChunkPath::Bm25,
                1,
                2,
            ),
        ];
        // Use shorter text to ensure overlap is small
        chunks[0].text = "First sentence. Second sentence.".to_string();
        chunks[1].text = "Third sentence.".to_string();
        // Set token counts - chunk[1] should be large enough to accept overlap
        chunks[0].token_count = 10;
        chunks[1].token_count = 50; // Large enough to accept 20% overlap

        // Add code metadata so has_overlap flag can be set
        chunks[0].metadata = ChunkMetadata::for_code(
            "test.rs".to_string(),
            Span::default(),
            Language::Rust,
            CodeSpecificMetadata::default(),
        );
        chunks[1].metadata = ChunkMetadata::for_code(
            "test.rs".to_string(),
            Span::default(),
            Language::Rust,
            CodeSpecificMetadata::default(),
        );

        // Initially no overlap
        assert!(!chunks[0].metadata.has_overlap());
        assert!(!chunks[1].metadata.has_overlap());

        manager.apply_overlap(&mut chunks, ChunkPath::Bm25);

        // After apply_overlap, has_overlap should be set on chunk_1 (it gets prev_overlap from chunk_0)
        assert!(chunks[1].metadata.has_overlap());
    }

    #[test]
    fn test_overlap_byte_range() {
        let config = ChunkingConfig::default();
        let manager = OverlapManager::new(config);

        let mut chunks = vec![
            ChunkedResult::new(
                "chunk_0".to_string(),
                "group_1".to_string(),
                ChunkPath::Bm25,
                0,
                2,
            ),
            ChunkedResult::new(
                "chunk_1".to_string(),
                "group_1".to_string(),
                ChunkPath::Bm25,
                1,
                2,
            ),
        ];
        chunks[0].text = "First chunk with some content.".to_string();
        chunks[1].text = "Second chunk with more content.".to_string();

        manager.apply_overlap(&mut chunks, ChunkPath::Bm25);

        // Check that overlap has valid byte range
        if let Some(ref overlap) = chunks[1].prev_overlap {
            assert!(overlap.end_byte > overlap.start_byte);
            assert!(overlap.end_byte <= chunks[0].text.len());
        }
    }

    #[test]
    fn test_overlap_entities_field_exists() {
        // Test that overlap_entities field is accessible and can store entity IDs
        use super::super::result::{ChunkMetadata, CodeSpecificMetadata};
        use cce_types::entity::EntityId;

        let mut metadata = ChunkMetadata::default();

        // Should be empty initially (no code_metadata yet)
        assert!(metadata.code_metadata.is_none());

        // Create code metadata with overlap entities
        let mut code_meta = CodeSpecificMetadata::default();
        code_meta.overlap_entities.push(EntityId(1));
        code_meta.overlap_entities.push(EntityId(2));

        metadata.code_metadata = Some(code_meta);

        assert_eq!(
            metadata
                .code_metadata
                .as_ref()
                .unwrap()
                .overlap_entities
                .len(),
            2
        );
    }
}
