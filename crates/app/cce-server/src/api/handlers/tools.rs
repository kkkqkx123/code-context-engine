//! Tools API handlers
//!
//! This module provides HTTP handlers for tool APIs that support programming tasks.
//! These tools offer functionality similar to LSP features but operate on-demand
//! without side effects.
//!
//! # Available Endpoints
//!
//! - **Compression**: `/api/tools/compress`, `/api/tools/compress/batch`
//! - **AST Diagnosis**: `/api/tools/diagnose`
//! - **Keyword Search**: `/api/tools/keyword-search`
//! - **Symbol Lookup**: `/api/tools/symbols`, `/api/tools/references`, `/api/tools/definition`

pub mod compression;
pub mod diagnosis;
pub mod keyword;
pub mod symbol;

pub use compression::{handle_compress, handle_compress_batch};
pub use diagnosis::handle_diagnose;
pub use keyword::handle_keyword_search;
pub use symbol::{handle_find_references, handle_get_symbols, handle_goto_definition};
