//! Generic summarizer trait for document processing
//!
//! This trait provides a common interface for summarizing different document types
//! (JSON, XML, TOML, YAML, Markdown) to reduce code duplication.
//!
//! # Design
//!
//! Summarizers only extract reliable structural metadata from parsed nodes and groups.
//! No semantic summary text or importance estimation is performed — those are
//! unreliable for document files and provide negligible retrieval value.

use crate::common::node::DocumentNode;
use crate::common::types::is_stopword;
use crate::types::{DocSummary, DocType};

use super::GenericGroup;

/// Trait for generic document summarization
///
/// Only extracts reliable structural metadata:
/// - Title (from first heading or filename)
/// - Main structural entries (headings, root keys, etc.)
/// - Line count
pub trait GenericSummarizer<N, G>
where
    N: DocumentNode,
    G: GenericGroup<N>,
{
    /// Get the document type for this summarizer
    fn doc_type(&self) -> DocType;

    /// Extract document title
    fn extract_title(&self, nodes: &[N], file_path: &str) -> Option<String>;

    /// Extract main structural entries (headings, root-level keys, etc.) for main_headings
    fn extract_structural_entries(&self, nodes: &[N]) -> Vec<String>;

    /// Count lines from nodes
    fn count_lines(&self, nodes: &[N]) -> u32 {
        nodes
            .iter()
            .map(|n| {
                n.span()
                    .end_position
                    .row
                    .saturating_sub(n.span().start_position.row)
            })
            .sum::<usize>() as u32
    }

    /// Generate complete summary (default implementation)
    ///
    /// Only populates reliable structural fields:
    /// - file_path, doc_type
    /// - title (from extract_title)
    /// - main_headings (from extract_structural_entries)
    /// - line_count
    fn summarize(&self, nodes: &[N], groups: &[G], file_path: &str) -> DocSummary {
        let mut summary = DocSummary::new(file_path.to_string(), self.doc_type());

        summary.title = self.extract_title(nodes, file_path);
        summary.main_headings = self.extract_structural_entries(nodes);
        summary.line_count = self.count_lines(nodes);

        let _ = groups;
        summary
    }
}

/// Helper function to infer title from filename for config files
pub fn infer_title_from_filename(file_path: &str) -> Option<String> {
    let file_name = std::path::Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    match file_name.as_str() {
        "package" => Some("Package Configuration".to_string()),
        "tsconfig" => Some("TypeScript Configuration".to_string()),
        "eslint" | ".eslintrc" => Some("ESLint Configuration".to_string()),
        "prettier" | ".prettierrc" => Some("Prettier Configuration".to_string()),
        "babel" | ".babelrc" => Some("Babel Configuration".to_string()),
        "webpack" => Some("Webpack Configuration".to_string()),
        "vite" => Some("Vite Configuration".to_string()),
        "cargo" => Some("Cargo Configuration".to_string()),
        "pyproject" => Some("Python Project Configuration".to_string()),
        "docker-compose" => Some("Docker Compose Configuration".to_string()),
        "deployment" | "kubernetes" | "k8s" => Some("Kubernetes Configuration".to_string()),
        "ansible" | "playbook" => Some("Ansible Playbook".to_string()),
        "pom" => Some("Maven POM".to_string()),
        "web" | "web.xml" => Some("Web Application Descriptor".to_string()),
        "applicationcontext" => Some("Spring Application Context".to_string()),
        "beans" => Some("Spring Beans Configuration".to_string()),
        "config" => Some("Configuration".to_string()),
        "settings" => Some("Settings".to_string()),
        _ => None,
    }
}

/// Extract root-level keys/identifiers from nodes (for main_headings)
pub fn extract_root_keys<N: DocumentNode>(
    nodes: &[N],
    get_key: impl Fn(&N) -> Option<&str>,
) -> Vec<String> {
    nodes
        .iter()
        .filter(|n| n.depth() == 1)
        .filter_map(get_key)
        .filter(|k| !k.is_empty() && !is_stopword(k) && k.len() < 50)
        .take(10)
        .map(|s| s.to_string())
        .collect()
}
