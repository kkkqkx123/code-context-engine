//! Entity query handlers
//!
//! This module provides handlers for entity queries including:
//! - Function details
//! - Function calls/callers
//! - Call chains
//! - Class relations
//! - Classification queries

pub mod calls;
pub mod classification;
pub mod detail;
pub mod relation;

pub use calls::{handle_function_callers, handle_function_calls};
pub use classification::{get_classification_stats, get_relations_by_classification};
pub use detail::handle_function_detail;
pub use relation::{
    handle_call_chain, handle_call_path, handle_class_implementations, handle_class_inheritance,
};
