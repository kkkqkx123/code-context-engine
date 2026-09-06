//! Post-processing module: entity enrichment after capture parsing
//!
//! This module contains independent stages that transform entities after they
//! have been extracted from tree-sitter captures. Each stage is a pure function
//! that takes an entity and enriches it with additional metadata.
//!
//! # Pipeline order
//!
//! 1. `attribute_extractor` - Rust #[...] attributes from source
//! 2. `modifier_extractor` - Visibility/keyword modifiers
//! 3. `stdlib_classifier` - Standard library detection
//! 4. `impl_metadata` - Impl block metadata
//! 5. `child_resolver` - Parent-child relationships

pub mod attribute_extractor;
pub mod child_resolver;
pub mod export_marker;
pub mod impl_metadata;
pub mod modifier;
pub mod modifier_extractor;
pub mod receiver_extractor;
pub mod stdlib_classifier;

pub use attribute_extractor::extract_rust_attributes;
pub use child_resolver::fill_children;
pub use export_marker::mark_cpp_access_sections;
pub use export_marker::mark_exported_entities;
pub use impl_metadata::extract_impl_block_metadata;
pub use modifier_extractor::extract_modifiers;
pub use receiver_extractor::extract_receiver_for_entities;
pub use stdlib_classifier::mark_stdlib;
