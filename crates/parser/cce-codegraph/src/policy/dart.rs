use crate::symbol::{ScopeContext, Visibility};

/// Dart has no modifier-based visibility signals; library-private is naming-based.
pub fn visibility_from_signal(_signal: &str) -> Option<Visibility> {
    None
}

/// Dart naming: leading `_` is library-private.
pub fn visibility_from_name(name: &str) -> Option<Visibility> {
    if name.starts_with('_') {
        Some(Visibility::Private)
    } else {
        Some(Visibility::Public)
    }
}

/// Default visibility for Dart.
pub fn default_visibility(name: &str) -> Visibility {
    if name.starts_with('_') {
        Visibility::Private
    } else {
        Visibility::Public
    }
}

/// Dart visibility is handled as package permissive for now.
/// Falls back to `true` for all, preserving previous permissive behavior.
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
    fn dart_naming() {
        assert_eq!(visibility_from_name("_private"), Some(Visibility::Private));
        assert_eq!(visibility_from_name("public"), Some(Visibility::Public));
    }
}
