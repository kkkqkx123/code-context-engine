//! Query module for Tree-sitter query patterns
//!
//! This module provides:
//! - Language-specific Tree-sitter query patterns
//! - Query loading and caching
//! - Query execution
//! - Common tree-sitter utility functions
//! - Metrics collection for query operations
//!
//! Note: Call chain queries are in the relation module.

pub(crate) mod capture;
pub(crate) mod error;
pub(crate) mod executor;
pub(crate) mod loader;

pub(crate) mod parser_types;
pub(crate) mod scheme;
pub(crate) mod utils;

// Re-export main types for convenience
pub use capture::entity_name;
pub use capture::entity_name_with_subtype;

pub use error::{QueryError, Result};
pub use executor::{Capture, QueryExecutor, QueryMatch};
pub use loader::{QueryLoader, QueryType};

pub use parser_types::{CaptureName, CaptureParseError, Domain, ParseResult};
pub use scheme::{
    extract_call_category, extract_dependency_category, extract_entity_category, is_call_capture,
    is_comment_capture, is_dependency_capture, is_entity_capture,
};
pub use utils::{
    find_child_by_kind, find_children_by_kind, find_descendants_by_kind, node_text, node_to_span,
};
