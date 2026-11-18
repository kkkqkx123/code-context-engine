//! Graph panel: neighbourhood, path, subgraph, components, cycles, impact,
//! module and structural answers over one project scope.
//!
//! The panel accepts a seed from the search panel through the global slot; the
//! delivery number guards against applying the same hit twice.

use cce_api::models::{
    GraphComponentsResponse, GraphCyclesResponse, GraphImpactResponse, GraphModuleResponse,
    GraphPathResponse, GraphStructuralResponse, GraphSubgraphResponse,
};
use gpui_kit::component::{
    ActiveTheme as _, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    v_flex,
};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Div, Entity, FontWeight, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, Window, div, px,
};

use crate::components::{
    Cell, Column, Tone, choice_row, data_grid, empty_state, failure_block, field, field_group,
    hint, loading_row, page_body, panel_header, stack, stale_epoch, surface, toast,
};
use crate::connection::ClientStore;
use crate::graph::{CanvasEvent, DEFAULT_MAX_NODES, GraphCanvas, LayoutKind, Topology};
use crate::i18n::{locale, t};
use crate::navigator::Target;
use crate::services::export_service;
use crate::services::graph_service::{self, CYCLE_LEVELS, EGO_DIRECTIONS, STRUCTURAL_KINDS};
use crate::services::history::{self, HistoryKind};
use crate::services::refresh;
use crate::services::request::{self, Load};
use crate::services::time;
use crate::state::AppState;
use crate::views::PanelKind;

/// Graph answer families, one per operation tab.
#[derive(Debug)]
enum GraphAnswer {
    Subgraph(GraphSubgraphResponse),
    Path(GraphPathResponse),
    Components(GraphComponentsResponse),
    Cycles(GraphCyclesResponse),
    Impact(GraphImpactResponse),
    Structural(GraphStructuralResponse),
    Module(GraphModuleResponse),
}

impl GraphAnswer {
    /// Epoch the answer was served from, recorded so a later rebase can be
    /// detected instead of silently presenting stale topology.
    fn relation_epoch(&self) -> Option<i64> {
        match self {
            Self::Subgraph(response) => Some(response.relation_epoch),
            Self::Path(response) => Some(response.relation_epoch),
            Self::Components(response) => Some(response.relation_epoch),
            Self::Cycles(response) => Some(response.relation_epoch),
            Self::Impact(response) => Some(response.relation_epoch),
            Self::Structural(response) => Some(response.relation_epoch),
            Self::Module(response) => Some(response.relation_epoch),
        }
    }
}

/// Operation tabs, in display order.
const TABS: [&str; 9] = [
    "graph.tab.ego",
    "graph.tab.path",
    "graph.tab.subgraph",
    "graph.tab.components",
    "graph.tab.cycles",
    "graph.tab.impact",
    "graph.tab.module",
    "graph.tab.structural",
    "graph.tab.export",
];

const DIRECTION_KEYS: [&str; 3] = ["graph.dir.both", "graph.dir.out", "graph.dir.in"];
const LEVEL_KEYS: [&str; 2] = ["graph.level.entity", "graph.level.file"];
/// Presentation modes: the tables stay the default, the canvas is opt-in.
const PRESENTATION_KEYS: [&str; 2] = ["graph.view.table", "graph.view.canvas"];
const STRUCTURAL_KEYS: [&str; 8] = [
    "graph.kind.trait_bound",
    "graph.kind.child_elements",
    "graph.kind.parent_element",
    "graph.kind.event_handlers",
    "graph.kind.handler_elements",
    "graph.kind.parameter_bindings",
    "graph.kind.template_references",
    "graph.kind.template_ref_owners",
];

pub struct GraphView {
    seed: Entity<InputState>,
    target: Entity<InputState>,
    ids: Entity<InputState>,
    file: Entity<InputState>,
    depth: Entity<InputState>,
    limit: Entity<InputState>,
    tab: usize,
    direction: usize,
    level: usize,
    structural: usize,
    result: Load<GraphAnswer>,
    /// Relation epoch the current answer came from.
    served_epoch: Option<i64>,
    history: Vec<String>,
    /// Relation canvas shared by every answer that carries topology.
    canvas: Entity<GraphCanvas>,
    /// 0 renders tables, 1 renders the canvas.
    presentation: usize,
}

impl GraphView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let locale = crate::i18n::locale(cx);
        let seed = cx
            .new(|cx| InputState::new(window, cx).placeholder(t("graph.seedPlaceholder", locale)));
        let target = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("graph.targetPlaceholder", locale))
        });
        let ids =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("graph.idsPlaceholder", locale)));
        let file = cx
            .new(|cx| InputState::new(window, cx).placeholder(t("graph.filePlaceholder", locale)));
        let depth = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t("graph.depthPlaceholder", locale))
                .default_value("2")
        });
        let limit = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t("graph.limitPlaceholder", locale))
                .default_value("200")
        });
        let history = history::entries(HistoryKind::Graph, cx);
        let canvas = cx.new(GraphCanvas::new);
        // A double click on a node re-roots the neighbourhood: the canvas only
        // reports the intent, the panel owns the request.
        cx.subscribe_in(
            &canvas,
            window,
            |this, _, event: &CanvasEvent, window, cx| match event {
                CanvasEvent::Expand { label } => this.expand(label.clone(), window, cx),
            },
        )
        .detach();
        Self {
            seed,
            target,
            ids,
            file,
            depth,
            limit,
            tab: 0,
            direction: 0,
            level: 0,
            structural: 0,
            result: Load::idle(),
            served_epoch: None,
            history,
            canvas,
            presentation: 0,
        }
    }

    /// Re-roots the ego query on one node label.
    fn expand(&mut self, label: String, window: &mut Window, cx: &mut Context<Self>) {
        self.tab = 0;
        self.presentation = 1;
        self.seed
            .update(cx, |state, cx| state.set_value(label, window, cx));
        self.run(cx);
    }

    /// Apply a link delivered by another panel.
    ///
    /// An entity seed fills the ego form and runs it; a file link fills the
    /// file field and runs the impact tab, which is the question a file link
    /// asks in practice.
    pub fn on_navigate(&mut self, target: &Target, window: &mut Window, cx: &mut Context<Self>) {
        match target {
            Target::GraphSeed { entity_id, .. } => {
                self.tab = 0;
                let entity_id = entity_id.to_string();
                self.seed
                    .update(cx, |state, cx| state.set_value(entity_id, window, cx));
            }
            Target::GraphFile { file } => {
                self.tab = 5;
                let file = file.to_string();
                self.file
                    .update(cx, |state, cx| state.set_value(file, window, cx));
            }
            _ => return,
        }
        self.run(cx);
    }

    /// Re-run the current inputs on the next refresh tick.
    pub fn replay(&mut self, cx: &mut Context<Self>) {
        if matches!(self.result, Load::Idle) {
            return;
        }
        refresh::touch(PanelKind::Graph, cx);
        self.run(cx);
    }

    fn number(&self, input: &Entity<InputState>, fallback: usize, cx: &Context<Self>) -> usize {
        input
            .read(cx)
            .value()
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|value| *value > 0)
            .unwrap_or(fallback)
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.result = Load::Failed(request::Failure::disconnected());
            cx.notify();
            return;
        };
        let Some(project_id) = AppState::global(cx).read(cx).active_project_id() else {
            self.result = Load::Failed(request::Failure {
                key: "error.noProject",
                detail: SharedString::default(),
                candidates: Vec::new(),
            });
            cx.notify();
            return;
        };

        let seed = self.seed.read(cx).value().trim().to_string();
        let target = self.target.read(cx).value().trim().to_string();
        let ids = crate::services::split_list(&self.ids.read(cx).value());
        let file = self.file.read(cx).value().trim().to_string();
        let depth = self.number(&self.depth, 2, cx);
        let limit = self.number(&self.limit, 200, cx);
        let direction = EGO_DIRECTIONS[self.direction.min(EGO_DIRECTIONS.len() - 1)].to_string();
        let level = CYCLE_LEVELS[self.level.min(CYCLE_LEVELS.len() - 1)].to_string();
        let structural = STRUCTURAL_KINDS[self.structural.min(STRUCTURAL_KINDS.len() - 1)];
        let tab = self.tab;

        if matches!(tab, 0 | 1 | 5 | 7) && seed.is_empty() {
            self.result = Load::Failed(request::Failure {
                key: "graph.seedRequired",
                detail: SharedString::default(),
                candidates: Vec::new(),
            });
            cx.notify();
            return;
        }
        if tab == 1 && target.is_empty() {
            self.result = Load::Failed(request::Failure {
                key: "graph.targetRequired",
                detail: SharedString::default(),
                candidates: Vec::new(),
            });
            cx.notify();
            return;
        }
        if tab == 2 && ids.is_empty() {
            self.result = Load::Failed(request::Failure {
                key: "graph.idsRequired",
                detail: SharedString::default(),
                candidates: Vec::new(),
            });
            cx.notify();
            return;
        }
        if matches!(tab, 5 | 6) && file.is_empty() {
            self.result = Load::Failed(request::Failure {
                key: "graph.fileRequired",
                detail: SharedString::default(),
                candidates: Vec::new(),
            });
            cx.notify();
            return;
        }

        match tab {
            0 => history::push(HistoryKind::Graph, &seed, cx),
            2 => history::push(HistoryKind::Graph, &ids.join(","), cx),
            5 | 6 => history::push(HistoryKind::Graph, &file, cx),
            _ => history::push(HistoryKind::Graph, &seed, cx),
        }
        self.history = history::entries(HistoryKind::Graph, cx);

        let task = match tab {
            0 => {
                let task =
                    graph_service::ego(cx, client, project_id, seed, depth, direction, limit);
                Erased::Subgraph(task)
            }
            1 => {
                let task = graph_service::path(cx, client, project_id, seed, target, depth);
                Erased::Path(task)
            }
            2 => Erased::Subgraph(graph_service::subgraph(cx, client, project_id, ids, limit)),
            3 => Erased::Components(graph_service::components(cx, client, project_id, limit)),
            4 => Erased::Cycles(graph_service::cycles(cx, client, project_id, level, limit)),
            5 => Erased::Impact(graph_service::impact(cx, client, project_id, file)),
            6 => Erased::Module(graph_service::module(cx, client, project_id, file)),
            7 => Erased::Structural(graph_service::structural(
                cx,
                client,
                project_id,
                seed,
                structural.to_string(),
            )),
            _ => Erased::Subgraph(graph_service::export(cx, client, project_id, limit)),
        };
        self.result.begin();
        cx.notify();

        match task {
            Erased::Subgraph(task) => self.await_into(cx, task, GraphAnswer::Subgraph),
            Erased::Path(task) => self.await_into(cx, task, GraphAnswer::Path),
            Erased::Components(task) => self.await_into(cx, task, GraphAnswer::Components),
            Erased::Cycles(task) => self.await_into(cx, task, GraphAnswer::Cycles),
            Erased::Impact(task) => self.await_into(cx, task, GraphAnswer::Impact),
            Erased::Module(task) => self.await_into(cx, task, GraphAnswer::Module),
            Erased::Structural(task) => self.await_into(cx, task, GraphAnswer::Structural),
        }
    }

    fn await_into<T: 'static>(
        &mut self,
        cx: &mut Context<Self>,
        task: gpui_kit::Task<Result<T, cce_client::ClientError>>,
        wrap: impl FnOnce(T) -> GraphAnswer + 'static,
    ) {
        request::spawn_into(cx, task, move |this, outcome, cx| {
            match outcome {
                Ok(value) => {
                    let answer = wrap(value);
                    this.served_epoch = answer.relation_epoch();
                    let epoch = this.served_epoch;
                    AppState::global(cx).update(cx, |state, cx| state.note_epoch(epoch, cx));
                    this.sync_canvas(&answer, cx);
                    this.result = Load::Ready(answer);
                }
                Err(error) => {
                    this.served_epoch = None;
                    this.result = Load::Failed(request::Failure::from_error(&error));
                }
            }
            cx.notify();
        });
    }

    /// Hands the topology of one answer to the canvas.
    ///
    /// Only the two answers that carry nodes and edges feed the canvas; the
    /// other families keep whatever the last drawn topology was, so switching
    /// tabs does not blank the picture the user is reading.
    fn sync_canvas(&mut self, answer: &GraphAnswer, cx: &mut Context<Self>) {
        let topology = match answer {
            GraphAnswer::Subgraph(response) => {
                Topology::from_answer(&response.nodes, &response.edges)
            }
            GraphAnswer::Path(response) if response.path_found => {
                Topology::from_answer(&response.nodes, &response.edges)
            }
            _ => return,
        };
        let limit = self.number(&self.limit, DEFAULT_MAX_NODES, cx);
        let (topology, dropped) = topology.capped(limit);
        self.canvas
            .update(cx, |canvas, cx| canvas.set_topology(topology, dropped, cx));
    }

    /// Write the whole graph as a PNG into the download directory.
    ///
    /// Export is deliberately a plain file write: no new dependency and no
    /// window-level dialog, and the resulting path is reported in a toast so
    /// the caller can find it.
    fn export_png(&self, cx: &mut Context<Self>) {
        let Some(bytes) = self.canvas.read(cx).export_png() else {
            toast::push(t("graph.canvas.exportEmpty", locale(cx)), false, cx);
            return;
        };
        let name = format!("cce-graph-{}.png", time::file_stamp(time::unix_seconds()));
        let mut path = export_service::export_dir().join(name);
        let mut sequence = 2;
        while path.exists() {
            path = export_service::export_dir().join(format!(
                "cce-graph-{}-{}.png",
                time::file_stamp(time::unix_seconds()),
                sequence
            ));
            sequence += 1;
        }
        match std::fs::write(&path, bytes) {
            Ok(()) => toast::push(path.display().to_string(), true, cx),
            Err(_) => toast::push(t("graph.canvas.exportFailed", locale(cx)), false, cx),
        }
    }

    /// True when the answer has topology the canvas can draw.
    fn has_topology(answer: &GraphAnswer) -> bool {
        match answer {
            GraphAnswer::Subgraph(response) => !response.nodes.is_empty(),
            GraphAnswer::Path(response) => response.path_found && !response.nodes.is_empty(),
            _ => false,
        }
    }

    /// Epoch line under the form.
    ///
    /// While the answer is still current this is the static explanation the
    /// panel always carried. Once the server's relation epoch moved past the
    /// one this answer was served from, it becomes a warning plus an explicit
    /// replay button instead of an automatic reload.
    fn render_epoch_row(&self, locale: crate::settings::Locale, cx: &mut Context<Self>) -> Div {
        let current = AppState::global(cx).read(cx).epoch();
        match stale_epoch(self.served_epoch, current, locale, cx) {
            Some(banner) => h_flex()
                .gap_2()
                .items_center()
                .child(banner.flex_1())
                .child(
                    Button::new("graph-replay")
                        .small()
                        .label(t("common.replay", locale))
                        .on_click(cx.listener(|this, _, _, cx| this.replay(cx))),
                ),
            None => div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(SharedString::from(t("graph.epochNote", locale))),
        }
    }

    fn render_answer(&self, answer: &GraphAnswer, cx: &mut Context<Self>) -> AnyElement {
        let locale = locale(cx);
        match answer {
            GraphAnswer::Subgraph(response) => {
                self.render_subgraph(&response.nodes, &response.edges, response.truncated, cx)
            }
            GraphAnswer::Path(response) => {
                if !response.path_found {
                    return empty_state(
                        t("graph.noPath", locale),
                        Some(SharedString::from(format!(
                            "epoch {}",
                            response.relation_epoch
                        ))),
                        None,
                        cx,
                    )
                    .into_any_element();
                }
                self.render_subgraph(&response.nodes, &response.edges, false, cx)
            }
            GraphAnswer::Components(response) => {
                let columns = [
                    Column::fixed("#", 50.0),
                    Column::grow(t("graph.colMembers", locale)),
                ];
                let rows: Vec<Vec<Cell>> = response
                    .components
                    .iter()
                    .enumerate()
                    .map(|(index, members)| {
                        vec![
                            Cell::muted(SharedString::from((index + 1).to_string())),
                            Cell::new(SharedString::from(members.join(", "))),
                        ]
                    })
                    .collect();
                self.grid_block(
                    &columns,
                    &rows,
                    Some(format!("{} components", response.total_components)),
                    cx,
                )
            }
            GraphAnswer::Cycles(response) => {
                let columns = [
                    Column::fixed("#", 50.0),
                    Column::grow(t("graph.colMembers", locale)),
                ];
                let rows: Vec<Vec<Cell>> = response
                    .cycles
                    .iter()
                    .enumerate()
                    .map(|(index, cycle)| {
                        vec![
                            Cell::muted(SharedString::from((index + 1).to_string())),
                            Cell::new(SharedString::from(cycle.members.join(" → "))),
                        ]
                    })
                    .collect();
                self.grid_block(
                    &columns,
                    &rows,
                    Some(format!(
                        "{} · {} · truncated={}",
                        response.total_cycles, response.level, response.truncated
                    )),
                    cx,
                )
            }
            GraphAnswer::Impact(response) => {
                let columns = [
                    Column::grow(t("graph.colDependent", locale)),
                    Column::fixed(t("graph.colHop", locale), 80.0),
                ];
                let mut rows: Vec<Vec<Cell>> = Vec::new();
                for dependent in &response.direct_dependents {
                    rows.push(vec![
                        Cell::new(SharedString::from(dependent.clone())),
                        Cell::toned(SharedString::from("1"), Tone::Warning),
                    ]);
                }
                for dependent in &response.indirect_dependents {
                    rows.push(vec![
                        Cell::new(SharedString::from(dependent.clone())),
                        Cell::muted(SharedString::from("2+")),
                    ]);
                }
                self.grid_block(
                    &columns,
                    &rows,
                    Some(format!(
                        "{} · impact {:.2}",
                        response.changed_file, response.impact_score
                    )),
                    cx,
                )
            }
            GraphAnswer::Module(response) => {
                let columns = [
                    Column::fixed(t("graph.colRelation", locale), 220.0),
                    Column::grow(t("graph.colTarget", locale)),
                    Column::fixed(t("graph.colDomain", locale), 110.0),
                ];
                let rows: Vec<Vec<Cell>> = response
                    .imports
                    .iter()
                    .map(|relation| {
                        vec![
                            Cell::new(SharedString::from(relation.relation.clone())),
                            Cell::new(SharedString::from(relation.target.clone())),
                            Cell::muted(SharedString::from(relation.domain.clone())),
                        ]
                    })
                    .collect();
                self.grid_block(
                    &columns,
                    &rows,
                    Some(format!(
                        "{} · {} exports · {} caller files",
                        response.file,
                        response.exports.len(),
                        response.caller_files.len()
                    )),
                    cx,
                )
            }
            GraphAnswer::Structural(response) => {
                let columns = [
                    Column::grow(t("graph.colLabel", locale)),
                    Column::fixed(t("graph.colRelation", locale), 220.0),
                    Column::grow(t("graph.colFile", locale)),
                ];
                let rows: Vec<Vec<Cell>> = response
                    .relations
                    .iter()
                    .map(|relation| {
                        vec![
                            Cell::new(SharedString::from(relation.label.clone())),
                            Cell::new(SharedString::from(relation.relation.clone())),
                            Cell::muted(SharedString::from(relation.source_file.clone())),
                        ]
                    })
                    .collect();
                self.grid_block(
                    &columns,
                    &rows,
                    Some(format!(
                        "{} · {} · {} relations",
                        response.kind, response.direction, response.total_relations
                    )),
                    cx,
                )
            }
        }
    }

    fn grid_block(
        &self,
        columns: &[Column],
        rows: &[Vec<Cell>],
        summary: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dense = AppState::global(cx).read(cx).density.is_dense();
        let locale = locale(cx);
        if rows.is_empty() {
            return empty_state(
                t("graph.noRows", locale),
                summary.map(SharedString::from),
                None,
                cx,
            )
            .into_any_element();
        }
        let mut block = surface(dense, cx);
        if let Some(summary) = summary {
            block = block.child(hint(SharedString::from(summary), cx));
        }
        block
            .child(data_grid(PanelKind::Graph, columns, rows, dense, &mut *cx))
            .into_any_element()
    }

    fn render_subgraph(
        &self,
        nodes: &[cce_api::models::GraphNode],
        edges: &[cce_api::models::GraphEdge],
        truncated: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locale = locale(cx);
        let dense = AppState::global(cx).read(cx).density.is_dense();
        let node_columns = [
            Column::grow(t("graph.colLabel", locale)),
            Column::fixed(t("graph.colKind", locale), 100.0),
            Column::grow(t("graph.colFile", locale)),
            Column::fixed(t("graph.colLocation", locale), 100.0),
        ];
        let node_rows: Vec<Vec<Cell>> = nodes
            .iter()
            .map(|node| {
                vec![
                    Cell::new(SharedString::from(node.label.clone())),
                    Cell::toned(SharedString::from(node.kind.clone()), Tone::Info),
                    Cell::muted(SharedString::from(node.source_file.clone())),
                    Cell::muted(SharedString::from(node.source_location.clone())),
                ]
            })
            .collect();

        let edge_columns = [
            Column::grow(t("graph.colSource", locale)),
            Column::fixed(t("graph.colRelation", locale), 180.0),
            Column::grow(t("graph.colTarget", locale)),
            Column::fixed(t("graph.colConfidence", locale), 100.0),
        ];
        let edge_rows: Vec<Vec<Cell>> = edges
            .iter()
            .map(|edge| {
                vec![
                    Cell::new(SharedString::from(edge.source.clone())),
                    Cell::new(SharedString::from(edge.relation.clone())),
                    Cell::new(SharedString::from(edge.target.clone())),
                    Cell::muted(SharedString::from(edge.confidence.clone())),
                ]
            })
            .collect();

        let summary = format!(
            "{} nodes · {} edges{}",
            nodes.len(),
            edges.len(),
            if truncated { " · truncated" } else { "" }
        );
        v_flex()
            .gap_3()
            .w_full()
            .child(hint(SharedString::from(summary), cx))
            .child(surface(dense, cx).child(data_grid(
                PanelKind::Graph,
                &node_columns,
                &node_rows,
                dense,
                &mut *cx,
            )))
            .child(surface(dense, cx).child(data_grid(
                PanelKind::Graph,
                &edge_columns,
                &edge_rows,
                dense,
                &mut *cx,
            )))
            .into_any_element()
    }

    fn render_secondary_inputs(&self, cx: &mut Context<Self>) -> AnyElement {
        let locale = locale(cx);
        match self.tab {
            1 => field(t("graph.target", locale), Input::new(&self.target), cx).into_any_element(),
            2 => field(t("graph.ids", locale), Input::new(&self.ids), cx).into_any_element(),
            5 | 6 => field(t("graph.file", locale), Input::new(&self.file), cx).into_any_element(),
            4 => h_flex()
                .gap_3()
                .items_center()
                .child(hint(t("graph.level", locale), cx))
                .child(choice_row(
                    "graph-level",
                    &LEVEL_KEYS,
                    self.level,
                    |this: &mut Self, index| this.level = index,
                    cx,
                ))
                .into_any_element(),
            7 => h_flex()
                .gap_3()
                .items_center()
                .child(hint(t("graph.structuralKind", locale), cx))
                .child(choice_row(
                    "graph-structural",
                    &STRUCTURAL_KEYS,
                    self.structural,
                    |this: &mut Self, index| this.structural = index,
                    cx,
                ))
                .into_any_element(),
            _ => div().into_any_element(),
        }
    }

    /// Presentation switch plus the canvas controls that only apply to it.
    fn render_presentation_row(&self, cx: &mut Context<Self>) -> Div {
        let locale = locale(cx);
        let mut row = h_flex()
            .gap_3()
            .items_center()
            .flex_wrap()
            .child(choice_row(
                "graph-presentation",
                &PRESENTATION_KEYS,
                self.presentation,
                |this: &mut Self, index| this.presentation = index,
                cx,
            ));
        if self.presentation == 1 {
            row = row
                .child(self.render_layout_buttons(cx))
                .child(
                    Button::new("graph-canvas-zoom-in")
                        .small()
                        .ghost()
                        .label(t("graph.canvas.zoomIn", locale))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.canvas.update(cx, |canvas, cx| canvas.zoom_in(cx));
                        })),
                )
                .child(
                    Button::new("graph-canvas-zoom-out")
                        .small()
                        .ghost()
                        .label(t("graph.canvas.zoomOut", locale))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.canvas.update(cx, |canvas, cx| canvas.zoom_out(cx));
                        })),
                )
                .child(
                    Button::new("graph-canvas-fit")
                        .small()
                        .ghost()
                        .label(t("graph.canvas.fit", locale))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.canvas.update(cx, |canvas, cx| {
                                canvas.fit();
                                cx.notify();
                            });
                        })),
                )
                .child(
                    Button::new("graph-canvas-export")
                        .small()
                        .ghost()
                        .label(t("graph.canvas.export", locale))
                        .on_click(cx.listener(|this, _, _, cx| this.export_png(cx))),
                );
        }
        row
    }

    /// Layout family buttons for the canvas.
    ///
    /// Built by hand instead of with `choice_row` because switching a layout
    /// has to reach the canvas entity, and that row's setter only sees the
    /// panel.
    fn render_layout_buttons(&self, cx: &mut Context<Self>) -> Div {
        let locale = locale(cx);
        let current = self.canvas.read(cx).layout_kind();
        let mut row = h_flex().gap_1().items_center();
        for (index, kind) in LayoutKind::ALL.into_iter().enumerate() {
            let active = kind == current;
            let mut button = Button::new(format!("graph-layout-{index}"))
                .small()
                .label(t(kind.key(), locale))
                .selected(active)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.canvas
                        .update(cx, |canvas, cx| canvas.set_layout(kind, cx));
                }));
            button = if active {
                button.primary()
            } else {
                button.ghost()
            };
            row = row.child(button);
        }
        row
    }

    /// Canvas block: the picture, the gesture hint and the node inspector.
    fn render_canvas_panel(&self, answer: &GraphAnswer, cx: &mut Context<Self>) -> AnyElement {
        let locale = locale(cx);
        if !Self::has_topology(answer) {
            return empty_state(
                t("graph.canvas.noTopology", locale),
                Some(SharedString::from(t("graph.canvas.noTopologyHint", locale))),
                None,
                cx,
            )
            .into_any_element();
        }
        let dense = AppState::global(cx).read(cx).density.is_dense();
        let theme = cx.theme();
        surface(dense, cx)
            .child(
                div()
                    .h(px(CANVAS_HEIGHT))
                    .w_full()
                    .overflow_hidden()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(theme.border)
                    .child(self.canvas.clone()),
            )
            .child(hint(
                SharedString::from(t("graph.canvas.gestures", locale)),
                cx,
            ))
            .child({
                let dropped = self.canvas.read(cx).dropped_nodes();
                if dropped == 0 {
                    div().into_any_element()
                } else {
                    hint(
                        SharedString::from(format!(
                            "{}: {}",
                            t("graph.canvas.dropped", locale),
                            dropped
                        )),
                        cx,
                    )
                    .into_any_element()
                }
            })
            .child(self.render_inspector(cx))
            .into_any_element()
    }

    /// Selected node details, with the two links a node selection implies.
    fn render_inspector(&self, cx: &mut Context<Self>) -> Div {
        let locale = locale(cx);
        let (selected, outgoing, incoming) = {
            let canvas = self.canvas.read(cx);
            let (out, inc) = canvas.selected_neighbours();
            (
                canvas.selected_node().cloned(),
                out.into_iter()
                    .map(|node| node.label.clone())
                    .collect::<Vec<_>>(),
                inc.into_iter()
                    .map(|node| node.label.clone())
                    .collect::<Vec<_>>(),
            )
        };
        let Some(node) = selected else {
            return hint(SharedString::from(t("graph.canvas.selectHint", locale)), cx);
        };
        let location = format!("{}:{}", node.file, node.location);
        let outgoing_line = format!(
            "{} {} · {}",
            t("graph.canvas.out", locale),
            outgoing.len(),
            join_labels(&outgoing)
        );
        let incoming_line = format!(
            "{} {} · {}",
            t("graph.canvas.in", locale),
            incoming.len(),
            join_labels(&incoming)
        );
        let title = node.label.clone();

        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .flex_wrap()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(SharedString::from(title.clone())),
                    )
                    .child(hint(SharedString::from(node.kind.clone()), cx))
                    .child(hint(SharedString::from(location), cx))
                    .child(
                        Button::new("graph-canvas-entity")
                            .small()
                            .ghost()
                            .label(t("graph.canvas.openEntity", locale))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let Some(node) = this.canvas.read(cx).selected_node().cloned()
                                else {
                                    return;
                                };
                                AppState::global(cx).update(cx, |state, cx| {
                                    state.navigate(
                                        Target::Entity {
                                            id: SharedString::from(node.id),
                                        },
                                        cx,
                                    )
                                });
                            })),
                    )
                    .child(
                        Button::new("graph-canvas-expand")
                            .small()
                            .ghost()
                            .label(t("graph.canvas.expand", locale))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.expand(title.clone(), window, cx);
                            })),
                    ),
            )
            .child(hint(SharedString::from(outgoing_line), cx))
            .child(hint(SharedString::from(incoming_line), cx))
    }
}

/// Height reserved for the canvas inside the panel scroll area.
const CANVAS_HEIGHT: f32 = 520.0;

/// Joins neighbour labels for the inspector, keeping the line readable.
fn join_labels(labels: &[String]) -> String {
    if labels.is_empty() {
        return "—".to_string();
    }
    let mut text = labels
        .iter()
        .take(8)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if labels.len() > 8 {
        text.push_str(" …");
    }
    text
}

/// One in-flight graph request, kept typed until it is wrapped into an answer.
enum Erased {
    Subgraph(gpui_kit::Task<Result<GraphSubgraphResponse, cce_client::ClientError>>),
    Path(gpui_kit::Task<Result<GraphPathResponse, cce_client::ClientError>>),
    Components(gpui_kit::Task<Result<GraphComponentsResponse, cce_client::ClientError>>),
    Cycles(gpui_kit::Task<Result<GraphCyclesResponse, cce_client::ClientError>>),
    Impact(gpui_kit::Task<Result<GraphImpactResponse, cce_client::ClientError>>),
    Module(gpui_kit::Task<Result<GraphModuleResponse, cce_client::ClientError>>),
    Structural(gpui_kit::Task<Result<GraphStructuralResponse, cce_client::ClientError>>),
}

impl Render for GraphView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = locale(cx);

        let tabs = choice_row(
            "graph-tab",
            &TABS,
            self.tab,
            |this: &mut Self, index| {
                this.tab = index;
                this.result = Load::idle();
            },
            cx,
        );

        let direction = h_flex()
            .gap_3()
            .items_center()
            .child(hint(t("graph.direction", locale), cx))
            .child(choice_row(
                "graph-direction",
                &DIRECTION_KEYS,
                self.direction,
                |this: &mut Self, index| this.direction = index,
                cx,
            ));

        let form = field_group(cx)
            .child(tabs)
            .child(field(t("graph.seed", locale), Input::new(&self.seed), cx))
            .child(self.render_secondary_inputs(cx))
            .child(
                h_flex()
                    .gap_3()
                    .items_end()
                    .child(field(t("graph.depth", locale), Input::new(&self.depth), cx))
                    .child(field(t("graph.limit", locale), Input::new(&self.limit), cx))
                    .child(direction)
                    .child(
                        Button::new("graph-run")
                            .small()
                            .primary()
                            .label(t("common.run", locale))
                            .loading(self.result.is_loading())
                            .on_click(cx.listener(|this, _, _, cx| this.run(cx))),
                    ),
            );

        let history_row = if self.history.is_empty() {
            None
        } else {
            let mut row = h_flex().gap_1().flex_wrap();
            for entry in self.history.iter().take(6) {
                let value = entry.clone();
                row = row.child(
                    Button::new(format!("graph-history-{entry}"))
                        .small()
                        .ghost()
                        .label(SharedString::from(entry.clone()))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.seed
                                .update(cx, |state, cx| state.set_value(value.clone(), window, cx));
                            cx.notify();
                        })),
                );
            }
            Some(
                v_flex()
                    .gap_1()
                    .child(hint(t("common.recent", locale), cx))
                    .child(row),
            )
        };

        let answer = match &self.result {
            Load::Idle => empty_state(
                t("graph.emptyTitle", locale),
                Some(SharedString::from(t("graph.emptyHint", locale))),
                None,
                cx,
            )
            .into_any_element(),
            Load::Loading => loading_row(t("graph.loading", locale), cx).into_any_element(),
            Load::Failed(failure) => failure_block(failure, locale, cx).into_any_element(),
            Load::Ready(answer) if self.presentation == 1 => self.render_canvas_panel(answer, cx),
            Load::Ready(answer) => self.render_answer(answer, cx),
        };

        page_body("graph")
            .child(panel_header(
                t("graph.title", locale),
                Some(SharedString::from(t("graph.subtitle", locale))),
                None,
                cx,
            ))
            .child(
                stack()
                    .child(form)
                    .children(history_row)
                    .child(self.render_epoch_row(locale, cx))
                    .child(self.render_presentation_row(cx))
                    .child(answer),
            )
    }
}
