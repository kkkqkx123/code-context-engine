//! BM25 related type definitions
//!
//! Canonical definitions live in `cce_storage_common::fulltext`; this module
//! keeps the branch-facing names so existing branch code and downstream
//! callers keep compiling against one vocabulary.

pub use cce_storage_common::fulltext::{
    FulltextDocument as Bm25Document, FulltextHit as Bm25SearchResult,
    FulltextSearchOptions as Bm25SearchOptions,
};

pub use cce_config::modules::search::TermOperator;
