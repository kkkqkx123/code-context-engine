//! Stable panel identities shared by navigation, docking and export.

use gpui_kit::assets::IconName;
use serde::{Deserialize, Serialize};

use crate::settings::Locale;

/// One navigable workspace panel.
///
/// The `panel_name` value is part of the persisted dock layout. It is a
/// contract with the layout file and never changes with language or title.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelKind {
    Search,
    Graph,
    Entities,
    Index,
    Projects,
    Tools,
    Storage,
    Watch,
    Health,
    Metrics,
    Config,
    Tasks,
}

impl PanelKind {
    pub const WORKSPACE: [Self; 5] = [
        Self::Search,
        Self::Graph,
        Self::Entities,
        Self::Index,
        Self::Projects,
    ];

    pub const OPERATIONS: [Self; 7] = [
        Self::Tools,
        Self::Storage,
        Self::Watch,
        Self::Health,
        Self::Metrics,
        Self::Config,
        Self::Tasks,
    ];

    pub const ALL: [Self; 12] = [
        Self::Search,
        Self::Graph,
        Self::Entities,
        Self::Index,
        Self::Projects,
        Self::Tools,
        Self::Storage,
        Self::Watch,
        Self::Health,
        Self::Metrics,
        Self::Config,
        Self::Tasks,
    ];

    /// Persisted identity in dock layouts and capture snapshots.
    pub fn panel_name(self) -> &'static str {
        match self {
            Self::Search => "cce.search",
            Self::Graph => "cce.graph",
            Self::Entities => "cce.entities",
            Self::Index => "cce.index",
            Self::Projects => "cce.projects",
            Self::Tools => "cce.tools",
            Self::Storage => "cce.storage",
            Self::Watch => "cce.watch",
            Self::Health => "cce.health",
            Self::Metrics => "cce.metrics",
            Self::Config => "cce.config",
            Self::Tasks => "cce.tasks",
        }
    }

    /// File-name slug, without the persisted `cce.` prefix.
    pub fn file_slug(self) -> &'static str {
        self.panel_name().trim_start_matches("cce.")
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Search => "nav.search",
            Self::Graph => "nav.graph",
            Self::Entities => "nav.entities",
            Self::Index => "nav.index",
            Self::Projects => "nav.projects",
            Self::Tools => "nav.tools",
            Self::Storage => "nav.storage",
            Self::Watch => "nav.watch",
            Self::Health => "nav.health",
            Self::Metrics => "nav.metrics",
            Self::Config => "nav.config",
            Self::Tasks => "nav.tasks",
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            Self::Search => IconName::Search,
            Self::Graph => IconName::Network,
            Self::Entities => IconName::Database,
            Self::Index => IconName::Layers,
            Self::Projects => IconName::Folder,
            Self::Tools => IconName::Wrench,
            Self::Storage => IconName::HardDrive,
            Self::Watch => IconName::Activity,
            Self::Health => IconName::Gauge,
            Self::Metrics => IconName::ChartColumn,
            Self::Config => IconName::Settings,
            Self::Tasks => IconName::ListChecks,
        }
    }

    /// Whether the panel needs an active project to produce a result.
    pub fn needs_project(self) -> bool {
        !matches!(
            self,
            Self::Health | Self::Metrics | Self::Config | Self::Projects
        )
    }

    /// Whether the locale is the only input needed to label this panel.
    pub fn label(self, locale: Locale) -> gpui_kit::SharedString {
        gpui_kit::SharedString::from(crate::i18n::t(self.label_key(), locale))
    }
}

impl core::fmt::Display for PanelKind {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.panel_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_names_are_unique_and_stable() {
        let mut names: Vec<&str> = PanelKind::ALL
            .iter()
            .map(|kind| kind.panel_name())
            .collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count);
        assert_eq!(PanelKind::Search.panel_name(), "cce.search");
        assert_eq!(PanelKind::Tasks.panel_name(), "cce.tasks");
    }

    #[test]
    fn scope_rule_matches_panel_groups() {
        assert!(PanelKind::Search.needs_project());
        assert!(PanelKind::Tasks.needs_project());
        assert!(!PanelKind::Projects.needs_project());
        assert!(!PanelKind::Health.needs_project());
    }
}
