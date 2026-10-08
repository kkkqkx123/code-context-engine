use crate::symbol::{ScopeContext, Visibility};

/// Map visibility signals for C#.
pub fn visibility_from_signal(signal: &str) -> Option<Visibility> {
    let trimmed = signal.to_lowercase();
    let trimmed = trimmed.trim();
    match trimmed {
        "public" | "pub" | "export" | "exported" => Some(Visibility::Public),
        "protected internal" => Some(Visibility::ProtectedInternal),
        "private protected" => Some(Visibility::PrivateProtected),
        "protected" => Some(Visibility::Protected),
        "internal" | "package" | "crate" | "pub(crate)" => Some(Visibility::Internal),
        "private" | "pub(self)" | "self" => Some(Visibility::Private),
        _ if trimmed.starts_with("friend") => Some(Visibility::Friend {
            allowed: Vec::new(),
        }),
        _ => None,
    }
}

/// No naming-based visibility for C#.
pub fn visibility_from_name(_name: &str) -> Option<Visibility> {
    None
}

/// Default for C# members is package/internal for first phase.
pub fn default_visibility() -> Visibility {
    Visibility::Package
}

/// C# visibility rules.
pub fn is_visible(
    visibility: &Visibility,
    from_scope: &ScopeContext,
    defined_in: &ScopeContext,
) -> bool {
    match visibility {
        Visibility::Public => true,
        Visibility::Protected => from_scope.package == defined_in.package,
        Visibility::Internal => from_scope.package == defined_in.package,
        Visibility::Package => from_scope.package == defined_in.package,
        Visibility::ProtectedInternal => from_scope.package == defined_in.package,
        Visibility::PrivateProtected => from_scope.package == defined_in.package,
        Visibility::Private => from_scope.file_path == defined_in.file_path,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csharp_signal() {
        assert_eq!(visibility_from_signal("public"), Some(Visibility::Public));
        assert_eq!(
            visibility_from_signal("internal"),
            Some(Visibility::Internal)
        );
        assert_eq!(
            visibility_from_signal("protected internal"),
            Some(Visibility::ProtectedInternal)
        );
        assert_eq!(
            visibility_from_signal("private protected"),
            Some(Visibility::PrivateProtected)
        );
        assert_eq!(
            visibility_from_signal("protected"),
            Some(Visibility::Protected)
        );
        assert_eq!(visibility_from_signal("private"), Some(Visibility::Private));
    }

    #[test]
    fn csharp_is_visible() {
        let same = ScopeContext::new("a.cs", "pkg");
        let other = ScopeContext::new("b.cs", "other");
        let def = ScopeContext::new("a.cs", "pkg");
        assert!(is_visible(&Visibility::Public, &other, &def));
        assert!(is_visible(&Visibility::Internal, &same, &def));
        assert!(!is_visible(&Visibility::Internal, &other, &def));
        assert!(is_visible(&Visibility::ProtectedInternal, &same, &def));
    }
}
