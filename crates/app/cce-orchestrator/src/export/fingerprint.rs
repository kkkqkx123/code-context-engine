//! Deterministic fingerprints over the inputs that determine an exported NL
//! document's rendered content.
//!
//! Recovery re-exports a document only when the stored fingerprint still
//! matches the current inputs; otherwise the document may have been rendered
//! under a different configuration and must be regenerated.

use cce_utils::hash::{calculate_hash, hash_serializable};
use serde::Serialize;

/// Fingerprint of a configuration value.
///
/// Serializes the value deterministically (field order is fixed by the struct
/// definition) and hashes it with SHA-256. Unserializable values degrade to a
/// stable marker hash rather than failing the export path.
pub fn config_fingerprint<T: Serialize>(config: &T) -> String {
    hash_serializable(config)
}

/// Fingerprint of a rendered file summary.
///
/// Serializes the summary deterministically. When no summary participates in
/// the rendering (`include_summary` off) the caller passes `None`, which hashes
/// a stable marker so enabling summaries later invalidates stale documents.
pub fn summary_content_fingerprint(
    summary: Option<&super::summary_view::ExportSummaryView>,
) -> String {
    match summary {
        Some(summary) => config_fingerprint(summary),
        None => calculate_hash(b"<no-summary>"),
    }
}

/// Compute the render fingerprint that pins an exported document to the
/// inputs it was rendered from.
///
/// The returned value is persisted alongside `export_path`; recovery compares
/// the recomputed fingerprint with the stored one before skipping re-export.
///
/// Uses the stable SHA-256 hash shared across the codebase: the fingerprint is
/// persisted and compared across process restarts, so it must not depend on an
/// algorithm that is unspecified across Rust releases.
pub fn render_fingerprint(
    export_config_fingerprint: &str,
    ast_to_nl_fingerprint: &str,
    grouper_fingerprint: &str,
    relation_epoch: i64,
    summary_content_fingerprint: &str,
    content_hash: &str,
) -> String {
    let mut buf = Vec::with_capacity(
        export_config_fingerprint.len()
            + ast_to_nl_fingerprint.len()
            + grouper_fingerprint.len()
            + relation_epoch.to_string().len()
            + summary_content_fingerprint.len()
            + content_hash.len()
            + 6,
    );
    for part in [
        export_config_fingerprint,
        ast_to_nl_fingerprint,
        grouper_fingerprint,
        &relation_epoch.to_string(),
        summary_content_fingerprint,
        content_hash,
    ] {
        buf.extend_from_slice(part.as_bytes());
        buf.push(0xff);
    }
    calculate_hash(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Serialize)]
    struct Sample {
        flag: bool,
        size: usize,
    }

    #[test]
    fn config_fingerprint_is_deterministic() {
        let a = Sample {
            flag: true,
            size: 3,
        };
        assert_eq!(config_fingerprint(&a), config_fingerprint(&a));
        let b = Sample {
            flag: false,
            size: 3,
        };
        assert_ne!(config_fingerprint(&a), config_fingerprint(&b));
    }

    #[test]
    fn render_fingerprint_changes_with_any_input() {
        let base = render_fingerprint("a", "b", "c", 1, "s", "h");
        assert_ne!(base, render_fingerprint("A", "b", "c", 1, "s", "h"));
        assert_ne!(base, render_fingerprint("a", "B", "c", 1, "s", "h"));
        assert_ne!(base, render_fingerprint("a", "b", "C", 1, "s", "h"));
        assert_ne!(base, render_fingerprint("a", "b", "c", 2, "s", "h"));
        assert_ne!(base, render_fingerprint("a", "b", "c", 1, "S", "h"));
        assert_ne!(base, render_fingerprint("a", "b", "c", 1, "s", "H"));
        assert_eq!(base, render_fingerprint("a", "b", "c", 1, "s", "h"));
    }
}
