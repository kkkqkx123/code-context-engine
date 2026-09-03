use cce_parser::summary::FileSummary;

/// Export-side view over a file summary.
///
/// Extracts the fields the export pipeline actually renders, isolating it from
/// internal `FileSummary` changes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExportSummaryView {
    pub summary_text: String,
    pub main_entities: Vec<String>,
    pub imports: Vec<String>,
    pub exports: Vec<String>,
    pub line_count: u32,
    pub file_doc_comment: Option<String>,
}

impl From<&FileSummary> for ExportSummaryView {
    fn from(summary: &FileSummary) -> Self {
        Self {
            summary_text: summary.summary_text.clone(),
            main_entities: summary.main_entities.clone(),
            imports: summary.imports.clone(),
            exports: summary.exports.clone(),
            line_count: summary.line_count,
            file_doc_comment: summary.file_doc_comment.clone(),
        }
    }
}

impl From<FileSummary> for ExportSummaryView {
    fn from(summary: FileSummary) -> Self {
        Self::from(&summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_parser::summary::FileSummary;

    #[test]
    fn view_copies_expected_fields() {
        let summary = FileSummary::new("src/main.rs")
            .with_summary("Entry point")
            .with_entities(vec!["main".into()])
            .with_imports(vec!["std::io".into()])
            .with_exports(vec!["run".into()])
            .with_line_count(128)
            .with_file_doc_comment(Some("//! Crate root".into()));

        let view = ExportSummaryView::from(&summary);

        assert_eq!(view.summary_text, "Entry point");
        assert_eq!(view.main_entities, vec!["main"]);
        assert_eq!(view.imports, vec!["std::io"]);
        assert_eq!(view.exports, vec!["run"]);
        assert_eq!(view.line_count, 128);
        assert_eq!(view.file_doc_comment.as_deref(), Some("//! Crate root"));
    }
}
