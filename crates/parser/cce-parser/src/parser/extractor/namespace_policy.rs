use cce_types::NamespacePath;
use cce_types::language::Language;

/// Language-specific namespace semantics.
pub trait NamespacePolicy: Send + Sync {
    /// Whether namespace declarations span the entire file scope.
    ///
    /// C# and PHP: namespace declaration covers the file from declaration to end.
    /// C++: namespace body has explicit braces.
    /// Rust/Go: no namespace concept (use module path instead).
    fn covers_file_scope(&self) -> bool;

    /// Whether namespace can be nested.
    fn supports_nesting(&self) -> bool;

    /// Whether the namespace declaration creates an implicit visibility scope.
    ///
    /// C#: namespace members are accessible within the namespace.
    /// C++: namespace members need `using` to be accessible.
    fn creates_visibility_scope(&self) -> bool;

    /// The separator used in qualified names.
    fn separator(&self) -> &str {
        "::"
    }

    /// Parse a qualified name into namespace segments and leaf name.
    fn parse_qualified(&self, name: &str) -> NamespacePath {
        NamespacePath::parse(name)
    }
}

pub struct CSharpNamespacePolicy;

impl NamespacePolicy for CSharpNamespacePolicy {
    fn covers_file_scope(&self) -> bool {
        true
    }
    fn supports_nesting(&self) -> bool {
        true
    }
    fn creates_visibility_scope(&self) -> bool {
        true
    }
    fn separator(&self) -> &str {
        "."
    }

    fn parse_qualified(&self, name: &str) -> NamespacePath {
        let parts: Vec<&str> = name.split('.').collect();
        if parts.len() <= 1 {
            NamespacePath::new(name.to_string())
        } else {
            NamespacePath::with_namespace(
                parts[..parts.len() - 1]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                parts.last().expect("last exists").to_string(),
            )
        }
    }
}

pub struct PhpNamespacePolicy;

impl NamespacePolicy for PhpNamespacePolicy {
    fn covers_file_scope(&self) -> bool {
        true
    }
    fn supports_nesting(&self) -> bool {
        true
    }
    fn creates_visibility_scope(&self) -> bool {
        true
    }
    fn separator(&self) -> &str {
        "\\"
    }

    fn parse_qualified(&self, name: &str) -> NamespacePath {
        let parts: Vec<&str> = name.split('\\').collect();
        if parts.len() <= 1 {
            NamespacePath::new(name.to_string())
        } else {
            NamespacePath::with_namespace(
                parts[..parts.len() - 1]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                parts.last().expect("last exists").to_string(),
            )
        }
    }
}

pub struct CppNamespacePolicy;

impl NamespacePolicy for CppNamespacePolicy {
    fn covers_file_scope(&self) -> bool {
        false
    }
    fn supports_nesting(&self) -> bool {
        true
    }
    fn creates_visibility_scope(&self) -> bool {
        false
    }
    fn separator(&self) -> &str {
        "::"
    }
}

/// Get the namespace policy for a language, if it has namespace semantics.
pub fn namespace_policy_for(lang: Language) -> Option<Box<dyn NamespacePolicy>> {
    match lang {
        Language::CSharp => Some(Box::new(CSharpNamespacePolicy)),
        Language::Php => Some(Box::new(PhpNamespacePolicy)),
        Language::Cpp => Some(Box::new(CppNamespacePolicy)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csharp_namespace_parsing() {
        let policy = CSharpNamespacePolicy;
        let path = policy.parse_qualified("System.Collections.Generic");
        assert_eq!(path.segments, vec!["System", "Collections"]);
        assert_eq!(path.module, "Generic");
    }

    #[test]
    fn test_php_namespace_parsing() {
        let policy = PhpNamespacePolicy;
        let path = policy.parse_qualified("App\\Http\\Controllers");
        assert_eq!(path.segments, vec!["App", "Http"]);
        assert_eq!(path.module, "Controllers");
    }

    #[test]
    fn test_cpp_namespace_default() {
        let policy = CppNamespacePolicy;
        assert!(!policy.covers_file_scope());
        assert!(policy.supports_nesting());
        assert!(!policy.creates_visibility_scope());
        assert_eq!(policy.separator(), "::");
        let path = policy.parse_qualified("A::B::C");
        assert_eq!(path.segments, vec!["A", "B"]);
        assert_eq!(path.module, "C");
    }

    #[test]
    fn test_namespace_policy_for_unknown() {
        assert!(namespace_policy_for(Language::Rust).is_none());
        assert!(namespace_policy_for(Language::Go).is_none());
        assert!(namespace_policy_for(Language::Python).is_none());
    }

    #[test]
    fn test_namespace_policy_for_supported() {
        assert!(namespace_policy_for(Language::CSharp).is_some());
        assert!(namespace_policy_for(Language::Php).is_some());
        assert!(namespace_policy_for(Language::Cpp).is_some());
    }
}
