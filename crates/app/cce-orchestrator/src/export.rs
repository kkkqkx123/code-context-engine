//! Export staging surface of the orchestration layer.
//!
//! Pure document rendering (aggregation, formatting, presentation,
//! relation enhancement, summary views, path helpers, fingerprinting,
//! config rebuild) lives in `cce-export`. Only the stateful commit stages
//! remain here: fingerprint-guarded skip decisions against the live store,
//! staged writes, and the checkpoint-backed export transaction.

pub(crate) mod export_fingerprint;
pub(crate) mod export_staging;
pub(crate) mod export_transaction;

// Re-export the pure rendering surface so existing paths keep resolving.
pub use cce_export::{
    DirectExportDocument, DirectExportGenerator, DirectExporter, EntityNlDocument, ExportConfig,
    ExportError, ExportResult, ExportSummaryView, FileAggregator, FileExportMetadata,
    FileNlDocument, MarkdownFormatter, NlDocumentExporter, PresentationConverter, RelatedEntity,
    RelationEnhancer, RelationEnhancerConfig, cleanup_temp_file, compute_nl_doc_output_path,
    config_fingerprint, paths_match, rebuild_from_config, relative_source_path, render_fingerprint,
    strip_index_context, summary_content_fingerprint, write_file_atomic,
};
