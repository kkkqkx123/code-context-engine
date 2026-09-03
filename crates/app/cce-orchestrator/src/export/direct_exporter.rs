//! Direct exporter using enriched GroupConversions.
//!
//! All exports go through the complete processing pipeline (including
//! IndexTextEnricher) to ensure consistent output.

use std::path::PathBuf;
use std::sync::Arc;

use cce_parser::ast_to_nl::converter::group_converter::GroupConversions;

use super::config::ExportConfig;
use super::direct_generator::DirectExportGenerator;
use super::error::ExportError;
use super::formatter::{FileExportMetadata, MarkdownFormatter};
use super::path_utils::{compute_nl_doc_output_path, write_file_atomic};

/// Direct exporter using enriched GroupConversions
pub struct DirectExporter {
    config: ExportConfig,
    formatter: Arc<MarkdownFormatter>,
}

impl DirectExporter {
    pub fn new(config: ExportConfig) -> Self {
        let formatter = Arc::new(MarkdownFormatter::with_project_root(
            config.project_root.clone(),
        ));
        Self { config, formatter }
    }

    /// Export a single group using enriched conversions
    pub async fn export_group(
        &self,
        conversions: &GroupConversions,
        file_path: &str,
    ) -> Result<PathBuf, ExportError> {
        let export_doc =
            DirectExportGenerator::generate(conversions).map_err(ExportError::Formatter)?;
        let metadata = FileExportMetadata::default();
        let content = self
            .formatter
            .format_file_export(file_path, &[export_doc], &metadata)?;
        self.write_document(file_path, &content).await
    }

    /// Export multiple groups using enriched conversions
    pub async fn export_groups(
        &self,
        conversions: &[GroupConversions],
        file_path: &str,
    ) -> Result<PathBuf, ExportError> {
        let exports: Result<Vec<_>, _> = conversions
            .iter()
            .map(DirectExportGenerator::generate)
            .collect::<Result<_, _>>()
            .map_err(ExportError::Formatter);

        let exports = exports?;
        let metadata = FileExportMetadata::default();
        let content = self
            .formatter
            .format_file_export(file_path, &exports, &metadata)?;
        self.write_document(file_path, &content).await
    }

    /// Write document to disk (atomically: temp file + rename)
    async fn write_document(
        &self,
        source_path: &str,
        content: &str,
    ) -> Result<PathBuf, ExportError> {
        let output_path = compute_nl_doc_output_path(
            source_path,
            &self.config.output_dir(),
            &self.config.project_root,
        );

        write_file_atomic(&output_path, content).await?;

        Ok(output_path)
    }
}
