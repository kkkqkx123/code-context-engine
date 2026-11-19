//! Metrics panel: Prometheus text, the JSON snapshot and aggregated history.

use cce_api::models::AggregatedMetric;
use gpui_kit::component::{
    ActiveTheme as _, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Window, div, px,
};

use crate::components::{
    Cell, Column, Confirm, actions, choice_row, confirm_bar, data_grid, empty_state, failure_block,
    field, field_group, hint, loading_row, notice, page_body, panel_header, stack, surface, toast,
};
use crate::connection::ClientStore;
use crate::i18n::{locale, t};
use crate::metrics::CARD_RADIUS;
use crate::services::ops_service;
use crate::services::refresh;
use crate::services::request::{self, Load};
use crate::state::AppState;
use crate::views::PanelKind;

const METRICS_TABS: [&str; 3] = [
    "metrics.tab.text",
    "metrics.tab.json",
    "metrics.tab.history",
];

#[derive(Debug)]
enum MetricsAnswer {
    Text(String),
    Json(serde_json::Value),
    History(Vec<AggregatedMetric>),
}

pub struct MetricsView {
    hours: Entity<InputState>,
    metric: Entity<InputState>,
    /// Explicit window bounds; both must be set to override `hours`.
    from: Entity<InputState>,
    to: Entity<InputState>,
    /// Whether the history query is narrowed to the active project.
    project_scope: bool,
    tab: usize,
    answer: Load<MetricsAnswer>,
    /// Cleanup drops recorded metrics, so it is armed before it runs.
    confirm: Confirm,
    outcome: Option<(bool, &'static str)>,
}

impl MetricsView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let locale = crate::i18n::locale(cx);
        let hours = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t("metrics.hoursPlaceholder", locale))
                .default_value("24")
        });
        let metric = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("metrics.metricPlaceholder", locale))
        });
        let from = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("metrics.fromPlaceholder", locale))
        });
        let to = cx
            .new(|cx| InputState::new(window, cx).placeholder(t("metrics.toPlaceholder", locale)));
        Self {
            hours,
            metric,
            from,
            to,
            project_scope: false,
            confirm: Confirm::none(),
            tab: 0,
            answer: Load::idle(),
            outcome: None,
        }
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.answer = Load::Failed(request::Failure::disconnected());
            cx.notify();
            return;
        };
        let tab = self.tab;
        let hours = self
            .hours
            .read(cx)
            .value()
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .unwrap_or(24);
        let metric = crate::services::optional_text(&self.metric.read(cx).value());

        self.answer.begin();
        cx.notify();

        match tab {
            0 => {
                let task = ops_service::metrics_text(cx, client);
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.answer = match outcome {
                        Ok(text) => Load::Ready(MetricsAnswer::Text(text)),
                        Err(error) => Load::Failed(request::Failure::from_error(&error)),
                    };
                    cx.notify();
                });
            }
            1 => {
                let task = ops_service::metrics_json(cx, client);
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.answer = match outcome {
                        Ok(value) => Load::Ready(MetricsAnswer::Json(value)),
                        Err(error) => Load::Failed(request::Failure::from_error(&error)),
                    };
                    cx.notify();
                });
            }
            _ => {
                let input = ops_service::HistoryInput {
                    hours,
                    from: crate::services::optional_text(&self.from.read(cx).value()),
                    to: crate::services::optional_text(&self.to.read(cx).value()),
                    metric,
                    project_id: self
                        .project_scope
                        .then(|| AppState::global(cx).read(cx).active_project_id())
                        .flatten(),
                };
                let task = ops_service::metrics_history(cx, client, input);
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.answer = match outcome {
                        Ok(rows) => Load::Ready(MetricsAnswer::History(rows)),
                        Err(error) => Load::Failed(request::Failure::from_error(&error)),
                    };
                    cx.notify();
                });
            }
        }
    }

    /// Re-run the active metric view on the next refresh tick.
    pub fn replay(&mut self, cx: &mut Context<Self>) {
        if matches!(self.answer, Load::Idle) {
            return;
        }
        refresh::touch(PanelKind::Metrics, cx);
        self.run(cx);
    }

    fn cleanup(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.outcome = Some((false, "error.notConnected"));
            cx.notify();
            return;
        };
        self.outcome = None;
        cx.notify();
        let task = ops_service::cleanup_metrics(cx, client, None);
        request::spawn_into(cx, task, |this, outcome, cx| {
            let result = match outcome {
                Ok(_) => (true, "metrics.cleaned"),
                Err(_) => (false, "metrics.cleanFailed"),
            };
            this.outcome = Some(result);
            toast::push_key(result.1, result.0, cx);
            cx.notify();
        });
    }

    fn mono_block(&self, text: String, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme().colors;
        div()
            .w_full()
            .p_3()
            .rounded(px(CARD_RADIUS))
            .border_1()
            .border_color(theme.border)
            .font_family(cx.theme().mono_font_family.clone())
            .text_xs()
            .child(SharedString::from(text))
            .into_any_element()
    }

    fn render_answer(&self, answer: &MetricsAnswer, cx: &mut Context<Self>) -> AnyElement {
        let locale = locale(cx);
        let dense = AppState::global(cx).read(cx).density.is_dense();
        match answer {
            MetricsAnswer::Text(text) => self.mono_block(text.clone(), cx),
            MetricsAnswer::Json(value) => {
                let text = match serde_json::to_string_pretty(value) {
                    Ok(text) => text,
                    Err(err) => format!("snapshot is not renderable: {err}"),
                };
                self.mono_block(text, cx)
            }
            MetricsAnswer::History(rows) => {
                if rows.is_empty() {
                    return empty_state(t("metrics.noHistory", locale), None, None, cx)
                        .into_any_element();
                }
                let columns = [
                    Column::fixed(t("metrics.colTimestamp", locale), 190.0),
                    Column::grow(t("metrics.colName", locale)),
                    Column::fixed(t("metrics.colType", locale), 100.0),
                    Column::fixed(t("metrics.colCount", locale), 80.0),
                    Column::fixed(t("metrics.colAvg", locale), 80.0),
                    Column::fixed(t("metrics.colP99", locale), 80.0),
                ];
                let cells: Vec<Vec<Cell>> = rows
                    .iter()
                    .map(|row| {
                        vec![
                            Cell::muted(SharedString::from(row.timestamp.clone())),
                            Cell::new(SharedString::from(row.metric_name.clone())),
                            Cell::muted(SharedString::from(row.metric_type.clone())),
                            Cell::muted(SharedString::from(row.count.to_string())),
                            Cell::muted(SharedString::from(
                                row.avg.map(|avg| format!("{avg:.3}")).unwrap_or_default(),
                            )),
                            Cell::muted(SharedString::from(
                                row.p99.map(|p99| format!("{p99:.3}")).unwrap_or_default(),
                            )),
                        ]
                    })
                    .collect();
                surface(dense, cx)
                    .child(data_grid(
                        PanelKind::Metrics,
                        &columns,
                        &cells,
                        dense,
                        &mut *cx,
                    ))
                    .into_any_element()
            }
        }
    }
}

impl Render for MetricsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = locale(cx);
        let tabs = choice_row(
            "metrics-tab",
            &METRICS_TABS,
            self.tab,
            |this: &mut Self, index| {
                this.tab = index;
                this.answer = Load::idle();
            },
            cx,
        );

        let form = field_group(cx).child(tabs).child(
            h_flex()
                .gap_3()
                .items_end()
                .child(field(
                    t("metrics.hours", locale),
                    Input::new(&self.hours),
                    cx,
                ))
                .child(field(
                    t("metrics.metric", locale),
                    Input::new(&self.metric),
                    cx,
                ))
                .child(field(t("metrics.from", locale), Input::new(&self.from), cx))
                .child(field(t("metrics.to", locale), Input::new(&self.to), cx))
                .child(
                    Button::new("metrics-project-scope")
                        .small()
                        .label(t("metrics.projectScope", locale))
                        .selected(self.project_scope)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.project_scope = !this.project_scope;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("metrics-run")
                        .small()
                        .primary()
                        .label(t("common.run", locale))
                        .loading(self.answer.is_loading())
                        .on_click(cx.listener(|this, _, _, cx| this.run(cx))),
                ),
        );

        let header_actions = actions().child(
            Button::new("metrics-cleanup")
                .small()
                .danger()
                .selected(self.confirm.is_armed(0))
                .label(t("metrics.cleanup", locale))
                .on_click(cx.listener(|this, _, _, cx| {
                    if this.confirm.request(0) {
                        this.cleanup(cx);
                    } else {
                        cx.notify();
                    }
                })),
        );

        let confirm_block = if self.confirm.is_armed(0) {
            Some(confirm_bar(
                "confirm-metrics-cleanup",
                SharedString::from(t("metrics.confirmCleanup", locale)),
                locale,
                cx,
                cx.listener(|this, _, _, cx| {
                    this.confirm.cancel();
                    this.cleanup(cx);
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

        let body = match &self.answer {
            Load::Idle => empty_state(
                t("metrics.emptyTitle", locale),
                Some(SharedString::from(t("metrics.emptyHint", locale))),
                None,
                cx,
            )
            .into_any_element(),
            Load::Loading => loading_row(t("metrics.loading", locale), cx).into_any_element(),
            Load::Failed(failure) => failure_block(failure, locale, cx).into_any_element(),
            Load::Ready(answer) => self.render_answer(answer, cx),
        };

        page_body("metrics")
            .child(panel_header(
                t("metrics.title", locale),
                Some(SharedString::from(t("metrics.subtitle", locale))),
                Some(header_actions.into_any_element()),
                cx,
            ))
            .child(
                stack()
                    .children(outcome_row)
                    .child(form)
                    .children(confirm_block)
                    .child(hint(t("metrics.cleanupHint", locale), cx))
                    .child(body),
            )
    }
}
