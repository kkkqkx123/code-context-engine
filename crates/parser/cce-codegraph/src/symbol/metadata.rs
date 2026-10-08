//! Symbol metadata types
//!
//! Provides unified metadata representation for symbols across all languages.
//! Consolidates duplicate fields from ExportSymbol, Entity, etc.
//!
//! # Memory Optimization
//! Uses `Arc<str>` for string fields to minimize cloning overhead when
//! sharing metadata across multiple symbol references.

use super::{scope::ScopeContext, visibility::Visibility};
use cce_types::{
    Span,
    entity::{EntityId, EntityKind},
    language::Language,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Symbol metadata (unified across languages)
///
/// Uses `Arc<str>` for string fields to enable efficient sharing
/// across multiple symbol references without cloning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolMetadata {
    /// Symbol name (Arc<str> for efficient sharing)
    #[serde(with = "arc_str_serde")]
    pub name: Arc<str>,

    /// Symbol kind
    pub kind: EntityKind,

    /// Definition location
    pub location: SymbolLocation,

    /// Visibility
    pub visibility: Visibility,

    /// Symbol source (local, reexport, external)
    pub source: SymbolSource,

    /// Optional documentation (Arc<str> for efficient sharing)
    #[serde(with = "option_arc_str_serde")]
    pub documentation: Option<Arc<str>>,

    /// Language-specific attributes
    #[serde(with = "vec_arc_str_serde")]
    pub attributes: Vec<Arc<str>>,
}

impl SymbolMetadata {
    /// Create new symbol metadata
    pub fn new(name: impl Into<Arc<str>>, kind: EntityKind, location: SymbolLocation) -> Self {
        Self {
            name: name.into(),
            kind,
            location,
            visibility: Visibility::Private,
            source: SymbolSource::Local,
            documentation: None,
            attributes: Vec::new(),
        }
    }

    /// Set visibility
    pub fn with_visibility(mut self, visibility: Visibility) -> Self {
        self.visibility = visibility;
        self
    }

    /// Set source
    pub fn with_source(mut self, source: SymbolSource) -> Self {
        self.source = source;
        self
    }

    /// Set documentation
    pub fn with_documentation(mut self, doc: impl Into<Arc<str>>) -> Self {
        self.documentation = Some(doc.into());
        self
    }

    /// Check if this symbol is visible from a given scope
    pub fn is_visible_from(&self, from: &ScopeContext) -> bool {
        self.visibility.is_visible_from(
            from,
            &self.location.to_scope_context(),
            self.location.language,
        )
    }

    /// Check if this is a reexport
    pub fn is_reexport(&self) -> bool {
        matches!(self.source, SymbolSource::Reexport { .. })
    }

    /// Check if this is from an external package
    pub fn is_external(&self) -> bool {
        matches!(self.source, SymbolSource::External { .. })
    }

    /// Get name as string slice
    pub fn name_str(&self) -> &str {
        &self.name
    }

    /// Get documentation as string slice (if present)
    pub fn documentation_str(&self) -> Option<&str> {
        self.documentation.as_deref()
    }
}

/// Symbol location information
///
/// Uses `Arc<str>` for string fields to enable efficient sharing
/// across multiple symbol references without cloning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolLocation {
    /// File path (Arc<str> for efficient sharing)
    #[serde(with = "arc_str_serde")]
    pub file_path: Arc<str>,

    /// Package path (optional)
    #[serde(with = "option_arc_str_serde")]
    pub package_path: Option<Arc<str>>,

    /// Module path (optional, language-specific)
    #[serde(with = "option_arc_str_serde")]
    pub module_path: Option<Arc<str>>,

    /// Source span
    pub span: Span,

    /// Language
    pub language: Language,
}

/// Helper module for serializing/deserializing Arc<str>
mod arc_str_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::sync::Arc;

    pub fn serialize<S>(value: &Arc<str>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        value.as_ref().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Arc<str>, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(|s| s.into())
    }
}

/// Helper module for serializing/deserializing Option<Arc<str>>
mod option_arc_str_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::sync::Arc;

    pub fn serialize<S>(value: &Option<Arc<str>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        value.as_ref().map(|s| s.as_ref()).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Arc<str>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt = Option::<String>::deserialize(deserializer)?;
        Ok(opt.map(|s| s.into()))
    }
}

/// Helper module for serializing/deserializing Vec<Arc<str>>
mod vec_arc_str_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::sync::Arc;

    pub fn serialize<S>(value: &[Arc<str>], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        value
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>()
            .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Arc<str>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let vec = Vec::<String>::deserialize(deserializer)?;
        Ok(vec.into_iter().map(|s| s.into()).collect())
    }
}

impl SymbolLocation {
    /// Create new symbol location
    pub fn new(file_path: impl Into<Arc<str>>, span: Span, language: Language) -> Self {
        Self {
            file_path: file_path.into(),
            package_path: None,
            module_path: None,
            span,
            language,
        }
    }

    /// Set package path
    pub fn with_package(mut self, package: impl Into<Arc<str>>) -> Self {
        self.package_path = Some(package.into());
        self
    }

    /// Set module path
    pub fn with_module(mut self, module: impl Into<Arc<str>>) -> Self {
        self.module_path = Some(module.into());
        self
    }

    /// Convert to scope context for visibility checking
    pub fn to_scope_context(&self) -> ScopeContext {
        ScopeContext {
            file_path: self.file_path.to_string(),
            package: self
                .package_path
                .as_ref()
                .map(|s| s.to_string())
                .unwrap_or_default(),
            module_path: self.module_path.as_ref().map(|s| s.to_string()),
            crate_root: None,
        }
    }

    /// Get file path as string slice
    pub fn file_path_str(&self) -> &str {
        &self.file_path
    }

    /// Get package path as string slice (if present)
    pub fn package_path_str(&self) -> Option<&str> {
        self.package_path.as_deref()
    }

    /// Get module path as string slice (if present)
    pub fn module_path_str(&self) -> Option<&str> {
        self.module_path.as_deref()
    }
}

/// Symbol source (where the symbol comes from)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SymbolSource {
    /// Locally defined
    #[default]
    Local,

    /// Reexport (with chain information)
    Reexport {
        /// Original file
        original_file: String,
        /// Chain depth (1 = direct reexport)
        chain_depth: u8,
        /// Original name (if different)
        original_name: Option<String>,
    },

    /// External dependency
    External {
        /// Package name
        package: String,
        /// Version (if known)
        version: Option<String>,
    },

    /// Import from another module
    Import {
        /// Import path
        path: String,
        /// Whether it's a wildcard import
        is_wildcard: bool,
    },
}

/// Symbol reference (used in symbol tables)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRef {
    /// Entity ID (unified with the runtime entity ID space)
    pub symbol_id: EntityId,

    /// Symbol metadata
    pub metadata: SymbolMetadata,

    /// Source module (for cross-module references)
    #[serde(with = "option_arc_str_serde")]
    pub source_module: Option<Arc<str>>,
}

impl SymbolRef {
    /// Create new symbol reference
    pub fn new(symbol_id: EntityId, metadata: SymbolMetadata) -> Self {
        Self {
            symbol_id,
            metadata,
            source_module: None,
        }
    }

    /// Set source module
    pub fn with_source_module(mut self, module: impl Into<Arc<str>>) -> Self {
        self.source_module = Some(module.into());
        self
    }

    /// Get entity ID
    pub fn symbol_id(&self) -> EntityId {
        self.symbol_id
    }

    /// Get name as string slice
    pub fn name(&self) -> &str {
        &self.metadata.name
    }

    /// Get visibility
    pub fn visibility(&self) -> &Visibility {
        &self.metadata.visibility
    }

    /// Get source module as string slice (if present)
    pub fn source_module_str(&self) -> Option<&str> {
        self.source_module.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_metadata_builder() {
        let location = SymbolLocation::new(
            "src/lib.rs".to_string(),
            Span {
                start_byte: 0,
                end_byte: 10,
                start_position: Default::default(),
                end_position: Default::default(),
            },
            Language::Rust,
        );

        let metadata = SymbolMetadata::new("MyStruct".to_string(), EntityKind::Struct, location)
            .with_visibility(Visibility::Public)
            .with_documentation("A test struct".to_string());

        assert_eq!(metadata.name, "MyStruct".into());
        assert!(matches!(metadata.visibility, Visibility::Public));
        assert_eq!(metadata.documentation, Some("A test struct".into()));
    }

    #[test]
    fn test_symbol_source_reexport() {
        let source = SymbolSource::Reexport {
            original_file: "src/original.rs".to_string(),
            chain_depth: 1,
            original_name: None,
        };

        assert!(matches!(source, SymbolSource::Reexport { .. }));
    }

    #[test]
    fn test_symbol_ref() {
        let location = SymbolLocation::new(
            "src/lib.rs".to_string(),
            Span {
                start_byte: 0,
                end_byte: 10,
                start_position: Default::default(),
                end_position: Default::default(),
            },
            Language::Rust,
        );

        let metadata = SymbolMetadata::new("test".to_string(), EntityKind::Function, location);
        let entity_id = EntityId(42);
        let symbol_ref = SymbolRef::new(entity_id, metadata);

        assert_eq!(symbol_ref.symbol_id().0, 42);
        assert_eq!(symbol_ref.name(), "test");
    }
}
