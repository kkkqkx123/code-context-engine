//! Log panel: filtered buffer pages, log files and the running logger config.
//!
//! The web console polls this data; the desktop panel follows the same
//! cadence through the shared refresh schedule. Destructive file actions are
//! armed in place so a stray click cannot drop the active log file.

use cce_api::models::{LogConfig, LogFileInfo, LogsResponse, UpdateLogConfigRequest};
use gpui_kit::component::{
    Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Window, div,
};

use crate::components::{
    Cell, Column, Confirm, Tone, actions, confirm_bar, data_grid, failure_block, field,
    field_group, hint, loading_row, page_body, panel_header, stack, stat_line, surface,
};
use crate::connection::ClientStore;
use crate::i18n::{locale, t};
use crate::services::logs_service::{self, LogsInput};
use crate::services::refresh;
use crate::services::request::{self, Load};
use crate::state::AppState;
use crate::views::PanelKind;

const CONFIRM_DELETE: usize = 0;
const CONFIRM_ROTATE: usize = 1;
const PAGE_SIZE: usize = 50;

pub struct LogsView {
    level: Entity<InputState>,
    module: Entity<InputState>,
    query: Entity<InputState>,
    config_level: Entity<InputState>,
    config_format: Entity<InputState>,
    config_filter: Entity<InputState>,
    input: LogsInput,
    entries: Load<LogsResponse>,
    files: Load<Vec<LogFileInfo>>,
    config: Load<LogConfig>,
    confirm: Confirm,
    pending_file: Option<String>,
    outcome: Option<(bool, &'static str)>,
}

impl LogsView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let locale = crate::i18n::locale(cx);
        let input_field =
            |placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
                cx.new(|cx| InputState::new(window, cx).placeholder(t(placeholder, locale)))
            };
        Self {
            level: input_field("logs.levelPlaceholder", window, cx),
            module: input_field("logs.modulePlaceholder", window, cx),
            query: input_field("logs.queryPlaceholder", window, cx),
            config_level: input_field("logs.configLevelPlaceholder", window, cx),
            config_format: input_field("logs.configFormatPlaceholder", window, cx),
            config_filter: input_field("logs.configFilterPlaceholder", window, cx),
            input: LogsInput {
                page: 1,
                page_size: PAGE_SIZE,
                ..LogsInput::default()
            },
            entries: Load::idle(),
            files: Load::idle(),
            config: Load::idle(),
            confirm: Confirm::none(),
            pending_file: None,
            outcome: None,
        }
    }

    fn sync_input(&mut self, cx: &Context<Self>) {
        self.input.level = crate::services::optional_text(&self.level.read(cx).value());
        self.input.module = crate::services::optional_text(&self.module.read(cx).value());
        self.input.query = crate::services::optional_text(&self.query.read(cx).value());
    }

    fn load_entries(&mut self, cx: &mut Context<Self>) {
        self.sync_input(cx);
        let Some(client) = ClientStore::client(cx) else {
            self.entries = Load::Failed(request::Failure::disconnected());
            cx.notify();
            return;
        };
        self.entries.begin();
        cx.notify();
        let task = logs_service::entries(cx, client, self.input.clone());
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.entries = match outcome {
                Ok(value) => Load::Ready(value),
                Err(error) => Load::Failed(request::Failure::from_error(&error)),
            };
            cx.notify();
        });
    }

    fn load_files(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.files = Load::Failed(request::Failure::disconnected());
            cx.notify();
            return;
        };
        self.files.begin();
        cx.notify();
        let task = logs_service::files(cx, client);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.files = match outcome {
                Ok(value) => Load::Ready(value),
                Err(error) => Load::Failed(request::Failure::from_error(&error)),
            };
            cx.notify();
        });
    }

    fn load_config(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.config = Load::Failed(request::Failure::disconnected());
            cx.notify();
            return;
        };
        self.config.begin();
        cx.notify();
        let task = logs_service::config(cx, client);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.config = match outcome {
                Ok(value) => Load::Ready(value),
                Err(error) => Load::Failed(request::Failure::from_error(&error)),
            };
            cx.notify();
        });
    }

    fn go_to_page(&mut self, page: usize, cx: &mut Context<Self>) {
        self.input.page = page.max(1);
        self.load_entries(cx);
    }

    fn save_config(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.outcome = Some((false, "error.notConnected"));
            cx.notify();
            return;
        };
        let request_data = UpdateLogConfigRequest {
            level: crate::services::optional_text(&self.config_level.read(cx).value()),
            format: crate::services::optional_text(&self.config_format.read(cx).value()),
            filter: crate::services::optional_text(&self.config_filter.read(cx).value()),
        };
        let task = logs_service::update_config(cx, client, request_data);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.outcome = Some(match outcome {
                Ok(_) => (true, "logs.configSaved"),
                Err(_) => (false, "logs.configSaveFailed"),
            });
            this.load_config(cx);
            cx.notify();
        });
    }

    fn delete_file(&mut self, cx: &mut Context<Self>) {
        let Some(name) = self.pending_file.take() else {
            return;
        };
        let Some(client) = ClientStore::client(cx) else {
            self.outcome = Some((false, "error.notConnected"));
            cx.notify();
            return;
        };
        let task = logs_service::delete_file(cx, client, name);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.outcome = Some(match outcome {
                Ok(_) => (true, "logs.fileDeleted"),
                Err(_) => (false, "logs.fileDeleteFailed"),
            });
            this.load_files(cx);
            cx.notify();
        });
    }

    fn rotate(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.outcome = Some((false, "error.notConnected"));
            cx.notify();
            return;
        };
        let task = logs_service::rotate(cx, client);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.outcome = Some(match outcome {
                Ok(_) => (true, "logs.rotated"),
                Err(_) => (false, "logs.rotateFailed"),
            });
            this.load_files(cx);
            cx.notify();
        });
    }

    /// Re-run the last query on the next refresh tick.
    pub fn replay(&mut self, cx: &mut Context<Self>) {
        if matches!(self.entries, Load::Idle) {
            return;
        }
        refresh::touch(PanelKind::Logs, cx);
        self.load_entries(cx);
    }

    fn render_entries(&self, cx: &mut Context<Self>) -> AnyElement {
        let locale = locale(cx);
        let dense = AppState::global(cx).read(cx).density.is_dense();
        let response = match &self.entries {
            Load::Ready(value) => value,
            Load::Idle => {
                return hint(t("logs.idle", locale), cx).into_any_element();
            }
            Load::Loading => {
                return loading_row(t("logs.loading", locale), cx).into_any_element();
            }
            Load::Failed(failure) => {
                return failure_block(failure, locale, cx).into_any_element();
            }
        };
        let columns = [
            Column::fixed(t("logs.colTime", locale), 190.0),
            Column::fixed(t("logs.colLevel", locale), 70.0),
            Column::fixed(t("logs.colTarget", locale), 180.0),
            Column::grow(t("logs.colMessage", locale)),
        ];
        let rows: Vec<Vec<Cell>> = response
            .entries
            .iter()
            .map(|entry| {
                let tone = match entry.level.to_ascii_uppercase().as_str() {
                    "ERROR" => Tone::Danger,
                    "WARN" | "WARNING" => Tone::Warning,
                    "INFO" => Tone::Info,
                    _ => Tone::Muted,
                };
                vec![
                    Cell::muted(SharedString::from(entry.timestamp.clone())),
                    Cell::toned(SharedString::from(entry.level.clone()), tone),
                    Cell::muted(SharedString::from(entry.target.clone())),
                    Cell::new(SharedString::from(entry.message.clone())),
                ]
            })
            .collect();
        let last = logs_service::last_page(response);
        let pager = h_flex()
            .gap_2()
            .items_center()
            .child(
                Button::new("logs-prev")
                    .small()
                    .label(t("common.previous", locale))
                    .disabled(response.page <= 1)
                    .on_click(cx.listener(|this, _, _, cx| {
                        let page = this.input.page.saturating_sub(1);
                        this.go_to_page(page, cx);
                    })),
            )
            .child(hint(
                SharedString::from(format!("{} / {}", response.page, last)),
                cx,
            ))
            .child(
                Button::new("logs-next")
                    .small()
                    .label(t("common.next", locale))
                    .disabled(response.page >= last)
                    .on_click(cx.listener(|this, _, _, cx| {
                        let page = this.input.page + 1;
                        this.go_to_page(page, cx);
                    })),
            );
        surface(dense, cx)
            .child(data_grid(PanelKind::Logs, &columns, &rows, dense, &mut *cx))
            .child(pager)
            .into_any_element()
    }

    fn render_files(&self, cx: &mut Context<Self>) -> AnyElement {
        let locale = locale(cx);
        let dense = AppState::global(cx).read(cx).density.is_dense();
        let files = match &self.files {
            Load::Ready(value) => value.as_slice(),
            Load::Idle | Load::Loading => &[],
            Load::Failed(failure) => {
                return failure_block(failure, locale, cx).into_any_element();
            }
        };
        let columns = [
            Column::grow(t("logs.colFile", locale)),
            Column::fixed(t("logs.colSize", locale), 100.0),
            Column::fixed(t("logs.colModified", locale), 190.0),
            Column::fixed(t("logs.colFileActions", locale), 90.0),
        ];
        let rows: Vec<Vec<Cell>> = files
            .iter()
            .map(|file| {
                vec![
                    Cell::new(SharedString::from(file.name.clone())),
                    Cell::muted(SharedString::from(file.size.to_string())),
                    Cell::muted(SharedString::from(file.modified.clone())),
                    Cell::new(SharedString::from(String::new())),
                ]
            })
            .collect();
        surface(dense, cx)
            .child(data_grid(PanelKind::Logs, &columns, &rows, dense, &mut *cx))
            .into_any_element()
    }

    fn render_config(&self, cx: &mut Context<Self>) -> AnyElement {
        let locale = locale(cx);
        let dense = AppState::global(cx).read(cx).density.is_dense();
        let config = match &self.config {
            Load::Ready(value) => Some(value),
            _ => None,
        };
        let mut block = surface(dense, cx);
        if let Some(config) = config {
            block = block
                .child(stat_line(
                    t("logs.configOutput", locale),
                    config.output.clone(),
                    cx,
                ))
                .child(stat_line(
                    t("logs.configBuffer", locale),
                    config.memory_buffer_size.to_string(),
                    cx,
                ))
                .child(stat_line(
                    t("logs.configFile", locale),
                    config.file.clone().unwrap_or_else(|| "-".to_string()),
                    cx,
                ));
        }
        block
            .child(
                h_flex()
                    .gap_3()
                    .items_end()
                    .child(field(
                        t("logs.configLevel", locale),
                        Input::new(&self.config_level),
                        cx,
                    ))
                    .child(field(
                        t("logs.configFormat", locale),
                        Input::new(&self.config_format),
                        cx,
                    ))
                    .child(field(
                        t("logs.configFilter", locale),
                        Input::new(&self.config_filter),
                        cx,
                    ))
                    .child(
                        Button::new("logs-config-save")
                            .small()
                            .label(t("logs.configSave", locale))
                            .on_click(cx.listener(|this, _, _, cx| this.save_config(cx))),
                    ),
            )
            .into_any_element()
    }

    fn render_confirm(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let locale = locale(cx);
        match (self.confirm.armed(), self.pending_file.as_deref()) {
            (Some(CONFIRM_DELETE), Some(name)) => {
                let detail =
                    SharedString::from(format!("{}: {}", t("logs.confirmDelete", locale), name));
                Some(
                    confirm_bar(
                        "logs-delete",
                        detail,
                        locale,
                        cx,
                        cx.listener(|this, _, _, cx| {
                            this.confirm.cancel();
                            this.delete_file(cx);
                        }),
                        cx.listener(|this, _, _, cx| {
                            this.confirm.cancel();
                            this.pending_file = None;
                            cx.notify();
                        }),
                    )
                    .into_any_element(),
                )
            }
            (Some(CONFIRM_ROTATE), _) => Some(
                confirm_bar(
                    "logs-rotate",
                    SharedString::from(t("logs.confirmRotate", locale)),
                    locale,
                    cx,
                    cx.listener(|this, _, _, cx| {
                        this.confirm.cancel();
                        this.rotate(cx);
                    }),
                    cx.listener(|this, _, _, cx| {
                        this.confirm.cancel();
                        cx.notify();
                    }),
                )
                .into_any_element(),
            ),
            _ => None,
        }
    }
}

impl Render for LogsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = locale(cx);
        let header_actions = actions()
            .child(
                Button::new("logs-run")
                    .small()
                    .primary()
                    .label(t("common.run", locale))
                    .on_click(cx.listener(|this, _, _, cx| this.load_entries(cx))),
            )
            .child(
                Button::new("logs-load-files")
                    .small()
                    .label(t("logs.loadFiles", locale))
                    .on_click(cx.listener(|this, _, _, cx| this.load_files(cx))),
            )
            .child(
                Button::new("logs-load-config")
                    .small()
                    .label(t("logs.loadConfig", locale))
                    .on_click(cx.listener(|this, _, _, cx| this.load_config(cx))),
            )
            .child(
                Button::new("logs-rotate")
                    .small()
                    .label(t("logs.rotate", locale))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.confirm.request(CONFIRM_ROTATE) {
                            this.rotate(cx);
                        }
                        cx.notify();
                    })),
            );

        let form = field_group(cx)
            .child(
                h_flex()
                    .gap_3()
                    .items_end()
                    .child(field(t("logs.level", locale), Input::new(&self.level), cx))
                    .child(field(
                        t("logs.module", locale),
                        Input::new(&self.module),
                        cx,
                    ))
                    .child(field(t("logs.query", locale), Input::new(&self.query), cx)),
            )
            .child(hint(t("logs.autoRefreshHint", locale), cx));

        let file_actions = if self.files.ready().is_some() {
            let mut row = h_flex().gap_2().flex_wrap();
            for file in self
                .files
                .ready()
                .into_iter()
                .flat_map(|files| files.iter())
            {
                let name = file.name.clone();
                row = row.child(
                    Button::new(format!("logs-delete-{}", file.name))
                        .small()
                        .ghost()
                        .label(SharedString::from(format!(
                            "{} · {}",
                            t("logs.delete", locale),
                            file.name
                        )))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            // Re-arming drops a previous selection, so the
                            // confirm bar always names the file just clicked.
                            this.confirm.cancel();
                            this.pending_file = Some(name.clone());
                            this.confirm.request(CONFIRM_DELETE);
                            cx.notify();
                        })),
                );
            }
            row.into_any_element()
        } else {
            div().into_any_element()
        };

        let outcome_row = self
            .outcome
            .map(|(positive, key)| crate::components::notice(t(key, locale), positive, cx));

        page_body("logs")
            .child(panel_header(
                t("logs.title", locale),
                Some(SharedString::from(t("logs.subtitle", locale))),
                Some(header_actions.into_any_element()),
                cx,
            ))
            .child(
                stack()
                    .children(outcome_row)
                    .child(form)
                    .children(self.render_confirm(cx))
                    .child(self.render_entries(cx))
                    .child(self.render_files(cx))
                    .child(file_actions)
                    .child(self.render_config(cx)),
            )
    }
}
