//! Multi-language visibility model
//!
//! Provides unified visibility enum supporting various language models:
//! - Rust: pub/pub(crate)/pub(super)/pub(in path)
//! - Java: public/protected/package/private
//! - Go: exported/unexported (based on naming)
//! - Python: public/_private/__dunder__
//! - C#: public/internal/protected/private
//! - C++: public/protected/private/friend
//!
//! # Inheritance Support
//! Inheritance-aware protected member visibility checking is provided
//! by the `relation::oop` sub-module. Use
//! [`oop::is_visible_from_with_inheritance`](crate::oop::is_visible_from_with_inheritance)
//! when analyzing OOP languages that need subclass relationship checks.

use cce_types::language::Language;
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

/// Unified visibility enumeration supporting multi-language models
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    Default,
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
)]
pub enum Visibility {
    /// Fully public (Rust `pub`, Java `public`, Go exported)
    Public,

    /// Package/library level visibility (Rust `pub(crate)`, Java package-private)
    Package,

    /// Module/submodule level visibility (Rust `pub(super)`)
    Module,

    /// Parent level visibility
    Super,

    /// Private (current scope only)
    #[default]
    Private,

    /// Restricted visibility (Rust `pub(in path)`)
    Restricted { path: String },

    /// Protected (Java/Kotlin/C++)
    Protected,

    /// Internal visibility (C# `internal`)
    Internal,

    /// Protected internal (C# `protected internal`)
    ProtectedInternal,

    /// Private protected (C# `private protected`)
    PrivateProtected,

    /// Friend visibility (C++ `friend`)
    Friend { allowed: Vec<String> },
}

impl Visibility {
    /// Check if visible from a specific context (language-aware)
    ///
    /// Delegates to per-language policy in [`crate::policy`]. The skeleton
    /// dispatch is retained here to preserve the public API while language
    /// specifics live in `policy::*` submodules.
    pub fn is_visible_from(
        &self,
        from_scope: &super::scope::ScopeContext,
        defined_in: &super::scope::ScopeContext,
        language: Language,
    ) -> bool {
        crate::policy::is_visible(self, from_scope, defined_in, language)
    }

    /// Get the most permissive visibility for a language's default
    pub fn default_for_language(language: Language) -> Self {
        match language {
            Language::Rust => Visibility::Private,
            Language::Java => Visibility::Package, // package-private
            Language::Go => Visibility::Public,    // exported if uppercase
            Language::CSharp => Visibility::Private,
            Language::Cpp => Visibility::Private,
            Language::Python => Visibility::Public,
            Language::JavaScript | Language::TypeScript => Visibility::Public,
            _ => Visibility::Public,
        }
    }

    /// Check if this visibility level is at least as permissive as other
    pub fn is_at_least(&self, other: &Visibility) -> bool {
        use Visibility::*;
        match (self, other) {
            (Public, _) => true,
            (ProtectedInternal, Public) => false,
            (ProtectedInternal, _) => true,
            (Protected, Public | ProtectedInternal) => false,
            (Protected, _) => true,
            (Internal, Public | ProtectedInternal | Protected) => false,
            (Internal, _) => true,
            (Package, Public | ProtectedInternal | Protected | Internal) => false,
            (Package, _) => true,
            (Module, Public | ProtectedInternal | Protected | Internal | Package) => false,
            (Module, _) => true,
            (Super, Public | ProtectedInternal | Protected | Internal | Package | Module) => false,
            (Super, _) => true,
            (
                PrivateProtected,
                Public | ProtectedInternal | Protected | Internal | Package | Module | Super,
            ) => false,
            (PrivateProtected, _) => true,
            (
                Private,
                Public | ProtectedInternal | Protected | Internal | Package | Module | Super
                | PrivateProtected,
            ) => false,
            (Private, Private | Restricted { .. } | Friend { .. }) => true,
            (
                Restricted { .. },
                Public | ProtectedInternal | Protected | Internal | Package | Module | Super
                | PrivateProtected | Private,
            ) => false,
            (Restricted { .. }, Restricted { .. }) => true,
            (Restricted { .. }, Friend { .. }) => true,
            (
                Friend { .. },
                Public
                | ProtectedInternal
                | Protected
                | Internal
                | Package
                | Module
                | Super
                | PrivateProtected
                | Private
                | Restricted { .. },
            ) => false,
            (Friend { .. }, Friend { .. }) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::scope::ScopeContext;

    #[test]
    fn test_visibility_rust_public() {
        let vis = Visibility::Public;
        let from = ScopeContext::new("src/a.rs", "pkg");
        let def = ScopeContext::new("src/b.rs", "pkg");

        assert!(vis.is_visible_from(&from, &def, Language::Rust));
    }

    #[test]
    fn test_visibility_rust_private() {
        let vis = Visibility::Private;
        let from = ScopeContext::new("src/a.rs", "pkg");
        let def_same = ScopeContext::new("src/a.rs", "pkg");
        let def_diff = ScopeContext::new("src/b.rs", "pkg");

        assert!(vis.is_visible_from(&from, &def_same, Language::Rust));
        assert!(!vis.is_visible_from(&from, &def_diff, Language::Rust));
    }

    #[test]
    fn test_is_at_least() {
        assert!(Visibility::Public.is_at_least(&Visibility::Private));
        assert!(Visibility::Public.is_at_least(&Visibility::Public));
        assert!(!Visibility::Private.is_at_least(&Visibility::Public));
        assert!(Visibility::Package.is_at_least(&Visibility::Private));
    }
}
