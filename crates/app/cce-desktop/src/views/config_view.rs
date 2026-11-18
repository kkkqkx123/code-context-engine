//! Configuration panel: effective server configuration, validation and reload.

use cce_api::models::{ConfigInfoResponse, ConfigValidateResponse};
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    v_flex,
};
use gpui_kit::{
    AnyElement, Context, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Window, div, px,
};

use crate::components::{
    Cell, Column, Tone, actions, data_grid, failure_block, hint, loading_row, notice, page_body,
    panel_header, stack, stat_line, surface,
};
use crate::connection::ClientStore;
use crate::i18n::{locale, t};
use crate::metrics::CARD_RADIUS;
use crate::services::ops_service;
use crate::services::request::{self, Load};
use crate::state::AppState;
use crate::views::PanelKind;

#[derive(Debug)]
enum ConfigAnswer {
    Info(ConfigInfoResponse),
    Validation(ConfigValidateResponse),
}

pub struct ConfigView {
    info: Load<ConfigAnswer>,
    outcome: Option<(bool, &'static str)>,
}

impl ConfigView {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            info: Load::idle(),
            outcome: None,
        }
    }

    fn load_info(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.info = Load::Failed(request::Failure::disconnected());
            cx.notify();
            return;
        };
        self.info.begin();
        cx.notify();
        let task = ops_service::config_info(cx, client);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.info = match outcome {
                Ok(value) => Load::Ready(ConfigAnswer::Info(value)),
                Err(error) => Load::Failed(request::Failure::from_error(&error)),
            };
            cx.notify();
        });
    }

    fn validate(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.info = Load::Failed(request::Failure::disconnected());
            cx.notify();
            return;
        };
        self.info.begin();
        cx.notify();
        let task = ops_service::config_validate(cx, client);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.info = match outcome {
                Ok(value) => Load::Ready(ConfigAnswer::Validation(value)),
                Err(error) => Load::Failed(request::Failure::from_error(&error)),
            };
            cx.notify();
        });
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.outcome = Some((false, "error.notConnected"));
            cx.notify();
            return;
        };
        let project_id = AppState::global(cx).read(cx).active_project_id();
        self.outcome = None;
        cx.notify();
        let task = ops_service::config_reload(cx, client, project_id);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.outcome = Some(match outcome {
                Ok(_) => (true, "config.reloaded"),
                Err(_) => (false, "config.reloadFailed"),
            });
            cx.notify();
        });
    }

    fn render_answer(&self, answer: &ConfigAnswer, cx: &mut Context<Self>) -> AnyElement {
        let locale = locale(cx);
        let dense = AppState::global(cx).read(cx).density.is_dense();
        let theme = cx.theme().colors;
        match answer {
            ConfigAnswer::Info(response) => {
                let mut block = surface(dense, cx)
                    .child(stat_line(
                        t("config.initialized", locale),
                        if response.initialized {
                            t("common.yes", locale).to_string()
                        } else {
                            t("common.no", locale).to_string()
                        },
                        cx,
                    ))
                    .child(stat_line(
                        t("config.projects", locale),
                        response.project_count.to_string(),
                        cx,
                    ));
                for (label, value) in [
                    ("database", &response.database),
                    ("embedder", &response.embedder),
                    ("config", &response.config),
                ] {
                    let text = match serde_json::to_string_pretty(value) {
                        Ok(text) => text,
                        Err(err) => format!("{label} is not renderable: {err}"),
                    };
                    block = block.child(
                        v_flex()
                            .gap_1()
                            .w_full()
                            .child(hint(SharedString::from(label.to_string()), cx))
                            .child(
                                div()
                                    .w_full()
                                    .p_3()
                                    .rounded(px(CARD_RADIUS))
                                    .border_1()
                                    .border_color(theme.border)
                                    .font_family(cx.theme().mono_font_family.clone())
                                    .text_xs()
                                    .child(SharedString::from(text)),
                            ),
                    );
                }
                block.into_any_element()
            }
            ConfigAnswer::Validation(response) => {
                let columns = [
                    Column::fixed(t("config.colSeverity", locale), 100.0),
                    Column::fixed(t("config.colField", locale), 220.0),
                    Column::grow(t("config.colSuggestion", locale)),
                ];
                let mut rows: Vec<Vec<Cell>> = response
                    .errors
                    .iter()
                    .map(|error| {
                        vec![
                            Cell::toned(SharedString::from("error"), Tone::Danger),
                            Cell::muted(SharedString::from("")),
                            Cell::new(SharedString::from(error.clone())),
                        ]
                    })
                    .collect();
                for warning in &response.warnings {
                    rows.push(vec![
                        Cell::toned(SharedString::from("warning"), Tone::Warning),
                        Cell::muted(SharedString::from("")),
                        Cell::new(SharedString::from(warning.clone())),
                    ]);
                }
                for warning in &response.dependency_warnings {
                    rows.push(vec![
                        Cell::toned(SharedString::from(warning.severity.clone()), Tone::Warning),
                        Cell::muted(SharedString::from(warning.field.clone())),
                        Cell::new(SharedString::from(warning.suggestion.clone())),
                    ]);
                }
                let mut block = surface(dense, cx).child(stat_line(
                    t("config.valid", locale),
                    if response.valid {
                        t("common.yes", locale).to_string()
                    } else {
                        t("common.no", locale).to_string()
                    },
                    cx,
                ));
                if rows.is_empty() {
                    return block
                        .child(hint(t("config.noIssues", locale), cx))
                        .into_any_element();
                }
                block = block.child(data_grid(
                    PanelKind::Config,
                    &columns,
                    &rows,
                    dense,
                    &mut *cx,
                ));
                block.into_any_element()
            }
        }
    }
}

impl Render for ConfigView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = locale(cx);
        let header_actions = actions()
            .child(
                Button::new("config-info")
                    .small()
                    .label(t("config.showInfo", locale))
                    .on_click(cx.listener(|this, _, _, cx| this.load_info(cx))),
            )
            .child(
                Button::new("config-validate")
                    .small()
                    .label(t("config.validate", locale))
                    .on_click(cx.listener(|this, _, _, cx| this.validate(cx))),
            )
            .child(
                Button::new("config-reload")
                    .small()
                    .primary()
                    .label(t("config.reload", locale))
                    .on_click(cx.listener(|this, _, _, cx| this.reload(cx))),
            );

        let outcome_row = self
            .outcome
            .map(|(positive, key)| notice(t(key, locale), positive, cx));

        let body = match &self.info {
            Load::Idle => hint(t("config.idle", locale), cx).into_any_element(),
            Load::Loading => loading_row(t("config.loading", locale), cx).into_any_element(),
            Load::Failed(failure) => failure_block(failure, locale, cx).into_any_element(),
            Load::Ready(answer) => self.render_answer(answer, cx),
        };

        page_body("config")
            .child(panel_header(
                t("config.title", locale),
                Some(SharedString::from(t("config.subtitle", locale))),
                Some(header_actions.into_any_element()),
                cx,
            ))
            .child(
                stack()
                    .children(outcome_row)
                    .child(hint(t("config.reloadHint", locale), cx))
                    .child(body),
            )
    }
}
