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
    ActiveTheme as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    v_flex,
};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Window, div,
};

use crate::components::{
    Cell, Column, Tone, choice_row, data_grid, empty_state, failure_block, field, field_group,
    hint, loading_row, page_body, panel_header, stack, surface,
};
use crate::connection::ClientStore;
use crate::i18n::{locale, t};
use crate::services::graph_service::{self, CYCLE_LEVELS, EGO_DIRECTIONS, STRUCTURAL_KINDS};
use crate::services::history::{self, HistoryKind};
use crate::services::request::{self, Load};
use crate::state::{AppState, GraphSeed};

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
    /// Highest seed delivery already applied.
    applied_seed: u64,
    history: Vec<String>,
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
            applied_seed: 0,
            history,
        }
    }

    /// Apply a seed delivered by the search panel.
    ///
    /// Returns `true` when the seed was new, so the window can switch to this
    /// panel only for a delivery it has not shown yet.
    pub fn apply_seed(
        &mut self,
        seed: &GraphSeed,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if seed.generation <= self.applied_seed {
            return false;
        }
        self.applied_seed = seed.generation;
        self.tab = 0;
        let entity_id = seed.entity_id.to_string();
        self.seed
            .update(cx, |state, cx| state.set_value(entity_id, window, cx));
        self.run(cx);
        true
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
            });
            cx.notify();
            return;
        }
        if tab == 1 && target.is_empty() {
            self.result = Load::Failed(request::Failure {
                key: "graph.targetRequired",
                detail: SharedString::default(),
            });
            cx.notify();
            return;
        }
        if tab == 2 && ids.is_empty() {
            self.result = Load::Failed(request::Failure {
                key: "graph.idsRequired",
                detail: SharedString::default(),
            });
            cx.notify();
            return;
        }
        if matches!(tab, 5 | 6) && file.is_empty() {
            self.result = Load::Failed(request::Failure {
                key: "graph.fileRequired",
                detail: SharedString::default(),
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
            this.result = match outcome {
                Ok(value) => Load::Ready(wrap(value)),
                Err(error) => Load::Failed(request::Failure::from_error(&error)),
            };
            cx.notify();
        });
    }

    fn render_answer(&self, answer: &GraphAnswer, cx: &Context<Self>) -> AnyElement {
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
        cx: &Context<Self>,
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
            .child(data_grid(columns, rows, dense, cx))
            .into_any_element()
    }

    fn render_subgraph(
        &self,
        nodes: &[cce_api::models::GraphNode],
        edges: &[cce_api::models::GraphEdge],
        truncated: bool,
        cx: &Context<Self>,
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
            .child(surface(dense, cx).child(data_grid(&node_columns, &node_rows, dense, cx)))
            .child(surface(dense, cx).child(data_grid(&edge_columns, &edge_rows, dense, cx)))
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
        let theme = cx.theme().colors;

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
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(SharedString::from(t("graph.epochNote", locale))),
                    )
                    .child(answer),
            )
    }
}
