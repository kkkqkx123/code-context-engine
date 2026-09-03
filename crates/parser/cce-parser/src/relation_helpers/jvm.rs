use super::Visibility;

pub fn visibility_from_signal(signal: &str) -> Option<Visibility> {
    let t = signal.to_lowercase();
    let t = t.trim();
    match t {
        "public" | "pub" => Some(Visibility::Public),
        "private" => Some(Visibility::Private),
        "protected" | "protected internal" | "private protected" | "internal" | "package"
        | "pub(crate)" | "pub(super)" | "super" => Some(Visibility::Package),
        _ if t.starts_with("friend") => Some(Visibility::Private),
        _ if t.starts_with("pub(in") => Some(Visibility::Package),
        _ => None,
    }
}

#[allow(dead_code)]
pub fn visibility_from_name(_name: &str) -> Option<Visibility> {
    None
}

pub fn default_visibility() -> Visibility {
    Visibility::Package
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jvm_signal() {
        assert_eq!(visibility_from_signal("public"), Some(Visibility::Public));
        assert_eq!(visibility_from_signal("private"), Some(Visibility::Private));
        assert_eq!(
            visibility_from_signal("protected"),
            Some(Visibility::Package)
        );
    }
}
