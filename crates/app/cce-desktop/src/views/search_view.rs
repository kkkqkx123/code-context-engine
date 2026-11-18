//! Search panel: single and aggregated retrieval, with a hand-off into the
//! graph panel for hits that carry stable symbol ids.

use cce_api::models::{ContentStateDto, SearchResponse, SearchResultItem};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _,
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
    actions, choice_row, empty_state, failure_block, field, field_group, hint, loading_row,
    page_body, panel_header, stack,
};
use crate::connection::ClientStore;
use crate::highlight::{self, Kind};
use crate::i18n::{locale, t};
use crate::metrics::CARD_RADIUS;
use crate::navigator::Target;
use crate::services::history::{self, HistoryKind};
use crate::services::request::{self, Load};
use crate::services::search_service::{self, QUERY_TYPES, SearchInput};
use crate::state::AppState;

/// Query-type catalogue keys, aligned with [`QUERY_TYPES`].
const QUERY_TYPE_KEYS: [&str; 3] = [
    "search.type.hybrid",
    "search.type.vector",
    "search.type.bm25",
];

/// Default page size for a retrieval request.
const DEFAULT_LIMIT: usize = 10;
/// Ceiling for the over-fetch a deep page needs, so page 30 cannot ask the
/// server for 300 rows on one call.
const MAX_FETCH: usize = 200;

pub struct SearchView {
    query: Entity<InputState>,
    sub_queries: Entity<TextareaState>,
    prefix: Entity<InputState>,
    limit: Entity<InputState>,
    min_score: Entity<InputState>,
    include_patterns: Entity<InputState>,
    exclude_patterns: Entity<InputState>,
    mode: usize,
    query_type: usize,
    exclude_tests: bool,
    rerank: bool,
    /// Zero-based page shown from the current answer.
    page: usize,
    result: Load<SearchResponse>,
    history: Vec<String>,
}

impl SearchView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let locale = crate::i18n::locale(cx);
        let query = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("search.queryPlaceholder", locale))
        });
        let sub_queries = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(4)
                .placeholder(t("search.aggregatedPlaceholder", locale))
        });
        let prefix = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("search.prefixPlaceholder", locale))
        });
        let limit = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t("search.limitPlaceholder", locale))
                .default_value(DEFAULT_LIMIT.to_string())
        });
        let min_score = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("search.minScorePlaceholder", locale))
        });
        let include_patterns = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("search.patternsPlaceholder", locale))
        });
        let exclude_patterns = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("search.patternsPlaceholder", locale))
        });
        let history = history::entries(HistoryKind::Search, cx);
        Self {
            query,
            sub_queries,
            prefix,
            limit,
            min_score,
            include_patterns,
            exclude_patterns,
            mode: 0,
            query_type: 0,
            exclude_tests: false,
            rerank: false,
            page: 0,
            result: Load::idle(),
            history,
        }
    }

    fn limit_value(&self, cx: &Context<Self>) -> usize {
        self.limit
            .read(cx)
            .value()
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|limit| *limit > 0)
            .unwrap_or(DEFAULT_LIMIT)
    }

    /// Rows wanted per page.
    fn page_size(&self, cx: &Context<Self>) -> usize {
        self.limit_value(cx)
    }

    /// Rows to request for the current page: the wire request has no offset,
    /// so a deep page over-fetches and is sliced client-side.
    fn fetch_size(&self, cx: &Context<Self>) -> usize {
        self.page_size(cx)
            .saturating_mul(self.page + 1)
            .min(MAX_FETCH)
    }

    fn min_score_value(&self, cx: &Context<Self>) -> Option<f32> {
        self.min_score
            .read(cx)
            .value()
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|score| (0.0..=1.0).contains(score))
    }

    fn go_to_page(&mut self, page: usize, cx: &mut Context<Self>) {
        if page == self.page {
            return;
        }
        self.page = page;
        self.run(cx);
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

        let limit = self.fetch_size(cx);
        let prefix = crate::services::optional_text(&self.prefix.read(cx).value());
        let aggregated = self.mode == 1;
        let query_type = QUERY_TYPES[self.query_type.min(QUERY_TYPES.len() - 1)].to_string();
        let exclude_tests = self.exclude_tests;
        let min_score = self.min_score_value(cx);
        let include_patterns = crate::services::split_list(&self.include_patterns.read(cx).value());
        let exclude_patterns = crate::services::split_list(&self.exclude_patterns.read(cx).value());
        let rerank = self.rerank;

        let task = if aggregated {
            let queries =
                search_service::sub_queries(&self.sub_queries.read(cx).value(), &query_type);
            if queries.is_empty() {
                self.result = Load::Failed(request::Failure {
                    key: "search.emptyQuery",
                    detail: SharedString::default(),
                    candidates: Vec::new(),
                });
                cx.notify();
                return;
            }
            history::push(HistoryKind::Search, &self.sub_queries.read(cx).value(), cx);
            search_service::run_aggregated(
                cx,
                client,
                search_service::AggregatedInput {
                    project_id,
                    queries,
                    limit,
                    directory_prefix: prefix,
                    exclude_tests,
                    min_score,
                    include_patterns,
                    exclude_patterns,
                    rerank,
                },
            )
        } else {
            let input = SearchInput {
                project_id,
                query: self.query.read(cx).value().to_string(),
                query_type,
                limit,
                directory_prefix: prefix,
                exclude_tests,
                min_score,
                include_patterns,
                exclude_patterns,
                rerank,
            };
            if input.query.trim().is_empty() {
                self.result = Load::Failed(request::Failure {
                    key: "search.emptyQuery",
                    detail: SharedString::default(),
                    candidates: Vec::new(),
                });
                cx.notify();
                return;
            }
            history::push(HistoryKind::Search, &input.query, cx);
            search_service::run(cx, client, input)
        };

        self.history = history::entries(HistoryKind::Search, cx);
        self.result.begin();
        cx.notify();
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.result = Load::settle(outcome);
            cx.notify();
        });
    }

    fn use_history(&mut self, entry: String, window: &mut Window, cx: &mut Context<Self>) {
        self.mode = 0;
        self.query
            .update(cx, |state, cx| state.set_value(entry, window, cx));
        cx.notify();
    }

    fn render_result_item(&self, item: &SearchResultItem, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().colors;
        let locale = locale(cx);
        let mono = cx.theme().mono_font_family.clone();
        let seed = search_service::graph_seed(item);

        let mut sources = h_flex().gap_1().flex_wrap();
        for source in &item.sources {
            sources = sources.child(
                div()
                    .px_1()
                    .rounded(px(CARD_RADIUS))
                    .bg(theme.muted)
                    .text_xs()
                    .child(SharedString::from(source.clone())),
            );
        }

        let reference = matches!(
            item.content_state,
            ContentStateDto::ReferenceFileMissing
                | ContentStateDto::ReferenceFileLevel
                | ContentStateDto::ReferenceChunkMissing
                | ContentStateDto::ReferenceOverLimit
        );
        let mut actions_row = actions();
        if let Some((entity_id, label)) = seed {
            actions_row = actions_row
                .child(
                    Button::new(format!("to-entity-{}-{}", item.file_path, item.start_line))
                        .small()
                        .label(t("search.openInEntity", locale))
                        .on_click(cx.listener({
                            let entity_id = entity_id.clone();
                            move |_, _, _, cx| {
                                let id = entity_id.clone();
                                AppState::global(cx).update(cx, |state, cx| {
                                    state.navigate(Target::Entity { id: id.into() }, cx);
                                });
                            }
                        })),
                )
                .child(
                    Button::new(format!("to-graph-{}-{}", item.file_path, item.start_line))
                        .small()
                        .label(t("search.openInGraph", locale))
                        .on_click(cx.listener(move |_, _, _, cx| {
                            let entity_id = entity_id.clone();
                            let label = label.clone();
                            AppState::global(cx).update(cx, |state, cx| {
                                state.navigate(
                                    Target::GraphSeed {
                                        entity_id: entity_id.into(),
                                        label: label.into(),
                                    },
                                    cx,
                                );
                            });
                        })),
                );
        } else {
            // A hit with no stable symbol id cannot seed an ego query, but the
            // file it lives in still has topology worth showing.
            actions_row = actions_row.child(
                Button::new(format!(
                    "to-graph-file-{}-{}",
                    item.file_path, item.start_line
                ))
                .small()
                .label(t("search.openFileInGraph", locale))
                .on_click(cx.listener({
                    let file = item.file_path.clone();
                    move |_, _, _, cx| {
                        let file = file.clone();
                        AppState::global(cx).update(cx, |state, cx| {
                            state.navigate(Target::GraphFile { file: file.into() }, cx);
                        });
                    }
                })),
            );
        }

        v_flex()
            .gap_2()
            .w_full()
            .p_3()
            .border_1()
            .border_color(theme.border)
            .rounded(px(CARD_RADIUS))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().truncate().child(SharedString::from(format!(
                        "{}:{}-{}",
                        item.file_path, item.start_line, item.end_line
                    ))))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(SharedString::from(format!("{:.3}", item.score))),
                    ),
            )
            .child(if item.entity_names.is_empty() {
                hint(t("search.noEntities", locale), cx).into_any_element()
            } else {
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(SharedString::from(item.entity_names.join(", ")))
                    .into_any_element()
            })
            .child({
                let mut badges = h_flex().gap_1().flex_wrap();
                badges = badges.child(status_badge(
                    format!("{:?}", item.content_state),
                    &theme,
                    cx,
                ));
                if let Some(kind) = &item.entity_type {
                    badges = badges.child(status_badge(kind.clone(), &theme, cx));
                }
                if item.expanded {
                    badges = badges.child(status_badge(
                        format!(
                            "{} +{}/{}",
                            t("search.expanded", locale),
                            item.forward_nodes,
                            item.backward_nodes
                        ),
                        &theme,
                        cx,
                    ));
                }
                if item.contributing_sub_queries > 0 {
                    badges = badges.child(status_badge(
                        format!(
                            "{} ×{}",
                            t("search.subQueries", locale),
                            item.contributing_sub_queries
                        ),
                        &theme,
                        cx,
                    ));
                }
                badges
            })
            .child({
                let mut block = v_flex()
                    .w_full()
                    .p_2()
                    .rounded(px(CARD_RADIUS))
                    .bg(theme.muted)
                    .font_family(mono)
                    .text_xs();
                for line in preview_lines(&item.code_chunk) {
                    let mut row = h_flex().gap_0();
                    for token in highlight::tokenize(&line) {
                        let color = match token.kind {
                            Kind::Keyword => theme.accent,
                            Kind::String => theme.success,
                            Kind::Comment => theme.muted_foreground,
                            Kind::Number => theme.warning,
                            Kind::Plain => theme.foreground,
                        };
                        row = row.child(
                            div()
                                .text_color(color)
                                .child(SharedString::from(token.text)),
                        );
                    }
                    block = block.child(row);
                }
                block
            })
            .child(if reference {
                hint(t("search.referenceOnly", locale), cx).into_any_element()
            } else {
                div().into_any_element()
            })
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .justify_between()
                    .child(sources)
                    .child(actions_row),
            )
            .into_any_element()
    }
}

/// First lines of a chunk, so one hit cannot fill the panel.
fn preview_lines(chunk: &str) -> Vec<String> {
    const MAX_LINES: usize = 12;
    let mut lines: Vec<String> = chunk.lines().take(MAX_LINES).map(str::to_string).collect();
    if chunk.lines().count() > MAX_LINES {
        lines.push("…".to_string());
    }
    lines
}

/// Small state chip used in a result card's summary row.
fn status_badge(
    text: String,
    theme: &gpui_kit::component::ThemeColor,
    _cx: &mut Context<SearchView>,
) -> AnyElement {
    div()
        .px_2()
        .rounded(px(CARD_RADIUS))
        .bg(theme.muted)
        .text_xs()
        .text_color(theme.muted_foreground)
        .child(SharedString::from(text))
        .into_any_element()
}

impl Render for SearchView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = locale(cx);
        let theme = cx.theme().colors;

        let mode_row = choice_row(
            "search-mode",
            &["search.mode.single", "search.mode.aggregated"],
            self.mode,
            |this: &mut Self, index| this.mode = index,
            cx,
        );

        let input_block = if self.mode == 1 {
            v_flex()
                .gap_2()
                .child(field(
                    t("search.aggregated", locale),
                    Textarea::new(&self.sub_queries),
                    cx,
                ))
                .child(hint(t("search.aggregatedHint", locale), cx))
        } else {
            field(t("search.query", locale), Input::new(&self.query), cx)
        };

        let form = field_group(cx)
            .child(h_flex().gap_3().items_center().child(mode_row))
            .child(input_block)
            .child(
                h_flex()
                    .gap_3()
                    .items_end()
                    .child(field(
                        t("search.prefix", locale),
                        Input::new(&self.prefix),
                        cx,
                    ))
                    .child(field(
                        t("search.limit", locale),
                        Input::new(&self.limit),
                        cx,
                    ))
                    .child(
                        Button::new("search-exclude-tests")
                            .small()
                            .label(t("search.excludeTests", locale))
                            .selected(self.exclude_tests)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.exclude_tests = !this.exclude_tests;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                h_flex()
                    .gap_3()
                    .items_end()
                    .child(field(
                        t("search.minScore", locale),
                        Input::new(&self.min_score),
                        cx,
                    ))
                    .child(field(
                        t("search.includePatterns", locale),
                        Input::new(&self.include_patterns),
                        cx,
                    ))
                    .child(field(
                        t("search.excludePatterns", locale),
                        Input::new(&self.exclude_patterns),
                        cx,
                    ))
                    .child(
                        Button::new("search-rerank")
                            .small()
                            .label(t("search.rerank", locale))
                            .selected(self.rerank)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.rerank = !this.rerank;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .justify_between()
                    .child(choice_row(
                        "search-type",
                        &QUERY_TYPE_KEYS,
                        self.query_type,
                        |this: &mut Self, index| this.query_type = index,
                        cx,
                    ))
                    .child(
                        Button::new("search-run")
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
                    Button::new(format!("search-history-{entry}"))
                        .small()
                        .ghost()
                        .label(SharedString::from(entry.clone()))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.use_history(value.clone(), window, cx)
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

        let result_block = match &self.result {
            Load::Idle => empty_state(
                t("search.emptyTitle", locale),
                Some(SharedString::from(t("search.emptyHint", locale))),
                None,
                cx,
            )
            .into_any_element(),
            Load::Loading => loading_row(t("search.searching", locale), cx).into_any_element(),
            Load::Failed(failure) => failure_block(failure, locale, cx).into_any_element(),
            Load::Ready(response) if response.items.is_empty() => empty_state(
                t("search.noResults", locale),
                Some(SharedString::from(format!(
                    "{} ms · {}",
                    response.elapsed_ms, response.total
                ))),
                None,
                cx,
            )
            .into_any_element(),
            Load::Ready(response) => {
                let page_size = self.page_size(cx);
                let page =
                    search_service::page(&response.items, response.total, self.page, page_size);
                let start = self.page.saturating_mul(page_size);
                let items: Vec<AnyElement> = response
                    .items
                    .iter()
                    .skip(start)
                    .map(|item| self.render_result_item(item, cx))
                    .collect();
                let range = if page.shown == 0 {
                    String::from("0")
                } else {
                    format!("{}-{}", page.first, page.first + page.shown - 1)
                };
                let pager = h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("search-prev")
                            .small()
                            .ghost()
                            .label(t("common.previous", locale))
                            .disabled(self.page == 0)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let page = this.page.saturating_sub(1);
                                this.go_to_page(page, cx);
                            })),
                    )
                    .child(div().text_xs().text_color(theme.muted_foreground).child(
                        SharedString::from(format!(
                            "{range} / {} · {} ms · {}",
                            page.total,
                            response.elapsed_ms,
                            response.sources_used.join(", ")
                        )),
                    ))
                    .child(
                        Button::new("search-next")
                            .small()
                            .ghost()
                            .label(t("common.next", locale))
                            .disabled(!page.has_more())
                            .on_click(cx.listener(|this, _, _, cx| {
                                let page = this.page + 1;
                                this.go_to_page(page, cx);
                            })),
                    );
                v_flex()
                    .gap_3()
                    .w_full()
                    .child(pager)
                    .children(items)
                    .into_any_element()
            }
        };

        page_body("search")
            .child(panel_header(
                t("search.title", locale),
                Some(SharedString::from(t("search.subtitle", locale))),
                None,
                cx,
            ))
            .child(
                stack()
                    .child(form)
                    .children(history_row)
                    .child(result_block),
            )
    }
}
