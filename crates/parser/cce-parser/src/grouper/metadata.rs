//! Metadata key constants
//!
//! Centralized definitions for all metadata keys used across the grouper module.
//! This ensures consistency between entity producers (parsers) and consumers (detectors/processors).
//!
//! # Usage
//!
//! ```ignore
//! use crate::grouper::metadata;
//!
//! if let Some(methods) = entity.metadata.get(metadata::METHODS) {
//!     // process methods
//! }
//! ```

/// Methods/functions defined in the entity
pub const METHODS: &str = "methods";

/// Fields/properties defined in the entity
pub const FIELDS: &str = "fields";

/// Base types (extends/implements relationships)
pub const BASE_TYPES: &str = "base_types";

/// Constructor definitions
pub const CONSTRUCTORS: &str = "constructors";

/// Access modifiers (public, private, static, etc.)
pub const MODIFIERS: &str = "modifiers";

/// Implemented interfaces (Rust-specific)
pub const IMPLEMENTS: &str = "implements";

/// Local call references within the entity
pub const LOCAL_CALLS: &str = "local_calls";

/// Merged calls metadata (set by call merger processor)
pub const MERGED_CALLS: &str = "merged_calls";

/// Property name that a getter targets
pub const GETTER_FOR: &str = "getter_for";

/// Property name that a setter targets
pub const SETTER_FOR: &str = "setter_for";
