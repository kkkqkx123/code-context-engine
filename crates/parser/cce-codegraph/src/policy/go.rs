use crate::symbol::{ScopeContext, Visibility};

/// Go has no explicit visibility modifiers; return `None` for any signal.
pub fn visibility_from_signal(_signal: &str) -> Option<Visibility> {
    None
}

/// Go naming convention: leading uppercase is exported.
pub fn visibility_from_name(name: &str) -> Option<Visibility> {
    let first = name.chars().next()?;
    if first.is_uppercase() {
        Some(Visibility::Public)
    } else {
        Some(Visibility::Package)
    }
}

/// Default visibility for Go when name is empty.
pub fn default_visibility(name: &str) -> Visibility {
    let exported = name
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false);
    if exported {
        Visibility::Public
    } else {
        Visibility::Package
    }
}

/// Go visibility check.
pub fn is_visible(
    visibility: &Visibility,
    from_scope: &ScopeContext,
    defined_in: &ScopeContext,
) -> bool {
    match visibility {
        Visibility::Public => true,
        Visibility::Package | Visibility::Private => from_scope.package == defined_in.package,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_naming() {
        assert_eq!(visibility_from_name("Exported"), Some(Visibility::Public));
        assert_eq!(visibility_from_name("private"), Some(Visibility::Package));
        assert_eq!(visibility_from_name(""), None);
    }

    #[test]
    fn go_default() {
        assert_eq!(default_visibility("MyStruct"), Visibility::Public);
        assert_eq!(default_visibility("field"), Visibility::Package);
        assert_eq!(default_visibility(""), Visibility::Package);
    }

    #[test]
    fn go_is_visible() {
        let from_same = ScopeContext::new("a.go", "pkg");
        let from_other = ScopeContext::new("b.go", "other");
        let def = ScopeContext::new("a.go", "pkg");
        assert!(is_visible(&Visibility::Public, &from_other, &def));
        assert!(is_visible(&Visibility::Package, &from_same, &def));
        assert!(!is_visible(&Visibility::Package, &from_other, &def));
    }
}
