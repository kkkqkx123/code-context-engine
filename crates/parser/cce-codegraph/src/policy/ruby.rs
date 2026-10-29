use crate::symbol::{ScopeContext, Visibility};

/// Map Ruby visibility signals.
///
/// Ruby has no modifier extraction; signals arrive via metadata when the
/// extractor records access sections. Kept explicit so the fallback is
/// documented rather than silent.
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

/// Ruby naming rule: `initialize` is always private.
pub fn visibility_from_name(name: &str) -> Option<Visibility> {
    if name == "initialize" {
        Some(Visibility::Private)
    } else {
        None
    }
}

/// Default for Ruby is public.
pub fn default_visibility() -> Visibility {
    Visibility::Public
}

/// Ruby is dynamic; cross-file resolution stays permissive by design.
pub fn is_visible(
    _visibility: &Visibility,
    _from_scope: &ScopeContext,
    _defined_in: &ScopeContext,
) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruby_signal() {
        assert_eq!(visibility_from_signal("public"), Some(Visibility::Public));
        assert_eq!(
            visibility_from_signal("protected"),
            Some(Visibility::Protected)
        );
        assert_eq!(visibility_from_signal("private"), Some(Visibility::Private));
        assert_eq!(visibility_from_signal("static"), None);
    }

    #[test]
    fn ruby_naming() {
        assert_eq!(
            visibility_from_name("initialize"),
            Some(Visibility::Private)
        );
        assert_eq!(visibility_from_name("save"), None);
    }

    #[test]
    fn ruby_permissive() {
        let def = ScopeContext::new("a.rb", "pkg");
        let other = ScopeContext::new("b.rb", "other");
        assert!(is_visible(&Visibility::Private, &other, &def));
    }
}
