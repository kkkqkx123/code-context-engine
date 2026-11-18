//! Entity panel: detail, call relations, call chains, call paths, class
//! relations and classification statistics for one stable symbol id.
//!
//! A name lookup runs the entity index first, so a symbol can be reached
//! without knowing its stable id in advance.

use cce_api::models::{
    CallChainResponse, CallPathResponse, ClassImplementationsResponse, ClassInheritanceResponse,
    ClassificationRelation, ClassificationStatsResponse, EntitySearchResponse,
    FunctionCallersResponse, FunctionCallsResponse, FunctionDetailResponse,
};
use gpui_kit::component::{
    ActiveTheme as _, Selectable as _, Sizable as _,
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
    hint, loading_row, page_body, panel_header, stack, stale_epoch, surface,
};
use crate::connection::ClientStore;
use crate::i18n::{locale, t};
use crate::navigator::Target;
use crate::services::entity_service::{self, CHAIN_DIRECTIONS, ChainFilter};
use crate::services::history::{self, HistoryKind};
use crate::services::refresh;
use crate::services::request::{self, Load};
use crate::services::search_service;
use crate::state::AppState;
use crate::views::PanelKind;

const ENTITY_TABS: [&str; 8] = [
    "entity.tab.detail",
    "entity.tab.calls",
    "entity.tab.callers",
    "entity.tab.chain",
    "entity.tab.path",
    "entity.tab.class",
    "entity.tab.implementations",
    "entity.tab.classification",
];

const DIRECTION_KEYS: [&str; 2] = ["entity.dir.down", "entity.dir.up"];

#[derive(Debug)]
enum EntityAnswer {
    Detail(FunctionDetailResponse),
    Calls(FunctionCallsResponse),
    Callers(FunctionCallersResponse),
    Chain(CallChainResponse),
    Path(CallPathResponse),
    Inheritance(ClassInheritanceResponse),
    Implementations(ClassImplementationsResponse),
    Classification(ClassificationStatsResponse),
}

impl EntityAnswer {
    /// Epoch the answer was served from, when the payload carries one.
    ///
    /// The detail and classification answers have no epoch on the wire, so
    /// they never raise a staleness warning instead of guessing.
    fn relation_epoch(&self) -> Option<i64> {
        match self {
            Self::Detail(_) | Self::Classification(_) => None,
            Self::Calls(response) => Some(response.relation_epoch),
            Self::Callers(response) => Some(response.relation_epoch),
            Self::Chain(response) => Some(response.relation_epoch),
            Self::Path(response) => Some(response.relation_epoch),
            Self::Inheritance(response) => Some(response.relation_epoch),
            Self::Implementations(response) => Some(response.relation_epoch),
        }
    }
}

pub struct EntityView {
    symbol: Entity<InputState>,
    target: Entity<InputState>,
    lookup: Entity<InputState>,
    depth: Entity<InputState>,
    limit: Entity<InputState>,
    /// Directory prefix limiting the relation queries.
    prefix: Entity<InputState>,
    /// Coarse domains kept in the relation queries, comma separated.
    domains: Entity<InputState>,
    tab: usize,
    direction: usize,
    exclude_tests: bool,
    include_external: bool,
    /// Classification category used by the detail query.
    category: Entity<InputState>,
    /// Typed detail rows of the selected classification.
    detail: Load<Vec<ClassificationRelation>>,
    /// Relation epoch the current answer came from.
    served_epoch: Option<i64>,
    answer: Load<EntityAnswer>,
    matches: Load<EntitySearchResponse>,
    history: Vec<String>,
}

impl EntityView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let locale = crate::i18n::locale(cx);
        let symbol = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("entity.symbolPlaceholder", locale))
        });
        let target = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("entity.targetPlaceholder", locale))
        });
        let lookup = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("entity.lookupPlaceholder", locale))
        });
        let depth = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t("entity.depthPlaceholder", locale))
                .default_value("3")
        });
        let limit = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t("entity.limitPlaceholder", locale))
                .default_value("20")
        });
        let prefix = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("entity.prefixPlaceholder", locale))
        });
        let domains = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("entity.domainsPlaceholder", locale))
        });
        let category = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t("entity.categoryPlaceholder", locale))
                .default_value("stdlib")
        });
        let history = history::entries(HistoryKind::Entity, cx);
        Self {
            symbol,
            target,
            lookup,
            depth,
            limit,
            prefix,
            domains,
            category,
            detail: Load::idle(),
            tab: 0,
            direction: 0,
            exclude_tests: false,
            include_external: true,
            served_epoch: None,
            answer: Load::idle(),
            matches: Load::idle(),
            history,
        }
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

    fn lookup_symbol(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.matches = Load::Failed(request::Failure::disconnected());
            cx.notify();
            return;
        };
        let Some(project_id) = AppState::global(cx).read(cx).active_project_id() else {
            self.matches = Load::Failed(request::Failure {
                key: "error.noProject",
                detail: SharedString::default(),
                candidates: Vec::new(),
            });
            cx.notify();
            return;
        };
        let query = self.lookup.read(cx).value().trim().to_string();
        if query.is_empty() {
            return;
        }
        self.matches.begin();
        cx.notify();
        let task = search_service::run_entity(cx, client, project_id, query, 20);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.matches = Load::settle(outcome);
            cx.notify();
        });
    }

    /// Relation filters as the wire wants them.
    fn filter(&self, cx: &Context<Self>) -> ChainFilter {
        ChainFilter {
            exclude_tests: self.exclude_tests,
            directory_prefix: crate::services::optional_text(&self.prefix.read(cx).value()),
            domains: crate::services::split_list(&self.domains.read(cx).value()),
            include_external: self.include_external,
        }
    }

    /// Record a finished answer and the epoch it was served from.
    fn finish(
        &mut self,
        outcome: Result<EntityAnswer, cce_client::ClientError>,
        cx: &mut Context<Self>,
    ) {
        match outcome {
            Ok(answer) => {
                self.served_epoch = answer.relation_epoch();
                let epoch = self.served_epoch;
                AppState::global(cx).update(cx, |state, cx| state.note_epoch(epoch, cx));
                self.answer = Load::Ready(answer);
            }
            Err(error) => {
                self.served_epoch = None;
                self.answer = Load::Failed(request::Failure::from_error(&error));
            }
        }
    }

    /// Load a symbol handed over by another panel.
    pub fn on_navigate(&mut self, target: &Target, window: &mut Window, cx: &mut Context<Self>) {
        let Target::Entity { id } = target else {
            return;
        };
        let id = id.to_string();
        self.symbol
            .update(cx, |state, cx| state.set_value(id, window, cx));
        self.tab = 0;
        self.run(cx);
    }

    /// Re-run the current answer on the next refresh tick.
    pub fn replay(&mut self, cx: &mut Context<Self>) {
        if matches!(self.answer, Load::Idle) {
            return;
        }
        refresh::touch(PanelKind::Entities, cx);
        self.run(cx);
    }

    fn use_match(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.symbol
            .update(cx, |state, cx| state.set_value(id, window, cx));
        self.tab = 0;
        self.run(cx);
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.answer = Load::Failed(request::Failure::disconnected());
            cx.notify();
            return;
        };
        let Some(project_id) = AppState::global(cx).read(cx).active_project_id() else {
            self.answer = Load::Failed(request::Failure {
                key: "error.noProject",
                detail: SharedString::default(),
                candidates: Vec::new(),
            });
            cx.notify();
            return;
        };
        let symbol = self.symbol.read(cx).value().trim().to_string();
        let target = crate::services::optional_text(&self.target.read(cx).value());
        if symbol.is_empty() {
            self.answer = Load::Failed(request::Failure {
                key: "entity.symbolRequired",
                detail: SharedString::default(),
                candidates: Vec::new(),
            });
            cx.notify();
            return;
        }
        let depth = self.number(&self.depth, 3, cx);
        let limit = self.number(&self.limit, 20, cx);
        let direction =
            CHAIN_DIRECTIONS[self.direction.min(CHAIN_DIRECTIONS.len() - 1)].to_string();
        let tab = self.tab;

        history::push(HistoryKind::Entity, &symbol, cx);
        self.history = history::entries(HistoryKind::Entity, cx);
        self.answer.begin();
        cx.notify();

        match tab {
            0 => {
                let task = entity_service::detail(cx, client, project_id, symbol);
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.finish(outcome.map(EntityAnswer::Detail), cx);
                    cx.notify();
                });
            }
            1 => {
                let filter = self.filter(cx);
                let task =
                    entity_service::calls(cx, client, project_id, symbol, depth, limit, filter);
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.finish(outcome.map(EntityAnswer::Calls), cx);
                    cx.notify();
                });
            }
            2 => {
                let filter = self.filter(cx);
                let task =
                    entity_service::callers(cx, client, project_id, symbol, depth, limit, filter);
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.finish(outcome.map(EntityAnswer::Callers), cx);
                    cx.notify();
                });
            }
            3 => {
                let task = entity_service::chain(
                    cx,
                    client,
                    project_id,
                    entity_service::ChainRequest {
                        function_id: symbol,
                        direction,
                        max_depth: depth,
                        limit,
                        filter: self.filter(cx),
                    },
                );
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.finish(outcome.map(EntityAnswer::Chain), cx);
                    cx.notify();
                });
            }
            4 => {
                let Some(target_id) = target.filter(|value| !value.is_empty()) else {
                    self.fail("entity.targetRequired", cx);
                    return;
                };
                let task =
                    entity_service::call_path(cx, client, project_id, symbol, target_id, depth);
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.finish(outcome.map(EntityAnswer::Path), cx);
                    cx.notify();
                });
            }
            5 => {
                let task = entity_service::inheritance(cx, client, project_id, symbol.clone());
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.finish(outcome.map(EntityAnswer::Inheritance), cx);
                    cx.notify();
                });
            }
            6 => {
                let task = entity_service::implementations(cx, client, project_id, symbol.clone());
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.finish(outcome.map(EntityAnswer::Implementations), cx);
                    cx.notify();
                });
            }
            _ => {
                let task = entity_service::classification_stats(cx, client, project_id);
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.finish(outcome.map(EntityAnswer::Classification), cx);
                    cx.notify();
                });
            }
        }
    }

    /// Load the typed detail rows for the selected classification.
    ///
    /// The limit is shared with the other relation tabs: one input controls
    /// how much data the panel holds, whichever query produced it.
    fn load_detail(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.detail = Load::Failed(request::Failure::disconnected());
            cx.notify();
            return;
        };
        let Some(project_id) = AppState::global(cx).read(cx).active_project_id() else {
            self.detail = Load::Failed(request::Failure {
                key: "error.noProject",
                detail: SharedString::default(),
                candidates: Vec::new(),
            });
            cx.notify();
            return;
        };
        let classification = self.category.read(cx).value().trim().to_ascii_lowercase();
        if !matches!(
            classification.as_str(),
            "stdlib" | "external" | "dev" | "local" | "unknown"
        ) {
            self.detail = Load::Failed(request::Failure {
                key: "entity.categoryInvalid",
                detail: SharedString::default(),
                candidates: Vec::new(),
            });
            cx.notify();
            return;
        }
        let limit = self.number(&self.limit, 100, cx);
        self.detail.begin();
        cx.notify();
        let task =
            entity_service::classification_relations(cx, client, project_id, classification, limit);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.detail = match outcome {
                Ok(rows) => Load::Ready(rows),
                Err(error) => Load::Failed(request::Failure::from_error(&error)),
            };
            cx.notify();
        });
    }

    fn fail(&mut self, key: &'static str, cx: &mut Context<Self>) {
        self.answer = Load::Failed(request::Failure {
            key,
            detail: SharedString::default(),
            candidates: Vec::new(),
        });
        cx.notify();
    }

    fn render_matches(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let response = self.matches.ready()?;
        if response.items.is_empty() {
            return None;
        }
        let locale = locale(cx);
        let mut list = v_flex().gap_1().w_full();
        for item in response.items.iter().take(10) {
            let id = item.id.to_string();
            list = list.child(
                Button::new(format!("entity-match-{}", item.id))
                    .small()
                    .ghost()
                    .label(SharedString::from(format!(
                        "{} · {} · {}",
                        item.name,
                        item.kind,
                        item.file_path.clone().unwrap_or_default()
                    )))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.use_match(id.clone(), window, cx)
                    })),
            );
        }
        Some(
            v_flex()
                .gap_1()
                .child(hint(t("entity.matches", locale), cx))
                .child(list)
                .into_any_element(),
        )
    }

    fn render_answer(&self, answer: &EntityAnswer, cx: &mut Context<Self>) -> AnyElement {
        let locale = locale(cx);
        let dense = AppState::global(cx).read(cx).density.is_dense();
        match answer {
            EntityAnswer::Detail(response) => {
                let function = &response.function;
                let mut block = surface(dense, cx)
                    .child(
                        div()
                            .text_sm()
                            .child(SharedString::from(function.signature.clone())),
                    )
                    .child(hint(
                        SharedString::from(format!(
                            "{} · {}:{}-{}",
                            function.id, function.file_path, function.start_line, function.end_line
                        )),
                        cx,
                    ));
                if let Some(doc) = &function.doc_comment {
                    block = block.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(doc.clone())),
                    );
                }
                block.into_any_element()
            }
            EntityAnswer::Calls(response) => {
                let columns = call_columns(locale);
                let rows: Vec<Vec<Cell>> = response.callees.iter().map(call_row).collect();
                self.block(
                    &columns,
                    &rows,
                    format!("{} {}", response.total_callees, t("entity.callees", locale)),
                    cx,
                )
            }
            EntityAnswer::Callers(response) => {
                let columns = call_columns(locale);
                let rows: Vec<Vec<Cell>> = response.callers.iter().map(call_row).collect();
                self.block(
                    &columns,
                    &rows,
                    format!("{} {}", response.total_callers, t("entity.callers", locale)),
                    cx,
                )
            }
            EntityAnswer::Chain(response) => {
                let columns = call_columns(locale);
                let rows: Vec<Vec<Cell>> = response.call_chain.iter().map(call_row).collect();
                self.block(
                    &columns,
                    &rows,
                    format!("{} · epoch {}", response.direction, response.relation_epoch),
                    cx,
                )
            }
            EntityAnswer::Path(response) => {
                if !response.path_found {
                    return empty_state(
                        t("entity.noPath", locale),
                        Some(SharedString::from(format!(
                            "epoch {}",
                            response.relation_epoch
                        ))),
                        None,
                        cx,
                    )
                    .into_any_element();
                }
                let columns = call_columns(locale);
                let rows: Vec<Vec<Cell>> = response.path.iter().map(call_row).collect();
                self.block(
                    &columns,
                    &rows,
                    format!("{} {}", response.path_length, t("entity.hops", locale)),
                    cx,
                )
            }
            EntityAnswer::Inheritance(response) => {
                let columns = [
                    Column::grow(t("entity.colClass", locale)),
                    Column::fixed(t("entity.colRelation", locale), 120.0),
                    Column::grow(t("entity.colFile", locale)),
                ];
                let mut rows: Vec<Vec<Cell>> = Vec::new();
                for base in &response.base_classes {
                    rows.push(vec![
                        Cell::new(SharedString::from(base.class_name.clone())),
                        Cell::toned(SharedString::from("base"), Tone::Info),
                        Cell::muted(SharedString::from(base.file_path.clone())),
                    ]);
                }
                for derived in &response.derived_classes {
                    rows.push(vec![
                        Cell::new(SharedString::from(derived.class_name.clone())),
                        Cell::toned(SharedString::from("derived"), Tone::Success),
                        Cell::muted(SharedString::from(derived.file_path.clone())),
                    ]);
                }
                self.block(&columns, &rows, response.class_name.clone(), cx)
            }
            EntityAnswer::Implementations(response) => {
                let columns = [
                    Column::grow(t("entity.colClass", locale)),
                    Column::fixed(t("entity.colRelation", locale), 120.0),
                    Column::grow(t("entity.colFile", locale)),
                ];
                let mut rows: Vec<Vec<Cell>> = Vec::new();
                for interface in &response.implemented_interfaces {
                    rows.push(vec![
                        Cell::new(SharedString::from(interface.interface_name.clone())),
                        Cell::toned(SharedString::from("implements"), Tone::Info),
                        Cell::muted(SharedString::from(interface.file_path.clone())),
                    ]);
                }
                for class in &response.implementing_classes {
                    rows.push(vec![
                        Cell::new(SharedString::from(class.class_name.clone())),
                        Cell::toned(SharedString::from("implemented-by"), Tone::Success),
                        Cell::muted(SharedString::from(class.file_path.clone())),
                    ]);
                }
                self.block(&columns, &rows, response.class_name.clone(), cx)
            }
            EntityAnswer::Classification(response) => {
                let columns = [
                    Column::grow(t("entity.colClassification", locale)),
                    Column::fixed(t("entity.colCount", locale), 90.0),
                ];
                let mut entries: Vec<(&String, &usize)> = response.stats.iter().collect();
                entries.sort_by(|left, right| right.1.cmp(left.1));
                let rows: Vec<Vec<Cell>> = entries
                    .into_iter()
                    .map(|(name, count)| {
                        vec![
                            Cell::new(SharedString::from(name.clone())),
                            Cell::muted(SharedString::from(count.to_string())),
                        ]
                    })
                    .collect();
                let stats = self.block(
                    &columns,
                    &rows,
                    format!("{} {}", response.total, t("entity.relations", locale)),
                    cx,
                );
                let detail_columns = [
                    Column::grow(t("entity.colCaller", locale)),
                    Column::grow(t("entity.colCallee", locale)),
                    Column::fixed(t("entity.colRelation", locale), 140.0),
                    Column::fixed(t("entity.colTarget", locale), 160.0),
                    Column::fixed(t("entity.colCallSite", locale), 180.0),
                ];
                let detail = match &self.detail {
                    Load::Idle => div().into_any_element(),
                    Load::Loading => {
                        loading_row(t("entity.detailLoading", locale), cx).into_any_element()
                    }
                    Load::Failed(failure) => failure_block(failure, locale, cx).into_any_element(),
                    Load::Ready(rows) => {
                        let rows: Vec<Vec<Cell>> = rows
                            .iter()
                            .map(|row| {
                                vec![
                                    Cell::new(SharedString::from(format!(
                                        "{} ({})",
                                        row.caller_name, row.caller_stable_id
                                    ))),
                                    Cell::new(SharedString::from(row.callee_name.clone())),
                                    Cell::toned(
                                        SharedString::from(row.relation_type.clone()),
                                        Tone::Info,
                                    ),
                                    Cell::muted(SharedString::from(
                                        row.external_target.clone().unwrap_or_default(),
                                    )),
                                    Cell::muted(SharedString::from(format!(
                                        "{}:{}",
                                        row.caller_file_path,
                                        row.call_row + 1
                                    ))),
                                ]
                            })
                            .collect();
                        self.block(
                            &detail_columns,
                            &rows,
                            format!("{} {}", rows.len(), t("entity.detailRows", locale)),
                            cx,
                        )
                    }
                };
                v_flex()
                    .gap_3()
                    .w_full()
                    .child(stats)
                    .child(detail)
                    .into_any_element()
            }
        }
    }

    fn block(
        &self,
        columns: &[Column],
        rows: &[Vec<Cell>],
        summary: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dense = AppState::global(cx).read(cx).density.is_dense();
        let locale = locale(cx);
        if rows.is_empty() {
            return empty_state(t("entity.noRows", locale), Some(summary.into()), None, cx)
                .into_any_element();
        }
        surface(dense, cx)
            .child(hint(SharedString::from(summary), cx))
            .child(data_grid(
                PanelKind::Entities,
                columns,
                rows,
                dense,
                &mut *cx,
            ))
            .into_any_element()
    }
}

fn call_columns(locale: crate::settings::Locale) -> [Column; 4] {
    [
        Column::grow(t("entity.colName", locale)),
        Column::fixed(t("entity.colDepth", locale), 70.0),
        Column::fixed(t("entity.colRelation", locale), 140.0),
        Column::grow(t("entity.colFile", locale)),
    ]
}

fn call_row(node: &cce_api::models::CallChainNode) -> Vec<Cell> {
    vec![
        Cell::new(SharedString::from(node.function_name.clone())),
        Cell::muted(SharedString::from(node.depth.to_string())),
        Cell::toned(SharedString::from(node.relation_type.clone()), Tone::Info),
        Cell::muted(SharedString::from(node.file_path.clone())),
    ]
}

impl Render for EntityView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = locale(cx);

        let tabs = choice_row(
            "entity-tab",
            &ENTITY_TABS,
            self.tab,
            |this: &mut Self, index| {
                this.tab = index;
                this.answer = Load::idle();
            },
            cx,
        );

        let direction = h_flex()
            .gap_3()
            .items_center()
            .child(hint(t("entity.direction", locale), cx))
            .child(choice_row(
                "entity-direction",
                &DIRECTION_KEYS,
                self.direction,
                |this: &mut Self, index| this.direction = index,
                cx,
            ));

        let form = field_group(cx)
            .child(tabs)
            .child(
                h_flex()
                    .gap_3()
                    .items_end()
                    .child(field(
                        t("entity.symbol", locale),
                        Input::new(&self.symbol),
                        cx,
                    ))
                    .child(field(
                        t("entity.lookup", locale),
                        Input::new(&self.lookup),
                        cx,
                    ))
                    .child(
                        Button::new("entity-lookup")
                            .small()
                            .label(t("entity.find", locale))
                            .on_click(cx.listener(|this, _, _, cx| this.lookup_symbol(cx))),
                    ),
            )
            .child(if self.tab == 4 {
                field(t("entity.target", locale), Input::new(&self.target), cx).into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if self.tab == 7 {
                h_flex()
                    .gap_3()
                    .items_end()
                    .child(field(
                        t("entity.category", locale),
                        Input::new(&self.category),
                        cx,
                    ))
                    .child(
                        Button::new("entity-category-detail")
                            .small()
                            .label(t("entity.loadDetail", locale))
                            .loading(self.detail.is_loading())
                            .on_click(cx.listener(|this, _, _, cx| this.load_detail(cx))),
                    )
                    .into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if matches!(self.tab, 1..=3) {
                h_flex()
                    .gap_3()
                    .items_end()
                    .child(field(
                        t("entity.prefix", locale),
                        Input::new(&self.prefix),
                        cx,
                    ))
                    .child(field(
                        t("entity.domains", locale),
                        Input::new(&self.domains),
                        cx,
                    ))
                    .child(
                        Button::new("entity-exclude-tests")
                            .small()
                            .label(t("entity.excludeTests", locale))
                            .selected(self.exclude_tests)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.exclude_tests = !this.exclude_tests;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("entity-include-external")
                            .small()
                            .label(t("entity.includeExternal", locale))
                            .selected(self.include_external)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.include_external = !this.include_external;
                                cx.notify();
                            })),
                    )
                    .into_any_element()
            } else {
                div().into_any_element()
            })
            .child(
                h_flex()
                    .gap_3()
                    .items_end()
                    .child(field(
                        t("entity.depth", locale),
                        Input::new(&self.depth),
                        cx,
                    ))
                    .child(field(
                        t("entity.limit", locale),
                        Input::new(&self.limit),
                        cx,
                    ))
                    .child(direction)
                    .child(
                        Button::new("entity-run")
                            .small()
                            .primary()
                            .label(t("common.run", locale))
                            .loading(self.answer.is_loading())
                            .on_click(cx.listener(|this, _, _, cx| this.run(cx))),
                    )
                    .child(
                        Button::new("entity-to-graph")
                            .small()
                            .ghost()
                            .label(t("entity.openInGraph", locale))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let symbol = this.symbol.read(cx).value().trim().to_string();
                                if symbol.is_empty() {
                                    return;
                                }
                                AppState::global(cx).update(cx, |state, cx| {
                                    state.navigate(
                                        Target::GraphSeed {
                                            entity_id: SharedString::from(symbol.clone()),
                                            label: SharedString::from(symbol),
                                        },
                                        cx,
                                    );
                                });
                            })),
                    ),
            );

        let matches_block = match &self.matches {
            Load::Idle => None,
            Load::Loading => {
                Some(loading_row(t("entity.searching", locale), cx).into_any_element())
            }
            Load::Failed(failure) => Some(failure_block(failure, locale, cx).into_any_element()),
            Load::Ready(_) => self.render_matches(cx),
        };

        let history_row = if self.history.is_empty() {
            None
        } else {
            let mut row = h_flex().gap_1().flex_wrap();
            for entry in self.history.iter().take(6) {
                let value = entry.clone();
                row = row.child(
                    Button::new(format!("entity-history-{entry}"))
                        .small()
                        .ghost()
                        .label(SharedString::from(entry.clone()))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.symbol
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

        let answer = match &self.answer {
            Load::Idle => empty_state(
                t("entity.emptyTitle", locale),
                Some(SharedString::from(t("entity.emptyHint", locale))),
                None,
                cx,
            )
            .into_any_element(),
            Load::Loading => loading_row(t("entity.loading", locale), cx).into_any_element(),
            Load::Failed(failure) => failure_block(failure, locale, cx).into_any_element(),
            Load::Ready(answer) => self.render_answer(answer, cx),
        };

        let epoch_row = {
            let current = AppState::global(cx).read(cx).epoch();
            stale_epoch(self.served_epoch, current, locale, cx).map(|banner| {
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(banner.flex_1())
                    .child(
                        Button::new("entity-replay")
                            .small()
                            .label(t("common.replay", locale))
                            .on_click(cx.listener(|this, _, _, cx| this.replay(cx))),
                    )
                    .into_any_element()
            })
        };

        page_body("entity")
            .child(panel_header(
                t("entity.title", locale),
                Some(SharedString::from(t("entity.subtitle", locale))),
                None,
                cx,
            ))
            .child(
                stack()
                    .child(form)
                    .children(matches_block)
                    .children(history_row)
                    .children(epoch_row)
                    .child(answer),
            )
    }
}
