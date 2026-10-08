use crate::symbol::Visibility;

/// Normalize a visibility signal for case-insensitive matching.
pub fn normalize_signal(signal: &str) -> String {
    signal.to_lowercase().trim().to_string()
}

/// Strip a leading `crate::` segment from a Rust-style module path.
pub fn strip_crate_prefix(path: &str) -> &str {
    path.strip_prefix("crate::").unwrap_or(path)
}

/// Parent module of a `::`-separated path.
pub fn parent_of(path: &str) -> Option<&str> {
    path.rfind("::")
        .map(|i| &path[..i])
        .filter(|p| !p.is_empty())
}

/// Whether a visibility level is considered exported outside its defining scope.
///
/// First phase exports `Public`, `Package`, `Module`, `Restricted`,
/// `Protected` and `Internal` / `ProtectedInternal`. `Private`, `Super`,
/// `PrivateProtected` and `Friend` remain non-exported.
pub fn is_exported_visibility(vis: &Visibility) -> bool {
    matches!(
        vis,
        Visibility::Public
            | Visibility::Package
            | Visibility::Module
            | Visibility::Restricted { .. }
            | Visibility::Protected
            | Visibility::Internal
            | Visibility::ProtectedInternal
    )
}
