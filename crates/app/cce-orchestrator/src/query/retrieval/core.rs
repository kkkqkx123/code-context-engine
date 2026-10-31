//! Core retrieval implementations
//!
//! Low-level, stateless retrieval implementations that interact directly with storage backends.
//! Core modules return raw results without client dependencies or high-level orchestration.

pub mod dense;
pub mod vector;

pub use dense::DenseRetrieval;
pub use vector::FilterOptions;
