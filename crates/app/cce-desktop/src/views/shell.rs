//! Window root: swaps between the connection form and the workspace as
//! the global connection status goes live or drops. The title bar stays here
//! so both states keep the same window chrome.

use gpui_kit::component::{ActiveTheme as _, TitleBar, h_flex, v_flex};
use gpui_kit::{
    AppContext as _, Context, Entity, FontWeight, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Window, div,
};

use crate::i18n::{locale, t};
use crate::state::{AppState, ConnectionStatus};
use crate::views::{ConnectionView, MainWindow};

pub struct Shell {
    connection: Entity<ConnectionView>,
    main: Option<Entity<MainWindow>>,
}

impl Shell {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let connection = cx.new(|cx| ConnectionView::new(window, cx));
        let state = AppState::global(cx);
        cx.observe_in(&state, window, |this, _, window, cx| this.sync(window, cx))
            .detach();

        let mut shell = Self {
            connection,
            main: None,
        };
        shell.sync(window, cx);
        shell
    }

    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match AppState::global(cx).read(cx).status {
            ConnectionStatus::Connected | ConnectionStatus::Unhealthy => {
                if self.main.is_none() {
                    self.main = Some(cx.new(|cx| MainWindow::new(window, cx)));
                }
            }
            ConnectionStatus::Disconnected | ConnectionStatus::Connecting => {
                self.main = None;
            }
        }
        cx.notify();
    }

    fn render_title_bar(&self, cx: &Context<Self>) -> impl IntoElement {
        TitleBar::new().child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(SharedString::from(t("app.name", locale(cx)))),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from(t("app.desktopClient", locale(cx)))),
                ),
        )
    }
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match self.main.clone() {
            Some(main) => main.into_any_element(),
            None => self.connection.clone().into_any_element(),
        };

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_title_bar(cx))
            .child(div().flex_1().min_h_0().child(content))
    }
}
