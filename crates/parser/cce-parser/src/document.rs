//! Document processing module (re-export from cce-document)
//!
//! This module retains the original `cce_parser::document` path for backward
//! compatibility after the document pipeline was extracted into the standalone
//! `cce-document` crate. New code should prefer `cce_document` directly.

pub use cce_document::*;

use cce_types::FileCategory;

/// Convert a document summary into a file summary for storage.
///
/// This helper lives in `cce-parser` (not `cce-document`) because the target
/// type `crate::summary::FileSummary` belongs to the parser crate. Keeping the
/// conversion here avoids a circular dependency between `cce-document` and
/// `cce-parser`.
pub fn doc_summary_to_file_summary(doc: &cce_document::DocSummary) -> crate::summary::FileSummary {
    let language = match doc.doc_type {
        cce_document::DocType::Markdown => "markdown".to_string(),
        cce_document::DocType::Xml => "xml".to_string(),
        cce_document::DocType::Config => std::path::Path::new(&doc.file_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("config")
            .to_string(),
        cce_document::DocType::PlainText => "text".to_string(),
    };

    let category = match doc.doc_type {
        cce_document::DocType::Markdown | cce_document::DocType::PlainText => {
            FileCategory::Documentation
        }
        cce_document::DocType::Config | cce_document::DocType::Xml => FileCategory::Config,
    };

    let mut summary = crate::summary::FileSummary::new(&doc.file_path)
        .with_language(language)
        .with_category(category)
        .with_entities(doc.main_headings.clone())
        .with_line_count(doc.line_count)
        .with_file_level_test_info(&cce_types::Language::Unknown, &doc.file_path, None);

    if let Some(text) = doc.summary_text() {
        summary = summary.with_summary(text);
    }

    summary
}

/// Extension trait for `DocSummary` to retain the historical `to_file_summary`
/// method syntax. Prefer the free function `doc_summary_to_file_summary` in
/// new code.
pub trait DocSummaryExt {
    fn to_file_summary(&self) -> crate::summary::FileSummary;
}

impl DocSummaryExt for cce_document::DocSummary {
    fn to_file_summary(&self) -> crate::summary::FileSummary {
        doc_summary_to_file_summary(self)
    }
}
