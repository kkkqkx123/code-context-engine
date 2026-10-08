use crate::symbol::{ScopeContext, Visibility};

/// Map visibility signals for C++.
pub fn visibility_from_signal(signal: &str) -> Option<Visibility> {
    let trimmed = signal.to_lowercase();
    let trimmed = trimmed.trim();
    match trimmed {
        "public" | "pub" | "export" | "exported" => Some(Visibility::Public),
        "protected" => Some(Visibility::Protected),
        "private" | "pub(self)" | "self" => Some(Visibility::Private),
        _ if trimmed.starts_with("friend") => Some(Visibility::Friend {
            allowed: Vec::new(),
        }),
        _ => None,
    }
}

/// No naming-based visibility for C++.
pub fn visibility_from_name(_name: &str) -> Option<Visibility> {
    None
}

/// Default for C++.
pub fn default_visibility() -> Visibility {
    Visibility::Private
}

/// C++ visibility rules.
pub fn is_visible(
    visibility: &Visibility,
    from_scope: &ScopeContext,
    defined_in: &ScopeContext,
) -> bool {
    match visibility {
        Visibility::Public => true,
        Visibility::Protected => from_scope.package == defined_in.package,
        Visibility::Private => from_scope.file_path == defined_in.file_path,
        Visibility::Friend { allowed } => allowed.contains(&from_scope.file_path),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpp_signal() {
        assert_eq!(visibility_from_signal("public"), Some(Visibility::Public));
        assert_eq!(
            visibility_from_signal("protected"),
            Some(Visibility::Protected)
        );
        assert_eq!(visibility_from_signal("private"), Some(Visibility::Private));
        assert_eq!(
            visibility_from_signal("friend class Foo"),
            Some(Visibility::Friend {
                allowed: Vec::new()
            })
        );
    }

    #[test]
    fn cpp_is_visible() {
        let same = ScopeContext::new("a.cpp", "pkg");
        let def = ScopeContext::new("a.cpp", "pkg");
        assert!(is_visible(&Visibility::Public, &same, &def));
        assert!(!is_visible(
            &Visibility::Private,
            &ScopeContext::new("b.cpp", "pkg"),
            &def
        ));
    }
}
