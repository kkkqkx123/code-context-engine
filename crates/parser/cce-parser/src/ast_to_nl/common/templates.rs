//! Common template utilities shared by BM25 and Embedding generators
//!
//! This module provides shared template infrastructure:
//! - GroupTemplateBase trait: Common methods for member filtering
//! - Helper functions: Shared utilities for text processing

pub mod group_trait_base;
pub mod helpers;

// Re-export main types
pub use group_trait_base::GroupTemplateBase;
pub use helpers::TemplateHelpers;
