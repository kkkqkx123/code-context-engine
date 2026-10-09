//! Content reference types shared by query materialization and annotation.
//!
//! A reference replaces an unavailable body with a file-and-range pointer plus
//! a reason, so consumers always receive a non-empty, actionable result even
//! when the body was dropped for budget, the source file vanished, or the hit
//! is file-level by construction.

use serde::{Deserialize, Serialize};

/// Why a body was replaced by a file-and-range reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DowngradeReason {
    /// The body exceeded its token budget.
    OverLimit,
    /// The source file no longer exists or is unreadable.
    FileMissing,
    /// The hit is file-level (e.g. a file summary) and carries no body.
    FileLevel,
    /// No chunk record exists for the hit, so lines and body cannot be resolved.
    ChunkMissing,
}

impl DowngradeReason {
    /// Short human-readable note rendered after the reference line.
    pub fn note(self) -> &'static str {
        match self {
            Self::OverLimit => "omitted: over budget; read the file range on demand",
            Self::FileMissing => "file not found; path may be stale, adjust or skip",
            Self::FileLevel => "file-level result; read the file for source",
            Self::ChunkMissing => {
                "chunk record missing; index and query stores diverged, reindex or check metadata wiring"
            }
        }
    }
}

/// Whether a result carries its full body or a reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentState {
    /// The result carries the full body.
    #[default]
    Full,
    /// The result carries a file-and-range reference for the given reason.
    Reference(DowngradeReason),
}

impl ContentState {
    /// Whether the result is a reference rather than a full body.
    pub fn is_reference(&self) -> bool {
        matches!(self, Self::Reference(_))
    }
}

/// Render a file-path-plus-range reference line.
///
/// The line carries the location, an optional token-magnitude estimate of the
/// dropped body, and the downgrade reason so the model can decide whether to
/// read on. A zero magnitude (the body is unknown, e.g. the file is gone) is
/// omitted rather than rendered as `~0 tokens`.
pub fn reference_content(
    file_path: &str,
    start_line: u32,
    end_line: u32,
    body_tokens: usize,
    reason: DowngradeReason,
) -> String {
    if body_tokens == 0 {
        format!(
            "// [reference] {}:{}-{} ({})",
            file_path,
            start_line,
            end_line,
            reason.note()
        )
    } else {
        format!(
            "// [reference] {}:{}-{} (~{} tokens, {})",
            file_path,
            start_line,
            end_line,
            body_tokens,
            reason.note()
        )
    }
}

/// Render a file-level reference line for hits without a line range.
pub fn file_level_reference(file_path: &str, reason: DowngradeReason) -> String {
    format!("// [reference] {} ({})", file_path, reason.note())
}
