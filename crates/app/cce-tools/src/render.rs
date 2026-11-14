//! Semantic text rendering for compression results.
//!
//! The full presentation converter lives in the export crate; tools only
//! depend on the [`SemanticRenderer`] port so rendering stays injectable.
//! [`PlainSemanticRenderer`] is the neutral built-in fallback used when no
//! export-backed renderer is wired.

use std::sync::Arc;

use cce_parser::grouper::types::EntityGroup;

/// Renders grouped entities into semantic text.
pub trait SemanticRenderer: Send + Sync {
    /// Render entity groups of one file into semantic text.
    fn render(&self, groups: &[EntityGroup], file_path: &str) -> String;
}

/// Neutral built-in renderer: group headers plus member signatures.
///
/// Carries no presentation markup; the export crate provides the full
/// converter for display-quality text.
#[derive(Debug, Default, Clone, Copy)]
pub struct PlainSemanticRenderer;

impl SemanticRenderer for PlainSemanticRenderer {
    fn render(&self, groups: &[EntityGroup], file_path: &str) -> String {
        let mut parts = Vec::new();
        parts.push(format!("File: {file_path}"));
        for group in groups {
            let mut lines = vec![format!("{} {}", group.kind, group.name)];
            if let Some(header) = &group.header
                && !header.signature.is_empty()
            {
                lines.push(header.signature.clone());
            }
            for member in &group.members {
                if member.signature.is_empty() {
                    lines.push(format!("{} {}", member.kind, member.name));
                } else {
                    lines.push(member.signature.clone());
                }
            }
            parts.push(lines.join("\n"));
        }
        parts.join("\n\n")
    }
}

/// Shared renderer handle injected into compression retrieval.
pub type SharedRenderer = Arc<dyn SemanticRenderer>;
