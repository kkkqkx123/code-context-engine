use crate::symbol::{ScopeContext, Visibility};

/// Map JVM signals for Java/Kotlin.
pub fn visibility_from_signal(signal: &str) -> Option<Visibility> {
    let trimmed = signal.to_lowercase();
    let trimmed = trimmed.trim();
    match trimmed {
        "public" | "pub" | "export" | "exported" => Some(Visibility::Public),
        "protected" => Some(Visibility::Protected),
        "private" | "pub(self)" | "self" => Some(Visibility::Private),
        "internal" | "package" | "crate" | "pub(crate)" => Some(Visibility::Internal),
        "protected internal" => Some(Visibility::ProtectedInternal),
        "private protected" => Some(Visibility::PrivateProtected),
        _ if trimmed.starts_with("friend") => Some(Visibility::Friend {
            allowed: Vec::new(),
        }),
        _ => None,
    }
}

/// No naming-based visibility for Java/Kotlin.
pub fn visibility_from_name(_name: &str) -> Option<Visibility> {
    None
}

/// Default for Java/Kotlin is package-private.
pub fn default_visibility() -> Visibility {
    Visibility::Package
}

/// Java visibility rules.
pub fn is_visible(
    visibility: &Visibility,
    from_scope: &ScopeContext,
    defined_in: &ScopeContext,
) -> bool {
    match visibility {
        Visibility::Public => true,
        Visibility::Protected => from_scope.package == defined_in.package,
        Visibility::Package => from_scope.package == defined_in.package,
        Visibility::Private => from_scope.file_path == defined_in.file_path,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_signal() {
        assert_eq!(visibility_from_signal("public"), Some(Visibility::Public));
        assert_eq!(
            visibility_from_signal("protected"),
            Some(Visibility::Protected)
        );
        assert_eq!(visibility_from_signal("private"), Some(Visibility::Private));
        assert_eq!(visibility_from_signal("unknown"), None);
    }

    #[test]
    fn java_is_visible() {
        let same_pkg = ScopeContext::new("a.java", "pkg");
        let other_pkg = ScopeContext::new("b.java", "other");
        let def = ScopeContext::new("a.java", "pkg");
        assert!(is_visible(&Visibility::Public, &other_pkg, &def));
        assert!(is_visible(&Visibility::Package, &same_pkg, &def));
        assert!(!is_visible(&Visibility::Package, &other_pkg, &def));
        assert!(is_visible(&Visibility::Protected, &same_pkg, &def));
        assert!(!is_visible(&Visibility::Protected, &other_pkg, &def));
    }
}
