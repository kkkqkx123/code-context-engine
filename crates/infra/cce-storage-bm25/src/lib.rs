//! BM25 full-text index storage client
//!
//! Provides embedded Tantivy-based BM25 indexing and deletion functionality.

pub mod batch;
pub mod client;
pub mod config;
pub mod delete;
pub mod error;
pub mod highlight;
pub mod manager;
pub mod metrics;
pub mod retrieval;
pub mod schema;
pub mod types;

pub use batch::batch_add_documents;
pub use client::Bm25Client;
pub use config::{Bm25AlgorithmConfig, Bm25Config, IndexManagerConfig};
pub use delete::{
    delete_document, delete_documents_by_file_path, delete_documents_by_file_path_and_project,
    delete_documents_by_file_path_project_epoch, delete_documents_by_project,
    delete_documents_by_project_epoch,
};
pub use error::Bm25Error;
pub use manager::IndexManager;
pub use metrics::Bm25Metrics;
pub use retrieval::Bm25Retrieval;
pub use schema::IndexSchema;
pub use types::{Bm25Document, Bm25SearchOptions, Bm25SearchResult, TermOperator};
