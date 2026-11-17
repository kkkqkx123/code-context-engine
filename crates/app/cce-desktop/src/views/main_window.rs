//! Main workspace window: sidebar navigation, placeholder panels and a
//! status bar. Stage 3 replaces each placeholder with its business panel;
//! the health entry already renders the live probe snapshot.

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    sidebar::{
        Sidebar, SidebarCollapsible, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu,
        SidebarMenuItem,
    },
    status_bar::StatusBar,
    v_flex,
};
use gpui_kit::{
    Context, IntoElement, ParentElement as _, Render, SharedString, Styled as _, Window, div, px,
};

use crate::components::{empty_state, page_body, panel_header};
use crate::i18n::{locale, set_locale, t};
use crate::metrics::SIDEBAR_W;
use crate::services::connection_service;
use crate::settings::{Density, Locale, ThemeModeSetting};
use crate::state::{AppState, ConnectionStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NavItem {
    Search,
    Graph,
    Entities,
    Index,
    Projects,
    Health,
}

impl NavItem {
    const WORKSPACE: [NavItem; 5] = [
        NavItem::Search,
        NavItem::Graph,
        NavItem::Entities,
        NavItem::Index,
        NavItem::Projects,
    ];

    fn label_key(self) -> &'static str {
        match self {
            NavItem::Search => "nav.search",
            NavItem::Graph => "nav.graph",
            NavItem::Entities => "nav.entities",
            NavItem::Index => "nav.index",
            NavItem::Projects => "nav.projects",
            NavItem::Health => "nav.health",
        }
    }

    fn icon(self) -> IconName {
        match self {
            NavItem::Search => IconName::Search,
            NavItem::Graph => IconName::Network,
            NavItem::Entities => IconName::Database,
            NavItem::Index => IconName::Layers,
            NavItem::Projects => IconName::Folder,
            NavItem::Health => IconName::Activity,
        }
    }
}

pub struct MainWindow {
    selected: NavItem,
}

impl MainWindow {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            selected: NavItem::Search,
        }
    }

    fn menu_items(&self, items: &[NavItem], locale: Locale, cx: &Context<Self>) -> SidebarMenu {
        let entity = cx.entity();
        let selected = self.selected;
        let mut menu = SidebarMenu::new();
        for item in items {
            let nav = *item;
            let view = entity.clone();
            menu = menu.child(
                SidebarMenuItem::new(t(nav.label_key(), locale))
                    .icon(nav.icon())
                    .active(selected == nav)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.selected = nav;
                            cx.notify();
                        });
                    }),
            );
        }
        menu
    }

    fn render_sidebar(&self, cx: &Context<Self>) -> impl IntoElement {
        let locale = locale(cx);
        let theme = cx.theme();
        Sidebar::new("cce-nav")
            .collapsible(SidebarCollapsible::None)
            .w(px(SIDEBAR_W))
            .header(
                SidebarHeader::new().child(
                    div()
                        .text_sm()
                        .child(SharedString::from(t("app.name", locale))),
                ),
            )
            .child(
                SidebarGroup::new(t("nav.workspace", locale)).child(self.menu_items(
                    &NavItem::WORKSPACE,
                    locale,
                    cx,
                )),
            )
            .child(
                SidebarGroup::new(t("nav.system", locale)).child(self.menu_items(
                    &[NavItem::Health],
                    locale,
                    cx,
                )),
            )
            .footer(
                SidebarFooter::new().child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(status_base_url(cx)),
                ),
            )
    }

    fn render_content(&self, cx: &Context<Self>) -> impl IntoElement {
        match self.selected {
            NavItem::Health => self.render_health(cx).into_any_element(),
            item => {
                let locale = locale(cx);
                page_body("stage-placeholder")
                    .child(panel_header(t(item.label_key(), locale), None, None, cx))
                    .child(empty_state(
                        t(item.label_key(), locale),
                        Some(SharedString::from(t("common.stageNote", locale))),
                        None,
                        cx,
                    ))
                    .into_any_element()
            }
        }
    }

    fn render_health(&self, cx: &Context<Self>) -> impl IntoElement {
        let locale = locale(cx);
        let theme = cx.theme();
        let health = AppState::global(cx).read(cx).health.clone();
        let body = match health {
            Some(summary) => div()
                .text_sm()
                .text_color(if summary.healthy {
                    theme.success
                } else {
                    theme.danger
                })
                .child(summary.detail),
            None => div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(SharedString::from(t("health.waiting", locale))),
        };
        page_body("health")
            .child(panel_header(
                t("health.title", locale),
                Some(SharedString::from(t("health.subtitle", locale))),
                None,
                cx,
            ))
            .child(body)
    }

    fn render_status_bar(&self, cx: &Context<Self>) -> impl IntoElement {
        let locale = locale(cx);
        let theme = cx.theme();
        let state = AppState::global(cx).read(cx);
        let (status_key, status_color) = match state.status {
            ConnectionStatus::Connected => ("status.connected", theme.success),
            ConnectionStatus::Connecting => ("status.connecting", theme.warning),
            ConnectionStatus::Disconnected => ("status.disconnected", theme.muted_foreground),
            ConnectionStatus::Unhealthy => ("status.unhealthy", theme.danger),
        };
        let theme_label = match crate::settings::global(cx).theme_mode {
            ThemeModeSetting::System => t("theme.system", locale),
            ThemeModeSetting::Light => t("theme.light", locale),
        };
        let density_label = match state.density {
            Density::Comfortable => t("density.comfortable", locale),
            Density::Dense => t("density.dense", locale),
        };
        let locale_label = match locale {
            Locale::En => "中文",
            Locale::Zh => "EN",
        };

        StatusBar::new()
            .left(
                div()
                    .text_xs()
                    .text_color(status_color)
                    .child(SharedString::from(t(status_key, locale))),
            )
            .right(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("status-theme")
                            .ghost()
                            .small()
                            .label(theme_label)
                            .on_click(cx.listener(|_, _, window, cx| {
                                let next = match crate::settings::global(cx).theme_mode {
                                    ThemeModeSetting::System => ThemeModeSetting::Light,
                                    ThemeModeSetting::Light => ThemeModeSetting::System,
                                };
                                crate::settings::update(cx, |settings| {
                                    settings.theme_mode = next;
                                });
                                crate::theme::set_mode(next, Some(window), cx);
                            })),
                    )
                    .child(
                        Button::new("status-density")
                            .ghost()
                            .small()
                            .label(density_label)
                            .on_click(cx.listener(|_, _, _, cx| {
                                let next = crate::settings::global(cx).density.toggled();
                                crate::settings::update(cx, |settings| {
                                    settings.density = next;
                                });
                                AppState::global(cx).update(cx, |state, cx| {
                                    state.set_density(next, cx);
                                });
                            })),
                    )
                    .child(
                        Button::new("status-locale")
                            .ghost()
                            .small()
                            .label(locale_label)
                            .on_click(cx.listener(|_, _, _, cx| {
                                let next = match crate::i18n::locale(cx) {
                                    Locale::En => Locale::Zh,
                                    Locale::Zh => Locale::En,
                                };
                                set_locale(next, cx);
                            })),
                    )
                    .child(
                        Button::new("status-disconnect")
                            .ghost()
                            .small()
                            .label(t("connection.disconnect", locale))
                            .on_click(cx.listener(|_, _, _, cx| {
                                connection_service::disconnect(cx);
                            })),
                    ),
            )
    }
}

fn status_base_url(cx: &Context<MainWindow>) -> SharedString {
    AppState::global(cx)
        .read(cx)
        .base_url
        .clone()
        .unwrap_or_else(|| SharedString::from(t("status.disconnected", locale(cx))))
}

impl Render for MainWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(self.render_sidebar(cx))
                    .child(v_flex().flex_1().min_w_0().child(self.render_content(cx))),
            )
            .child(self.render_status_bar(cx))
    }
}
