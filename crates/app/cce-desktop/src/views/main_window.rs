//! Main workspace window: sidebar navigation, the active panel and the status
//! bar.
//!
//! Panels are created lazily and dropped when the project scope changes, so a
//! panel never shows an answer that belongs to another project. A search hit
//! delivered through the global slot switches to the graph panel exactly once
//! per delivery number.

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
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, SharedString,
    Styled as _, Window, div, px,
};

use crate::components::{empty_state, page_body, panel_header};
use crate::i18n::{locale, set_locale, t};
use crate::metrics::SIDEBAR_W;
use crate::services::connection_service;
use crate::settings::{Density, Locale, ThemeModeSetting};
use crate::state::{AppState, ConnectionStatus};
use crate::views::{
    ConfigView, EntityView, GraphView, HealthView, IndexView, MetricsView, ProjectsView,
    SearchView, StorageView, TasksView, ToolsView, WatchView,
};

/// Navigation entries, split into the two sidebar groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NavItem {
    Search,
    Graph,
    Entities,
    Index,
    Projects,
    Tools,
    Storage,
    Watch,
    Health,
    Metrics,
    Config,
    Tasks,
}

impl NavItem {
    const WORKSPACE: [NavItem; 5] = [
        NavItem::Search,
        NavItem::Graph,
        NavItem::Entities,
        NavItem::Index,
        NavItem::Projects,
    ];

    const OPERATIONS: [NavItem; 7] = [
        NavItem::Tools,
        NavItem::Storage,
        NavItem::Watch,
        NavItem::Health,
        NavItem::Metrics,
        NavItem::Config,
        NavItem::Tasks,
    ];

    fn label_key(self) -> &'static str {
        match self {
            NavItem::Search => "nav.search",
            NavItem::Graph => "nav.graph",
            NavItem::Entities => "nav.entities",
            NavItem::Index => "nav.index",
            NavItem::Projects => "nav.projects",
            NavItem::Tools => "nav.tools",
            NavItem::Storage => "nav.storage",
            NavItem::Watch => "nav.watch",
            NavItem::Health => "nav.health",
            NavItem::Metrics => "nav.metrics",
            NavItem::Config => "nav.config",
            NavItem::Tasks => "nav.tasks",
        }
    }

    fn icon(self) -> IconName {
        match self {
            NavItem::Search => IconName::Search,
            NavItem::Graph => IconName::Network,
            NavItem::Entities => IconName::Database,
            NavItem::Index => IconName::Layers,
            NavItem::Projects => IconName::Folder,
            NavItem::Tools => IconName::Wrench,
            NavItem::Storage => IconName::HardDrive,
            NavItem::Watch => IconName::Activity,
            NavItem::Health => IconName::Gauge,
            NavItem::Metrics => IconName::ChartColumn,
            NavItem::Config => IconName::Settings,
            NavItem::Tasks => IconName::ListChecks,
        }
    }

    /// `true` when the panel needs a project scope to run.
    fn needs_project(self) -> bool {
        !matches!(
            self,
            NavItem::Health | NavItem::Metrics | NavItem::Config | NavItem::Projects
        )
    }
}

/// Lazily created panels, all built for the current project scope.
#[derive(Default)]
struct Panels {
    search: Option<Entity<SearchView>>,
    graph: Option<Entity<GraphView>>,
    entities: Option<Entity<EntityView>>,
    index: Option<Entity<IndexView>>,
    projects: Option<Entity<ProjectsView>>,
    tools: Option<Entity<ToolsView>>,
    storage: Option<Entity<StorageView>>,
    watch: Option<Entity<WatchView>>,
    health: Option<Entity<HealthView>>,
    metrics: Option<Entity<MetricsView>>,
    config: Option<Entity<ConfigView>>,
    tasks: Option<Entity<TasksView>>,
}

impl Panels {
    fn clear(&mut self) {
        *self = Self::default();
    }
}

pub struct MainWindow {
    selected: NavItem,
    /// Project scope the cached panels were built for.
    scope: Option<i64>,
    applied_seed: u64,
    panels: Panels,
}

impl MainWindow {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = AppState::global(cx);
        let (scope, applied_seed) = {
            let snapshot = state.read(cx);
            let seed = snapshot.graph_seed().map(|seed| seed.generation);
            (snapshot.active_project_id(), seed.unwrap_or_default())
        };
        cx.observe_in(&state, window, |this, _, window, cx| this.sync(window, cx))
            .detach();
        Self {
            selected: NavItem::Search,
            scope,
            applied_seed,
            panels: Panels::default(),
        }
    }

    /// React to scope and hand-off changes.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = AppState::global(cx);
        let (scope, seed) = {
            let snapshot = state.read(cx);
            (snapshot.active_project_id(), snapshot.graph_seed().cloned())
        };
        if scope != self.scope {
            self.scope = scope;
            self.panels.clear();
        }

        if let Some(seed) = seed
            && seed.generation > self.applied_seed
        {
            self.applied_seed = seed.generation;
            self.selected = NavItem::Graph;
            let graph = self.graph(window, cx);
            graph.update(cx, |view, cx| {
                view.apply_seed(&seed, window, cx);
            });
        }
        cx.notify();
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

    fn search(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<SearchView> {
        self.panels
            .search
            .get_or_insert_with(|| cx.new(|cx| SearchView::new(window, cx)))
            .clone()
    }

    fn graph(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<GraphView> {
        self.panels
            .graph
            .get_or_insert_with(|| cx.new(|cx| GraphView::new(window, cx)))
            .clone()
    }

    fn entities(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<EntityView> {
        self.panels
            .entities
            .get_or_insert_with(|| cx.new(|cx| EntityView::new(window, cx)))
            .clone()
    }

    fn index(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<IndexView> {
        self.panels
            .index
            .get_or_insert_with(|| cx.new(|cx| IndexView::new(window, cx)))
            .clone()
    }

    fn projects(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<ProjectsView> {
        self.panels
            .projects
            .get_or_insert_with(|| cx.new(|cx| ProjectsView::new(window, cx)))
            .clone()
    }

    fn tools(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<ToolsView> {
        self.panels
            .tools
            .get_or_insert_with(|| cx.new(|cx| ToolsView::new(window, cx)))
            .clone()
    }

    fn storage(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<StorageView> {
        self.panels
            .storage
            .get_or_insert_with(|| cx.new(|cx| StorageView::new(window, cx)))
            .clone()
    }

    fn watch(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<WatchView> {
        self.panels
            .watch
            .get_or_insert_with(|| cx.new(|cx| WatchView::new(window, cx)))
            .clone()
    }

    /// The health panel probes on first open so the panel is never blank.
    fn health(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<HealthView> {
        if let Some(panel) = &self.panels.health {
            return panel.clone();
        }
        let panel = cx.new(|cx| HealthView::new(window, cx));
        panel.update(cx, |view, cx| view.load(cx));
        self.panels.health = Some(panel.clone());
        panel
    }

    fn metrics(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<MetricsView> {
        self.panels
            .metrics
            .get_or_insert_with(|| cx.new(|cx| MetricsView::new(window, cx)))
            .clone()
    }

    fn config(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<ConfigView> {
        self.panels
            .config
            .get_or_insert_with(|| cx.new(|cx| ConfigView::new(window, cx)))
            .clone()
    }

    fn tasks(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<TasksView> {
        self.panels
            .tasks
            .get_or_insert_with(|| cx.new(|cx| TasksView::new(window, cx)))
            .clone()
    }

    fn render_content(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let locale = locale(cx);
        let scoped = AppState::global(cx).read(cx).active_project_id().is_some();
        if self.selected.needs_project() && !scoped {
            return page_body("no-scope")
                .child(panel_header(
                    t(self.selected.label_key(), locale),
                    None,
                    None,
                    cx,
                ))
                .child(empty_state(
                    t("panel.noProjectTitle", locale),
                    Some(SharedString::from(t("panel.noProjectHint", locale))),
                    None,
                    cx,
                ))
                .into_any_element();
        }

        let panel: gpui_kit::AnyElement = match self.selected {
            NavItem::Search => self.search(window, cx).into_any_element(),
            NavItem::Graph => self.graph(window, cx).into_any_element(),
            NavItem::Entities => self.entities(window, cx).into_any_element(),
            NavItem::Index => self.index(window, cx).into_any_element(),
            NavItem::Projects => self.projects(window, cx).into_any_element(),
            NavItem::Tools => self.tools(window, cx).into_any_element(),
            NavItem::Storage => self.storage(window, cx).into_any_element(),
            NavItem::Watch => self.watch(window, cx).into_any_element(),
            NavItem::Health => self.health(window, cx).into_any_element(),
            NavItem::Metrics => self.metrics(window, cx).into_any_element(),
            NavItem::Config => self.config(window, cx).into_any_element(),
            NavItem::Tasks => self.tasks(window, cx).into_any_element(),
        };
        v_flex()
            .size_full()
            .min_h_0()
            .child(panel)
            .into_any_element()
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
                    &NavItem::OPERATIONS,
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = self.render_content(window, cx);
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
    }
}
