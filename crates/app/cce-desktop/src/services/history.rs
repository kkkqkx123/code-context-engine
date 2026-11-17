//! Recent inputs per panel.
//!
//! Panels restore the last query or path from here and offer the rest as
//! one-click re-runs. Entries live in the settings file, so the cap is what
//! keeps that file bounded no matter how long a session runs.

use gpui_kit::App;

use crate::settings;

/// Maximum entries kept per panel.
pub const HISTORY_LIMIT: usize = 20;

/// Maximum length of one entry; longer inputs are dropped rather than
/// truncated, because a truncated query would silently change its meaning.
const ENTRY_MAX_LEN: usize = 512;

/// Panels that keep an input history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryKind {
    Search,
    Graph,
    Entity,
    Index,
    Tools,
}

impl HistoryKind {
    fn key(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Graph => "graph",
            Self::Entity => "entity",
            Self::Index => "index",
            Self::Tools => "tools",
        }
    }
}

/// Entries for `kind`, newest first.
///
/// The stored list is re-capped on read so a hand-edited settings file cannot
/// grow the panel beyond the limit.
pub fn entries(kind: HistoryKind, cx: &App) -> Vec<String> {
    capped(
        settings::global(cx)
            .history
            .get(kind.key())
            .cloned()
            .unwrap_or_default(),
    )
}

/// Record `value` as the newest entry of `kind`.
pub fn push(kind: HistoryKind, value: &str, cx: &mut App) {
    let value = value.trim();
    if value.is_empty() || value.len() > ENTRY_MAX_LEN {
        return;
    }
    let key = kind.key().to_string();
    settings::update(cx, |settings| {
        let entries = settings.history.entry(key).or_default();
        entries.retain(|entry| entry != value);
        entries.insert(0, value.to_string());
        entries.truncate(HISTORY_LIMIT);
    });
}

/// Apply the cap to one list.
fn capped(entries: Vec<String>) -> Vec<String> {
    let mut entries: Vec<String> = entries
        .into_iter()
        .filter(|entry| !entry.trim().is_empty() && entry.len() <= ENTRY_MAX_LEN)
        .collect();
    entries.truncate(HISTORY_LIMIT);
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_keeps_the_newest_entries() {
        let entries = (0..64).map(|ix| format!("query-{ix}")).collect();
        let capped = capped(entries);
        assert_eq!(capped.len(), HISTORY_LIMIT);
        assert_eq!(capped.first().map(String::as_str), Some("query-0"));
        let last = format!("query-{}", HISTORY_LIMIT - 1);
        assert_eq!(capped.last().map(String::as_str), Some(last.as_str()));
    }

    #[test]
    fn cap_drops_blank_and_oversized_entries() {
        let blank = String::new();
        let oversized = "x".repeat(ENTRY_MAX_LEN + 1);
        assert!(capped(vec![blank, oversized]).is_empty());
    }

    #[test]
    fn kinds_map_to_distinct_settings_keys() {
        let keys = [
            HistoryKind::Search.key(),
            HistoryKind::Graph.key(),
            HistoryKind::Entity.key(),
            HistoryKind::Index.key(),
            HistoryKind::Tools.key(),
        ];
        let unique: std::collections::BTreeSet<&str> = keys.iter().copied().collect();
        assert_eq!(unique.len(), keys.len());
    }
}
