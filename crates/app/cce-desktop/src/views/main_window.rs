//! Main workspace window: sidebar navigation, a dock area and the status bar.
//!
//! The dock owns every panel entity. Navigation opens or focuses a stable
//! panel identity, and a project-scope change rebuilds all panels from the
//! current layout so no panel can keep another project's results.

use std::{rc::Rc, sync::Mutex, time::Duration};

use gpui_kit::component::{
    ActiveTheme as _, IndexPath, Sizable as _,
    button::{Button, ButtonVariants as _},
    command::{Command, CommandGroup, CommandItem, CommandState},
    dock::{DockArea, DockEvent, DockPlacement, DockSkin},
    h_flex,
    sidebar::{
        Sidebar, SidebarCollapsible, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu,
        SidebarMenuItem,
    },
    status_bar::StatusBar,
    v_flex,
};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Task, Window, div, px,
};

use crate::dock_layout::{self, LAYOUT_VERSION};
use crate::i18n::{locale, set_locale, t};
use crate::metrics::SIDEBAR_W;
use crate::navigator::Applied;
use crate::services::connection_service;
use crate::settings::{Density, Locale, ThemeModeSetting};
use crate::state::{AppState, ConnectionStatus};
use crate::views::PanelKind;

/// One active dock layout, held for the lifetime of the window.
pub struct MainWindow {
    dock_area: Entity<DockArea>,
    _skin: Rc<DockSkin>,
    selected: PanelKind,
    scope: Option<i64>,
    applied_nav: Applied,
    /// Command palette state, shared across renders so the query survives.
    palette: Entity<CommandState>,
    palette_open: bool,
    save_generation: u64,
    _save_task: Option<Task<()>>,
}

impl MainWindow {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (dock_area, skin) = DockSkin::dock_area("cce-main", Some(LAYOUT_VERSION), window, cx);
        let layout = dock_layout::load_or_default(cx);

        let state = AppState::global(cx);
        let (scope, applied_nav) = {
            let snapshot = state.read(cx);
            let generation = snapshot
                .pending_nav()
                .map(|request| request.generation)
                .unwrap_or_default();
            (snapshot.active_project_id(), Applied::accepting(generation))
        };
        let selected = if PanelKind::ALL.contains(&layout.active_panel) {
            layout.active_panel
        } else {
            PanelKind::Search
        };

        cx.observe_in(&state, window, |this, _, window, cx| this.sync(window, cx))
            .detach();
        cx.subscribe_in(&dock_area, window, |this, _, event, window, cx| {
            if matches!(event, DockEvent::LayoutChanged) {
                this.schedule_save(window, cx);
            }
        })
        .detach();

        dock_area.update(cx, |area, cx| {
            if let Err(err) = area.load(layout.dock, window, cx) {
                log::warn!("failed to load the saved layout: {err}");
                return;
            }
            for placement in [
                DockPlacement::Left,
                DockPlacement::Bottom,
                DockPlacement::Right,
            ] {
                area.set_dock_collapsible(placement, true, window, cx);
            }
        });

        let mut this = Self {
            dock_area,
            _skin: skin,
            selected,
            scope,
            applied_nav,
            palette: cx.new(|cx| CommandState::new(window, cx)),
            palette_open: false,
            save_generation: 0,
            _save_task: None,
        };
        this.remember_layout(cx);
        this.select_tab(selected, window, cx);
        this
    }

    fn remember_layout(&self, cx: &mut App) {
        dock_layout::set_layout_handle(
            dock_layout::LayoutHandle {
                area: self.dock_area.downgrade(),
                active: self.selected,
            },
            cx,
        );
    }

    /// React to scope and navigation changes.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = AppState::global(cx);
        let (scope, nav) = {
            let snapshot = state.read(cx);
            (
                snapshot.active_project_id(),
                snapshot.pending_nav().cloned(),
            )
        };
        if scope != self.scope {
            self.scope = scope;
            let saved = self.dock_area.read(cx).dump(cx);
            dock_layout::clear_panels(cx);
            let area = self.dock_area.clone();
            area.update(cx, |dock, cx| {
                if let Err(err) = dock.load(saved, window, cx) {
                    log::warn!("failed to rebuild panels for a new scope: {err}");
                }
            });
            self.select_tab(self.selected, window, cx);
            // A request addressed to the previous scope's panels has no
            // receiver left, so it is dropped rather than replayed against
            // unrelated data.
            if let Some(request) = nav.as_ref() {
                self.applied_nav.accept(request.generation);
            }
        } else if let Some(request) = nav
            && self.applied_nav.accept(request.generation)
        {
            self.open(request.target.panel(), window, cx);
            if !dock_layout::apply_nav(&request.target, window, cx) {
                log::warn!(
                    "navigation target {} had no receiver after opening its panel",
                    request.target.name()
                );
            }
        }
        cx.notify();
    }

    fn select_tab(&mut self, kind: PanelKind, window: &mut Window, cx: &mut App) {
        if let Some(entry) = dock_layout::panel(kind, cx) {
            self.dock_area.update(cx, |dock, cx| {
                dock.select_panel(entry.id, window, cx);
            });
        }
    }

    /// Open a panel as a centre tab, or focus the existing tab.
    fn open(&mut self, kind: PanelKind, window: &mut Window, cx: &mut App) {
        let id = match dock_layout::panel(kind, cx) {
            Some(entry) => entry.id,
            None => {
                let (id, handle) = dock_layout::build_panel(kind, window, cx);
                self.dock_area.update(cx, |dock, cx| {
                    dock.add_panel_view(handle, DockPlacement::Center, None, window, cx);
                });
                id
            }
        };
        self.selected = kind;
        self.dock_area.update(cx, |dock, cx| {
            dock.select_panel(id, window, cx);
        });
        self.remember_layout(cx);
        self.schedule_save_in(cx);
    }

    fn schedule_save(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.schedule_save_in(cx);
    }

    fn schedule_save_in(&mut self, cx: &mut App) {
        self.save_generation = self.save_generation.wrapping_add(1);
        let generation = self.save_generation;
        let area = self.dock_area.clone();
        self._save_task = Some(cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(Duration::from_millis(800))
                .await;
            cx.update(|cx| {
                let current = cx
                    .global::<dock_layout::SaveGenerationGlobal>()
                    .0
                    .lock()
                    .unwrap_or_else(|err| err.into_inner());
                if *current != generation {
                    return;
                }
                drop(current);
                let active = cx
                    .global::<dock_layout::LayoutHandleGlobal>()
                    .0
                    .lock()
                    .unwrap_or_else(|err| err.into_inner())
                    .as_ref()
                    .map(|handle| handle.active)
                    .unwrap_or(PanelKind::Search);
                dock_layout::save(&area, active, cx);
            });
        }));
        if let Some(global) = cx.try_global::<dock_layout::SaveGenerationGlobal>() {
            let mut current = global.0.lock().unwrap_or_else(|err| err.into_inner());
            *current = generation;
        } else {
            cx.set_global(dock_layout::SaveGenerationGlobal(Mutex::new(generation)));
        }
    }

    fn menu_items(&self, items: &[PanelKind], locale: Locale, cx: &Context<Self>) -> SidebarMenu {
        let entity = cx.entity();
        let selected = self.selected;
        let mut menu = SidebarMenu::new();
        for kind in items {
            let nav = *kind;
            let view = entity.clone();
            menu = menu.child(
                SidebarMenuItem::new(nav.label(locale))
                    .icon(nav.icon())
                    .active(selected == nav)
                    .on_click(move |_, window, cx| {
                        view.update(cx, |this, cx| this.open(nav, window, cx));
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
                    &PanelKind::WORKSPACE,
                    locale,
                    cx,
                )),
            )
            .child(
                SidebarGroup::new(t("nav.system", locale)).child(self.menu_items(
                    &PanelKind::OPERATIONS,
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

    /// Command palette over the workspace.
    ///
    /// Entries are derived from `PanelKind::ALL`, so adding a panel to the
    /// sidebar also adds it here without a second list to keep in sync.
    fn render_palette(&self, cx: &mut Context<Self>) -> AnyElement {
        if !self.palette_open {
            return div().into_any_element();
        }
        let locale = locale(cx);
        let theme = cx.theme();
        let dock = self.dock_area.clone();
        let weak = cx.entity().downgrade();
        let items = PanelKind::ALL.into_iter().map(|kind| {
            CommandItem::new()
                .label(kind.label(locale))
                .keywords([kind.panel_name()])
        });
        let command = Command::new(&self.palette)
            .group(
                CommandGroup::new()
                    .label(t("command.panels", locale))
                    .items(items),
            )
            .placeholder(t("command.placeholder", locale))
            .max_h(px(420.))
            .on_confirm(move |index, window, cx| {
                for (position, kind) in PanelKind::ALL.into_iter().enumerate() {
                    if index.eq_row(IndexPath::new(position)) {
                        if let Some(entry) = dock_layout::panel(kind, cx) {
                            let dock = dock.clone();
                            dock.update(cx, |dock, cx| {
                                dock.select_panel(entry.id, window, cx);
                            });
                        }
                        weak.update(cx, |this, cx| {
                            this.selected = kind;
                            this.palette_open = false;
                            cx.notify();
                        })
                        .ok();
                        break;
                    }
                }
            });
        div()
            .absolute()
            .top(px(0.))
            .left(px(0.))
            .size_full()
            .bg(theme.background)
            .child(
                div().w_full().p_6().child(
                    div()
                        .w(px(640.))
                        .p_2()
                        .rounded(px(8.))
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.muted)
                        .child(command),
                ),
            )
            .into_any_element()
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
        let project = state
            .active_project()
            .map(|project| project.label())
            .unwrap_or_else(|| SharedString::from(t("panel.noProjectTitle", locale)));
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
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .text_color(status_color)
                            .child(SharedString::from(t(status_key, locale))),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(project),
                    ),
            )
            .right(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("status-commands")
                            .ghost()
                            .small()
                            .label(t("command.open", locale))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.palette_open = !this.palette_open;
                                cx.notify();
                            })),
                    )
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = self.dock_area.clone().into_any_element();
        let palette = self.render_palette(cx);
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
                    .child(v_flex().flex_1().min_w_0().min_h_0().child(content)),
            )
            .child(self.render_status_bar(cx))
            .child(palette)
    }
}
