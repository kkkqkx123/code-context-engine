//! Type definitions for semantic compression retrieval

use serde::{Deserialize, Serialize};
use thiserror::Error;

use cce_parser::grouper::types::EntityGroup;
use cce_types::Entity;

/// Semantic compression error type
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum CompressionError {
    /// File not found
    #[error("File not found: {0}")]
    FileNotFound(String),

    /// File not readable
    #[error("File not readable: {0}")]
    FileNotReadable(String),

    /// File too large
    #[error("File too large: {0}")]
    FileTooLarge(String),

    /// Unsupported file type
    #[error("Unsupported file type: {0}")]
    UnsupportedFileType(String),

    /// Parse error
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Language detection error
    #[error("Language detection error: {0}")]
    LanguageDetectionError(String),

    /// Cache error
    #[error("Cache error: {0}")]
    CacheError(String),
}

/// Result type for compression operations
pub type Result<T> = std::result::Result<T, CompressionError>;

/// Semantic compression request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionRequest {
    /// File path (absolute or relative)
    pub file_path: String,

    /// Whether to include entity information
    pub include_entities: bool,

    /// Whether to include preprocessing results (entity groups)
    pub include_groups: bool,
}

impl CompressionRequest {
    /// Create a new compression request with default options
    pub fn new(file_path: impl Into<String>) -> Self {
        Self {
            file_path: file_path.into(),
            include_entities: false,
            include_groups: false,
        }
    }

    /// Include entity information in the response
    pub fn with_entities(mut self, include: bool) -> Self {
        self.include_entities = include;
        self
    }

    /// Include entity groups in the response
    pub fn with_groups(mut self, include: bool) -> Self {
        self.include_groups = include;
        self
    }
}

/// Semantic compression response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionResponse {
    /// File path
    pub file_path: String,

    /// Programming language
    pub language: String,

    /// File hash (SHA-256)
    pub file_hash: String,

    /// Whether the result came from cache
    pub from_cache: bool,

    /// Entity list (optional)
    pub entities: Option<Vec<Entity>>,

    /// Entity groups (optional)
    pub groups: Option<Vec<EntityGroup>>,

    /// Semantic summary for human/LLM consumption (pure natural language)
    pub semantic_text: String,
}

/// Batch compression request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCompressionRequest {
    /// File paths to process
    pub file_paths: Vec<String>,

    /// Whether to include entity information
    pub include_entities: bool,

    /// Whether to include entity groups
    pub include_groups: bool,

    /// Maximum concurrent tasks
    pub max_concurrency: usize,
}

impl BatchCompressionRequest {
    /// Create a new batch compression request with default options
    pub fn new(file_paths: Vec<String>) -> Self {
        Self {
            file_paths,
            include_entities: false,
            include_groups: false,
            max_concurrency: 4,
        }
    }

    /// Include entity information in the response
    pub fn with_entities(mut self, include: bool) -> Self {
        self.include_entities = include;
        self
    }

    /// Include entity groups in the response
    pub fn with_groups(mut self, include: bool) -> Self {
        self.include_groups = include;
        self
    }

    /// Set maximum concurrent tasks
    pub fn with_max_concurrency(mut self, max: usize) -> Self {
        self.max_concurrency = max;
        self
    }
}

/// Batch compression response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCompressionResponse {
    /// Successful results
    pub successes: Vec<(String, CompressionResponse)>,

    /// Failed results
    pub failures: Vec<(String, CompressionError)>,
}

impl BatchCompressionResponse {
    /// Check if all files were processed successfully
    pub fn is_all_success(&self) -> bool {
        self.failures.is_empty()
    }

    /// Get total number of processed files
    pub fn total_count(&self) -> usize {
        self.successes.len() + self.failures.len()
    }

    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        if self.total_count() == 0 {
            return 0.0;
        }
        self.successes.len() as f64 / self.total_count() as f64
    }
}
