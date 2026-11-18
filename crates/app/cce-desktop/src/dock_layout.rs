//! Dockable workspace panels and their versioned layout file.
//!
//! Panel identities are stable strings. A persisted layout is either fully
//! valid for the current version or discarded for the default layout; there
//! is no migration path for an expired schema.

use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use gpui_kit::component::dock::{
    BasePanel, DockArea, DockAreaState, Panel, PanelEvent, PanelId, PanelInfo, PanelState,
    panel_handle, register_panel,
};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, Global,
    IntoElement, ParentElement as _, Render, SharedString, Styled as _, WeakEntity, Window, div,
};
use serde::{Deserialize, Serialize};

use crate::components::{empty_state, export_button, export_notice, page_body, panel_header};
use crate::i18n::{locale, t};
use crate::navigator::Target;
use crate::settings;
use crate::state::AppState;
use crate::views::{
    ConfigView, EntityView, GraphView, HealthView, IndexView, LogsView, MetricsView, PanelKind,
    ProjectsView, SearchView, StorageView, TasksView, ToolsView, WatchView,
};

/// Bump when the default layout or the panel identity set changes.
pub const LAYOUT_VERSION: usize = 2;
const LAYOUT_FILE: &str = "layout.json";

/// Handle the dock holds for a panel entity.
pub type DockPanelHandle = Arc<dyn gpui_kit::component::dock::BasePanelView>;

/// Typed handles kept alongside the dock so navigation can select a tab,
/// cross-panel links can reach their receiver and the refresh loop can ask a
/// panel to replay without trait objects.
///
/// Only the variants that need to be addressed carry their entity; the rest
/// stay unit values so the dock remains the single owner of a panel nobody
/// links to.
#[derive(Clone)]
pub enum WorkspacePanel {
    Search,
    Graph(Entity<GraphView>),
    Entities(Entity<EntityView>),
    Index,
    Projects,
    Tools,
    Storage(Entity<StorageView>),
    Watch(Entity<WatchView>),
    Health(Entity<HealthView>),
    Metrics(Entity<MetricsView>),
    Config,
    Tasks(Entity<TasksView>),
    Logs(Entity<LogsView>),
}

#[derive(Clone)]
pub(crate) struct PanelEntry {
    pub(crate) id: PanelId,
    view: WorkspacePanel,
}

/// Panel index keyed by stable identity. The dock owns the entities; this map
/// only remembers their ids and typed handles.
struct PanelHandles(Mutex<BTreeMap<PanelKind, PanelEntry>>);

impl Global for PanelHandles {}

/// Area and active panel used by the process-wide flush on quit.
#[derive(Clone)]
pub struct LayoutHandle {
    pub area: WeakEntity<DockArea>,
    pub active: PanelKind,
}

pub struct LayoutHandleGlobal(pub Mutex<Option<LayoutHandle>>);

impl Global for LayoutHandleGlobal {}

/// Generation of the latest layout change, used to drop stale debounced saves.
pub struct SaveGenerationGlobal(pub Mutex<u64>);

impl Global for SaveGenerationGlobal {}

/// Persisted layout envelope. The dock state carries its own version too, but
/// `version` here is the single gate checked before loading it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutFile {
    pub(crate) version: usize,
    pub(crate) active_panel: PanelKind,
    pub(crate) dock: DockAreaState,
}

impl LayoutFile {
    fn default_layout() -> Self {
        Self {
            version: LAYOUT_VERSION,
            active_panel: PanelKind::Search,
            dock: DockAreaState {
                version: Some(LAYOUT_VERSION),
                center: PanelState {
                    panel_name: "TabPanel".to_string(),
                    children: PanelKind::WORKSPACE
                        .iter()
                        .map(|kind| PanelState::new(kind.panel_name()))
                        .collect(),
                    info: PanelInfo::tabs(0),
                },
                left_dock: None,
                right_dock: None,
                bottom_dock: None,
            },
        }
    }
}

/// Adapt a view to the dock without changing the view itself.
pub struct DockPanel<V> {
    kind: PanelKind,
    id: PanelId,
    view: Entity<V>,
    focus_handle: FocusHandle,
}

impl<V: 'static> DockPanel<V> {
    fn new(kind: PanelKind, view: Entity<V>, cx: &mut Context<Self>) -> Self {
        let status = crate::services::export_service::ExportStatus::global(cx);
        cx.observe(&status, |_, _, cx| cx.notify()).detach();
        Self {
            kind,
            id: PanelId::from(cx.entity_id()),
            view,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl<V: 'static> EventEmitter<PanelEvent> for DockPanel<V> {}

impl<V: 'static> Focusable for DockPanel<V> {
    fn focus_handle(&self, _: &gpui_kit::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl<V: Render + 'static> BasePanel for DockPanel<V> {
    fn panel_name(&self) -> &'static str {
        self.kind.panel_name()
    }

    fn closable(&self, _: &App) -> bool {
        true
    }

    fn zoomable(&self, _: &App) -> bool {
        true
    }

    fn on_removed(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        remove_handle(self.kind, self.id, cx);
    }

    fn dump(&self, _: &App) -> PanelState {
        PanelState::new(self.kind.panel_name())
    }
}

impl<V: Render + 'static> Panel for DockPanel<V> {
    fn tab_name(&self, cx: &App) -> Option<SharedString> {
        Some(self.kind.label(locale(cx)))
    }

    fn title(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().child(self.kind.label(locale(cx)))
    }

    fn inner_padding(&self, _: &App) -> bool {
        false
    }

    fn title_suffix(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let snapshot = crate::services::export_service::snapshot(self.kind, cx)?;
        if snapshot.rows.is_empty() {
            return None;
        }
        let project_id = AppState::global(cx).read(cx).active_project_id();
        Some(export_button(self.kind, project_id, cx))
    }
}

impl<V: Render + 'static> Render for DockPanel<V> {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = locale(cx);
        let scoped = AppState::global(cx).read(cx).active_project_id().is_some();
        let mut body = div().size_full().min_w_0().min_h_0();
        if let Some(notice) = export_notice(self.kind, cx) {
            body = body.child(notice);
        }
        if self.kind.needs_project() && !scoped {
            body.child(
                page_body(self.kind.panel_name())
                    .child(panel_header(self.kind.label(locale), None, None, cx))
                    .child(empty_state(
                        t("panel.noProjectTitle", locale),
                        Some(SharedString::from(t("panel.noProjectHint", locale))),
                        None,
                        cx,
                    )),
            )
        } else {
            body.child(self.view.clone())
        }
    }
}

/// Install panel builders in the global dock registry.
pub fn register_panels(cx: &mut App) {
    macro_rules! register {
        ($kind:expr) => {{
            let kind = $kind;
            register_panel(cx, kind.panel_name(), move |_context, window, cx| {
                build_panel(kind, window, cx).1
            });
        }};
    }
    register!(PanelKind::Search);
    register!(PanelKind::Graph);
    register!(PanelKind::Entities);
    register!(PanelKind::Index);
    register!(PanelKind::Projects);
    register!(PanelKind::Tools);
    register!(PanelKind::Storage);
    register!(PanelKind::Watch);
    register!(PanelKind::Health);
    register!(PanelKind::Metrics);
    register!(PanelKind::Config);
    register!(PanelKind::Tasks);
    register!(PanelKind::Logs);
}

/// Create a fresh view, register its typed handle and wrap it for the dock.
pub fn build_panel(
    kind: PanelKind,
    window: &mut Window,
    cx: &mut App,
) -> (PanelId, DockPanelHandle) {
    match kind {
        PanelKind::Search => make(
            kind,
            window,
            cx,
            |window, cx| cx.new(|cx| SearchView::new(window, cx)),
            |_| WorkspacePanel::Search,
        ),
        PanelKind::Graph => make(
            kind,
            window,
            cx,
            |window, cx| cx.new(|cx| GraphView::new(window, cx)),
            WorkspacePanel::Graph,
        ),
        PanelKind::Entities => make(
            kind,
            window,
            cx,
            |window, cx| cx.new(|cx| EntityView::new(window, cx)),
            WorkspacePanel::Entities,
        ),
        PanelKind::Index => make(
            kind,
            window,
            cx,
            |window, cx| cx.new(|cx| IndexView::new(window, cx)),
            |_| WorkspacePanel::Index,
        ),
        PanelKind::Projects => make(
            kind,
            window,
            cx,
            |window, cx| cx.new(|cx| ProjectsView::new(window, cx)),
            |_| WorkspacePanel::Projects,
        ),
        PanelKind::Tools => make(
            kind,
            window,
            cx,
            |window, cx| cx.new(|cx| ToolsView::new(window, cx)),
            |_| WorkspacePanel::Tools,
        ),
        PanelKind::Storage => make(
            kind,
            window,
            cx,
            |window, cx| cx.new(|cx| StorageView::new(window, cx)),
            WorkspacePanel::Storage,
        ),
        PanelKind::Watch => make(
            kind,
            window,
            cx,
            |window, cx| cx.new(|cx| WatchView::new(window, cx)),
            WorkspacePanel::Watch,
        ),
        PanelKind::Health => {
            let view = cx.new(|cx| HealthView::new(window, cx));
            view.update(cx, |health, cx| health.load(cx));
            wrap_panel(kind, view, cx, WorkspacePanel::Health)
        }
        PanelKind::Metrics => make(
            kind,
            window,
            cx,
            |window, cx| cx.new(|cx| MetricsView::new(window, cx)),
            WorkspacePanel::Metrics,
        ),
        PanelKind::Config => make(
            kind,
            window,
            cx,
            |window, cx| cx.new(|cx| ConfigView::new(window, cx)),
            |_| WorkspacePanel::Config,
        ),
        PanelKind::Tasks => make(
            kind,
            window,
            cx,
            |window, cx| cx.new(|cx| TasksView::new(window, cx)),
            WorkspacePanel::Tasks,
        ),
        PanelKind::Logs => make(
            kind,
            window,
            cx,
            |window, cx| cx.new(|cx| LogsView::new(window, cx)),
            WorkspacePanel::Logs,
        ),
    }
}

fn make<V, B>(
    kind: PanelKind,
    window: &mut Window,
    cx: &mut App,
    build: B,
    wrap: fn(Entity<V>) -> WorkspacePanel,
) -> (PanelId, DockPanelHandle)
where
    V: Render + 'static,
    B: FnOnce(&mut Window, &mut App) -> Entity<V>,
{
    wrap_panel(kind, build(window, cx), cx, wrap)
}

fn wrap_panel<V>(
    kind: PanelKind,
    view: Entity<V>,
    cx: &mut App,
    wrap: fn(Entity<V>) -> WorkspacePanel,
) -> (PanelId, DockPanelHandle)
where
    V: Render + 'static,
{
    let panel = cx.new(|cx| DockPanel::new(kind, view.clone(), cx));
    let id = PanelId::from(panel.entity_id());
    let mut handles = cx
        .global::<PanelHandles>()
        .0
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    handles.insert(
        kind,
        PanelEntry {
            id,
            view: wrap(view),
        },
    );
    (id, panel_handle(panel))
}

/// Look up the typed handle of an open panel.
pub fn panel(kind: PanelKind, cx: &App) -> Option<PanelEntry> {
    cx.global::<PanelHandles>()
        .0
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .get(&kind)
        .cloned()
}

/// Hand a navigation target to the panel that answers it.
///
/// Returns `false` when that panel is not open yet, so the window can open it
/// and deliver again instead of losing the request. Dispatch is a `match` on
/// the typed handle: no trait object and no second registry.
pub fn apply_nav(target: &Target, window: &mut Window, cx: &mut App) -> bool {
    let kind = target.panel();
    let Some(entry) = panel(kind, cx) else {
        return false;
    };
    match (&entry.view, target) {
        (WorkspacePanel::Graph(view), Target::GraphSeed { .. } | Target::GraphFile { .. }) => {
            let view = view.clone();
            let target = target.clone();
            view.update(cx, |view, cx| {
                view.on_navigate(&target, window, cx);
            });
            true
        }
        (WorkspacePanel::Entities(view), Target::Entity { .. }) => {
            let view = view.clone();
            let target = target.clone();
            view.update(cx, |view, cx| {
                view.on_navigate(&target, window, cx);
            });
            true
        }
        _ => false,
    }
}

/// Ask one open panel to re-run its last request.
///
/// A panel that is not open, or that has no live answer yet, ignores the
/// signal: replaying an untouched panel would turn an empty state into an
/// unsolicited request.
pub fn replay(kind: PanelKind, cx: &mut App) {
    let Some(entry) = panel(kind, cx) else {
        return;
    };
    match &entry.view {
        WorkspacePanel::Health(view) => {
            let view = view.clone();
            view.update(cx, |view, cx| view.replay(cx));
        }
        WorkspacePanel::Metrics(view) => {
            let view = view.clone();
            view.update(cx, |view, cx| view.replay(cx));
        }
        WorkspacePanel::Watch(view) => {
            let view = view.clone();
            view.update(cx, |view, cx| view.replay(cx));
        }
        WorkspacePanel::Storage(view) => {
            let view = view.clone();
            view.update(cx, |view, cx| view.replay(cx));
        }
        WorkspacePanel::Tasks(view) => {
            let view = view.clone();
            view.update(cx, |view, cx| view.replay(cx));
        }
        WorkspacePanel::Logs(view) => {
            let view = view.clone();
            view.update(cx, |view, cx| view.replay(cx));
        }
        _ => {}
    }
}

fn remove_handle(kind: PanelKind, id: PanelId, cx: &App) {
    let mut handles = cx
        .global::<PanelHandles>()
        .0
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    let matches = handles
        .get(&kind)
        .map(|entry| entry.id == id)
        .unwrap_or(false);
    if matches {
        handles.remove(&kind);
    }
}

/// Remember the current dock area for the quit-time flush.
pub fn set_layout_handle(handle: LayoutHandle, cx: &mut App) {
    let mut global = cx
        .global_mut::<LayoutHandleGlobal>()
        .0
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    *global = Some(handle);
}

/// Flush the live dock layout to disk.
pub fn flush(cx: &mut App) {
    let handle = {
        let global = cx
            .global_mut::<LayoutHandleGlobal>()
            .0
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        global.clone()
    };
    let Some(handle) = handle else { return };
    let Some(area) = handle.area.upgrade() else {
        return;
    };
    let state = area.read(cx).dump(cx);
    write(&LayoutFile {
        version: LAYOUT_VERSION,
        active_panel: handle.active,
        dock: state,
    });
}

fn layout_path() -> Option<PathBuf> {
    settings::Settings::config_dir().map(|dir| dir.join(LAYOUT_FILE))
}

/// Load a valid layout, otherwise build and persist the default.
pub fn load_or_default(_cx: &App) -> LayoutFile {
    let loaded = layout_path()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|raw| serde_json::from_str::<LayoutFile>(&raw).ok())
        .filter(valid);
    loaded.unwrap_or_else(|| {
        let file = LayoutFile::default_layout();
        write(&file);
        file
    })
}

/// A layout is usable only when its version and panel set match this build.
pub fn valid(file: &LayoutFile) -> bool {
    if file.version != LAYOUT_VERSION {
        return false;
    }
    let mut names = Vec::new();
    collect_names(&file.dock.center, &mut names);
    for dock in [
        file.dock.left_dock.as_ref(),
        file.dock.right_dock.as_ref(),
        file.dock.bottom_dock.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        collect_names(dock.panel(), &mut names);
    }
    let known: Vec<&str> = PanelKind::ALL
        .iter()
        .map(|kind| kind.panel_name())
        .collect();
    if !names.iter().all(|name| known.contains(&name.as_str())) {
        return false;
    }
    names.sort_unstable();
    let count = names.len();
    names.dedup();
    count == names.len()
}

fn collect_names(state: &PanelState, out: &mut Vec<String>) {
    match &state.info {
        PanelInfo::Panel(_) => out.push(state.panel_name.clone()),
        _ => {
            if state.panel_name != "TabPanel" && state.panel_name != "StackPanel" {
                out.push(state.panel_name.clone());
            }
            for child in &state.children {
                collect_names(child, out);
            }
        }
    }
}

fn write(file: &LayoutFile) {
    let Some(path) = layout_path() else { return };
    let raw = match serde_json::to_string_pretty(file) {
        Ok(raw) => raw,
        Err(err) => {
            log::warn!("layout is not serializable: {err}");
            return;
        }
    };
    if let Some(parent) = path.parent()
        && let Err(err) = fs::create_dir_all(parent)
    {
        log::warn!("failed to create layout dir {}: {err}", parent.display());
        return;
    }
    let temporary = path.with_extension("json.tmp");
    if let Err(err) = fs::write(&temporary, raw) {
        log::warn!("failed to write layout {}: {err}", temporary.display());
        return;
    }
    if let Err(err) = fs::rename(&temporary, &path) {
        log::warn!("failed to replace layout {}: {err}", path.display());
    }
}

/// Write the live dock state with the active navigation item.
pub fn save(area: &Entity<DockArea>, active: PanelKind, cx: &App) {
    let state = area.read(cx).dump(cx);
    write(&LayoutFile {
        version: LAYOUT_VERSION,
        active_panel: active,
        dock: state,
    });
}

/// Install the panel index, layout handle and export stores.
pub fn init(cx: &mut App) {
    if cx.try_global::<PanelHandles>().is_none() {
        cx.set_global(PanelHandles(Mutex::new(BTreeMap::new())));
    }
    if cx.try_global::<LayoutHandleGlobal>().is_none() {
        cx.set_global(LayoutHandleGlobal(Mutex::new(None)));
    }
    crate::services::export_service::init(cx);
}

/// Clear typed handles before rebuilding panels for a new project scope.
pub fn clear_panels(cx: &App) {
    cx.global::<PanelHandles>()
        .0
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(version: usize, names: &[&str]) -> LayoutFile {
        let mut layout = LayoutFile::default_layout();
        layout.version = version;
        layout.dock.center = PanelState {
            panel_name: "TabPanel".to_string(),
            children: names.iter().map(|name| PanelState::new(*name)).collect(),
            info: PanelInfo::tabs(0),
        };
        layout
    }

    #[test]
    fn default_layout_carries_the_current_version() {
        let file = LayoutFile::default_layout();
        assert_eq!(file.version, LAYOUT_VERSION);
        assert_eq!(file.dock.center.children.len(), 5);
        assert_eq!(file.active_panel, PanelKind::Search);
        assert!(file.dock.left_dock.is_none());
    }

    #[test]
    fn valid_layout_accepts_known_panels() {
        let names: Vec<&str> = PanelKind::ALL
            .iter()
            .map(|kind| kind.panel_name())
            .collect();
        assert!(valid(&file(LAYOUT_VERSION, &names)));
    }

    #[test]
    fn expired_or_unknown_layouts_are_rejected() {
        assert!(!valid(&file(LAYOUT_VERSION - 1, &["cce.search"])));
        assert!(!valid(&file(LAYOUT_VERSION, &["cce.search", "cce.gone"])));
        assert!(!valid(&file(LAYOUT_VERSION, &["cce.search", "cce.search"])));
    }
}
