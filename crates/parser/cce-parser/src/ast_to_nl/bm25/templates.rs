//! BM25 templates for keyword-based text generation
//!
//! This module provides entity-group-oriented templates for generating
//! BM25-optimized text for keyword search.
//!
//! # Architecture
//!
//! Similar to embedding templates, but optimized for keyword matching:
//! - Preserves original names so the tokenizer emits whole-identifier tokens
//!   (spelling-accurate recall) alongside subword splits
//! - Includes normalized names for fuzzy matching
//! - Includes keywords for keyword search
//! - Compresses boilerplate code
//!
//! # Difference from Embedding Templates
//!
//! | Aspect | Embedding | BM25 |
//! |--------|-----------|------|
//! | Goal | Semantic similarity | Keyword matching |
//! | Names | Normalized only | Original + Normalized |
//! | Keywords | Not included | Included |
//! | Boilerplate | Compressed | Compressed |
//! | Counting | Never used | Never used |
//!
//! # Components
//!
//! - `dispatcher`: GroupTemplateDispatcher - dispatches to appropriate template
//! - `group_trait`: GroupTemplate trait - core template interface
//! - `design_patterns`: Design pattern templates
//! - `boilerplate_patterns`: Boilerplate pattern templates
//! - `regular`: Regular entity templates
//!
//! # Usage
//!
//! ```ignore
//! use crate::ast_to_nl::bm25::templates::GroupTemplateDispatcher;
//!
//! let dispatcher = GroupTemplateDispatcher::new();
//! let text = dispatcher.dispatch(&entity_group);
//! ```

pub mod dispatcher;
pub mod getter_setter;
pub mod group_trait;
pub mod regular;
pub mod stdlib;

// Re-exports
pub use dispatcher::GroupTemplateDispatcher;
pub use group_trait::GroupTemplate;
