use crate::symbol::{ScopeContext, Visibility};

/// Map PHP visibility signals.
pub fn visibility_from_signal(signal: &str) -> Option<Visibility> {
    let trimmed = signal.to_lowercase();
    let trimmed = trimmed.trim();
    match trimmed {
        "public" => Some(Visibility::Public),
        "protected" => Some(Visibility::Protected),
        "private" => Some(Visibility::Private),
        _ => None,
    }
}

/// No naming-based visibility for PHP.
pub fn visibility_from_name(_name: &str) -> Option<Visibility> {
    None
}

/// Default for PHP is public.
pub fn default_visibility() -> Visibility {
    Visibility::Public
}

/// PHP visibility rules.
pub fn is_visible(
    visibility: &Visibility,
    from_scope: &ScopeContext,
    defined_in: &ScopeContext,
) -> bool {
    match visibility {
        Visibility::Public => true,
        Visibility::Protected => from_scope.package == defined_in.package,
        Visibility::Private => from_scope.file_path == defined_in.file_path,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn php_signal() {
        assert_eq!(visibility_from_signal("public"), Some(Visibility::Public));
        assert_eq!(
            visibility_from_signal("protected"),
            Some(Visibility::Protected)
        );
        assert_eq!(visibility_from_signal("private"), Some(Visibility::Private));
        assert_eq!(visibility_from_signal("static"), None);
    }

    #[test]
    fn php_is_visible() {
        let def = ScopeContext::new("a.php", "App");
        let same_file = ScopeContext::new("a.php", "App");
        let same_pkg = ScopeContext::new("b.php", "App");
        let other = ScopeContext::new("c.php", "Other");
        assert!(is_visible(&Visibility::Public, &other, &def));
        assert!(is_visible(&Visibility::Protected, &same_pkg, &def));
        assert!(!is_visible(&Visibility::Protected, &other, &def));
        assert!(is_visible(&Visibility::Private, &same_file, &def));
        assert!(!is_visible(&Visibility::Private, &same_pkg, &def));
    }
}
