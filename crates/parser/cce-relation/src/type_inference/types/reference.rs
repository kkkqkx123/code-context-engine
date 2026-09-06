//! Reference-type helpers for Rust type strings.

/// Strip reference/lifetime annotations from a Rust type name.
pub fn strip_references(type_name: &str) -> (String, bool, bool) {
    let trimmed = type_name.trim();
    let is_ref = trimmed.starts_with('&');
    if !is_ref {
        return (trimmed.to_string(), false, false);
    }
    let rest = trimmed.trim_start_matches('&').trim();
    let (is_mut, rest) = if let Some(stripped) = rest.strip_prefix("mut") {
        (true, stripped.trim())
    } else {
        (false, rest)
    };
    // Strip lifetime `'a`
    let rest = if rest.starts_with('\'') {
        rest.split_whitespace()
            .skip(1)
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        rest.to_string()
    };
    let rest = rest.trim().to_string();
    if rest.is_empty() {
        (trimmed.to_string(), is_mut, is_ref)
    } else {
        (rest, is_mut, is_ref)
    }
}

/// Check if a type is a reference type.
pub fn is_reference(type_name: &str) -> bool {
    type_name.trim().starts_with('&')
}

/// Check if a type is a mutable reference.
pub fn is_mut_reference(type_name: &str) -> bool {
    let trimmed = type_name.trim();
    trimmed.starts_with("&mut")
}

#[cfg(test)]
mod tests {

    use super::*;

    // ==================== strip_references tests ====================

    #[test]
    fn test_strip_references_not_ref() {
        let (name, is_mut, is_ref) = strip_references("String");
        assert_eq!(name, "String");
        assert!(!is_mut);
        assert!(!is_ref);
    }

    #[test]
    fn test_strip_references_immutable_ref() {
        let (name, is_mut, is_ref) = strip_references("&str");
        assert_eq!(name, "str");
        assert!(!is_mut);
        assert!(is_ref);
    }

    #[test]
    fn test_strip_references_mutable_ref() {
        let (name, is_mut, is_ref) = strip_references("&mut String");
        assert_eq!(name, "String");
        assert!(is_mut);
        assert!(is_ref);
    }

    #[test]
    fn test_strip_references_with_lifetime() {
        let (name, is_mut, is_ref) = strip_references("&'a str");
        assert_eq!(name, "str");
        assert!(!is_mut);
        assert!(is_ref);
    }

    #[test]
    fn test_strip_references_empty_after_strip() {
        let (name, is_mut, is_ref) = strip_references("&");
        assert_eq!(name, "&");
        assert!(!is_mut);
        assert!(is_ref);
    }

    // ==================== is_reference / is_mut_reference tests ====================

    #[test]
    fn test_is_reference() {
        assert!(is_reference("&str"));
        assert!(is_reference("&mut String"));
        assert!(is_reference("  &i32"));
        assert!(!is_reference("String"));
        assert!(!is_reference("int"));
    }

    #[test]
    fn test_is_mut_reference() {
        assert!(is_mut_reference("&mut String"));
        assert!(is_mut_reference("  &mut Vec<T>"));
        assert!(!is_mut_reference("&str"));
        assert!(!is_mut_reference("String"));
    }
}
