//! Project commands and the shared project scope.
//!
//! Every project-bound panel queries within the scope held by
//! [`crate::state::AppState`]; this service keeps that scope in sync with the
//! server and performs the lifecycle calls the projects panel triggers.

use cce_api::models::{
    CreateProjectRequest, ProjectConfig, ProjectConfigGetResponse, ProjectConfigReloadResponse,
    ProjectDeleteResponse, ProjectIndexResponse, ProjectListResponse, UpdateProjectRequest,
};
use cce_client::{Client, ClientError};
use gpui_kit::{App, AsyncApp, Task};

use crate::connection::ClientStore;
use crate::runtime;
use crate::state::{AppState, ProjectEntry};

/// Wire id of a project is a string, while every project-scoped call takes an
/// integer. Entries that cannot be parsed are dropped instead of being passed
/// on as a zero id.
pub fn entry(project: &ProjectConfig) -> Option<ProjectEntry> {
    match project.id.parse::<i64>() {
        Ok(id) => Some(ProjectEntry {
            id,
            name: project.name.clone().into(),
            root_path: project.root_path.clone().into(),
        }),
        Err(err) => {
            log::warn!("ignoring project {} with unusable id: {err}", project.name);
            None
        }
    }
}

pub fn list(cx: &App, client: Client) -> Task<Result<ProjectListResponse, ClientError>> {
    runtime::spawn(cx, async move { client.list_projects().await })
}

pub fn create(
    cx: &App,
    client: Client,
    request: CreateProjectRequest,
) -> Task<Result<cce_api::models::ProjectDetailResponse, ClientError>> {
    runtime::spawn(cx, async move { client.create_project(&request).await })
}

/// Patch a project's editable fields.
///
/// `root_path` is not editable through this call: the server keeps a project's
/// root fixed after creation, so a wrong root is fixed by creating a new
/// project rather than by moving an indexed one.
pub fn update(
    cx: &App,
    client: Client,
    project_id: i64,
    request: UpdateProjectRequest,
) -> Task<Result<cce_api::models::ProjectDetailResponse, ClientError>> {
    runtime::spawn(cx, async move {
        client.update_project(project_id, &request).await
    })
}

pub fn delete(
    cx: &App,
    client: Client,
    project_id: i64,
) -> Task<Result<ProjectDeleteResponse, ClientError>> {
    runtime::spawn(cx, async move { client.delete_project(project_id).await })
}

pub fn index(
    cx: &App,
    client: Client,
    project_id: i64,
) -> Task<Result<ProjectIndexResponse, ClientError>> {
    runtime::spawn(cx, async move { client.index_project(project_id).await })
}

pub fn config(
    cx: &App,
    client: Client,
    project_id: i64,
) -> Task<Result<ProjectConfigGetResponse, ClientError>> {
    runtime::spawn(cx, async move { client.project_config(project_id).await })
}

pub fn reload(
    cx: &App,
    client: Client,
    project_id: i64,
) -> Task<Result<ProjectConfigReloadResponse, ClientError>> {
    runtime::spawn(cx, async move { client.reload_project(project_id).await })
}

/// Reload the shared project scope from the server.
///
/// Runs at application level rather than inside a panel so a reconnect
/// restores the scope even while another panel is on screen. A failure leaves
/// the previous scope untouched and is left to the projects panel to surface.
pub fn refresh_scope(cx: &mut App) {
    let Some(client) = ClientStore::client(cx) else {
        return;
    };
    let task = list(cx, client);
    cx.spawn(async move |cx: &mut AsyncApp| {
        let outcome = task.await;
        cx.update(|cx| match outcome {
            Ok(response) => {
                let entries: Vec<ProjectEntry> =
                    response.projects.iter().filter_map(entry).collect();
                AppState::global(cx).update(cx, |state, cx| state.set_projects(entries, cx));
            }
            Err(err) => log::warn!("failed to refresh the project scope: {err}"),
        });
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(id: &str, name: &str) -> ProjectConfig {
        ProjectConfig {
            id: id.to_string(),
            name: name.to_string(),
            root_path: format!("/src/{name}"),
            extensions: Vec::new(),
            exclude_dirs: Vec::new(),
            respect_gitignore: true,
            ignore_patterns: Vec::new(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            last_indexed: None,
        }
    }

    #[test]
    fn numeric_ids_become_scope_entries() {
        let entry = entry(&config("42", "kernel")).expect("entry");
        assert_eq!(entry.id, 42);
        assert_eq!(entry.root_path, gpui_kit::SharedString::from("/src/kernel"));
    }

    #[test]
    fn unusable_ids_are_dropped() {
        assert!(entry(&config("not-a-number", "broken")).is_none());
    }
}
