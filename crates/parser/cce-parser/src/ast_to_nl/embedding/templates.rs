//! Embedding templates for semantic summary generation
//!
//! This module provides entity-group-oriented templates for generating
//! semantic summaries optimized for vector embedding.
//!
//! # Architecture
//!
//! Unlike the old single-entity templates, these templates operate on
//! EntityGroup level, enabling:
//! - Boilerplate code compression
//! - Pattern-aware description generation
//! - No counting information (pure semantic descriptions)
//!
//! # Components
//!
//! - `dispatcher`: GroupTemplateDispatcher - dispatches to appropriate template
//! - `group_trait`: GroupTemplate trait - core template interface
//! - `design_patterns`: Design pattern templates (Builder, Factory, etc.)
//! - `boilerplate_patterns`: Boilerplate pattern templates (DTO, Repository, etc.)
//! - `regular`: Regular entity templates (Class, Function, etc.)
//!
//! # Usage
//!
//! ```ignore
//! use crate::ast_to_nl::embedding::templates::GroupTemplateDispatcher;
//!
//! let dispatcher = GroupTemplateDispatcher::new();
//! let descriptions = dispatcher.dispatch(&entity_group);
//! ```

pub mod dispatcher;
pub mod getter_setter;
pub mod group_trait;
pub mod regular;
pub mod stdlib;

// Re-exports
pub use dispatcher::GroupTemplateDispatcher;
pub use group_trait::GroupTemplate;
