//! Per-operation staging state for the export processor.
//!
//! Writes and deletions are collected during `process_operation`, flushed to
//! `.cce/nl_docs/` only in `commit_operation`, and rolled back in
//! `abort_operation` so a failed hot update never leaves new documents behind.

use std::path::PathBuf;

use cce_parser::ast_to_nl::chunker::{ChunkedResult, GroupChunker};
use cce_parser::grouper::types::ProcessingResult;
use cce_types::entity::ParsedFile;

use crate::export::config::ExportConfig;
use crate::export::nl_exporter::NlDocumentExporter;
use crate::export::presentation::PresentationConverter;
use crate::export::summary_view::ExportSummaryView;
use crate::hot_update::{HotUpdateError, Result};

use super::export_fingerprint::compute_render_fingerprint;

/// A staged export write that is persisted only during `commit_operation`.
#[derive(Clone)]
pub struct StagedWrite {
    /// Source file path (used for checkpoint bookkeeping)
    pub source_path: String,
    /// Absolute output document path
    pub output_path: PathBuf,
    /// Output path relative to the project root (persisted to `export_path`)
    pub relative_output: String,
    /// Rendered Markdown content
    pub content: String,
    /// Render fingerprint of the inputs used to produce `content`.
    pub render_fingerprint: String,
}

/// Common export configuration parameters shared across staging functions.
pub struct ExportContext<'a> {
    pub exporter: &'a NlDocumentExporter,
    pub export_config: &'a ExportConfig,
    pub ast_to_nl_config: &'a cce_config::AstToNlConfig,
    pub grouper_fingerprint: &'a str,
    pub relation_epoch: i64,
}

/// Per-operation staging state for the export processor.
#[derive(Default)]
pub struct ExportStaging {
    pub writes: Vec<StagedWrite>,
    pub deletions: Vec<PathBuf>,
    /// Writes flushed during the current operation's commit (rolled back on abort).
    pub committed: Vec<StagedWrite>,
    /// Deletions flushed during the current operation's commit.
    pub committed_deletions: Vec<PathBuf>,
    /// (output, backup) pairs created while committing; restored on abort.
    pub backups: Vec<(PathBuf, PathBuf)>,
    /// Outputs that had a pre-existing document backed up during commit.
    pub backed_up: Vec<PathBuf>,
}

impl ExportStaging {
    /// Clear all staging state for a new operation.
    pub fn clear(&mut self) {
        self.writes.clear();
        self.deletions.clear();
        self.committed.clear();
        self.committed_deletions.clear();
        self.backed_up.clear();
    }

    /// Drain stale backups left over from a previously committed operation.
    pub fn drain_stale_backups(&mut self) -> Vec<(PathBuf, PathBuf)> {
        std::mem::take(&mut self.backups)
    }
}

/// Compute the staged write for a rendered source path.
pub async fn make_staged_write(
    exporter: &NlDocumentExporter,
    source_path: &str,
    content: String,
    render_fingerprint: String,
) -> Result<StagedWrite> {
    let output_path = exporter.output_path_for(source_path);
    let project_root = exporter.project_root().to_path_buf();
    let relative_output = output_path
        .strip_prefix(&project_root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| output_path.to_string_lossy().to_string());
    Ok(StagedWrite {
        source_path: source_path.to_string(),
        output_path,
        relative_output,
        content,
        render_fingerprint,
    })
}

/// Handle file update (add/modify) using enriched GroupConversions.
pub async fn stage_file_update_direct(
    ctx: &ExportContext<'_>,
    ast_converter: &cce_parser::ast_to_nl::AstToNlConverter,
    file_path: &std::path::Path,
    processing_result: &ProcessingResult,
    source: &str,
    summary: Option<&ExportSummaryView>,
    staging: &mut ExportStaging,
) -> Result<()> {
    if processing_result.groups.is_empty() {
        return Ok(());
    }

    let conversions = ast_converter.convert_entity_groups(
        &processing_result.groups,
        &file_path.to_string_lossy(),
        None,
        Some(processing_result),
        Some(source),
    );

    let (source_path, content) = ctx
        .exporter
        .render_direct(&conversions, &file_path.to_string_lossy(), summary)
        .map_err(|e| HotUpdateError::export(e.to_string()))?;

    let render_fingerprint = compute_render_fingerprint(
        ctx.exporter,
        source,
        summary,
        ctx.export_config,
        ctx.ast_to_nl_config,
        ctx.grouper_fingerprint,
        ctx.relation_epoch,
    )
    .await;

    staging
        .writes
        .push(make_staged_write(ctx.exporter, &source_path, content, render_fingerprint).await?);

    Ok(())
}

/// Handle file update (add/modify) using chunk-based fallback.
pub async fn stage_file_update(
    ctx: &ExportContext<'_>,
    _file_path: &std::path::Path,
    chunks: &[ChunkedResult],
    source: &str,
    summary: Option<&ExportSummaryView>,
    staging: &mut ExportStaging,
) -> Result<()> {
    if chunks.is_empty() {
        return Ok(());
    }

    let (source_path, content) = ctx
        .exporter
        .render_file(chunks, summary)
        .map_err(|e| HotUpdateError::export(e.to_string()))?;

    let render_fingerprint = compute_render_fingerprint(
        ctx.exporter,
        source,
        summary,
        ctx.export_config,
        ctx.ast_to_nl_config,
        ctx.grouper_fingerprint,
        ctx.relation_epoch,
    )
    .await;

    staging
        .writes
        .push(make_staged_write(ctx.exporter, &source_path, content, render_fingerprint).await?);

    Ok(())
}

/// Stage the removal of a deleted file's document.
pub fn stage_deleted_file(
    exporter: &NlDocumentExporter,
    path: &std::path::Path,
    staging: &mut ExportStaging,
) -> Result<()> {
    let output_path = exporter.output_path_for(&path.to_string_lossy());
    staging.deletions.push(output_path);
    Ok(())
}

/// Extract chunks from parse result.
///
/// This function processes the parse result through the conversion and chunking pipeline:
/// 1. Check if entity groups are available
/// 2. Convert EntityGroup[] to natural language (ConversionResult[])
/// 3. Chunk the conversion results (ChunkedResult[])
pub async fn extract_chunks_from_parse_result(
    converter: &PresentationConverter,
    chunker: &tokio::sync::Mutex<GroupChunker>,
    file_path: &std::path::Path,
    parsed_file: &ParsedFile,
    processing_result: Option<&ProcessingResult>,
) -> Vec<ChunkedResult> {
    let processing_result = match processing_result {
        Some(result) => result,
        None => {
            return Vec::new();
        }
    };

    if processing_result.groups.is_empty() {
        return Vec::new();
    }

    let group_conversions =
        converter.convert_entity_groups(&processing_result.groups, &file_path.to_string_lossy());

    let mut chunker = chunker.lock().await;
    let chunks = chunker.chunk_groups(&group_conversions, &file_path.to_string_lossy());
    let category = cce_parser::summary::FileCategory::determine(parsed_file);
    chunks
        .into_iter()
        .map(|mut chunk| {
            chunk.metadata.file_category = category;
            chunk
        })
        .collect()
}
