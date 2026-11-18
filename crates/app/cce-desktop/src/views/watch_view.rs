//! Watch panel: server-side directory watch for the active project.

use cce_api::models::WatchStatusResponse;
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
    Cell, Column, actions, data_grid, failure_block, field, field_group, hint, loading_row, notice,
    page_body, panel_header, stack, stat_line, surface,
};
use crate::connection::ClientStore;
use crate::i18n::{locale, t};
use crate::services::request::{self, Load};
use crate::services::watch_service;
use crate::state::AppState;
use crate::views::PanelKind;

pub struct WatchView {
    path: Entity<InputState>,
    extensions: Entity<InputState>,
    status: Load<WatchStatusResponse>,
    outcome: Option<(bool, &'static str)>,
}

impl WatchView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let locale = crate::i18n::locale(cx);
        let initial_root = AppState::global(cx)
            .read(cx)
            .active_project()
            .map(|project| project.root_path.to_string())
            .unwrap_or_default();
        let path = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t("watch.pathPlaceholder", locale))
                .default_value(initial_root)
        });
        let extensions = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("watch.extensionsPlaceholder", locale))
        });
        Self {
            path,
            extensions,
            status: Load::idle(),
            outcome: None,
        }
    }

    fn project(cx: &Context<Self>) -> Option<i64> {
        AppState::global(cx).read(cx).active_project_id()
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.status = Load::Failed(request::Failure::disconnected());
            cx.notify();
            return;
        };
        let Some(project_id) = Self::project(cx) else {
            self.status = Load::Failed(request::Failure {
                key: "error.noProject",
                detail: SharedString::default(),
            });
            cx.notify();
            return;
        };
        self.status.begin();
        cx.notify();
        let task = watch_service::status(cx, client, project_id);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.status = Load::settle(outcome);
            cx.notify();
        });
    }

    fn start(&mut self, cx: &mut Context<Self>) {
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
        let path = self.path.read(cx).value().trim().to_string();
        if path.is_empty() {
            self.outcome = Some((false, "watch.pathRequired"));
            cx.notify();
            return;
        }
        let extensions = crate::services::split_list(&self.extensions.read(cx).value());
        self.outcome = None;
        cx.notify();
        let task = watch_service::start(cx, client, project_id, path, extensions);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.outcome = Some(match outcome {
                Ok(_) => (true, "watch.started"),
                Err(_) => (false, "watch.startFailed"),
            });
            this.load(cx);
            cx.notify();
        });
    }

    fn stop(&mut self, cx: &mut Context<Self>) {
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
        let task = watch_service::stop(cx, client, project_id);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.outcome = Some(match outcome {
                Ok(_) => (true, "watch.stopped"),
                Err(_) => (false, "watch.stopFailed"),
            });
            this.load(cx);
            cx.notify();
        });
    }

    fn render_status(&self, cx: &mut Context<Self>) -> AnyElement {
        let locale = locale(cx);
        let dense = AppState::global(cx).read(cx).density.is_dense();
        match &self.status {
            Load::Idle => hint(t("watch.statusIdle", locale), cx).into_any_element(),
            Load::Loading => loading_row(t("watch.loading", locale), cx).into_any_element(),
            Load::Failed(failure) => failure_block(failure, locale, cx).into_any_element(),
            Load::Ready(response) => {
                let status = &response.status;
                let columns = [Column::grow(t("watch.colDir", locale))];
                let rows: Vec<Vec<Cell>> = status
                    .watched_dirs
                    .iter()
                    .map(|dir| vec![Cell::new(SharedString::from(dir.clone()))])
                    .collect();
                let mut block = surface(dense, cx).child(stat_line(
                    t("watch.active", locale),
                    if status.active {
                        t("common.yes", locale).to_string()
                    } else {
                        t("common.no", locale).to_string()
                    },
                    cx,
                ));
                block = block.child(stat_line(
                    t("watch.events", locale),
                    status.events_processed.to_string(),
                    cx,
                ));
                if let Some(started) = &status.started_at {
                    block =
                        block.child(stat_line(t("watch.startedAt", locale), started.clone(), cx));
                }
                if rows.is_empty() {
                    return block.into_any_element();
                }
                block
                    .child(data_grid(
                        PanelKind::Watch,
                        &columns,
                        &rows,
                        dense,
                        &mut *cx,
                    ))
                    .into_any_element()
            }
        }
    }
}

impl Render for WatchView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = locale(cx);
        let header_actions = actions().child(
            Button::new("watch-refresh")
                .small()
                .label(t("common.refresh", locale))
                .on_click(cx.listener(|this, _, _, cx| this.load(cx))),
        );

        let form = field_group(cx)
            .child(
                h_flex()
                    .gap_3()
                    .items_end()
                    .child(field(t("watch.path", locale), Input::new(&self.path), cx))
                    .child(field(
                        t("watch.extensions", locale),
                        Input::new(&self.extensions),
                        cx,
                    )),
            )
            .child(
                actions()
                    .child(
                        Button::new("watch-start")
                            .small()
                            .primary()
                            .label(t("watch.start", locale))
                            .on_click(cx.listener(|this, _, _, cx| this.start(cx))),
                    )
                    .child(
                        Button::new("watch-stop")
                            .small()
                            .danger()
                            .label(t("watch.stop", locale))
                            .on_click(cx.listener(|this, _, _, cx| this.stop(cx))),
                    ),
            );

        let outcome_row = self
            .outcome
            .map(|(positive, key)| notice(t(key, locale), positive, cx));

        page_body("watch")
            .child(panel_header(
                t("watch.title", locale),
                Some(SharedString::from(t("watch.subtitle", locale))),
                Some(header_actions.into_any_element()),
                cx,
            ))
            .child(
                stack()
                    .children(outcome_row)
                    .child(form)
                    .child(self.render_status(cx))
                    .child(
                        div()
                            .text_xs()
                            .child(SharedString::from(t("watch.debounceNote", locale))),
                    ),
            )
    }
}
