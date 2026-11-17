//! Projects panel: list, create, remove, index and inspect configuration.
//!
//! This panel also owns the project scope the other panels query within:
//! switching the active project here changes what every project-bound panel
//! sends on its next run.

use cce_api::models::{CreateProjectRequest, ProjectConfig, ProjectConfigGetResponse};
use gpui_kit::component::{
    ActiveTheme as _, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    v_flex,
};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Window, div, px,
};

use crate::components::{
    Cell, Column, Tone, actions, data_grid, empty_state, failure_block, field, field_group, hint,
    loading_row, notice, page_body, panel_header, surface,
};
use crate::connection::ClientStore;
use crate::i18n::{locale, t};
use crate::metrics::CARD_RADIUS;
use crate::services::project_service;
use crate::services::request::{self, Load};
use crate::state::{AppState, ProjectEntry};

pub struct ProjectsView {
    name: Entity<InputState>,
    root_path: Entity<InputState>,
    extensions: Entity<InputState>,
    projects: Load<Vec<ProjectConfig>>,
    config: Load<ProjectConfigGetResponse>,
    /// Outcome of the last mutating command, as (success, catalogue key).
    outcome: Option<(bool, &'static str)>,
    busy: bool,
}

impl ProjectsView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let locale = crate::i18n::locale(cx);
        let name = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("projects.namePlaceholder", locale))
        });
        let root_path = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("projects.pathPlaceholder", locale))
        });
        let extensions = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("projects.extensionsPlaceholder", locale))
        });
        Self {
            name,
            root_path,
            extensions,
            projects: Load::idle(),
            config: Load::idle(),
            outcome: None,
            busy: false,
        }
    }

    fn client(cx: &Context<Self>) -> Option<cce_client::Client> {
        ClientStore::client(cx)
    }

    /// Reload the list and republish the scope so other panels see the change.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let Some(client) = Self::client(cx) else {
            self.projects = Load::Failed(request::Failure::disconnected());
            cx.notify();
            return;
        };
        self.projects.begin();
        cx.notify();
        let task = project_service::list(cx, client);
        request::spawn_into(cx, task, |this, outcome, cx| {
            match outcome {
                Ok(response) => {
                    let entries: Vec<ProjectEntry> = response
                        .projects
                        .iter()
                        .filter_map(project_service::entry)
                        .collect();
                    AppState::global(cx).update(cx, |state, cx| state.set_projects(entries, cx));
                    this.projects = Load::Ready(response.projects);
                }
                Err(error) => this.projects = Load::settle(Err(error)),
            }
            cx.notify();
        });
    }

    fn create(&mut self, cx: &mut Context<Self>) {
        let Some(client) = Self::client(cx) else {
            self.outcome = Some((false, "error.notConnected"));
            cx.notify();
            return;
        };
        let root_path = self.root_path.read(cx).value().trim().to_string();
        if root_path.is_empty() {
            self.outcome = Some((false, "projects.pathRequired"));
            cx.notify();
            return;
        }
        let name = self.name.read(cx).value().trim().to_string();
        let request = CreateProjectRequest {
            name: if name.is_empty() { None } else { Some(name) },
            root_path,
            extensions: crate::services::split_list(&self.extensions.read(cx).value()),
            exclude_dirs: Vec::new(),
            respect_gitignore: true,
            ignore_patterns: Vec::new(),
            supply_mode: None,
        };
        self.busy = true;
        self.outcome = None;
        cx.notify();
        let task = project_service::create(cx, client, request);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.busy = false;
            match outcome {
                Ok(_) => {
                    this.outcome = Some((true, "projects.created"));
                    this.refresh(cx);
                }
                Err(error) => this.projects = Load::settle(Err(error)),
            }
            cx.notify();
        });
    }

    fn delete_active(&mut self, cx: &mut Context<Self>) {
        let Some(client) = Self::client(cx) else {
            self.outcome = Some((false, "error.notConnected"));
            cx.notify();
            return;
        };
        let Some(project_id) = AppState::global(cx).read(cx).active_project_id() else {
            self.outcome = Some((false, "projects.noneActive"));
            cx.notify();
            return;
        };
        self.busy = true;
        self.outcome = None;
        cx.notify();
        let task = project_service::delete(cx, client, project_id);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.busy = false;
            match outcome {
                Ok(_) => {
                    this.outcome = Some((true, "projects.deleted"));
                    this.config = Load::idle();
                    this.refresh(cx);
                }
                Err(error) => this.projects = Load::settle(Err(error)),
            }
            cx.notify();
        });
    }

    fn index_active(&mut self, cx: &mut Context<Self>) {
        let Some(client) = Self::client(cx) else {
            self.outcome = Some((false, "error.notConnected"));
            cx.notify();
            return;
        };
        let Some(project_id) = AppState::global(cx).read(cx).active_project_id() else {
            self.outcome = Some((false, "projects.noneActive"));
            cx.notify();
            return;
        };
        self.busy = true;
        self.outcome = None;
        cx.notify();
        let task = project_service::index(cx, client, project_id);
        request::spawn_into(cx, task, move |this, outcome, cx| {
            this.busy = false;
            match outcome {
                Ok(response) => {
                    this.outcome = Some((true, "projects.indexDone"));
                    log::info!(
                        "project {project_id} indexed {} files / {} entities",
                        response.indexed_files,
                        response.total_entities
                    );
                }
                Err(error) => this.projects = Load::settle(Err(error)),
            }
            cx.notify();
        });
    }

    fn load_config(&mut self, cx: &mut Context<Self>) {
        let Some(client) = Self::client(cx) else {
            self.config = Load::Failed(request::Failure::disconnected());
            cx.notify();
            return;
        };
        let Some(project_id) = AppState::global(cx).read(cx).active_project_id() else {
            self.outcome = Some((false, "projects.noneActive"));
            cx.notify();
            return;
        };
        self.config.begin();
        cx.notify();
        let task = project_service::config(cx, client, project_id);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.config = Load::settle(outcome);
            cx.notify();
        });
    }

    fn reload_config(&mut self, cx: &mut Context<Self>) {
        let Some(client) = Self::client(cx) else {
            self.outcome = Some((false, "error.notConnected"));
            cx.notify();
            return;
        };
        let Some(project_id) = AppState::global(cx).read(cx).active_project_id() else {
            self.outcome = Some((false, "projects.noneActive"));
            cx.notify();
            return;
        };
        self.busy = true;
        self.outcome = None;
        cx.notify();
        let task = project_service::reload(cx, client, project_id);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.busy = false;
            match outcome {
                Ok(_) => this.outcome = Some((true, "projects.reloaded")),
                Err(error) => this.config = Load::settle(Err(error)),
            }
            cx.notify();
        });
    }

    /// Switch the active project without leaving the panel.
    fn scope_buttons(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = AppState::global(cx);
        let state = state.read(cx);
        let projects = state.projects();
        if projects.is_empty() {
            return hint(t("projects.noneActive", locale(cx)), cx).into_any_element();
        }
        let active = state.active_project_id();
        let mut row = h_flex().gap_1().flex_wrap();
        for project in projects {
            let project_id = project.id;
            let selected = active == Some(project_id);
            let mut button = Button::new(format!("scope-{project_id}"))
                .small()
                .label(project.label())
                .selected(selected)
                .on_click(cx.listener(move |_, _, _, cx| {
                    AppState::global(cx)
                        .update(cx, |state, cx| state.select_project(project_id, cx));
                }));
            button = if selected {
                button.primary()
            } else {
                button.ghost()
            };
            row = row.child(button);
        }
        row.into_any_element()
    }

    fn render_rows(&self, cx: &Context<Self>) -> AnyElement {
        let locale = locale(cx);
        let dense = AppState::global(cx).read(cx).density.is_dense();
        let active = AppState::global(cx).read(cx).active_project_id();
        let columns = [
            Column::grow(t("projects.colName", locale)),
            Column::fixed(t("projects.colId", locale), 60.0),
            Column::grow(t("projects.colRoot", locale)),
            Column::fixed(t("projects.colLastIndexed", locale), 150.0),
        ];
        let Some(projects) = self.projects.ready() else {
            return div().into_any_element();
        };
        let rows: Vec<Vec<Cell>> = projects
            .iter()
            .map(|project| {
                let id = project.id.parse::<i64>().ok();
                let is_active = id.is_some() && id == active;
                vec![
                    Cell::toned(
                        SharedString::from(project.name.clone()),
                        if is_active { Tone::Info } else { Tone::Normal },
                    ),
                    Cell::muted(SharedString::from(project.id.clone())),
                    Cell::muted(SharedString::from(project.root_path.clone())),
                    Cell::muted(SharedString::from(
                        project
                            .last_indexed
                            .clone()
                            .unwrap_or_else(|| "—".to_string()),
                    )),
                ]
            })
            .collect();
        surface(dense, cx)
            .child(data_grid(&columns, &rows, dense, cx))
            .into_any_element()
    }

    fn config_block(&self, cx: &Context<Self>) -> AnyElement {
        let locale = locale(cx);
        match &self.config {
            Load::Idle => hint(t("projects.configIdle", locale), cx).into_any_element(),
            Load::Loading => {
                loading_row(t("projects.configLoading", locale), cx).into_any_element()
            }
            Load::Failed(failure) => failure_block(failure, locale, cx).into_any_element(),
            Load::Ready(response) => {
                let text = match serde_json::to_string_pretty(&response.config) {
                    Ok(text) => text,
                    Err(err) => format!("configuration is not renderable: {err}"),
                };
                div()
                    .w_full()
                    .p_3()
                    .rounded(px(CARD_RADIUS))
                    .border_1()
                    .border_color(cx.theme().border)
                    .text_xs()
                    .child(SharedString::from(text))
                    .into_any_element()
            }
        }
    }

    fn list_block(&self, cx: &mut Context<Self>) -> AnyElement {
        let locale = locale(cx);
        match &self.projects {
            Load::Idle => empty_state(
                t("projects.emptyTitle", locale),
                Some(SharedString::from(t("projects.emptyHint", locale))),
                None,
                cx,
            )
            .into_any_element(),
            Load::Loading => loading_row(t("projects.loading", locale), cx).into_any_element(),
            Load::Failed(failure) => failure_block(failure, locale, cx).into_any_element(),
            Load::Ready(projects) if projects.is_empty() => empty_state(
                t("projects.emptyTitle", locale),
                Some(SharedString::from(t("projects.emptyHint", locale))),
                None,
                cx,
            )
            .into_any_element(),
            Load::Ready(_) => {
                let rows = self.render_rows(cx);
                let management = actions()
                    .child(
                        Button::new("projects-index")
                            .small()
                            .label(t("projects.indexNow", locale))
                            .on_click(cx.listener(|this, _, _, cx| this.index_active(cx))),
                    )
                    .child(
                        Button::new("projects-config")
                            .small()
                            .label(t("projects.loadConfig", locale))
                            .on_click(cx.listener(|this, _, _, cx| this.load_config(cx))),
                    )
                    .child(
                        Button::new("projects-reload")
                            .small()
                            .label(t("projects.reloadConfig", locale))
                            .on_click(cx.listener(|this, _, _, cx| this.reload_config(cx))),
                    )
                    .child(
                        Button::new("projects-delete")
                            .small()
                            .danger()
                            .label(t("projects.delete", locale))
                            .on_click(cx.listener(|this, _, _, cx| this.delete_active(cx))),
                    );
                v_flex()
                    .gap_3()
                    .w_full()
                    .child(rows)
                    .child(management)
                    .into_any_element()
            }
        }
    }
}

impl Render for ProjectsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = locale(cx);
        let busy = self.busy;

        let header_actions = actions()
            .child(
                Button::new("projects-refresh")
                    .small()
                    .label(t("common.refresh", locale))
                    .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
            )
            .child(
                Button::new("projects-create")
                    .small()
                    .primary()
                    .label(t("projects.create", locale))
                    .loading(busy)
                    .on_click(cx.listener(|this, _, _, cx| this.create(cx))),
            );

        let form = field_group(cx)
            .child(
                h_flex()
                    .gap_3()
                    .items_end()
                    .child(field(
                        t("projects.name", locale),
                        Input::new(&self.name),
                        cx,
                    ))
                    .child(field(
                        t("projects.rootPath", locale),
                        Input::new(&self.root_path),
                        cx,
                    ))
                    .child(field(
                        t("projects.extensions", locale),
                        Input::new(&self.extensions),
                        cx,
                    )),
            )
            .child(hint(t("projects.extensionsHint", locale), cx));

        let scope_row = v_flex()
            .gap_2()
            .child(hint(t("projects.scope", locale), cx))
            .child(self.scope_buttons(cx));

        let outcome_row = self
            .outcome
            .map(|(positive, key)| notice(t(key, locale), positive, cx));

        let body = v_flex()
            .gap_4()
            .w_full()
            .child(scope_row)
            .child(form)
            .child(self.list_block(cx))
            .child(self.config_block(cx));

        page_body("projects")
            .child(panel_header(
                t("projects.title", locale),
                Some(SharedString::from(t("projects.subtitle", locale))),
                Some(header_actions.into_any_element()),
                cx,
            ))
            .child(body)
            .children(outcome_row)
    }
}
