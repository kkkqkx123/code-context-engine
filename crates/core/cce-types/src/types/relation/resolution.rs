//! Persistent cross-session relation resolution
//!
//! Provides stable, source-code-level symbol identification that persists
//! across parsing sessions without depending on ephemeral EntityId values.
//! This enables consistent relation tracking and incremental updates.

pub mod capture;
pub mod relation;
pub mod symbol;
pub mod verification;

pub use capture::RelationCapture;
pub use relation::VirtualRelation;
pub use symbol::VirtualSymbolId;
pub use verification::RelationVerificationStatus;
