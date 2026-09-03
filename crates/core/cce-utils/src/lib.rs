//! Utility functions for the code context engine

pub mod encoding;
pub use encoding::{Detector, Encoder, Error};

pub mod file;
pub use file::{
    format_file_size, has_text_extension, is_text_file, read_file_to_utf8, read_file_to_utf8_async,
    read_file_to_utf8_with_encoding_async,
};

pub mod text;
pub use text::{
    is_blank, normalize_code_fragment, normalize_whitespace,
    normalize_whitespace_preserving_newlines, remove_quotes, split_camel_case,
};

pub mod comment_cleaner;
pub use comment_cleaner::{clean_comment_markers, strip_comment_markers};

pub mod hash;
pub use hash::{calculate_hash, calculate_hash_with_limit};

pub mod time;
pub use time::{current_timestamp_ms, current_timestamp_secs};

pub mod token_estimation;
pub use token_estimation::{TokenEstimator, estimate_tokens};

pub mod glob;
pub use glob::Glob;
