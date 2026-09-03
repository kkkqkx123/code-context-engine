use super::Visibility;

#[allow(dead_code)]
pub fn is_exported(vis: &Visibility) -> bool {
    !matches!(vis, Visibility::Private)
}
