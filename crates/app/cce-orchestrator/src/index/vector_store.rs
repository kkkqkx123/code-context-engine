//! Backend-enum vector storage dispatch.
//!
//! The assembly layer selects one backend per configuration and uses the same
//! instance for indexing and retrieval. No trait objects are used.

mod fulltext;
mod metadata;
mod vector;

pub use fulltext::{FulltextDiagnostics, FulltextStore};
pub use metadata::{
    MetadataDiagnostics, MetadataStore, in_memory_metadata_store, open_metadata_store,
};
pub use vector::{VectorBatchWriter, VectorDiagnostics, VectorStore};
