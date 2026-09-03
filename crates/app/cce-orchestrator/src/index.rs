//! Index orchestrator module
//!
//! This module provides high-level coordination for indexing operations,
//! organized into sub-modules for better separation of concerns.
//!
//! # Module Structure
//!
//! - `options`: Indexing configuration options
//! - `result`: Indexing result types
//! - `file_processor`: File parsing and processing logic
//! - `storage_coordinator`: Multi-backend storage coordination
//! - `orchestrator`: Main indexing orchestrator
//! - `resolution_pipeline`: Virtual relation resolution pipeline (Phase 2)

mod export_spool;
mod file_indexer;
mod file_processor;
mod options;
mod orchestrator;
mod relation_base_cache;
mod relation_build_spool;
pub mod relation_publisher;
pub mod relation_store_trait;
pub mod resolution_pipeline;
mod result;
mod storage_coordinator;

pub use file_indexer::FileIndexer;
pub use file_processor::FileProcessor;
pub(crate) use file_processor::read_verified_utf8;
pub use options::IndexOptions;
pub use orchestrator::IndexOrchestrator;
pub use relation_base_cache::RelationBaseCache;
pub use relation_publisher::{RelationPublication, RelationSnapshotPublisher};
pub use resolution_pipeline::ResolutionPipelineService;
pub use result::IndexResult;
pub use storage_coordinator::{StorageCoordinator, build_bm25_documents};
