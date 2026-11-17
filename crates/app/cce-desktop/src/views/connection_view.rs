//! Server connection form: base URL plus an optional token.
//!
//! An empty token falls back to the `CCE_API_TOKEN` environment variable
//! inside the client constructor. The token is held in memory only and is
//! never written to settings.

use gpui_kit::component::{
    ActiveTheme as _,
    button::{Button, ButtonVariants as _},
    input::{Input, InputState},
    v_flex,
};
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, SharedString,
    Styled as _, Window, div,
};

use crate::components::page_body;
use crate::connection::{ClientStore, DEFAULT_BASE_URL};
use crate::i18n::{locale, t};
use crate::metrics::{CARD_RADIUS, CONTENT_MAX_W, card_pad};
use crate::services::connection_service;
use crate::state::AppState;

pub struct ConnectionView {
    base_url: Entity<InputState>,
    token: Entity<InputState>,
    busy: bool,
    error: Option<SharedString>,
}

impl ConnectionView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let initial = crate::settings::global(cx)
            .last_base_url
            .clone()
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let base_url = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t("connection.baseUrl", crate::settings::global(cx).locale))
                .default_value(initial)
        });
        let token = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder(t("connection.token", crate::settings::global(cx).locale))
        });
        Self {
            base_url,
            token,
            busy: false,
            error: None,
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        AppState::global(cx).update(cx, |state, cx| state.set_connecting(cx));

        let base_url = self.base_url.read(cx).value().to_string();
        let token = self.token.read(cx).value().to_string();
        let task = connection_service::probe(cx, &base_url, &token);
        let base_url_shared: SharedString = base_url.clone().into();
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                this.busy = false;
                match outcome {
                    Ok((client, health)) => {
                        let summary = connection_service::summarize(&health);
                        ClientStore::replace(client, cx);
                        crate::settings::update(cx, |settings| {
                            settings.last_base_url = Some(base_url.clone());
                        });
                        AppState::global(cx).update(cx, |state, cx| {
                            state.set_connected(base_url_shared, summary, cx);
                        });
                    }
                    Err(err) => {
                        this.error = Some(err.to_string().into());
                        AppState::global(cx).update(cx, |state, cx| state.set_disconnected(cx));
                    }
                }
            })
            .ok();
        })
        .detach();
    }
}

impl Render for ConnectionView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = locale(cx);
        let theme = cx.theme();
        let busy = self.busy;
        let dense = AppState::global(cx).read(cx).density.is_dense();

        page_body("connection").child(
            div()
                .w_full()
                .max_w(gpui_kit::px(CONTENT_MAX_W))
                .mx_auto()
                .child(
                    v_flex()
                        .gap_4()
                        .p(gpui_kit::px(card_pad(dense)))
                        .border_1()
                        .border_color(theme.border)
                        .rounded(gpui_kit::px(CARD_RADIUS))
                        .bg(theme.background)
                        .child(
                            div()
                                .text_xl()
                                .child(SharedString::from(t("connection.title", locale))),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child(SharedString::from(t("connection.subtitle", locale))),
                        )
                        .child(Input::new(&self.base_url))
                        .child(Input::new(&self.token))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(SharedString::from(t("connection.tokenHint", locale))),
                        )
                        .children(
                            self.error
                                .clone()
                                .map(|error| div().text_sm().text_color(theme.danger).child(error)),
                        )
                        .child(
                            Button::new("connect")
                                .primary()
                                .label(if busy {
                                    SharedString::from(t("connection.connecting", locale))
                                } else {
                                    SharedString::from(t("connection.connect", locale))
                                })
                                .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                        ),
                ),
        )
    }
}
