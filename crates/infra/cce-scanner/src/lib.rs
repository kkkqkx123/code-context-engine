//! File system scanner

pub(crate) mod content;
pub(crate) mod error;
pub(crate) mod file_processor;
pub(crate) mod ignore;

pub(crate) mod models;
pub(crate) mod path_tracker;
pub(crate) mod pattern_matcher;
pub(crate) mod walker;

pub use cce_config::ScannerConfig;
pub use content::{
    decode_verified_bytes, file_matches_scan_hash, read_verified_payload, read_verified_utf8,
    read_verified_utf8_for_entry, read_verified_utf8_for_entry_with_hint,
};
pub use error::{Result, ScannerError};
pub use file_processor::{FileProcessor, FileProcessorConfig, compute_content_hash};
pub use ignore::IgnoreMatcher;

pub use models::{FileContentPayload, FileEntry, FileSetView, FileViewEntry};
pub use path_tracker::PathTracker;
pub use pattern_matcher::{PatternLoadOptions, PatternMatcher};
pub use walker::{FSScanner, ScanFailure, ScanOptions, ScanReport, StreamingScanReport};
