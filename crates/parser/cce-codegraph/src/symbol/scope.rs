//! Scope context for visibility checking
//!
//! Provides context about where a symbol is being accessed from,
//! used for visibility checks across different languages.

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

/// Scope context for visibility checking
#[derive(Debug, Clone, PartialEq, Eq, Default, Archive, RkyvDeserialize, RkyvSerialize)]
pub struct ScopeContext {
    /// File path
    pub file_path: String,

    /// Package name
    pub package: String,

    /// Module path (language-specific, e.g., "crate::module::submodule")
    pub module_path: Option<String>,

    /// Crate root for Rust `pub(in crate::...)` normalization (optional)
    pub crate_root: Option<String>,
}

impl ScopeContext {
    /// Create new scope context
    pub fn new(file_path: &str, package: &str) -> Self {
        Self {
            file_path: file_path.to_string(),
            package: package.to_string(),
            module_path: None,
            crate_root: None,
        }
    }

    /// Create with module path
    pub fn with_module(file_path: &str, package: &str, module_path: &str) -> Self {
        Self {
            file_path: file_path.to_string(),
            package: package.to_string(),
            module_path: Some(module_path.to_string()),
            crate_root: None,
        }
    }

    /// Create with module path and crate root
    pub fn with_module_and_crate(
        file_path: &str,
        package: &str,
        module_path: &str,
        crate_root: &str,
    ) -> Self {
        Self {
            file_path: file_path.to_string(),
            package: package.to_string(),
            module_path: Some(module_path.to_string()),
            crate_root: Some(crate_root.to_string()),
        }
    }

    /// Set module path
    pub fn set_module_path(&mut self, module_path: String) {
        self.module_path = Some(module_path);
    }

    /// Set crate root
    pub fn set_crate_root(&mut self, crate_root: String) {
        self.crate_root = Some(crate_root);
    }

    /// Get the parent module path (for super:: resolution)
    pub fn parent_module(&self) -> Option<String> {
        self.module_path.as_ref().and_then(|path| {
            path.rfind("::")
                .map(|idx| path[..idx].to_string())
                .filter(|parent| !parent.is_empty())
        })
    }

    /// Check if this scope is within another module path
    pub fn is_within_module(&self, parent_path: &str) -> bool {
        self.module_path
            .as_ref()
            .map(|m| m.starts_with(parent_path))
            .unwrap_or(false)
    }

    /// Get the module name (last component)
    pub fn module_name(&self) -> Option<&str> {
        self.module_path.as_ref().and_then(|path| {
            path.rfind("::")
                .map(|idx| &path[idx + 2..])
                .or(Some(path.as_str()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_context_new() {
        let ctx = ScopeContext::new("src/lib.rs", "my-crate");
        assert_eq!(ctx.file_path, "src/lib.rs");
        assert_eq!(ctx.package, "my-crate");
        assert!(ctx.module_path.is_none());
    }

    #[test]
    fn test_scope_context_with_module() {
        let ctx = ScopeContext::with_module("src/lib.rs", "my-crate", "cce_utils");
        assert_eq!(ctx.module_path, Some("cce_utils".to_string()));
    }

    #[test]
    fn test_parent_module() {
        let ctx = ScopeContext::with_module("src/utils/mod.rs", "my-crate", "cce_utils::helpers");
        assert_eq!(ctx.parent_module(), Some("cce_utils".to_string()));

        let ctx_root = ScopeContext::with_module("src/lib.rs", "my-crate", "crate");
        assert_eq!(ctx_root.parent_module(), None);
    }

    #[test]
    fn test_is_within_module() {
        let ctx =
            ScopeContext::with_module("src/utils/helpers.rs", "my-crate", "cce_utils::helpers");
        assert!(ctx.is_within_module("cce_utils"));
        assert!(!ctx.is_within_module("crate::other"));
    }

    #[test]
    fn test_module_name() {
        let ctx = ScopeContext::with_module("src/utils/mod.rs", "my-crate", "cce_utils");
        assert_eq!(ctx.module_name(), Some("cce_utils"));

        let ctx_nested =
            ScopeContext::with_module("src/utils/helpers.rs", "my-crate", "cce_utils::helpers");
        assert_eq!(ctx_nested.module_name(), Some("helpers"));
    }
}
