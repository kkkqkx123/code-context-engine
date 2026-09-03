//! Split reason enumeration (cross-layer contract)
//!
//! Moved from `cce_parser::ast_to_nl::chunker::boundary` so the plugin
//! chunk contract (`cce_core::types::ast_to_nl::ChunkedResult`) can
//! reference it without depending on the parser crate.

use serde::{Deserialize, Serialize};

/// Split reason
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SplitReason {
    /// Member boundary (ClassWithMethods, etc.)
    MemberBoundary,
    /// Sentence boundary
    SentenceBoundary,
    /// Paragraph boundary
    ParagraphBoundary,
    /// Line boundary
    LineBoundary,
    /// Token limit reached
    TokenLimit,
    /// Hard limit (last resort)
    HardLimit,
    /// Not split (single chunk)
    #[default]
    NotSplit,
}

impl std::fmt::Display for SplitReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SplitReason::MemberBoundary => write!(f, "member_boundary"),
            SplitReason::SentenceBoundary => write!(f, "sentence_boundary"),
            SplitReason::ParagraphBoundary => write!(f, "paragraph_boundary"),
            SplitReason::LineBoundary => write!(f, "line_boundary"),
            SplitReason::TokenLimit => write!(f, "token_limit"),
            SplitReason::HardLimit => write!(f, "hard_limit"),
            SplitReason::NotSplit => write!(f, "not_split"),
        }
    }
}
