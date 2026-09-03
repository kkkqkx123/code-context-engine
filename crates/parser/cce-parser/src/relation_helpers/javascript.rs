use super::Visibility;

pub fn visibility_from_signal(signal: &str) -> Option<Visibility> {
    let t = signal.to_lowercase();
    let t = t.trim();
    match t {
        "public" | "pub" | "export" | "exported" => Some(Visibility::Public),
        "private" | "pub(self)" | "self" => Some(Visibility::Private),
        "protected" | "protected internal" | "private protected" | "internal" | "package" => {
            Some(Visibility::Package)
        }
        _ if t.starts_with("friend") => Some(Visibility::Private),
        _ if t.starts_with("pub(in") => Some(Visibility::Package),
        _ => None,
    }
}

pub fn visibility_from_name(name: &str) -> Option<Visibility> {
    if name.starts_with('#') {
        Some(Visibility::Private)
    } else {
        None
    }
}

pub fn default_visibility() -> Visibility {
    Visibility::Public
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_signal() {
        assert_eq!(visibility_from_signal("public"), Some(Visibility::Public));
        assert_eq!(visibility_from_signal("private"), Some(Visibility::Private));
    }

    #[test]
    fn js_naming() {
        assert_eq!(visibility_from_name("#private"), Some(Visibility::Private));
        assert_eq!(visibility_from_name("public"), None);
    }
}
