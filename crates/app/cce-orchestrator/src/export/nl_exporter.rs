//! Natural language document exporter
//!
//! This module provides the core exporter functionality for generating
//! natural language documentation from code.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use cce_parser::ast_to_nl::chunker::{ChunkPath, ChunkedResult};
use cce_relation::index::RelationIndex;

use super::aggregator::FileAggregator;
use super::config::{ExportConfig, RelationEnhancerConfig};
use super::error::ExportError;
use super::formatter::MarkdownFormatter;
use super::path_utils::{
    cleanup_temp_file, compute_nl_doc_output_path, strip_index_context, write_file_atomic,
};
use super::relation_enhancer::RelationEnhancer;
use super::summary_view::ExportSummaryView;

/// Export result
#[derive(Debug, Clone, Default)]
pub struct ExportResult {
    /// Number of files exported
    pub exported_count: usize,
    /// Number of files removed
    pub removed_count: usize,
    /// Failed files
    pub failed: Vec<(PathBuf, String)>,
    /// Output paths
    pub output_paths: Vec<PathBuf>,
}

impl ExportResult {
    /// Create a new export result
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if export was successful (no failures)
    pub fn is_success(&self) -> bool {
        self.failed.is_empty()
    }

    /// Get total processed count
    pub fn total_processed(&self) -> usize {
        self.exported_count + self.removed_count
    }
}

/// Natural language document exporter
///
/// Main exporter for generating natural language documentation from code.
pub struct NlDocumentExporter {
    /// Export configuration
    config: ExportConfig,
    /// File aggregator
    aggregator: FileAggregator,
    /// Markdown formatter
    formatter: MarkdownFormatter,
    /// Relation enhancer (optional, dynamically settable)
    relation_enhancer: RwLock<Option<RelationEnhancer>>,
}

impl NlDocumentExporter {
    /// Create a new exporter
    pub fn new(config: ExportConfig) -> Self {
        let project_root = config.project_root.clone();
        Self {
            config,
            aggregator: FileAggregator::new(),
            formatter: MarkdownFormatter::with_project_root(project_root),
            relation_enhancer: RwLock::new(None),
        }
    }

    /// Create an exporter with relation enhancement
    pub fn with_relation_enhancement(
        config: ExportConfig,
        relation_index: Arc<RelationIndex>,
        enhancer_config: RelationEnhancerConfig,
    ) -> Self {
        let project_root = config.project_root.clone();
        Self {
            config,
            aggregator: FileAggregator::new(),
            formatter: MarkdownFormatter::with_project_root(project_root),
            relation_enhancer: RwLock::new(Some(RelationEnhancer::new(
                relation_index,
                enhancer_config,
            ))),
        }
    }

    /// Get the export configuration
    pub fn config(&self) -> &ExportConfig {
        &self.config
    }

    /// Get the project root directory
    pub fn project_root(&self) -> &Path {
        &self.config.project_root
    }

    /// Compute the output document path for a source file.
    pub fn output_path_for(&self, source_path: &str) -> PathBuf {
        self.compute_output_path(source_path)
    }

    /// Set relation enhancement after construction
    ///
    /// This allows wiring up relation enhancement when the `RelationIndex`
    /// becomes available (e.g. after a full index completes).
    pub fn set_relation_enhancement(
        &self,
        relation_index: Arc<RelationIndex>,
        enhancer_config: RelationEnhancerConfig,
    ) {
        *self
            .relation_enhancer
            .write()
            .expect("Relation enhancer lock poisoned") =
            Some(RelationEnhancer::new(relation_index, enhancer_config));
    }

    /// Clear relation enhancement (disable it)
    pub fn clear_relation_enhancement(&self) {
        *self
            .relation_enhancer
            .write()
            .expect("Relation enhancer lock poisoned") = None;
    }

    /// Render a single file's documentation to Markdown without writing it.
    ///
    /// Returns `(source_path, content)`. This is the render half of
    /// [`Self::export_file`]; the hot-update export processor stages the
    /// content and writes it only during the operation commit phase.
    pub fn render_file(
        &self,
        chunks: &[ChunkedResult],
        summary: Option<&ExportSummaryView>,
    ) -> Result<(String, String), ExportError> {
        let export_chunks = self.prepare_export_chunks(chunks);

        // `include_summary` gates the metadata section (imports, exports,
        // summary line). When false, summaries are omitted entirely.
        let summary = if self.config.include_summary {
            summary.cloned()
        } else {
            None
        };

        // 1. Aggregate chunks into file document
        let mut doc = self.aggregator.aggregate(&export_chunks, summary)?;

        // 2. Apply relation enhancement if enabled
        if self.config.enable_relation_enhancement {
            if let Some(ref enhancer) = *self
                .relation_enhancer
                .read()
                .expect("Relation enhancer lock poisoned")
            {
                enhancer.enhance(&mut doc);
            } else {
                tracing::trace!(
                    path = %doc.source_path,
                    "Relation enhancement enabled but RelationIndex not yet available"
                );
            }
        }

        // 3. Format as Markdown
        let content = self.formatter.format(&doc)?;
        Ok((doc.source_path.clone(), content))
    }

    /// Render a set of direct group exports to Markdown without writing it.
    ///
    /// Returns `(source_path, content)`. Unlike the aggregated path this
    /// operates on enriched `GroupConversions` (the hot-update direct export
    /// path) and injects relation annotations before formatting.
    pub fn render_direct(
        &self,
        conversions: &[cce_parser::ast_to_nl::converter::group_converter::GroupConversions],
        file_path: &str,
        summary: Option<&ExportSummaryView>,
    ) -> Result<(String, String), ExportError> {
        let mut exports: Vec<_> = conversions
            .iter()
            .map(super::direct_generator::DirectExportGenerator::generate)
            .collect::<Result<_, _>>()
            .map_err(ExportError::Formatter)?;

        // Inject relation annotations sourced from the (SQLite-backed)
        // relation index so `enable_relation_enhancement` takes effect on the
        // direct export path as well.
        if self.config.enable_relation_enhancement {
            if let Some(ref enhancer) = *self
                .relation_enhancer
                .read()
                .expect("Relation enhancer lock poisoned")
            {
                for doc in &mut exports {
                    let related = enhancer.related_for_entity(&doc.name, file_path);
                    if !related.is_empty() {
                        doc.related_entities = related;
                    }
                }
            }
        }

        let metadata = if self.config.include_summary {
            summary
                .map(super::formatter::metadata_from_summary_view)
                .unwrap_or_default()
        } else {
            super::formatter::FileExportMetadata::default()
        };
        let content = self
            .formatter
            .format_file_export(file_path, &exports, &metadata)?;
        Ok((file_path.to_string(), content))
    }

    /// Export a single file's documentation
    ///
    /// # Arguments
    ///
    /// * `chunks` - Chunks to export (should all be from the same file)
    /// * `summary` - Optional export summary view. When `None`, the exported
    ///   file omits the metadata section (imports, exports, summary line) and
    ///   only contains entity documentation.
    ///
    /// # Returns
    ///
    /// Path to the exported file
    pub async fn export_file(
        &self,
        chunks: &[ChunkedResult],
        summary: Option<&ExportSummaryView>,
    ) -> Result<PathBuf, ExportError> {
        let (source_path, content) = self.render_file(chunks, summary)?;

        // Write to file
        self.write_document(&source_path, &content).await
    }

    /// Export multiple files
    ///
    /// # Arguments
    ///
    /// * `file_chunks` - Map of file paths to their chunks
    /// * `summaries` - Optional map of file paths to their summaries
    ///
    /// # Returns
    ///
    /// Export result with statistics
    pub async fn export_batch(
        &self,
        file_chunks: &HashMap<String, Vec<ChunkedResult>>,
        summaries: Option<&HashMap<String, ExportSummaryView>>,
    ) -> Result<ExportResult, ExportError> {
        let mut result = ExportResult::new();

        for (file_path, chunks) in file_chunks {
            let summary = summaries.and_then(|s| s.get(file_path));

            match self.export_file(chunks, summary).await {
                Ok(output_path) => {
                    result.exported_count += 1;
                    result.output_paths.push(output_path);
                }
                Err(e) => {
                    result
                        .failed
                        .push((PathBuf::from(file_path), e.to_string()));
                }
            }
        }

        Ok(result)
    }

    /// Prepare chunks for export by stripping index-only sidecar text.
    ///
    /// Export should use the pure presentation text, not the enriched index text
    /// that contains control-flow and behavior sidecars.
    fn prepare_export_chunks(&self, chunks: &[ChunkedResult]) -> Vec<ChunkedResult> {
        let mut cleaned: Vec<ChunkedResult> = chunks
            .iter()
            .filter(|chunk| chunk.path == ChunkPath::Embedding)
            .cloned()
            .map(|mut chunk| {
                chunk.text = strip_index_context(&chunk.text);
                chunk
            })
            .collect();

        if cleaned.is_empty() {
            cleaned = chunks
                .iter()
                .cloned()
                .map(|mut chunk| {
                    chunk.text = strip_index_context(&chunk.text);
                    chunk
                })
                .collect();
        }

        cleaned
    }

    /// Remove an exported document for a deleted file
    ///
    /// # Arguments
    ///
    /// * `source_path` - Path to the source file that was deleted
    pub async fn remove_file(&self, source_path: &Path) -> Result<(), ExportError> {
        let output_path = self.compute_output_path(&source_path.to_string_lossy());

        if output_path.exists() {
            tokio::fs::remove_file(&output_path).await?;
        }
        // Clean up stale temp files from interrupted atomic writes.
        if let Err(e) = cleanup_temp_file(&output_path).await {
            tracing::warn!(
                path = %output_path.display(),
                error = %e,
                "Failed to clean up temp file for removed export"
            );
        }

        Ok(())
    }

    /// Write a document to disk (atomically: temp file + rename)
    async fn write_document(
        &self,
        source_path: &str,
        content: &str,
    ) -> Result<PathBuf, ExportError> {
        // 1. Compute output path
        let output_path = self.compute_output_path(source_path);

        // 2. Write atomically (temp + rename) to keep readers consistent
        write_file_atomic(&output_path, content).await?;

        Ok(output_path)
    }

    /// Compute output path for a source file
    ///
    /// Converts the (possibly absolute) source path to a project-relative path,
    /// then outputs to `.cce/nl_docs/<relative_path>.md`.
    /// This ensures exports are isolated in the `.cce` directory and mirror the
    /// project's source tree structure rather than polluting the source directories.
    fn compute_output_path(&self, source_path: &str) -> PathBuf {
        compute_nl_doc_output_path(
            source_path,
            &self.config.output_dir(),
            &self.config.project_root,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_result() {
        let mut result = ExportResult::new();
        result.exported_count = 5;
        result.removed_count = 2;

        assert!(result.is_success());
        assert_eq!(result.total_processed(), 7);
    }

    #[test]
    fn test_compute_output_path() {
        let config = ExportConfig::new(PathBuf::from("/project"), 1);
        let exporter = NlDocumentExporter::new(config);

        let output = exporter.compute_output_path("src/main.rs");
        assert_eq!(
            output,
            PathBuf::from("/project/.cce/nl_docs/src/main.rs.md")
        );
    }

    #[test]
    fn test_compute_output_path_nested() {
        let config = ExportConfig::new(PathBuf::from("/project"), 1);
        let exporter = NlDocumentExporter::new(config);

        let output = exporter.compute_output_path("src/parser/coordinator.rs");
        assert_eq!(
            output,
            PathBuf::from("/project/.cce/nl_docs/src/parser/coordinator.rs.md")
        );
    }

    #[test]
    fn test_compute_output_path_absolute() {
        let project_root = PathBuf::from("/project");
        let config = ExportConfig::new(project_root, 1);
        let exporter = NlDocumentExporter::new(config);

        // Absolute path should be converted to project-relative
        let output =
            exporter.compute_output_path("/project/benches/fixtures/once_cell/src/imp_cs.rs");
        assert_eq!(
            output,
            PathBuf::from("/project/.cce/nl_docs/benches/fixtures/once_cell/src/imp_cs.rs.md")
        );
    }

    #[test]
    fn test_compute_output_path_windows_longpath() {
        let project_root = PathBuf::from("D:/project");
        let config = ExportConfig::new(project_root, 1);
        let exporter = NlDocumentExporter::new(config);

        // Windows long-path prefix (\\?\) should be stripped
        let output = exporter.compute_output_path(
            "\\\\?\\D:\\project\\benches\\fixtures\\once_cell\\src\\imp_cs.rs",
        );
        assert_eq!(
            output,
            PathBuf::from("D:/project/.cce/nl_docs/benches/fixtures/once_cell/src/imp_cs.rs.md")
        );
    }

    #[test]
    fn test_compute_output_path_lib() {
        let config = ExportConfig::new(PathBuf::from("/project"), 1);
        let exporter = NlDocumentExporter::new(config);

        let output = exporter.compute_output_path("/project/src/lib.rs");
        assert_eq!(output, PathBuf::from("/project/.cce/nl_docs/src/lib.rs.md"));
    }
}
