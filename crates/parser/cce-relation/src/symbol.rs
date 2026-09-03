//! Symbol abstraction layer for unified symbol management
//!
//! This module provides core types for symbol identification, metadata,
//! visibility, and scope context. It serves as the foundation for the
//! four-level symbol table architecture.

pub mod id;
pub mod metadata;
pub mod scope;
pub mod visibility;

pub use cce_types::entity::EntityId;
pub use metadata::{SymbolLocation, SymbolMetadata, SymbolRef};
pub use scope::ScopeContext;
pub use visibility::Visibility;
