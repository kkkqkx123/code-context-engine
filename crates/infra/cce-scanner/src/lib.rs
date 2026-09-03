//! File system scanner

pub(crate) mod error;
pub(crate) mod file_processor;
pub(crate) mod ignore;

pub(crate) mod models;
pub(crate) mod path_tracker;
pub(crate) mod pattern_matcher;
pub(crate) mod walker;

pub use cce_config::ScannerConfig;
pub use error::{Result, ScannerError};
pub use file_processor::{FileProcessor, FileProcessorConfig, compute_content_hash};
pub use ignore::IgnoreMatcher;

pub use models::FileEntry;
pub use path_tracker::PathTracker;
pub use pattern_matcher::{PatternLoadOptions, PatternMatcher};
pub use walker::{FSScanner, ScanOptions};
