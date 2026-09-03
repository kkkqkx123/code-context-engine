use super::common::{parent_of, strip_crate_prefix};
use crate::symbol::{ScopeContext, Visibility};

/// Map a normalized visibility signal to [`Visibility`] for Rust.
///
/// Recognizes `pub`, `pub(crate)`, `pub(super)`, `pub(self)` and
/// `pub(in path)` forms plus the language-neutral `public`/`export` spellings
/// used in metadata.
pub fn visibility_from_signal(signal: &str) -> Option<Visibility> {
    let trimmed = signal.to_lowercase();
    let trimmed = trimmed.trim();
    if trimmed == "pub" || trimmed == "public" || trimmed == "export" || trimmed == "exported" {
        return Some(Visibility::Public);
    }
    if trimmed == "pub(crate)" || trimmed == "crate" {
        return Some(Visibility::Package);
    }
    if trimmed == "pub(super)" || trimmed == "super" {
        return Some(Visibility::Super);
    }
    if trimmed == "pub(self)" || trimmed == "self" {
        return Some(Visibility::Private);
    }
    if trimmed.starts_with("pub(in") {
        if let Some(open) = trimmed.find('(') {
            if let Some(close) = trimmed.rfind(')') {
                if close > open {
                    let inside = trimmed[open + 1..close].trim();
                    let path = inside
                        .strip_prefix("in")
                        .unwrap_or(inside)
                        .trim()
                        .to_string();
                    return Some(Visibility::Restricted { path });
                }
            }
        }
        return None;
    }
    if trimmed == "private" {
        return Some(Visibility::Private);
    }
    if trimmed.starts_with("friend") {
        return Some(Visibility::Friend {
            allowed: Vec::new(),
        });
    }
    None
}

/// Rust has no naming-based visibility rule.
pub fn visibility_from_name(_name: &str) -> Option<Visibility> {
    None
}

/// Default visibility for Rust when no signal or naming rule applies.
pub fn default_visibility() -> Visibility {
    Visibility::Private
}

/// Rust visibility check.
pub fn is_visible(
    visibility: &Visibility,
    from_scope: &ScopeContext,
    defined_in: &ScopeContext,
) -> bool {
    match visibility {
        Visibility::Public => true,
        Visibility::Package => from_scope.package == defined_in.package,
        Visibility::Module | Visibility::Super => {
            let parent = defined_in.module_path.as_deref().and_then(parent_of);
            match (from_scope.module_path.as_deref(), parent) {
                (Some(from), Some(parent_path)) => {
                    strip_crate_prefix(from) == strip_crate_prefix(parent_path)
                }
                _ => false,
            }
        }
        Visibility::Restricted { path } => {
            let from_norm = from_scope
                .module_path
                .as_deref()
                .map(strip_crate_prefix)
                .unwrap_or("");
            let path_norm = strip_crate_prefix(path);
            from_norm == path_norm || from_norm.starts_with(&format!("{}::", path_norm))
        }
        Visibility::Private => match (
            from_scope.module_path.as_deref(),
            defined_in.module_path.as_deref(),
        ) {
            (Some(from), Some(def)) => from == def,
            _ => from_scope.file_path == defined_in.file_path,
        },
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::language::Language;

    fn scope_with_module(file: &str, package: &str, module: &str) -> ScopeContext {
        ScopeContext::with_module(file, package, module)
    }

    #[test]
    fn rust_signal_pub_variants() {
        assert_eq!(visibility_from_signal("pub"), Some(Visibility::Public));
        assert_eq!(
            visibility_from_signal("pub(crate)"),
            Some(Visibility::Package)
        );
        assert_eq!(
            visibility_from_signal("pub(super)"),
            Some(Visibility::Super)
        );
        assert_eq!(
            visibility_from_signal("pub(self)"),
            Some(Visibility::Private)
        );
        assert_eq!(
            visibility_from_signal("pub(in crate::a::b)"),
            Some(Visibility::Restricted {
                path: "crate::a::b".to_string()
            })
        );
        assert_eq!(
            visibility_from_signal("pub(in a::b)"),
            Some(Visibility::Restricted {
                path: "a::b".to_string()
            })
        );
        assert_eq!(visibility_from_signal("private"), Some(Visibility::Private));
        assert_eq!(visibility_from_signal("unknown"), None);
    }

    #[test]
    fn rust_restricted_visibility() {
        let vis = Visibility::Restricted {
            path: "crate::a".to_string(),
        };
        let def = scope_with_module("src/a/mod.rs", "pkg", "a");
        let from_child = scope_with_module("src/a/b.rs", "pkg", "a::b");
        let from_other = scope_with_module("src/other.rs", "pkg", "other");
        assert!(vis.is_visible_from(&from_child, &def, Language::Rust));
        assert!(is_visible(&vis, &from_child, &def));
        assert!(!is_visible(&vis, &from_other, &def));
    }

    #[test]
    fn rust_super_visibility() {
        let vis = Visibility::Super;
        let def = scope_with_module("src/a/b.rs", "pkg", "a::b");
        let parent = scope_with_module("src/a/mod.rs", "pkg", "a");
        let other = scope_with_module("src/other.rs", "pkg", "other");
        assert!(is_visible(&vis, &parent, &def));
        assert!(!is_visible(&vis, &other, &def));
    }
}
