//! Task panel: project dead letters and the retry queue.
//!
//! Dead letters are recorded per project and can be retried or acknowledged;
//! the retry queue is inspected globally, with an optional project scope.

use cce_api::models::{DeadLetterListResponse, RetryQueueDeadResponse, RetryQueueStatusResponse};
use gpui_kit::component::{
    Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Window, div,
};

use crate::components::{
    Cell, Column, Tone, actions, data_grid, empty_state, failure_block, field, field_group, hint,
    loading_row, notice, page_body, panel_header, stack, stat_line, surface,
};
use crate::connection::ClientStore;
use crate::i18n::{locale, t};
use crate::services::ops_service;
use crate::services::request::{self, Load};
use crate::state::AppState;

pub struct TasksView {
    file: Entity<InputState>,
    dead: Load<DeadLetterListResponse>,
    queue: Load<RetryQueueStatusResponse>,
    dead_queries: Load<RetryQueueDeadResponse>,
    outcome: Option<(bool, &'static str)>,
}

impl TasksView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let locale = crate::i18n::locale(cx);
        let file = cx
            .new(|cx| InputState::new(window, cx).placeholder(t("tasks.filePlaceholder", locale)));
        Self {
            file,
            dead: Load::idle(),
            queue: Load::idle(),
            dead_queries: Load::idle(),
            outcome: None,
        }
    }

    fn project(cx: &Context<Self>) -> Option<i64> {
        AppState::global(cx).read(cx).active_project_id()
    }

    fn load_dead(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.dead = Load::Failed(request::Failure::disconnected());
            cx.notify();
            return;
        };
        let Some(project_id) = Self::project(cx) else {
            self.dead = Load::Failed(request::Failure {
                key: "error.noProject",
                detail: SharedString::default(),
            });
            cx.notify();
            return;
        };
        self.dead.begin();
        cx.notify();
        let task = ops_service::dead_letters(cx, client, project_id);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.dead = Load::settle(outcome);
            cx.notify();
        });
    }

    fn load_queue(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.queue = Load::Failed(request::Failure::disconnected());
            cx.notify();
            return;
        };
        let project_id = Self::project(cx);
        self.queue.begin();
        cx.notify();
        let status = ops_service::retry_queue_status(cx, client.clone(), project_id);
        let dead = ops_service::retry_queue_dead(cx, client, project_id);
        cx.spawn(async move |this, cx| {
            let status = status.await;
            let dead = dead.await;
            this.update(cx, |this, cx| {
                this.queue = Load::settle(status);
                this.dead_queries = Load::settle(dead);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn retry_all(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.outcome = Some((false, "error.notConnected"));
            cx.notify();
            return;
        };
        let Some(project_id) = Self::project(cx) else {
            self.outcome = Some((false, "error.noProject"));
            cx.notify();
            return;
        };
        self.outcome = None;
        cx.notify();
        let task = ops_service::retry_dead_letters(cx, client, project_id, Vec::new());
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.outcome = Some(match outcome {
                Ok(_) => (true, "tasks.retried"),
                Err(_) => (false, "tasks.retryFailed"),
            });
            this.load_dead(cx);
            cx.notify();
        });
    }

    fn acknowledge(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.outcome = Some((false, "error.notConnected"));
            cx.notify();
            return;
        };
        let Some(project_id) = Self::project(cx) else {
            self.outcome = Some((false, "error.noProject"));
            cx.notify();
            return;
        };
        let file = self.file.read(cx).value().trim().to_string();
        if file.is_empty() {
            self.outcome = Some((false, "tasks.fileRequired"));
            cx.notify();
            return;
        }
        self.outcome = None;
        cx.notify();
        let task = ops_service::acknowledge_dead_letter(cx, client, project_id, file);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.outcome = Some(match outcome {
                Ok(_) => (true, "tasks.acknowledged"),
                Err(_) => (false, "tasks.acknowledgeFailed"),
            });
            this.load_dead(cx);
            cx.notify();
        });
    }

    fn process_queue(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.outcome = Some((false, "error.notConnected"));
            cx.notify();
            return;
        };
        let project_id = Self::project(cx);
        self.outcome = None;
        cx.notify();
        let task = ops_service::retry_queue_process(cx, client, project_id);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.outcome = Some(match outcome {
                Ok(_) => (true, "tasks.queueProcessed"),
                Err(_) => (false, "tasks.processFailed"),
            });
            this.load_queue(cx);
            cx.notify();
        });
    }

    fn clear_queue(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.outcome = Some((false, "error.notConnected"));
            cx.notify();
            return;
        };
        let project_id = Self::project(cx);
        self.outcome = None;
        cx.notify();
        let task = ops_service::retry_queue_clear(cx, client, project_id);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.outcome = Some(match outcome {
                Ok(_) => (true, "tasks.queueCleared"),
                Err(_) => (false, "tasks.clearFailed"),
            });
            this.load_queue(cx);
            cx.notify();
        });
    }

    fn clear_dead_queue(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.outcome = Some((false, "error.notConnected"));
            cx.notify();
            return;
        };
        let project_id = Self::project(cx);
        self.outcome = None;
        cx.notify();
        let task = ops_service::retry_queue_dead_clear(cx, client, project_id);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.outcome = Some(match outcome {
                Ok(_) => (true, "tasks.queueCleared"),
                Err(_) => (false, "tasks.clearFailed"),
            });
            this.load_queue(cx);
            cx.notify();
        });
    }

    fn render_dead(&self, cx: &Context<Self>) -> AnyElement {
        let locale = locale(cx);
        let dense = AppState::global(cx).read(cx).density.is_dense();
        match &self.dead {
            Load::Idle => hint(t("tasks.deadIdle", locale), cx).into_any_element(),
            Load::Loading => loading_row(t("tasks.loading", locale), cx).into_any_element(),
            Load::Failed(failure) => failure_block(failure, locale, cx).into_any_element(),
            Load::Ready(response) if response.files.is_empty() => empty_state(
                t("tasks.deadEmpty", locale),
                Some(SharedString::from(t("tasks.deadEmptyHint", locale))),
                None,
                cx,
            )
            .into_any_element(),
            Load::Ready(response) => {
                let columns = [
                    Column::grow(t("tasks.colFile", locale)),
                    Column::fixed(t("tasks.colModules", locale), 90.0),
                    Column::fixed(t("tasks.colUpdated", locale), 190.0),
                ];
                let rows: Vec<Vec<Cell>> = response
                    .files
                    .iter()
                    .map(|entry| {
                        let pending = entry
                            .modules
                            .iter()
                            .filter(|module| !module.acknowledged)
                            .count();
                        vec![
                            Cell::new(SharedString::from(entry.file_path.clone())),
                            Cell::toned(
                                SharedString::from(pending.to_string()),
                                if pending == 0 {
                                    Tone::Muted
                                } else {
                                    Tone::Warning
                                },
                            ),
                            Cell::muted(SharedString::from(entry.updated_at.clone())),
                        ]
                    })
                    .collect();
                surface(dense, cx)
                    .child(data_grid(&columns, &rows, dense, cx))
                    .into_any_element()
            }
        }
    }

    fn render_queue(&self, cx: &Context<Self>) -> AnyElement {
        let locale = locale(cx);
        let dense = AppState::global(cx).read(cx).density.is_dense();
        match &self.queue {
            Load::Idle => hint(t("tasks.queueIdle", locale), cx).into_any_element(),
            Load::Loading => loading_row(t("tasks.loading", locale), cx).into_any_element(),
            Load::Failed(failure) => failure_block(failure, locale, cx).into_any_element(),
            Load::Ready(response) => {
                let mut block = surface(dense, cx)
                    .child(stat_line(
                        t("tasks.pending", locale),
                        response.pending_count.to_string(),
                        cx,
                    ))
                    .child(stat_line(
                        t("tasks.dead", locale),
                        response.dead_count.to_string(),
                        cx,
                    ));
                if let Some(dead) = self.dead_queries.ready()
                    && !dead.entries.is_empty()
                {
                    let columns = [
                        Column::grow(t("tasks.colQuery", locale)),
                        Column::fixed(t("tasks.colRetries", locale), 90.0),
                    ];
                    let rows: Vec<Vec<Cell>> = dead
                        .entries
                        .iter()
                        .map(|entry| {
                            vec![
                                Cell::new(SharedString::from(entry.query.clone())),
                                Cell::muted(SharedString::from(entry.retry_count.to_string())),
                            ]
                        })
                        .collect();
                    block = block.child(data_grid(&columns, &rows, dense, cx));
                }
                block.into_any_element()
            }
        }
    }
}

impl Render for TasksView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = locale(cx);

        let header_actions = actions()
            .child(
                Button::new("tasks-load-dead")
                    .small()
                    .label(t("tasks.loadDead", locale))
                    .on_click(cx.listener(|this, _, _, cx| this.load_dead(cx))),
            )
            .child(
                Button::new("tasks-load-queue")
                    .small()
                    .label(t("tasks.loadQueue", locale))
                    .on_click(cx.listener(|this, _, _, cx| this.load_queue(cx))),
            );

        let dead_actions = actions()
            .child(
                Button::new("tasks-retry")
                    .small()
                    .primary()
                    .label(t("tasks.retryAll", locale))
                    .on_click(cx.listener(|this, _, _, cx| this.retry_all(cx))),
            )
            .child(
                Button::new("tasks-acknowledge")
                    .small()
                    .label(t("tasks.acknowledge", locale))
                    .on_click(cx.listener(|this, _, _, cx| this.acknowledge(cx))),
            );

        let dead_form = field_group(cx)
            .child(field(t("tasks.file", locale), Input::new(&self.file), cx))
            .child(dead_actions);

        let queue_actions = actions()
            .child(
                Button::new("tasks-process")
                    .small()
                    .label(t("tasks.processQueue", locale))
                    .on_click(cx.listener(|this, _, _, cx| this.process_queue(cx))),
            )
            .child(
                Button::new("tasks-clear-queue")
                    .small()
                    .label(t("tasks.clearQueue", locale))
                    .on_click(cx.listener(|this, _, _, cx| this.clear_queue(cx))),
            )
            .child(
                Button::new("tasks-clear-dead")
                    .small()
                    .danger()
                    .label(t("tasks.clearDeadQueue", locale))
                    .on_click(cx.listener(|this, _, _, cx| this.clear_dead_queue(cx))),
            );

        let outcome_row = self
            .outcome
            .map(|(positive, key)| notice(t(key, locale), positive, cx));

        page_body("tasks")
            .child(panel_header(
                t("tasks.title", locale),
                Some(SharedString::from(t("tasks.subtitle", locale))),
                Some(header_actions.into_any_element()),
                cx,
            ))
            .child(
                stack()
                    .children(outcome_row)
                    .child(dead_form)
                    .child(self.render_dead(cx))
                    .child(div())
                    .child(self.render_queue(cx))
                    .child(queue_actions)
                    .child(
                        h_flex()
                            .gap_2()
                            .child(hint(t("tasks.scopeNote", locale), cx)),
                    ),
            )
    }
}
