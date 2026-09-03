use serde::{Deserialize, Serialize};

/// Decomposed module path with optional namespace segments.
///
/// "App::Http::Controllers::UserController" →
///   NamespacePath { segments: ["App", "Http", "Controllers"], module: "UserController" }
///
/// "lib::utils" →
///   NamespacePath { segments: [], module: "utils" }
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NamespacePath {
    /// Namespace segments (outermost to innermost).
    pub segments: Vec<String>,
    /// Leaf module name (always present).
    pub module: String,
}

impl NamespacePath {
    pub fn new(module: impl Into<String>) -> Self {
        Self {
            segments: Vec::new(),
            module: module.into(),
        }
    }

    pub fn with_namespace(segments: Vec<String>, module: impl Into<String>) -> Self {
        Self {
            segments,
            module: module.into(),
        }
    }

    /// Full qualified path with `::` separator.
    pub fn qualified(&self) -> String {
        let mut parts = self.segments.clone();
        parts.push(self.module.clone());
        parts.join("::")
    }

    /// Namespace prefix (all segments except the leaf module).
    pub fn namespace_prefix(&self) -> Option<String> {
        if self.segments.is_empty() {
            None
        } else {
            Some(self.segments.join("::"))
        }
    }

    /// Parse from a `::` separated string.
    pub fn parse(path: &str) -> Self {
        let parts: Vec<&str> = path.split("::").collect();
        if parts.len() <= 1 {
            Self::new(path.to_string())
        } else {
            Self::with_namespace(
                parts[..parts.len() - 1]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                parts.last().expect("last element exists").to_string(),
            )
        }
    }
}

impl From<String> for NamespacePath {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl From<&str> for NamespacePath {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_path_parse_simple() {
        let p = NamespacePath::parse("utils");
        assert_eq!(p.segments, Vec::<String>::new());
        assert_eq!(p.module, "utils");
    }

    #[test]
    fn test_namespace_path_parse_nested() {
        let p = NamespacePath::parse("App::Http::Controllers::UserController");
        assert_eq!(p.segments, vec!["App", "Http", "Controllers"]);
        assert_eq!(p.module, "UserController");
    }

    #[test]
    fn test_namespace_path_qualified() {
        let p = NamespacePath::with_namespace(vec!["A".into(), "B".into()], "C");
        assert_eq!(p.qualified(), "A::B::C");
        assert_eq!(p.namespace_prefix(), Some("A::B".to_string()));
    }

    #[test]
    fn test_namespace_path_empty_segments() {
        let p = NamespacePath::new("mod");
        assert_eq!(p.qualified(), "mod");
        assert_eq!(p.namespace_prefix(), None);
    }

    #[test]
    fn test_namespace_path_parse_single_separator() {
        let p = NamespacePath::parse("A::B");
        assert_eq!(p.segments, vec!["A"]);
        assert_eq!(p.module, "B");
    }
}
