use crate::symbol::{ScopeContext, Visibility};

/// Python has no modifier-based visibility signals.
pub fn visibility_from_signal(_signal: &str) -> Option<Visibility> {
    None
}

/// Python naming convention.
pub fn visibility_from_name(name: &str) -> Option<Visibility> {
    if name.starts_with("__") && name.ends_with("__") && name.len() > 4 {
        Some(Visibility::Public)
    } else if name.starts_with('_') {
        Some(Visibility::Private)
    } else {
        Some(Visibility::Public)
    }
}

/// Default visibility for Python.
pub fn default_visibility(name: &str) -> Visibility {
    if name.starts_with('_') {
        // Dunder already handled as Public by visibility_from_name
        if name.starts_with("__") && name.ends_with("__") && name.len() > 4 {
            Visibility::Public
        } else {
            Visibility::Private
        }
    } else {
        Visibility::Public
    }
}

/// Python visibility check - permissive.
pub fn is_visible(
    visibility: &Visibility,
    _from_scope: &ScopeContext,
    _defined_in: &ScopeContext,
) -> bool {
    match visibility {
        Visibility::Public => true,
        Visibility::Private => true,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_naming() {
        assert_eq!(visibility_from_name("__init__"), Some(Visibility::Public));
        assert_eq!(visibility_from_name("__all__"), Some(Visibility::Public));
        assert_eq!(visibility_from_name("_private"), Some(Visibility::Private));
        assert_eq!(visibility_from_name("public"), Some(Visibility::Public));
        assert_eq!(visibility_from_name("__mangled"), Some(Visibility::Private));
    }

    #[test]
    fn python_is_visible_always_true() {
        let s = ScopeContext::new("a.py", "pkg");
        assert!(is_visible(&Visibility::Private, &s, &s));
        assert!(is_visible(&Visibility::Public, &s, &s));
    }
}
