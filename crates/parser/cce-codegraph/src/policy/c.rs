use crate::symbol::{ScopeContext, Visibility};

/// Map C storage-class signals.
///
/// `static` gives internal linkage (file-scoped), `extern` gives external
/// linkage (public). Other specifiers are orthogonal to visibility.
pub fn visibility_from_signal(signal: &str) -> Option<Visibility> {
    let trimmed = signal.to_lowercase();
    let trimmed = trimmed.trim();
    match trimmed {
        "static" => Some(Visibility::Private),
        "extern" => Some(Visibility::Public),
        _ => None,
    }
}

/// No naming-based visibility for C.
pub fn visibility_from_name(_name: &str) -> Option<Visibility> {
    None
}

/// Default for C is external linkage.
pub fn default_visibility() -> Visibility {
    Visibility::Public
}

/// C visibility rules.
pub fn is_visible(
    visibility: &Visibility,
    from_scope: &ScopeContext,
    defined_in: &ScopeContext,
) -> bool {
    match visibility {
        Visibility::Public => true,
        Visibility::Private => from_scope.file_path == defined_in.file_path,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_signal() {
        assert_eq!(visibility_from_signal("static"), Some(Visibility::Private));
        assert_eq!(visibility_from_signal("extern"), Some(Visibility::Public));
        assert_eq!(visibility_from_signal("inline"), None);
        assert_eq!(visibility_from_signal("public"), None);
    }

    #[test]
    fn c_is_visible() {
        let def = ScopeContext::new("a.c", "pkg");
        let same = ScopeContext::new("a.c", "pkg");
        let other = ScopeContext::new("b.c", "pkg");
        assert!(is_visible(&Visibility::Public, &other, &def));
        assert!(is_visible(&Visibility::Private, &same, &def));
        assert!(!is_visible(&Visibility::Private, &other, &def));
    }
}
