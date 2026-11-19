//! Configuration panel: effective server configuration, validation, reload,
//! the stored project profile and the project's LLM settings.
//!
//! Every write here has an HTTP endpoint in the shared contract already, so
//! the panel only wraps client methods. The project profile write is armed
//! first because it is hot-applied by the server; LLM updates are ephemeral
//! patches and a model probe persists nothing.

use cce_api::models::{
    ConfigInfoResponse, ConfigValidateResponse, LlmTestResponse, ProjectConfigGetResponse,
    ProjectLlmOverviewResponse,
};
use gpui_kit::component::{
    ActiveTheme as _, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState, Textarea, TextareaState},
    v_flex,
};
use gpui_kit::{
    AnyElement, AnyWindowHandle, AppContext as _, Context, Entity, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, Window, div, px,
};

use crate::components::{
    Cell, Column, Confirm, Tone, actions, confirm_bar, data_grid, failure_block, field, hint,
    loading_row, notice, page_body, panel_header, stack, stat_line, surface, toast,
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

const CONFIRM_PROJECT_CONFIG: usize = 0;

pub struct ConfigView {
    info: Load<ConfigAnswer>,
    project: Load<ProjectConfigGetResponse>,
    llm: Load<ProjectLlmOverviewResponse>,
    test: Load<LlmTestResponse>,
    profile: Entity<TextareaState>,
    chat: Entity<InputState>,
    rerank: Entity<InputState>,
    /// Rerank override; `None` leaves the server value untouched.
    rerank_enabled: Option<bool>,
    max_candidates: Entity<InputState>,
    test_service: Entity<InputState>,
    test_model: Entity<InputState>,
    confirm: Confirm,
    outcome: Option<(bool, &'static str)>,
}

impl ConfigView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let locale = crate::i18n::locale(cx);
        let input = |placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(t(placeholder, locale)))
        };
        Self {
            info: Load::idle(),
            project: Load::idle(),
            llm: Load::idle(),
            test: Load::idle(),
            profile: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .rows(10)
                    .placeholder(t("config.profilePlaceholder", locale))
            }),
            chat: input("config.chatPlaceholder", window, cx),
            rerank: input("config.rerankPlaceholder", window, cx),
            rerank_enabled: None,
            max_candidates: input("config.maxCandidatesPlaceholder", window, cx),
            test_service: input("config.testServicePlaceholder", window, cx),
            test_model: input("config.testModelPlaceholder", window, cx),
            confirm: Confirm::none(),
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
            let result = match outcome {
                Ok(_) => (true, "config.reloaded"),
                Err(_) => (false, "config.reloadFailed"),
            };
            this.outcome = Some(result);
            toast::push_key(result.1, result.0, cx);
            cx.notify();
        });
    }

    /// Load the stored project profile into the editor.
    ///
    /// The window handle is captured before the request because setting an
    /// input value needs a window; the callback upgrades it through `Context`.
    fn load_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.project = Load::Failed(request::Failure::disconnected());
            cx.notify();
            return;
        };
        let Some(project_id) = AppState::global(cx).read(cx).active_project_id() else {
            self.project = Load::Failed(request::Failure {
                key: "error.noProject",
                detail: SharedString::default(),
                candidates: Vec::new(),
            });
            cx.notify();
            return;
        };
        let handle: AnyWindowHandle = window.window_handle();
        let editor = self.profile.clone();
        self.project.begin();
        cx.notify();
        let task = ops_service::project_config(cx, client, project_id);
        request::spawn_into(cx, task, move |this, outcome, cx| {
            this.project = match outcome {
                Ok(value) => {
                    let text = serde_json::to_string_pretty(&value.config)
                        .unwrap_or_else(|_| value.config.to_string());
                    let editor = editor.clone();
                    let handle = handle;
                    cx.update_window(handle, |_, window, cx| {
                        editor.update(cx, |state, cx| state.set_value(text, window, cx));
                    })
                    .ok();
                    Load::Ready(value)
                }
                Err(error) => Load::Failed(request::Failure::from_error(&error)),
            };
            cx.notify();
        });
    }

    fn save_project(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.outcome = Some((false, "error.notConnected"));
            cx.notify();
            return;
        };
        let Some(project_id) = AppState::global(cx).read(cx).active_project_id() else {
            self.outcome = Some((false, "error.noProject"));
            cx.notify();
            return;
        };
        let text = self.profile.read(cx).value();
        let config = match serde_json::from_str(&text) {
            Ok(value) => value,
            Err(error) => {
                self.outcome = Some((false, "config.invalidJson"));
                let _ = error;
                cx.notify();
                return;
            }
        };
        self.outcome = None;
        cx.notify();
        let task = ops_service::update_project_config(
            cx,
            client,
            project_id,
            cce_api::models::ProjectConfigUpdateRequest { config },
        );
        request::spawn_into(cx, task, |this, outcome, cx| {
            let result = match outcome {
                Ok(_) => (true, "config.saved"),
                Err(_) => (false, "config.saveFailed"),
            };
            this.outcome = Some(result);
            toast::push_key(result.1, result.0, cx);
            cx.notify();
        });
    }

    fn load_llm(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.llm = Load::Failed(request::Failure::disconnected());
            cx.notify();
            return;
        };
        let Some(project_id) = AppState::global(cx).read(cx).active_project_id() else {
            self.llm = Load::Failed(request::Failure {
                key: "error.noProject",
                detail: SharedString::default(),
                candidates: Vec::new(),
            });
            cx.notify();
            return;
        };
        self.llm.begin();
        cx.notify();
        let task = ops_service::llm_overview(cx, client, project_id);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.llm = match outcome {
                Ok(value) => {
                    this.rerank_enabled = Some(value.rerank_enabled);
                    Load::Ready(value)
                }
                Err(error) => Load::Failed(request::Failure::from_error(&error)),
            };
            cx.notify();
        });
    }

    fn save_llm(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.outcome = Some((false, "error.notConnected"));
            cx.notify();
            return;
        };
        let Some(project_id) = AppState::global(cx).read(cx).active_project_id() else {
            self.outcome = Some((false, "error.noProject"));
            cx.notify();
            return;
        };
        let request_data = cce_api::models::ProjectLlmUpdateRequest {
            default_chat: crate::services::optional_text(&self.chat.read(cx).value()),
            default_rerank: crate::services::optional_text(&self.rerank.read(cx).value()),
            rerank_enabled: self.rerank_enabled,
            rerank_max_candidates: self
                .max_candidates
                .read(cx)
                .value()
                .trim()
                .parse::<usize>()
                .ok(),
            chat_model_params: None,
        };
        let task = ops_service::update_llm(cx, client, project_id, request_data);
        request::spawn_into(cx, task, |this, outcome, cx| {
            let result = match outcome {
                Ok(_) => (true, "config.llmSaved"),
                Err(_) => (false, "config.llmSaveFailed"),
            };
            this.outcome = Some(result);
            toast::push_key(result.1, result.0, cx);
            this.load_llm(cx);
            cx.notify();
        });
    }

    fn test_model(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.outcome = Some((false, "error.notConnected"));
            cx.notify();
            return;
        };
        let Some(project_id) = AppState::global(cx).read(cx).active_project_id() else {
            self.outcome = Some((false, "error.noProject"));
            cx.notify();
            return;
        };
        let service = self
            .test_service
            .read(cx)
            .value()
            .trim()
            .to_ascii_lowercase();
        let model_key = self.test_model.read(cx).value().trim().to_string();
        if !matches!(service.as_str(), "chat" | "embedding") || model_key.is_empty() {
            self.outcome = Some((false, "config.testInputInvalid"));
            cx.notify();
            return;
        }
        self.test.begin();
        cx.notify();
        let task = ops_service::test_llm(
            cx,
            client,
            project_id,
            cce_api::models::LlmTestRequest { service, model_key },
        );
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.test = match outcome {
                Ok(value) => Load::Ready(value),
                Err(error) => Load::Failed(request::Failure::from_error(&error)),
            };
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

    fn render_project(&self, cx: &mut Context<Self>) -> AnyElement {
        let locale = locale(cx);
        let dense = AppState::global(cx).read(cx).density.is_dense();
        let version = match &self.project {
            Load::Ready(value) => value.config_version.to_string(),
            _ => "-".to_string(),
        };
        let mut block =
            surface(dense, cx).child(stat_line(t("config.profileVersion", locale), version, cx));
        if let Load::Failed(failure) = &self.project {
            block = block.child(failure_block(failure, locale, cx));
        }
        block
            .child(field(
                t("config.profile", locale),
                Textarea::new(&self.profile),
                cx,
            ))
            .into_any_element()
    }

    fn render_llm(&self, cx: &mut Context<Self>) -> AnyElement {
        let locale = locale(cx);
        let dense = AppState::global(cx).read(cx).density.is_dense();
        let mut block = surface(dense, cx);
        match &self.llm {
            Load::Ready(overview) => {
                let columns = [
                    Column::grow(t("config.colModel", locale)),
                    Column::grow(t("config.colProvider", locale)),
                    Column::fixed(t("config.colKind", locale), 120.0),
                ];
                let mut rows: Vec<Vec<Cell>> = Vec::new();
                for model in &overview.embedding_models {
                    rows.push(vec![
                        Cell::new(SharedString::from(model.model.clone())),
                        Cell::muted(SharedString::from(model.provider_name.clone())),
                        Cell::toned(SharedString::from("embedding"), Tone::Info),
                    ]);
                }
                for model in &overview.chat_models {
                    rows.push(vec![
                        Cell::new(SharedString::from(model.model.clone())),
                        Cell::muted(SharedString::from(model.provider_name.clone())),
                        Cell::toned(SharedString::from("chat"), Tone::Info),
                    ]);
                }
                for model in &overview.rerank_models {
                    rows.push(vec![
                        Cell::new(SharedString::from(model.model.clone())),
                        Cell::muted(SharedString::from(model.provider_name.clone())),
                        Cell::toned(SharedString::from("rerank"), Tone::Info),
                    ]);
                }
                block = block
                    .child(stat_line(
                        t("config.llmEnabled", locale),
                        if overview.llm_enabled {
                            t("common.yes", locale).to_string()
                        } else {
                            t("common.no", locale).to_string()
                        },
                        cx,
                    ))
                    .child(stat_line(
                        t("config.defaultChat", locale),
                        overview.default_chat.clone().unwrap_or_default(),
                        cx,
                    ))
                    .child(stat_line(
                        t("config.defaultRerank", locale),
                        overview.default_rerank.clone().unwrap_or_default(),
                        cx,
                    ))
                    .child(data_grid(
                        PanelKind::Config,
                        &columns,
                        &rows,
                        dense,
                        &mut *cx,
                    ));
                if !overview.missing_api_key_providers.is_empty() {
                    block = block.child(hint(
                        SharedString::from(format!(
                            "{}: {}",
                            t("config.missingKeys", locale),
                            overview.missing_api_key_providers.join(", ")
                        )),
                        cx,
                    ));
                }
            }
            Load::Failed(failure) => {
                block = block.child(failure_block(failure, locale, cx));
            }
            Load::Idle | Load::Loading => {}
        }
        block
            .child(
                h_flex()
                    .gap_3()
                    .items_end()
                    .child(field(
                        t("config.chatModel", locale),
                        Input::new(&self.chat),
                        cx,
                    ))
                    .child(field(
                        t("config.rerankModel", locale),
                        Input::new(&self.rerank),
                        cx,
                    ))
                    .child(field(
                        t("config.maxCandidates", locale),
                        Input::new(&self.max_candidates),
                        cx,
                    ))
                    .child({
                        let label = match self.rerank_enabled {
                            Some(enabled) => format!(
                                "{}: {}",
                                t("config.rerankEnabled", locale),
                                t(if enabled { "common.yes" } else { "common.no" }, locale)
                            ),
                            None => t("config.rerankDefault", locale).to_string(),
                        };
                        Button::new("config-rerank-enabled")
                            .small()
                            .selected(self.rerank_enabled.unwrap_or(false))
                            .label(SharedString::from(label))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.rerank_enabled = Some(!this.rerank_enabled.unwrap_or(false));
                                cx.notify();
                            }))
                    })
                    .child(
                        Button::new("config-llm-save")
                            .small()
                            .label(t("config.llmSave", locale))
                            .on_click(cx.listener(|this, _, _, cx| this.save_llm(cx))),
                    ),
            )
            .child(
                h_flex()
                    .gap_3()
                    .items_end()
                    .child(field(
                        t("config.testService", locale),
                        Input::new(&self.test_service),
                        cx,
                    ))
                    .child(field(
                        t("config.testModel", locale),
                        Input::new(&self.test_model),
                        cx,
                    ))
                    .child(
                        Button::new("config-llm-test")
                            .small()
                            .label(t("config.llmTest", locale))
                            .loading(self.test.is_loading())
                            .on_click(cx.listener(|this, _, _, cx| this.test_model(cx))),
                    ),
            )
            .child(match &self.test {
                Load::Ready(response) => hint(
                    SharedString::from(format!(
                        "{} · {} · {} ms · {}",
                        response.service, response.model_key, response.latency_ms, response.message
                    )),
                    cx,
                )
                .into_any_element(),
                Load::Failed(failure) => failure_block(failure, locale, cx).into_any_element(),
                _ => div().into_any_element(),
            })
            .into_any_element()
    }
}

impl Render for ConfigView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .label(t("config.reload", locale))
                    .on_click(cx.listener(|this, _, _, cx| this.reload(cx))),
            );

        let outcome_row = self
            .outcome
            .map(|(positive, key)| notice(t(key, locale), positive, cx));

        let confirm_row = if self.confirm.is_armed(CONFIRM_PROJECT_CONFIG) {
            Some(
                confirm_bar(
                    "config-save",
                    SharedString::from(t("config.confirmSave", locale)),
                    locale,
                    cx,
                    cx.listener(|this, _, _, cx| {
                        this.confirm.cancel();
                        this.save_project(cx);
                    }),
                    cx.listener(|this, _, _, cx| {
                        this.confirm.cancel();
                        cx.notify();
                    }),
                )
                .into_any_element(),
            )
        } else {
            None
        };

        let answer = match &self.info {
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
                    .child(answer)
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("config-load-project")
                                    .small()
                                    .label(t("config.loadProfile", locale))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.load_project(window, cx)
                                    })),
                            )
                            .child(
                                Button::new("config-save-project")
                                    .small()
                                    .primary()
                                    .label(t("config.saveProfile", locale))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if this.confirm.request(CONFIRM_PROJECT_CONFIG) {
                                            this.save_project(cx);
                                        }
                                        cx.notify();
                                    })),
                            ),
                    )
                    .children(confirm_row)
                    .child(self.render_project(cx))
                    .child(
                        Button::new("config-load-llm")
                            .small()
                            .label(t("config.loadLlm", locale))
                            .on_click(cx.listener(|this, _, _, cx| this.load_llm(cx))),
                    )
                    .child(self.render_llm(cx)),
            )
    }
}
