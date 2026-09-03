//! Export module
//!
//! This module provides functionality to export natural language documentation
//! from code entities.
//!
//! # Architecture
//!
//! ```text
//! ChunkedResult[] (multiple chunks)
//!     │
//!     ▼
//! FileAggregator
//!     │
//!     ▼
//! FileNlDocument (file-level document)
//!     │
//!     ▼
//! RelationEnhancer (optional)
//!     │
//!     ▼
//! MarkdownFormatter
//!     │
//!     ▼
//! .cce/nl_docs/ (output)
//! ```
//!
//! # Features
//!
//! - **Semantic Documentation**: Converts code to human-readable natural language
//! - **Compression**: Natural language is more concise than raw code
//! - **Reference Documentation**: Provides basis for code understanding
//! - **Traceability**: Maintains correspondence with source code
//!
//! # Output
//!
//! - Format: Markdown (.md)
//! - Directory: .cce/nl_docs/
//! - Structure: Mirrors source code directory structure

pub(crate) mod aggregator;
pub(crate) mod config;
pub(crate) mod direct_exporter;
pub(crate) mod direct_generator;
pub(crate) mod error;
pub(crate) mod export_config_rebuild;
pub(crate) mod export_fingerprint;
pub(crate) mod export_staging;
pub(crate) mod export_transaction;
pub(crate) mod fingerprint;
pub(crate) mod formatter;
pub(crate) mod nl_exporter;
pub(crate) mod path_utils;
pub(crate) mod presentation;
pub(crate) mod relation_enhancer;
pub(crate) mod summary_view;

// Re-exports
pub use aggregator::{EntityNlDocument, FileAggregator, FileNlDocument, RelatedEntity};
pub use config::{ExportConfig, RelationEnhancerConfig};
pub use direct_exporter::DirectExporter;
pub use direct_generator::{DirectExportDocument, DirectExportGenerator};
pub use error::ExportError;
pub use formatter::{FileExportMetadata, MarkdownFormatter};
pub use nl_exporter::{ExportResult, NlDocumentExporter};
pub use path_utils::{
    cleanup_temp_file, compute_nl_doc_output_path, paths_match, relative_source_path,
    strip_index_context, write_file_atomic,
};
pub use relation_enhancer::RelationEnhancer;
pub use summary_view::ExportSummaryView;
