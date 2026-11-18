pub(crate) mod connection_service;
pub(crate) mod entity_service;
pub(crate) mod export_service;
pub(crate) mod graph_service;
pub(crate) mod history;
pub(crate) mod index_service;
pub(crate) mod logs_service;
pub(crate) mod ops_service;
pub(crate) mod project_service;
pub(crate) mod refresh;
pub(crate) mod request;
pub(crate) mod search_service;
pub(crate) mod storage_service;
pub(crate) mod time;
pub(crate) mod tools_service;
pub(crate) mod watch_service;

/// Split a comma or newline separated field into trimmed, non-empty entries.
///
/// Panels collect extension lists, path lists and file lists as free text; the
/// wire models take vectors, so the conversion lives here once.
pub fn split_list(text: &str) -> Vec<String> {
    text.split([',', '\n', '\r'])
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

/// Read an optional free-text field as `Some` only when it carries content.
pub fn optional_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_splitting_trims_and_drops_blanks() {
        assert_eq!(
            split_list("rs, ts\n ,go,"),
            vec!["rs".to_string(), "ts".to_string(), "go".to_string()]
        );
        assert!(split_list("   ").is_empty());
    }

    #[test]
    fn blank_text_is_absent() {
        assert_eq!(optional_text("  "), None);
        assert_eq!(optional_text(" src "), Some("src".to_string()));
    }
}
