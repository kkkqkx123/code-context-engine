//! Global UI state shared across views.
//!
//! The [`Client`] itself lives in [`crate::connection::ClientStore`]; this
//! entity only mirrors what the UI renders so views can observe it without
//! touching the client stack. CCE has no login session, so the state tracks
//! the base URL, the connection lifecycle, the latest health probe, the project
//! scope every project-bound panel queries within, and the hand-off slot that
//! carries a search hit into the graph panel.

use gpui_kit::{App, AppContext as _, Entity, Global, SharedString};

use crate::settings::{Density, Locale};

/// Connection lifecycle as displayed in the status bar and by view switching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    /// Server became unreachable while a connection was believed to be live.
    Unhealthy,
}

/// Health snapshot rendered by the status bar and the health panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthSummary {
    pub healthy: bool,
    pub detail: SharedString,
}

/// A project the server reports, trimmed to what the desktop renders.
///
/// The wire model carries the id as a string while every project-scoped call
/// takes an integer, so the parsed value is kept here and unparsable ids never
/// reach a panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectEntry {
    pub id: i64,
    pub name: SharedString,
    pub root_path: SharedString,
}

impl ProjectEntry {
    /// Compact label for pickers: name plus the root path when it differs.
    pub fn label(&self) -> SharedString {
        if self.name.is_empty() {
            self.root_path.clone()
        } else {
            self.name.clone()
        }
    }
}

/// One graph query handed over from the search panel.
///
/// Cross-panel delivery goes through a slot plus a monotonic generation number
/// so the receiving panel can drop a seed it already applied, and a repeated
/// click on the same hit still counts as a new delivery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphSeed {
    pub entity_id: SharedString,
    pub label: SharedString,
    pub generation: u64,
}

pub struct AppState {
    pub status: ConnectionStatus,
    pub base_url: Option<SharedString>,
    pub health: Option<HealthSummary>,
    pub locale: Locale,
    pub density: Density,
    projects: Vec<ProjectEntry>,
    active_project: Option<i64>,
    graph_seed: Option<GraphSeed>,
    seed_generation: u64,
}

pub struct AppStateGlobal(Entity<AppState>);

impl Global for AppStateGlobal {}

impl AppState {
    pub fn init(cx: &mut App) {
        let settings = crate::settings::global(cx);
        let locale = settings.locale;
        let density = settings.density;
        let entity = cx.new(|_| Self {
            status: ConnectionStatus::Disconnected,
            base_url: None,
            health: None,
            locale,
            density,
            projects: Vec::new(),
            active_project: None,
            graph_seed: None,
            seed_generation: 0,
        });
        cx.set_global(AppStateGlobal(entity));
    }

    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<AppStateGlobal>().0.clone()
    }

    /// Switch the interface language and notify observers so views re-render
    /// with the new catalogue.
    pub fn set_locale(&mut self, locale: Locale, cx: &mut gpui_kit::Context<Self>) {
        self.locale = locale;
        cx.notify();
    }

    /// Switch the row density and notify observers.
    pub fn set_density(&mut self, density: Density, cx: &mut gpui_kit::Context<Self>) {
        self.density = density;
        cx.notify();
    }

    pub fn set_connecting(&mut self, cx: &mut gpui_kit::Context<Self>) {
        self.status = ConnectionStatus::Connecting;
        cx.notify();
    }

    pub fn set_connected(
        &mut self,
        base_url: SharedString,
        health: HealthSummary,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        self.status = ConnectionStatus::Connected;
        self.base_url = Some(base_url);
        self.health = Some(health);
        cx.notify();
    }

    pub fn set_disconnected(&mut self, cx: &mut gpui_kit::Context<Self>) {
        self.status = ConnectionStatus::Disconnected;
        self.base_url = None;
        self.health = None;
        self.projects.clear();
        self.active_project = None;
        self.graph_seed = None;
        cx.notify();
    }

    pub fn set_unhealthy(&mut self, unhealthy: bool, cx: &mut gpui_kit::Context<Self>) {
        if self.status == ConnectionStatus::Connected && unhealthy {
            self.status = ConnectionStatus::Unhealthy;
            cx.notify();
        } else if self.status == ConnectionStatus::Unhealthy && !unhealthy {
            self.status = ConnectionStatus::Connected;
            cx.notify();
        }
    }

    /// Replace the known project list. The current scope survives when the
    /// project is still present; otherwise the first project becomes active so
    /// project-bound panels always have a usable scope.
    pub fn set_projects(&mut self, projects: Vec<ProjectEntry>, cx: &mut gpui_kit::Context<Self>) {
        let keep = next_active(self.active_project, &projects);
        self.active_project = keep;
        self.projects = projects;
        cx.notify();
    }

    pub fn projects(&self) -> &[ProjectEntry] {
        &self.projects
    }

    pub fn active_project_id(&self) -> Option<i64> {
        self.active_project
    }

    pub fn active_project(&self) -> Option<&ProjectEntry> {
        let id = self.active_project?;
        self.projects.iter().find(|project| project.id == id)
    }

    pub fn select_project(&mut self, id: i64, cx: &mut gpui_kit::Context<Self>) {
        if self.active_project == Some(id) {
            return;
        }
        if self.projects.iter().any(|project| project.id == id) {
            self.active_project = Some(id);
            cx.notify();
        }
    }

    /// Publish a search hit for the graph panel. Returns the delivery number
    /// the receiver compares against.
    pub fn deliver_graph_seed(
        &mut self,
        entity_id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        cx: &mut gpui_kit::Context<Self>,
    ) -> u64 {
        self.seed_generation += 1;
        let seed = GraphSeed {
            entity_id: entity_id.into(),
            label: label.into(),
            generation: self.seed_generation,
        };
        self.graph_seed = Some(seed);
        cx.notify();
        self.seed_generation
    }

    pub fn graph_seed(&self) -> Option<&GraphSeed> {
        self.graph_seed.as_ref()
    }
}

/// Next active project after the list changed.
///
/// Kept pure so the scope rule is testable without an entity context: the
/// current selection survives while it is still present, otherwise the first
/// project wins, otherwise there is no scope.
fn next_active(current: Option<i64>, projects: &[ProjectEntry]) -> Option<i64> {
    current
        .filter(|id| projects.iter().any(|project| project.id == *id))
        .or_else(|| projects.first().map(|project| project.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(id: i64, name: &str) -> ProjectEntry {
        ProjectEntry {
            id,
            name: SharedString::from(name),
            root_path: SharedString::from(format!("/src/{name}")),
        }
    }

    #[test]
    fn scope_survives_while_the_project_exists() {
        let projects = vec![project(2, "two"), project(7, "seven")];
        assert_eq!(next_active(Some(7), &projects), Some(7));
    }

    #[test]
    fn scope_falls_back_to_the_first_project() {
        let projects = vec![project(2, "two"), project(7, "seven")];
        assert_eq!(next_active(Some(99), &projects), Some(2));
        assert_eq!(next_active(None, &projects), Some(2));
        assert_eq!(next_active(Some(2), &[]), None);
    }

    #[test]
    fn project_label_prefers_the_name() {
        assert_eq!(project(1, "kernel").label(), SharedString::from("kernel"));
        let unnamed = ProjectEntry {
            id: 3,
            name: SharedString::default(),
            root_path: SharedString::from("/src/raw"),
        };
        assert_eq!(unnamed.label(), SharedString::from("/src/raw"));
    }
}
