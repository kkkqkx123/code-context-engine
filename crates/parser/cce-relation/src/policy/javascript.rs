use crate::symbol::{ScopeContext, Visibility};

/// Map visibility signals for JavaScript/TypeScript.
pub fn visibility_from_signal(signal: &str) -> Option<Visibility> {
    let trimmed = signal.to_lowercase();
    let trimmed = trimmed.trim();
    match trimmed {
        "public" | "pub" | "export" | "exported" => Some(Visibility::Public),
        "protected" => Some(Visibility::Protected),
        "private" | "pub(self)" | "self" => Some(Visibility::Private),
        "protected internal" => Some(Visibility::ProtectedInternal),
        "private protected" => Some(Visibility::PrivateProtected),
        "internal" | "package" => Some(Visibility::Internal),
        _ if trimmed.starts_with("friend") => Some(Visibility::Friend {
            allowed: Vec::new(),
        }),
        _ => None,
    }
}

/// JavaScript/TypeScript naming: `#` private fields.
pub fn visibility_from_name(name: &str) -> Option<Visibility> {
    if name.starts_with('#') {
        Some(Visibility::Private)
    } else {
        None
    }
}

/// Default for JS/TS is public.
pub fn default_visibility() -> Visibility {
    Visibility::Public
}

/// JavaScript/TypeScript visibility rules.
pub fn is_visible(
    visibility: &Visibility,
    from_scope: &ScopeContext,
    defined_in: &ScopeContext,
) -> bool {
    match visibility {
        Visibility::Public => true,
        Visibility::Protected => from_scope.package == defined_in.package,
        Visibility::Private => from_scope.file_path == defined_in.file_path,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_signal() {
        assert_eq!(visibility_from_signal("public"), Some(Visibility::Public));
        assert_eq!(visibility_from_signal("private"), Some(Visibility::Private));
        assert_eq!(
            visibility_from_signal("protected"),
            Some(Visibility::Protected)
        );
    }

    #[test]
    fn js_naming() {
        assert_eq!(visibility_from_name("#private"), Some(Visibility::Private));
        assert_eq!(visibility_from_name("public"), None);
    }

    #[test]
    fn js_is_visible() {
        let same = ScopeContext::new("a.ts", "pkg");
        let other = ScopeContext::new("b.ts", "other");
        let def = ScopeContext::new("a.ts", "pkg");
        assert!(is_visible(&Visibility::Public, &other, &def));
        assert!(is_visible(&Visibility::Private, &same, &def));
        assert!(!is_visible(
            &Visibility::Private,
            &ScopeContext::new("b.ts", "pkg"),
            &def
        ));
    }
}
