use super::Visibility;

pub fn visibility_from_signal(_signal: &str) -> Option<Visibility> {
    None
}

pub fn visibility_from_name(name: &str) -> Option<Visibility> {
    let first = name.chars().next()?;
    if first.is_uppercase() {
        Some(Visibility::Public)
    } else {
        Some(Visibility::Package)
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_naming() {
        assert_eq!(visibility_from_name("Exported"), Some(Visibility::Public));
        assert_eq!(visibility_from_name("private"), Some(Visibility::Package));
    }
}
