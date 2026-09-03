use super::Visibility;

pub fn visibility_from_signal(signal: &str) -> Option<Visibility> {
    let t = signal.to_lowercase();
    let t = t.trim();
    if t == "pub" || t == "public" || t == "export" || t == "exported" {
        return Some(Visibility::Public);
    }
    if t == "pub(crate)" || t == "crate" || t == "internal" || t == "package" {
        return Some(Visibility::Package);
    }
    if t == "pub(super)"
        || t == "super"
        || t == "protected"
        || t == "protected internal"
        || t == "private protected"
    {
        return Some(Visibility::Package);
    }
    if t == "pub(self)" || t == "self" || t == "private" {
        return Some(Visibility::Private);
    }
    if t.starts_with("pub(in") {
        return Some(Visibility::Package);
    }
    if t.starts_with("friend") {
        return Some(Visibility::Private);
    }
    None
}

#[allow(dead_code)]
pub fn visibility_from_name(_name: &str) -> Option<Visibility> {
    None
}

pub fn default_visibility() -> Visibility {
    Visibility::Private
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_signal() {
        assert_eq!(visibility_from_signal("pub"), Some(Visibility::Public));
        assert_eq!(
            visibility_from_signal("pub(crate)"),
            Some(Visibility::Package)
        );
        assert_eq!(
            visibility_from_signal("pub(in crate::a)"),
            Some(Visibility::Package)
        );
        assert_eq!(visibility_from_signal("private"), Some(Visibility::Private));
    }
}
