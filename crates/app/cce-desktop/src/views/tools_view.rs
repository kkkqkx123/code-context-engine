//! Tools panel: folding, compression, diagnosis, keyword search and symbol
//! navigation (symbols, references, definition).
//!
//! Snippet tools work on text typed into the panel; file tools resolve against
//! the indexed project and therefore need the active scope.

use cce_api::models::{
    BatchCompressResponse, BatchFoldResponse, CompressApiResponse, DiagnoseApiResponse,
    FindReferencesResponse, FoldResponse, GetSymbolsResponse, GotoDefinitionResponse,
    KeywordSearchApiResponse,
};
use gpui_kit::component::{
    ActiveTheme as _, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState, Textarea, TextareaState},
    v_flex,
};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Window, div, px,
};

use crate::components::{
    Cell, Column, Tone, choice_row, data_grid, empty_state, failure_block, field, field_group,
    hint, loading_row, page_body, panel_header, stack, surface,
};
use crate::connection::ClientStore;
use crate::i18n::{locale, t};
use crate::metrics::CARD_RADIUS;
use crate::services::history::{self, HistoryKind};
use crate::services::request::{self, Load};
use crate::services::tools_service;
use crate::state::AppState;
use crate::views::PanelKind;

const TOOL_TABS: [&str; 9] = [
    "tools.tab.fold",
    "tools.tab.compress",
    "tools.tab.diagnose",
    "tools.tab.keyword",
    "tools.tab.symbols",
    "tools.tab.references",
    "tools.tab.definition",
    "tools.tab.foldBatch",
    "tools.tab.compressBatch",
];

#[derive(Debug)]
enum ToolAnswer {
    Fold(FoldResponse),
    Compress(CompressApiResponse),
    FoldBatch(BatchFoldResponse),
    CompressBatch(BatchCompressResponse),
    Diagnose(DiagnoseApiResponse),
    Keyword(KeywordSearchApiResponse),
    Symbols(GetSymbolsResponse),
    References(FindReferencesResponse),
    Definition(GotoDefinitionResponse),
}

pub struct ToolsView {
    code: Entity<TextareaState>,
    language: Entity<InputState>,
    file: Entity<InputState>,
    line: Entity<InputState>,
    symbol: Entity<InputState>,
    query: Entity<InputState>,
    top_n: Entity<InputState>,
    /// One file path per line, used by the batch compress tab.
    paths: Entity<TextareaState>,
    tab: usize,
    include_ast: bool,
    all_terms: bool,
    answer: Load<ToolAnswer>,
    history: Vec<String>,
}

impl ToolsView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let locale = crate::i18n::locale(cx);
        let code = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(10)
                .placeholder(t("tools.codePlaceholder", locale))
        });
        let language = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("tools.languagePlaceholder", locale))
        });
        let file = cx
            .new(|cx| InputState::new(window, cx).placeholder(t("tools.filePlaceholder", locale)));
        let line = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t("tools.linePlaceholder", locale))
                .default_value("1")
        });
        let symbol = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("tools.symbolPlaceholder", locale))
        });
        let query = cx
            .new(|cx| InputState::new(window, cx).placeholder(t("tools.queryPlaceholder", locale)));
        let top_n = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t("tools.topNPlaceholder", locale))
                .default_value("20")
        });
        let paths = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(4)
                .placeholder(t("tools.pathsPlaceholder", locale))
        });
        let history = history::entries(HistoryKind::Tools, cx);
        Self {
            code,
            language,
            file,
            line,
            symbol,
            query,
            top_n,
            paths,
            tab: 0,
            include_ast: false,
            all_terms: false,
            answer: Load::idle(),
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

    fn run(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.answer = Load::Failed(request::Failure::disconnected());
            cx.notify();
            return;
        };
        let tab = self.tab;
        let code = self.code.read(cx).value().to_string();
        let language = crate::services::optional_text(&self.language.read(cx).value());
        let file = self.file.read(cx).value().trim().to_string();
        let symbol = crate::services::optional_text(&self.symbol.read(cx).value());
        let query = self.query.read(cx).value().trim().to_string();
        let include_ast = self.include_ast;
        let all_terms = self.all_terms;
        let top_n = self.number(&self.top_n, 20, cx);
        let line_text = self.line.read(cx).value().to_string();

        // Snippet tools need text, file tools need a path, and both navigation
        // tools need a usable line number.
        if matches!(tab, 0 | 2 | 7) && code.trim().is_empty() {
            self.fail("tools.codeRequired", cx);
            return;
        }
        if tab == 7 && tools_service::batch_items(&code, language.clone()).is_empty() {
            self.fail("tools.codeRequired", cx);
            return;
        }
        if tab == 8 && crate::services::split_list(&self.paths.read(cx).value()).is_empty() {
            self.fail("tools.pathsRequired", cx);
            return;
        }
        if matches!(tab, 1 | 4 | 5 | 6) && file.is_empty() {
            self.fail("tools.fileRequired", cx);
            return;
        }
        if tab == 3 && query.is_empty() {
            self.fail("tools.queryRequired", cx);
            return;
        }

        let line = if matches!(tab, 5 | 6) {
            match tools_service::parse_line(&line_text) {
                Ok(line) => line,
                Err(_) => {
                    self.fail("tools.lineRequired", cx);
                    return;
                }
            }
        } else {
            0
        };

        match tab {
            0 => history::push(
                HistoryKind::Tools,
                &code.chars().take(80).collect::<String>(),
                cx,
            ),
            1 => history::push(HistoryKind::Tools, &file, cx),
            3 => history::push(HistoryKind::Tools, &query, cx),
            _ => {}
        }
        self.history = history::entries(HistoryKind::Tools, cx);

        // Project-scoped tools need a scope; snippet tools do not.
        let project_id = if matches!(tab, 3..=6 | 8) {
            match AppState::global(cx).read(cx).active_project_id() {
                Some(id) => Some(id),
                None => {
                    self.fail("error.noProject", cx);
                    return;
                }
            }
        } else {
            None
        };

        self.answer.begin();
        cx.notify();

        match tab {
            0 => {
                let task = tools_service::fold(cx, client, code, language);
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.answer = match outcome {
                        Ok(value) => Load::Ready(ToolAnswer::Fold(value)),
                        Err(error) => Load::Failed(request::Failure::from_error(&error)),
                    };
                    cx.notify();
                });
            }
            1 => {
                let task = tools_service::compress(cx, client, file, false);
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.answer = match outcome {
                        Ok(value) => Load::Ready(ToolAnswer::Compress(value)),
                        Err(error) => Load::Failed(request::Failure::from_error(&error)),
                    };
                    cx.notify();
                });
            }
            7 => {
                let task = tools_service::fold_batch(cx, client, code, language);
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.answer = match outcome {
                        Ok(value) => Load::Ready(ToolAnswer::FoldBatch(value)),
                        Err(error) => Load::Failed(request::Failure::from_error(&error)),
                    };
                    cx.notify();
                });
            }
            8 => {
                let paths = crate::services::split_list(&self.paths.read(cx).value());
                let task = tools_service::compress_batch(cx, client, paths);
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.answer = match outcome {
                        Ok(value) => Load::Ready(ToolAnswer::CompressBatch(value)),
                        Err(error) => Load::Failed(request::Failure::from_error(&error)),
                    };
                    cx.notify();
                });
            }
            2 => {
                let task = tools_service::diagnose(cx, client, code, language, include_ast);
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.answer = match outcome {
                        Ok(value) => Load::Ready(ToolAnswer::Diagnose(value)),
                        Err(error) => Load::Failed(request::Failure::from_error(&error)),
                    };
                    cx.notify();
                });
            }
            3 => {
                let project_id = project_id.unwrap_or_default();
                let task = tools_service::keyword(cx, client, project_id, query, top_n, all_terms);
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.answer = match outcome {
                        Ok(value) => Load::Ready(ToolAnswer::Keyword(value)),
                        Err(error) => Load::Failed(request::Failure::from_error(&error)),
                    };
                    cx.notify();
                });
            }
            4 => {
                let project_id = project_id.unwrap_or_default();
                let paths = crate::services::split_list(&file);
                let task = tools_service::symbols(cx, client, project_id, paths);
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.answer = match outcome {
                        Ok(value) => Load::Ready(ToolAnswer::Symbols(value)),
                        Err(error) => Load::Failed(request::Failure::from_error(&error)),
                    };
                    cx.notify();
                });
            }
            5 => {
                let project_id = project_id.unwrap_or_default();
                let task = tools_service::references(cx, client, project_id, file, line, symbol);
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.answer = match outcome {
                        Ok(value) => Load::Ready(ToolAnswer::References(value)),
                        Err(error) => Load::Failed(request::Failure::from_error(&error)),
                    };
                    cx.notify();
                });
            }
            _ => {
                let project_id = project_id.unwrap_or_default();
                let task = tools_service::definition(cx, client, project_id, file, line, symbol);
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.answer = match outcome {
                        Ok(value) => Load::Ready(ToolAnswer::Definition(value)),
                        Err(error) => Load::Failed(request::Failure::from_error(&error)),
                    };
                    cx.notify();
                });
            }
        }
    }

    fn fail(&mut self, key: &'static str, cx: &mut Context<Self>) {
        self.answer = Load::Failed(request::Failure {
            key,
            detail: SharedString::default(),
            candidates: Vec::new(),
        });
        cx.notify();
    }

    fn mono_block(&self, text: String, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme().colors;
        div()
            .w_full()
            .p_3()
            .rounded(px(CARD_RADIUS))
            .border_1()
            .border_color(theme.border)
            .font_family(cx.theme().mono_font_family.clone())
            .text_xs()
            .child(SharedString::from(text))
            .into_any_element()
    }

    fn render_answer(&self, answer: &ToolAnswer, cx: &mut Context<Self>) -> AnyElement {
        let locale = locale(cx);
        let dense = AppState::global(cx).read(cx).density.is_dense();
        match answer {
            ToolAnswer::Fold(response) => v_flex()
                .gap_2()
                .w_full()
                .child(hint(
                    SharedString::from(format!(
                        "{} · {} → {} tokens · {}",
                        response.language,
                        response.original_tokens,
                        response.folded_tokens,
                        response.structure_known
                    )),
                    cx,
                ))
                .child(self.mono_block(response.folded_text.clone(), cx))
                .into_any_element(),
            ToolAnswer::FoldBatch(response) => {
                let columns = [
                    Column::grow(t("tools.colItem", locale)),
                    Column::fixed(t("tools.colTokens", locale), 130.0),
                    Column::fixed(t("tools.colStructure", locale), 110.0),
                ];
                let rows: Vec<Vec<Cell>> = response
                    .results
                    .iter()
                    .map(|item| {
                        vec![
                            Cell::new(SharedString::from(item.id.clone())),
                            Cell::muted(SharedString::from(format!(
                                "{} -> {}",
                                item.original_tokens, item.folded_tokens
                            ))),
                            Cell::muted(SharedString::from(if item.structure_known {
                                "known".to_string()
                            } else {
                                "unknown".to_string()
                            })),
                        ]
                    })
                    .collect();
                surface(dense, cx)
                    .child(hint(
                        SharedString::from(format!(
                            "{} -> {} tokens",
                            response.stats.total_original_tokens,
                            response.stats.total_folded_tokens
                        )),
                        cx,
                    ))
                    .child(data_grid(
                        PanelKind::Tools,
                        &columns,
                        &rows,
                        dense,
                        &mut *cx,
                    ))
                    .into_any_element()
            }
            ToolAnswer::CompressBatch(response) => {
                let mut rows: Vec<Vec<Cell>> = response
                    .successes
                    .iter()
                    .map(|entry| {
                        vec![
                            Cell::new(SharedString::from(entry.path.clone())),
                            Cell::toned(SharedString::from("ok"), Tone::Success),
                            Cell::muted(SharedString::from(format!(
                                "{} chars",
                                entry.result.semantic_text.chars().count()
                            ))),
                        ]
                    })
                    .collect();
                for failure in &response.failures {
                    rows.push(vec![
                        Cell::new(SharedString::from(failure.path.clone())),
                        Cell::toned(SharedString::from("failed"), Tone::Danger),
                        Cell::muted(SharedString::from(failure.error.clone())),
                    ]);
                }
                let columns = [
                    Column::grow(t("tools.colFile", locale)),
                    Column::fixed(t("tools.colOutcome", locale), 90.0),
                    Column::grow(t("tools.colDetail", locale)),
                ];
                surface(dense, cx)
                    .child(data_grid(
                        PanelKind::Tools,
                        &columns,
                        &rows,
                        dense,
                        &mut *cx,
                    ))
                    .into_any_element()
            }
            ToolAnswer::Compress(response) => {
                let Some(result) = &response.result else {
                    return hint(
                        SharedString::from(response.error.clone().unwrap_or_default()),
                        cx,
                    )
                    .into_any_element();
                };
                v_flex()
                    .gap_2()
                    .w_full()
                    .child(hint(
                        SharedString::from(format!(
                            "{} · {} · cache={}",
                            result.file_path, result.language, result.from_cache
                        )),
                        cx,
                    ))
                    .child(self.mono_block(result.semantic_text.clone(), cx))
                    .into_any_element()
            }
            ToolAnswer::Diagnose(response) => {
                let Some(result) = &response.result else {
                    return hint(
                        SharedString::from(response.error.clone().unwrap_or_default()),
                        cx,
                    )
                    .into_any_element();
                };
                let columns = [
                    Column::grow(t("tools.colMessage", locale)),
                    Column::fixed(t("tools.colKind", locale), 140.0),
                    Column::fixed(t("tools.colPrecision", locale), 110.0),
                    Column::fixed(t("tools.colPosition", locale), 110.0),
                ];
                let rows: Vec<Vec<Cell>> = result
                    .diagnostics
                    .iter()
                    .map(|entry| {
                        vec![
                            Cell::new(SharedString::from(entry.message.clone())),
                            Cell::toned(SharedString::from(entry.kind.clone()), Tone::Warning),
                            Cell::muted(SharedString::from(entry.precision.clone())),
                            Cell::muted(SharedString::from(format!(
                                "{}:{}",
                                entry.position.row, entry.position.column
                            ))),
                        ]
                    })
                    .collect();
                let summary = format!(
                    "{} · valid={} · {} {}",
                    result.language,
                    result.is_valid,
                    result.diagnostics.len(),
                    t("tools.diagnostics", locale)
                );
                if rows.is_empty() {
                    return empty_state(
                        t("tools.noDiagnostics", locale),
                        Some(summary.into()),
                        None,
                        cx,
                    )
                    .into_any_element();
                }
                surface(dense, cx)
                    .child(hint(SharedString::from(summary), cx))
                    .child(data_grid(
                        PanelKind::Tools,
                        &columns,
                        &rows,
                        dense,
                        &mut *cx,
                    ))
                    .into_any_element()
            }
            ToolAnswer::Keyword(response) => {
                let Some(result) = &response.result else {
                    return hint(
                        SharedString::from(response.error.clone().unwrap_or_default()),
                        cx,
                    )
                    .into_any_element();
                };
                let columns = [
                    Column::grow(t("tools.colFile", locale)),
                    Column::fixed(t("tools.colScore", locale), 90.0),
                ];
                let rows: Vec<Vec<Cell>> = result
                    .results
                    .iter()
                    .map(|item| {
                        vec![
                            Cell::new(SharedString::from(item.file_path.clone())),
                            Cell::muted(SharedString::from(format!("{:.3}", item.score))),
                        ]
                    })
                    .collect();
                if rows.is_empty() {
                    return empty_state(t("tools.noResults", locale), None, None, cx)
                        .into_any_element();
                }
                surface(dense, cx)
                    .child(hint(
                        SharedString::from(format!("{} {}", result.total, t("tools.hits", locale))),
                        cx,
                    ))
                    .child(data_grid(
                        PanelKind::Tools,
                        &columns,
                        &rows,
                        dense,
                        &mut *cx,
                    ))
                    .into_any_element()
            }
            ToolAnswer::Symbols(response) => {
                let Some(result) = &response.result else {
                    return hint(
                        SharedString::from(response.error.clone().unwrap_or_default()),
                        cx,
                    )
                    .into_any_element();
                };
                let columns = [
                    Column::grow(t("tools.colSymbol", locale)),
                    Column::fixed(t("tools.colKind", locale), 120.0),
                    Column::fixed(t("tools.colPosition", locale), 110.0),
                ];
                let mut rows: Vec<Vec<Cell>> = Vec::new();
                for file in &result.results {
                    for symbol in file.symbols.iter().flatten() {
                        rows.push(vec![
                            Cell::new(SharedString::from(symbol.name.clone())),
                            Cell::toned(SharedString::from(symbol.kind.clone()), Tone::Info),
                            Cell::muted(SharedString::from(format!(
                                "{}:{}",
                                file.path, symbol.line
                            ))),
                        ]);
                    }
                }
                if rows.is_empty() {
                    return empty_state(t("tools.noSymbols", locale), None, None, cx)
                        .into_any_element();
                }
                surface(dense, cx)
                    .child(data_grid(
                        PanelKind::Tools,
                        &columns,
                        &rows,
                        dense,
                        &mut *cx,
                    ))
                    .into_any_element()
            }
            ToolAnswer::References(response) => {
                let Some(result) = &response.result else {
                    return hint(
                        SharedString::from(response.error.clone().unwrap_or_default()),
                        cx,
                    )
                    .into_any_element();
                };
                let columns = [
                    Column::grow(t("tools.colFile", locale)),
                    Column::fixed(t("tools.colPosition", locale), 110.0),
                    Column::fixed(t("tools.colCount", locale), 80.0),
                ];
                let rows: Vec<Vec<Cell>> = result
                    .references
                    .iter()
                    .map(|group| {
                        let first = group.references.first();
                        let position = first
                            .map(|reference| format!("{}:{}", reference.line, reference.column))
                            .unwrap_or_default();
                        vec![
                            Cell::new(SharedString::from(group.path.clone())),
                            Cell::muted(SharedString::from(position)),
                            Cell::muted(SharedString::from(group.count.to_string())),
                        ]
                    })
                    .collect();
                if rows.is_empty() {
                    return empty_state(t("tools.noReferences", locale), None, None, cx)
                        .into_any_element();
                }
                surface(dense, cx)
                    .child(hint(
                        SharedString::from(format!(
                            "{} · {} {}",
                            result.total_count,
                            result.file_count,
                            t("tools.files", locale)
                        )),
                        cx,
                    ))
                    .child(data_grid(
                        PanelKind::Tools,
                        &columns,
                        &rows,
                        dense,
                        &mut *cx,
                    ))
                    .into_any_element()
            }
            ToolAnswer::Definition(response) => {
                let Some(result) = &response.result else {
                    return hint(
                        SharedString::from(response.error.clone().unwrap_or_default()),
                        cx,
                    )
                    .into_any_element();
                };
                if result.definitions.is_empty() {
                    return empty_state(t("tools.noDefinition", locale), None, None, cx)
                        .into_any_element();
                }
                let mut block = v_flex().gap_3().w_full();
                for definition in &result.definitions {
                    block = block.child(
                        v_flex()
                            .gap_2()
                            .w_full()
                            .child(hint(
                                SharedString::from(format!(
                                    "{} · {}:{}-{}",
                                    definition.signature,
                                    definition.location.path,
                                    definition.location.line,
                                    definition.location.end_line
                                )),
                                cx,
                            ))
                            .child(self.mono_block(definition.code.clone(), cx)),
                    );
                }
                block.into_any_element()
            }
        }
    }
}

impl Render for ToolsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = locale(cx);

        let tabs = choice_row(
            "tools-tab",
            &TOOL_TABS,
            self.tab,
            |this: &mut Self, index| {
                this.tab = index;
                this.answer = Load::idle();
            },
            cx,
        );

        let inputs = match self.tab {
            8 => v_flex()
                .gap_2()
                .child(field(
                    t("tools.paths", locale),
                    Textarea::new(&self.paths),
                    cx,
                ))
                .child(hint(t("tools.pathsHint", locale), cx))
                .into_any_element(),
            0 | 2 | 7 => v_flex()
                .gap_3()
                .child(field(
                    t("tools.code", locale),
                    Textarea::new(&self.code),
                    cx,
                ))
                .child(
                    h_flex()
                        .gap_3()
                        .items_end()
                        .child(field(
                            t("tools.language", locale),
                            Input::new(&self.language),
                            cx,
                        ))
                        .child(
                            Button::new("tools-include-ast")
                                .small()
                                .label(t("tools.includeAst", locale))
                                .selected(self.include_ast)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.include_ast = !this.include_ast;
                                    cx.notify();
                                })),
                        ),
                )
                .children(if self.tab == 7 {
                    Some(hint(t("tools.batchHint", locale), cx))
                } else {
                    None
                })
                .into_any_element(),
            1 => field(t("tools.file", locale), Input::new(&self.file), cx).into_any_element(),
            3 => v_flex()
                .gap_3()
                .child(field(t("tools.query", locale), Input::new(&self.query), cx))
                .child(
                    h_flex()
                        .gap_3()
                        .items_end()
                        .child(field(t("tools.topN", locale), Input::new(&self.top_n), cx))
                        .child(
                            Button::new("tools-all-terms")
                                .small()
                                .label(t("tools.allTerms", locale))
                                .selected(self.all_terms)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.all_terms = !this.all_terms;
                                    cx.notify();
                                })),
                        ),
                )
                .into_any_element(),
            4 => field(t("tools.files", locale), Input::new(&self.file), cx).into_any_element(),
            _ => v_flex()
                .gap_3()
                .child(field(t("tools.file", locale), Input::new(&self.file), cx))
                .child(
                    h_flex()
                        .gap_3()
                        .items_end()
                        .child(field(t("tools.line", locale), Input::new(&self.line), cx))
                        .child(field(
                            t("tools.symbol", locale),
                            Input::new(&self.symbol),
                            cx,
                        )),
                )
                .into_any_element(),
        };

        let form = field_group(cx).child(tabs).child(inputs).child(
            h_flex().gap_3().items_center().child(
                Button::new("tools-run")
                    .small()
                    .primary()
                    .label(t("common.run", locale))
                    .loading(self.answer.is_loading())
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
                    Button::new(format!("tools-history-{entry}"))
                        .small()
                        .ghost()
                        .label(SharedString::from(entry.clone()))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.file
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
                t("tools.emptyTitle", locale),
                Some(SharedString::from(t("tools.emptyHint", locale))),
                None,
                cx,
            )
            .into_any_element(),
            Load::Loading => loading_row(t("tools.loading", locale), cx).into_any_element(),
            Load::Failed(failure) => failure_block(failure, locale, cx).into_any_element(),
            Load::Ready(answer) => self.render_answer(answer, cx),
        };

        page_body("tools")
            .child(panel_header(
                t("tools.title", locale),
                Some(SharedString::from(t("tools.subtitle", locale))),
                None,
                cx,
            ))
            .child(stack().child(form).children(history_row).child(answer))
    }
}
