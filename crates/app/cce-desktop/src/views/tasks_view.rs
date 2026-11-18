//! Task panel: project dead letters and the retry queue.
//!
//! Dead letters are recorded per project and can be retried or acknowledged;
//! the retry queue is inspected globally, with an optional project scope.

use cce_api::models::{DeadLetterListResponse, RetryQueueDeadResponse, RetryQueueStatusResponse};
use gpui_kit::component::{
    Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    v_flex,
};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Window, div,
};

use crate::components::{
    Cell, Column, Confirm, actions, confirm_bar, data_grid, empty_state, failure_block, field,
    field_group, hint, loading_row, notice, page_body, panel_header, stack, stat_line, surface,
};
use crate::connection::ClientStore;
use crate::i18n::{locale, t};
use crate::services::ops_service;
use crate::services::refresh;
use crate::services::request::{self, Load};
use crate::state::AppState;
use crate::views::PanelKind;

pub struct TasksView {
    file: Entity<InputState>,
    dead: Load<DeadLetterListResponse>,
    queue: Load<RetryQueueStatusResponse>,
    dead_queries: Load<RetryQueueDeadResponse>,
    /// Dead-letter files ticked for a batch action.
    selected: std::collections::BTreeSet<String>,
    confirm: Confirm,
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
            selected: std::collections::BTreeSet::new(),
            confirm: Confirm::none(),
            outcome: None,
        }
    }

    /// Re-read whichever lists are already on screen.
    pub fn replay(&mut self, cx: &mut Context<Self>) {
        refresh::touch(PanelKind::Tasks, cx);
        if !matches!(self.dead, Load::Idle) {
            self.load_dead(cx);
        }
        if !matches!(self.queue, Load::Idle) {
            self.load_queue(cx);
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
                candidates: Vec::new(),
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

    /// Files ticked in the dead-letter list.
    fn selection(&self) -> Vec<String> {
        self.selected.iter().cloned().collect()
    }

    fn toggle(&mut self, file: &str, cx: &mut Context<Self>) {
        if !self.selected.remove(file) {
            self.selected.insert(file.to_string());
        }
        cx.notify();
    }

    /// Retry the ticked files instead of every file in the project.
    fn retry_selected(&mut self, cx: &mut Context<Self>) {
        let files = self.selection();
        if files.is_empty() {
            self.outcome = Some((false, "tasks.selected"));
            cx.notify();
            return;
        }
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
        let task = ops_service::retry_dead_letters(cx, client, project_id, files);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.outcome = Some(match outcome {
                Ok(_) => (true, "tasks.retried"),
                Err(_) => (false, "tasks.retryFailed"),
            });
            this.selected.clear();
            this.load_dead(cx);
            cx.notify();
        });
    }

    /// Acknowledge the ticked files.
    fn acknowledge_selected(&mut self, cx: &mut Context<Self>) {
        let files = self.selection();
        if files.is_empty() {
            self.outcome = Some((false, "tasks.selected"));
            cx.notify();
            return;
        }
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
        let task = ops_service::acknowledge_dead_letters(cx, client, project_id, files);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.outcome = Some(match outcome {
                Ok(_) => (true, "tasks.acknowledged"),
                Err(_) => (false, "tasks.acknowledgeFailed"),
            });
            this.selected.clear();
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

    fn render_dead(&self, cx: &mut Context<Self>) -> AnyElement {
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
                // The lightweight grid is read-only, so the batch actions get
                // their own selectable rows: a row that cannot be ticked is
                // exactly the thing a batch action needs.
                let mut list = v_flex().gap_1().w_full();
                for entry in &response.files {
                    let file = entry.file_path.clone();
                    let pending = entry
                        .modules
                        .iter()
                        .filter(|module| !module.acknowledged)
                        .count();
                    let ticked = self.selected.contains(&file);
                    list = list.child(
                        Button::new(SharedString::from(format!("tasks-pick-{file}")))
                            .small()
                            .selected(ticked)
                            .label(SharedString::from(format!(
                                "{}  /  {}  /  {}",
                                file, pending, entry.updated_at
                            )))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let file = file.clone();
                                this.toggle(&file, cx);
                            })),
                    );
                }
                let batch = actions()
                    .child(
                        Button::new("tasks-select-all")
                            .small()
                            .ghost()
                            .label(t("tasks.selectAll", locale))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Load::Ready(response) = &this.dead {
                                    for entry in &response.files {
                                        this.selected.insert(entry.file_path.clone());
                                    }
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("tasks-clear-selection")
                            .small()
                            .ghost()
                            .label(t("tasks.clearSelection", locale))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.selected.clear();
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("tasks-retry-selected")
                            .small()
                            .label(t("tasks.retrySelected", locale))
                            .on_click(cx.listener(|this, _, _, cx| this.retry_selected(cx))),
                    )
                    .child(
                        Button::new("tasks-ack-selected")
                            .small()
                            .label(t("tasks.acknowledgeSelected", locale))
                            .on_click(cx.listener(|this, _, _, cx| this.acknowledge_selected(cx))),
                    );
                surface(dense, cx)
                    .child(stat_line(
                        t("tasks.selected", locale),
                        self.selected.len().to_string(),
                        cx,
                    ))
                    .child(list)
                    .child(batch)
                    .into_any_element()
            }
        }
    }

    fn render_queue(&self, cx: &mut Context<Self>) -> AnyElement {
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
                    block = block.child(data_grid(
                        PanelKind::Tasks,
                        &columns,
                        &rows,
                        dense,
                        &mut *cx,
                    ));
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
                    .selected(self.confirm.is_armed(0))
                    .label(t("tasks.clearDeadQueue", locale))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.confirm.request(0) {
                            this.clear_dead_queue(cx);
                        } else {
                            cx.notify();
                        }
                    })),
            );

        let confirm_block = if self.confirm.is_armed(0) {
            Some(confirm_bar(
                "confirm-dead-cleanup",
                SharedString::from(t("tasks.confirmCleanup", locale)),
                locale,
                cx,
                cx.listener(|this, _, _, cx| {
                    this.confirm.cancel();
                    this.clear_dead_queue(cx);
                }),
                cx.listener(|this, _, _, cx| {
                    this.confirm.cancel();
                    cx.notify();
                }),
            ))
        } else {
            None
        };

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
                    .children(confirm_block)
                    .child(
                        h_flex()
                            .gap_2()
                            .child(hint(t("tasks.scopeNote", locale), cx)),
                    ),
            )
    }
}
