//! Symbol ID module - fully replaced by `cce_types::EntityId`.
//!
//! This module is kept empty for module path stability during the transition.
//! All symbol identification now uses `cce_types::EntityId` directly.

// EntityId is the canonical symbol identifier.
pub use cce_types::entity::EntityId;
