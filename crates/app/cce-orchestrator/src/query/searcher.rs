//! Unified searcher (facade)
//!
//! The implementation is split across submodules:
//! - `searcher_core` – core search logic
//! - `search_builder` – builder pattern
//! - `post_processing` – post-processing pipeline

pub mod post_processing;
pub mod search_builder;
pub mod searcher_core;

#[cfg(test)]
mod tests;

pub use search_builder::SearcherBuilder;
pub use searcher_core::Searcher;
pub use searcher_core::expand_multi_entity_results;
