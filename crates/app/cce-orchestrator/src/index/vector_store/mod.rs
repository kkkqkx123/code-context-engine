//! Backend-enum vector storage dispatch.
//!
//! The assembly layer selects one backend per configuration and uses the same
//! instance for indexing and retrieval. No trait objects are used.

mod fulltext;
mod relation;
mod vector;

pub use fulltext::{FulltextDiagnostics, FulltextStore};
pub use relation::{RelationDiagnostics, RelationStore};
pub use vector::{VectorDiagnostics, VectorStore};
