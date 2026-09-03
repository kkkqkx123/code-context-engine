//! Update processors for hot update
//!
//! This module provides processors for updating downstream modules during hot updates.

mod bm25;
mod context;
mod deletion;
mod embedding;
pub mod factory;
mod rechunk;
mod relation;
mod relation_support;
mod summary;
mod trait_def;

pub use bm25::Bm25UpdateProcessor;
pub use context::ProcessorContext;
pub use deletion::{process_deletions, remove_file_from_storage};
pub use embedding::EmbeddingUpdateProcessor;
pub use factory::ProcessorConfig;
pub use relation::{ExternalPackageData, RelationUpdateProcessor};
pub use summary::SummaryUpdateProcessor;
pub use trait_def::{BoxedUpdateProcessor, ProcessorCollection, UpdateProcessor};
