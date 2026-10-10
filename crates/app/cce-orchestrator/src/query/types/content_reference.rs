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

/// Render a downgrade reference as a self-closing XML tag.
///
/// Location and reason are encoded entirely as attributes (no inner text),
/// which is the lower-token-cost form compared to wrapping a reference line.
/// A zero magnitude (the body is unknown) omits the `tokens` attribute.
pub fn reference_content(
    file_path: &str,
    start_line: u32,
    end_line: u32,
    body_tokens: usize,
    reason: DowngradeReason,
) -> String {
    let mut tag = format!(
        "<reference path=\"{}\" lines=\"{}-{}\" reason=\"{}\"",
        file_path,
        start_line,
        end_line,
        reason_xml_key(reason)
    );
    if body_tokens > 0 {
        tag.push_str(&format!(" tokens=\"~{}\"", body_tokens));
    }
    tag.push_str("/>");
    tag
}

/// Render a file-level downgrade reference as a self-closing XML tag.
pub fn file_level_reference(file_path: &str, reason: DowngradeReason) -> String {
    format!(
        "<reference path=\"{}\" reason=\"{}\"/>",
        file_path,
        reason_xml_key(reason)
    )
}

/// Compact machine-oriented key for a downgrade reason in XML attributes.
fn reason_xml_key(reason: DowngradeReason) -> &'static str {
    match reason {
        DowngradeReason::OverLimit => "over_limit",
        DowngradeReason::FileMissing => "file_missing",
        DowngradeReason::FileLevel => "file_level",
        DowngradeReason::ChunkMissing => "chunk_missing",
    }
}
