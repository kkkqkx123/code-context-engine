use super::Visibility;

pub fn visibility_from_signal(_signal: &str) -> Option<Visibility> {
    None
}

pub fn visibility_from_name(name: &str) -> Option<Visibility> {
    if name.starts_with('_') {
        Some(Visibility::Private)
    } else {
        Some(Visibility::Public)
    }
}

pub fn default_visibility(name: &str) -> Visibility {
    if name.starts_with('_') {
        Visibility::Private
    } else {
        Visibility::Public
    }
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
