//! Index operation handlers
//!
//! This module provides handlers for indexing operations including:
//! - Full index execution
//! - Incremental indexing
//! - Single file parsing

pub mod execute;
pub mod incremental;
pub mod parse;

pub use execute::handle_index;
pub use incremental::handle_incremental;
pub use parse::handle_parse;
