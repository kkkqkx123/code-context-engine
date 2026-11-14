//! Pure NL document rendering over parsed code.
//!
//! This crate owns every display-side transformation: the NL document
//! exporter, direct exporters, markdown formatting, presentation text,
//! relation enhancement, summary views, path helpers, render fingerprints,
//! and config-rebuild helpers. No type here touches a storage connection,
//! a transaction, or the orchestration layer; stateful commit stages
//! (staging, transactions, processor wiring) stay in `cce-orchestrator` and
//! consume these renderers as injected building blocks.
//!
//! [`presentation::PresentationConverter`] implements the tools'
//! [`cce_tools::SemanticRenderer`] port so compression retrieval renders
//! through the export converter when the caller wires it.

pub mod aggregator;
pub mod config;
pub mod config_rebuild;
pub mod direct_exporter;
pub mod direct_generator;
pub mod error;
pub mod fingerprint;
pub mod formatter;
pub mod nl_exporter;
pub mod path_utils;
pub mod presentation;
pub mod relation_enhancer;
pub mod summary_view;

pub use aggregator::{EntityNlDocument, FileAggregator, FileNlDocument, RelatedEntity};
pub use config::{ExportConfig, RelationEnhancerConfig};
pub use config_rebuild::rebuild_from_config;
pub use direct_exporter::DirectExporter;
pub use direct_generator::{DirectExportDocument, DirectExportGenerator};
pub use error::ExportError;
pub use fingerprint::{config_fingerprint, render_fingerprint, summary_content_fingerprint};
pub use formatter::{FileExportMetadata, MarkdownFormatter};
pub use nl_exporter::{ExportResult, NlDocumentExporter};
pub use path_utils::{
    cleanup_temp_file, compute_nl_doc_output_path, paths_match, relative_source_path,
    strip_index_context, write_file_atomic,
};
pub use presentation::PresentationConverter;
pub use relation_enhancer::RelationEnhancer;
pub use summary_view::ExportSummaryView;
