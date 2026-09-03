//! Virtual symbol identification for cross-file resolution
//!
//! Provides stable, source-code-level symbol identification that persists
//! across parsing sessions without depending on ephemeral EntityId.

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize};
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

use super::super::super::entity::EntityKind;

/// Virtual Symbol ID - stable source-code-level symbol identification
///
/// Unlike EntityId which is parser-internal and ephemeral, VirtualSymbolId
/// provides stable identification based on source code location and scope.
/// This enables cross-session symbol tracking and persistent relation resolution.
///
/// Used for constructing stable relation records without EntityId dependency.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    SerdeSerialize,
    SerdeDeserialize,
    Archive,
    RkyvDeserialize,
    Serialize,
)]
pub struct VirtualSymbolId {
    /// File path where the symbol is defined
    pub file_path: String,
    /// Scoped name (e.g., "Module::Class::method")
    pub scoped_name: String,
    /// Entity kind
    pub kind: EntityKind,
    /// Symbol table version
    pub version: u64,
}

impl VirtualSymbolId {
    /// Create a new virtual symbol ID
    pub fn new(file_path: String, scoped_name: String, kind: EntityKind, version: u64) -> Self {
        Self {
            file_path,
            scoped_name,
            kind,
            version,
        }
    }

    /// Unique key for lookup (ignoring version)
    pub fn key(&self) -> (&str, &str, EntityKind) {
        (
            self.file_path.as_str(),
            self.scoped_name.as_str(),
            self.kind,
        )
    }
}

impl std::fmt::Display for VirtualSymbolId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}::{} ({}:v{})",
            self.file_path, self.scoped_name, self.kind, self.version
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtual_symbol_id_creation() {
        let symbol = VirtualSymbolId::new(
            "src/main.rs".to_string(),
            "Module::Class::method".to_string(),
            EntityKind::Function,
            1,
        );

        assert_eq!(symbol.file_path, "src/main.rs");
        assert_eq!(symbol.scoped_name, "Module::Class::method");
        assert_eq!(symbol.kind, EntityKind::Function);
        assert_eq!(symbol.version, 1);
    }

    #[test]
    fn test_virtual_symbol_id_key() {
        let symbol = VirtualSymbolId::new(
            "src/main.rs".to_string(),
            "Module::Class".to_string(),
            EntityKind::Class,
            1,
        );

        let key = symbol.key();
        assert_eq!(key.0, "src/main.rs");
        assert_eq!(key.1, "Module::Class");
        assert_eq!(key.2, EntityKind::Class);
    }

    #[test]
    fn test_virtual_symbol_id_display() {
        let symbol = VirtualSymbolId::new(
            "src/main.rs".to_string(),
            "Module::Class".to_string(),
            EntityKind::Class,
            1,
        );

        let display = format!("{}", symbol);
        assert_eq!(display, "src/main.rs::Module::Class (class:v1)");
    }
}
