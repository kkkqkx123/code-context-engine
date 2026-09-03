//! Render fingerprint computation and export-skip decision for the export processor.
//!
//! Recovery re-exports a document only when the stored fingerprint still
//! matches the current inputs; otherwise the document may have been rendered
//! under a different configuration and must be regenerated.

use crate::export::config::ExportConfig;
use crate::export::fingerprint::{
    config_fingerprint, render_fingerprint, summary_content_fingerprint,
};
use crate::export::nl_exporter::NlDocumentExporter;
use crate::export::summary_view::ExportSummaryView;
use crate::hot_update::ParseResultWithChanges;

/// Current published relation epoch used for enhancement, or 0 when
/// relation enhancement is disabled or no snapshot is published.
pub fn current_relation_epoch(
    sqlite: &Option<std::sync::Arc<cce_storage_sqlite::SqliteClient>>,
    project_id: i64,
) -> i64 {
    match sqlite.as_ref() {
        Some(sqlite) => {
            match sqlite.project_meta_get_int_optional(project_id, "active_relation_epoch") {
                Ok(Some(epoch)) => epoch.max(0),
                _ => 0,
            }
        }
        None => 0,
    }
}

/// Compute the render fingerprint that pins a rendered document to the
/// inputs it was produced from.
pub async fn compute_render_fingerprint(
    _exporter: &NlDocumentExporter,
    source: &str,
    summary: Option<&ExportSummaryView>,
    export_config: &ExportConfig,
    ast_to_nl_config: &cce_config::AstToNlConfig,
    grouper_fingerprint: &str,
    relation_epoch: i64,
) -> String {
    let export_fp = config_fingerprint(&(
        export_config.include_summary,
        export_config.enable_relation_enhancement,
        export_config.project_root.clone(),
    ));
    let summary_fp = if export_config.include_summary {
        summary_content_fingerprint(summary)
    } else {
        summary_content_fingerprint(None)
    };
    render_fingerprint(
        &export_fp,
        &config_fingerprint(ast_to_nl_config),
        grouper_fingerprint,
        relation_epoch,
        &summary_fp,
        &cce_utils::hash::calculate_hash(source.as_bytes()),
    )
}

/// Decide whether an already-exported document may be skipped on recovery.
///
/// Skips only when the document exists on disk and the persisted render
/// fingerprint still matches the current rendering inputs. A missing
/// stored fingerprint (pre-fingerprint checkpoint) or any configuration or
/// content drift forces a conservative re-export.
pub async fn should_skip_export(
    exporter: &NlDocumentExporter,
    parse_result: &ParseResultWithChanges,
    export_config: &ExportConfig,
    ast_to_nl_config: &cce_config::AstToNlConfig,
    grouper_fingerprint: &str,
    sqlite: &Option<std::sync::Arc<cce_storage_sqlite::SqliteClient>>,
    project_id: i64,
) -> bool {
    if !parse_result.already_exported {
        return false;
    }
    let Some(stored) = parse_result.stored_render_fingerprint.as_ref() else {
        return false;
    };
    let source = &*parse_result.parsed_file.source;
    let epoch = current_relation_epoch(sqlite, project_id);
    let export_view = parse_result
        .file_summary
        .as_ref()
        .map(ExportSummaryView::from);
    let recomputed = compute_render_fingerprint(
        exporter,
        source,
        export_view.as_ref(),
        export_config,
        ast_to_nl_config,
        grouper_fingerprint,
        epoch,
    )
    .await;
    stored == &recomputed
}
