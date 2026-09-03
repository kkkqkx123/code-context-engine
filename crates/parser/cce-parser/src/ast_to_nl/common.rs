//! Common utilities shared by BM25 and Embedding generators
//!
//! This module provides shared functionality for:
//! - Name normalization (snake_case, camelCase, PascalCase -> natural language)
//! - Utility functions (create_standalone_group, etc.)
//! - Template components (output strategy, group trait, helpers)

pub mod annotation_formatter;
pub mod normalizer;
pub mod templates;
pub mod utils;

// Re-export main types
pub use annotation_formatter::format_annotations;
pub use normalizer::NameNormalizer;
pub use utils::{clean_comment_content, create_standalone_group, safe_utf8_boundary};

// Re-export template types
pub use templates::{GroupTemplateBase, TemplateHelpers};
