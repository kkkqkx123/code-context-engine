//! Assembly error types

use std::fmt;

use cce_types::EntityId;

/// Assembly error type
#[derive(Debug)]
pub enum AssemblyError {
    /// Entity not found
    EntityNotFound(EntityId),
    /// Extraction failed
    ExtractionFailed { file_path: String, message: String },
    /// Invalid line range
    InvalidLineRange {
        file_path: String,
        start_line: u32,
        end_line: u32,
        total_lines: u32,
    },
    /// Content too large
    ContentTooLarge { size: usize, max_size: usize },
    /// IO error
    IoError(String),
}

impl AssemblyError {
    /// Create an entity not found error
    pub fn entity_not_found(id: EntityId) -> Self {
        Self::EntityNotFound(id)
    }

    /// Create an extraction failed error
    pub fn extraction_failed(file_path: &str, message: String) -> Self {
        Self::ExtractionFailed {
            file_path: file_path.to_string(),
            message,
        }
    }

    /// Create an invalid line range error
    pub fn invalid_line_range(
        file_path: &str,
        start_line: u32,
        end_line: u32,
        total_lines: u32,
    ) -> Self {
        Self::InvalidLineRange {
            file_path: file_path.to_string(),
            start_line,
            end_line,
            total_lines,
        }
    }

    /// Create a content too large error
    pub fn content_too_large(size: usize, max_size: usize) -> Self {
        Self::ContentTooLarge { size, max_size }
    }
}

impl fmt::Display for AssemblyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EntityNotFound(id) => write!(f, "Entity not found: {}", id),
            Self::ExtractionFailed { file_path, message } => {
                write!(f, "Failed to extract from '{}': {}", file_path, message)
            }
            Self::InvalidLineRange {
                file_path,
                start_line,
                end_line,
                total_lines,
            } => {
                write!(
                    f,
                    "Invalid line range {}-{} in '{}' (total lines: {})",
                    start_line, end_line, file_path, total_lines
                )
            }
            Self::ContentTooLarge { size, max_size } => {
                write!(f, "Content size {} exceeds maximum {}", size, max_size)
            }
            Self::IoError(msg) => write!(f, "IO error: {}", msg),
        }
    }
}

impl std::error::Error for AssemblyError {}

impl From<std::io::Error> for AssemblyError {
    fn from(err: std::io::Error) -> Self {
        Self::IoError(err.to_string())
    }
}

/// Assembly result type
pub type Result<T> = std::result::Result<T, AssemblyError>;
