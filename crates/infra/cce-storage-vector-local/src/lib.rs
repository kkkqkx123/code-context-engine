//! Embedded local vector storage.
//!
//! Wraps the vendored `simvec` engine (`crates/simvec`) behind the shared
//! `VectorStorage` contract. One logical collection (same name as the Qdrant
//! branch) holds all projects isolated by `group_id` payload, so generation,
//! grouping and truncation semantics stay identical across backends.

mod store;

pub use store::LocalVectorStore;
pub use store::{map_simvec_error, payload_to_map};
