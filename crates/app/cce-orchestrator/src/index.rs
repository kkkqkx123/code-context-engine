//! Index orchestrator module
//!
//! This module provides high-level coordination for indexing operations,
//! organized into sub-modules for better separation of concerns.
//!
//! # Module Structure
//!
//! - `options`: Indexing configuration options
//! - `result`: Indexing result types
//! - `file_indexer`: Deterministic file ordering and checkpoint-backed scanning
//! - `file_processor`: File parsing and processing logic
//! - `storage_coordinator`: Multi-backend storage coordination
//! - `orchestrator`: Main indexing orchestrator
//! - `vector_store`: Backend enums for vector, fulltext and relation storage
//! - `relation_build_spool` / `export_spool`: On-disk staging for large builds
//! - `relation_base_cache`: Materialized relation base reuse across publications
//! - `relation_publisher`: Publication boundary implemented by the server
//! - `resolution_pipeline`: Relation resolution pipeline

mod export_spool;
mod file_indexer;
mod file_processor;
mod options;
mod orchestrator;
mod relation_base_cache;
mod relation_build_spool;
pub mod relation_publisher;
pub mod resolution_pipeline;
mod result;
mod storage_coordinator;
pub mod vector_store;

pub use file_indexer::FileIndexer;
pub use file_processor::FileProcessor;
pub use options::IndexOptions;
pub use orchestrator::DeadLetterRetryReport;
pub use orchestrator::IndexOrchestrator;
pub use relation_base_cache::RelationBaseCache;
pub use relation_publisher::{RelationPublication, RelationSnapshotPublisher};
pub use resolution_pipeline::ResolutionPipelineService;
pub use result::IndexResult;
pub use storage_coordinator::{StorageCoordinator, build_bm25_documents};
pub use vector_store::{FulltextStore, RelationStore, VectorStore};
