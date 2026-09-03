//! Code form converter - transforms Entity/EntityGroup into structured code form descriptions
//!
//! This module provides a lightweight, unified layer for converting parsed entities
//! into structured code form descriptions. It serves as a shared foundation for:
//! - Summary generation (rule_based_generator)
//! - Export generation (direct_exporter)
//! - Other modules requiring code structure understanding
//!
//! # Architecture
//!
//! The converter operates at a semantic level between EntityGroup and NL text generation:
//!
//! ```text
//! EntityGroup → [CodeFormConverter] → CodeForm (lightweight structure)
//!                                         ↓
//!                                    Can be consumed by:
//!                                    - Summary (no NL needed)
//!                                    - Export (for code form export)
//!                                    - AstToNlConverter (for semantic NL)
//! ```
//!
//! # Key Principles
//!
//! - **Lightweight**: Only captures code structure, not NL text
//! - **No Duplication**: Derived directly from EntityGroup, avoiding re-parsing
//! - **Single Responsibility**: Transforms structure, doesn't generate NL
//! - **Reusable**: Can be consumed by multiple downstream processors
//!
//! # Example
//!
//! ```no_run
//! use cce_types::language::Language;
//! use cce_types::{Entity, EntityId, EntityKind, ParsedFile, Span};
//! use cce_parser::ast_to_nl::CodeFormConverter;
//! use cce_parser::grouper::EntityGroup;
//!
//! let entity = Entity::new(
//!     EntityId(1),
//!     EntityKind::Function,
//!     "process_data".to_string(),
//!     Span::default(),
//! );
//! let entity_groups = vec![EntityGroup::from_entity(entity, Language::Rust)];
//! let parsed_file = ParsedFile::default();
//!
//! let code_forms = CodeFormConverter::convert_groups(&entity_groups, &parsed_file);
//!
//! // Now code_forms can be consumed by Summary, Export, or other processors
//! for code_form in code_forms {
//!     println!("Main: {}", code_form.header.name);
//!     for member in &code_form.members {
//!         println!("  - {}: {}", member.kind, member.name);
//!     }
//! }
//! ```

mod converter;
mod types;

pub use converter::CodeFormConverter;
pub use types::{CodeFormContext, CodeFormEntity, CodeFormGroup};

#[cfg(test)]
mod tests;
