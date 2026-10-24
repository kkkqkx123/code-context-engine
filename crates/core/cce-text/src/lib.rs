//! Text processing utilities for the code context engine

pub mod budget;
pub mod segment;
pub mod text_cleaner;
pub mod tokenizer;

pub use budget::{ChunkUnit, split_to_fit, suffix_start_for_cost, truncate_to_cost};
pub use cce_utils::text::{ceil_char_boundary, find_case_insensitive, floor_char_boundary};
pub use segment::{
    MAX_TOKENIZE_CHARS, MixedToken, segment_text, shared_jieba, truncated_input_count,
};
pub use text_cleaner::{Bm25TextCleaner, Bm25TextCleanerConfig};
#[cfg(feature = "tantivy")]
pub use tokenizer::MixedTokenStream;
pub use tokenizer::MixedTokenizer;
