//! BM25 full-text index storage client
//!
//! Single-crate dual-branch layout: the embedded Tantivy branch (`local`
//! feature) and the remote search-service branch (`remote` feature) share
//! the document types, configuration, error classification, and metrics,
//! with the backend enum at the assembly layer selecting between them.
//! Either branch can be compiled out via its feature for lean builds; the
//! default enables both so existing callers are unaffected.

#[cfg(feature = "local")]
pub mod batch;
#[cfg(feature = "local")]
pub mod client;
pub mod config;
pub mod contract;
#[cfg(feature = "local")]
pub mod delete;
#[cfg(feature = "remote")]
pub mod elasticsearch;
pub mod error;
#[cfg(feature = "local")]
pub mod manager;
pub mod metrics;
#[cfg(feature = "local")]
pub mod retrieval;
#[cfg(feature = "local")]
pub mod schema;

#[cfg(feature = "local")]
pub use batch::batch_add_documents;
pub use cce_config::modules::search::TermOperator;
#[cfg(feature = "local")]
pub use client::Bm25Client;
pub use config::{Bm25AlgorithmConfig, Bm25Config, IndexManagerConfig};
#[cfg(feature = "local")]
pub use delete::{
    delete_document, delete_documents_by_file_path, delete_documents_by_file_path_and_project,
    delete_documents_by_file_path_project_epoch, delete_documents_by_project,
    delete_documents_by_project_epoch,
};
#[cfg(feature = "remote")]
pub use elasticsearch::{ElasticsearchClient, ElasticsearchConfig};
pub use error::Bm25Error;
#[cfg(feature = "local")]
pub use manager::IndexManager;
pub use metrics::Bm25Metrics;
#[cfg(feature = "local")]
pub use retrieval::{Bm25Retrieval, expand_query_tokens};
#[cfg(feature = "local")]
pub use schema::IndexSchema;
