//! Core retrieval implementations
//!
//! Low-level, stateless retrieval implementations that interact directly with storage backends.
//! Core modules return raw results without client dependencies or high-level orchestration.

pub mod dense;
pub mod relation;
pub mod summary;
pub mod vector;

pub use dense::DenseRetrieval;
pub use relation::{RelationOptions, RelationRetrieval};
pub use summary::SummaryRetrieval;
pub use vector::FilterOptions;
