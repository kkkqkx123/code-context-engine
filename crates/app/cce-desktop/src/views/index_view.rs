//! Index panel: full pass, incremental update, single-file parse, summary
//! generation and index statistics.

use cce_api::models::{
    IncrementalIndexResponse, IndexResponse, IndexStatsResponse, ParseResponse, SummaryResponse,
};
use gpui_kit::component::{
    Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    v_flex,
};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Window,
};

use crate::components::{
    Cell, Column, Tone, choice_row, data_grid, empty_state, failure_block, field, field_group,
    hint, loading_row, page_body, panel_header, stack, stat_line, surface,
};
use crate::connection::ClientStore;
use crate::i18n::{locale, t};
use crate::services::history::{self, HistoryKind};
use crate::services::index_service;
use crate::services::request::{self, Load};
use crate::state::AppState;
use crate::views::PanelKind;

const INDEX_TABS: [&str; 5] = [
    "index.tab.full",
    "index.tab.incremental",
    "index.tab.parse",
    "index.tab.summary",
    "index.tab.stats",
];

#[derive(Debug)]
enum IndexAnswer {
    Full(IndexResponse),
    Incremental(IncrementalIndexResponse),
    Parse(ParseResponse),
    Summary(SummaryResponse),
    Stats(IndexStatsResponse),
}

pub struct IndexView {
    path: Entity<InputState>,
    extensions: Entity<InputState>,
    add_files: Entity<InputState>,
    remove_files: Entity<InputState>,
    file: Entity<InputState>,
    directories: Entity<InputState>,
    tab: usize,
    force: bool,
    recursive: bool,
    answer: Load<IndexAnswer>,
    history: Vec<String>,
}

impl IndexView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let locale = crate::i18n::locale(cx);
        let initial_root = AppState::global(cx)
            .read(cx)
            .active_project()
            .map(|project| project.root_path.to_string())
            .unwrap_or_default();
        let path = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t("index.pathPlaceholder", locale))
                .default_value(initial_root)
        });
        let extensions = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("index.extensionsPlaceholder", locale))
        });
        let add_files =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("index.addPlaceholder", locale)));
        let remove_files = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("index.removePlaceholder", locale))
        });
        let file = cx
            .new(|cx| InputState::new(window, cx).placeholder(t("index.filePlaceholder", locale)));
        let directories = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("index.directoriesPlaceholder", locale))
        });
        let history = history::entries(HistoryKind::Index, cx);
        Self {
            path,
            extensions,
            add_files,
            remove_files,
            file,
            directories,
            tab: 0,
            force: false,
            recursive: true,
            answer: Load::idle(),
            history,
        }
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

        let tab = self.tab;
        let path = self.path.read(cx).value().trim().to_string();
        let extensions = crate::services::split_list(&self.extensions.read(cx).value());
        let add = crate::services::split_list(&self.add_files.read(cx).value());
        let remove = crate::services::split_list(&self.remove_files.read(cx).value());
        let file = self.file.read(cx).value().trim().to_string();
        let directories = crate::services::split_list(&self.directories.read(cx).value());
        let force = self.force;
        let recursive = self.recursive;

        // A full pass no longer needs a path: the project record is the
        // source of truth, and the field is only an override.
        let missing = match tab {
            2 => file.is_empty(),
            3 => file.is_empty() && directories.is_empty(),
            _ => false,
        };
        if missing {
            self.answer = Load::Failed(request::Failure {
                key: "index.pathRequired",
                detail: SharedString::default(),
                candidates: Vec::new(),
            });
            cx.notify();
            return;
        }

        match tab {
            0 => history::push(HistoryKind::Index, &path, cx),
            1 => history::push(HistoryKind::Index, &add.join(","), cx),
            2 => history::push(HistoryKind::Index, &file, cx),
            3 => history::push(HistoryKind::Index, &directories.join(","), cx),
            _ => {}
        }
        self.history = history::entries(HistoryKind::Index, cx);
        self.answer.begin();
        cx.notify();

        match tab {
            0 => {
                let overrides = index_service::IndexOverrides {
                    path: crate::services::optional_text(&path),
                    extensions: if extensions.is_empty() {
                        None
                    } else {
                        Some(extensions)
                    },
                    ..index_service::IndexOverrides::default()
                };
                let task = index_service::full(cx, client, project_id, overrides);
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.answer = match outcome {
                        Ok(value) => Load::Ready(IndexAnswer::Full(value)),
                        Err(error) => Load::Failed(request::Failure::from_error(&error)),
                    };
                    cx.notify();
                });
            }
            1 => {
                let task = index_service::incremental(cx, client, project_id, add, remove, force);
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.answer = match outcome {
                        Ok(value) => Load::Ready(IndexAnswer::Incremental(value)),
                        Err(error) => Load::Failed(request::Failure::from_error(&error)),
                    };
                    cx.notify();
                });
            }
            2 => {
                let task = index_service::parse(cx, client, file);
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.answer = match outcome {
                        Ok(value) => Load::Ready(IndexAnswer::Parse(value)),
                        Err(error) => Load::Failed(request::Failure::from_error(&error)),
                    };
                    cx.notify();
                });
            }
            3 => {
                let files = crate::services::split_list(&file);
                let task = index_service::summary(cx, client, files, directories, recursive);
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.answer = match outcome {
                        Ok(value) => Load::Ready(IndexAnswer::Summary(value)),
                        Err(error) => Load::Failed(request::Failure::from_error(&error)),
                    };
                    cx.notify();
                });
            }
            _ => {
                let task = index_service::stats(cx, client, project_id);
                request::spawn_into(cx, task, |this, outcome, cx| {
                    this.answer = match outcome {
                        Ok(value) => Load::Ready(IndexAnswer::Stats(value)),
                        Err(error) => Load::Failed(request::Failure::from_error(&error)),
                    };
                    cx.notify();
                });
            }
        }
    }

    fn render_answer(&self, answer: &IndexAnswer, cx: &mut Context<Self>) -> AnyElement {
        let locale = locale(cx);
        let dense = AppState::global(cx).read(cx).density.is_dense();
        match answer {
            IndexAnswer::Full(response) => surface(dense, cx)
                .child(stat_line(
                    t("index.filesIndexed", locale),
                    response.files_indexed.to_string(),
                    cx,
                ))
                .child(stat_line(
                    t("index.filesScanned", locale),
                    response.files_scanned.to_string(),
                    cx,
                ))
                .child(stat_line(
                    t("index.entities", locale),
                    response.total_entities.to_string(),
                    cx,
                ))
                .child(stat_line(
                    t("index.relations", locale),
                    response.total_relations.to_string(),
                    cx,
                ))
                .child(stat_line(
                    t("index.vectors", locale),
                    response.total_vectors.to_string(),
                    cx,
                ))
                .child(stat_line(
                    t("index.failedFiles", locale),
                    response.failed_files.to_string(),
                    cx,
                ))
                .child(stat_line(
                    t("index.elapsed", locale),
                    format!("{} ms", response.elapsed_ms),
                    cx,
                ))
                .child(hint(SharedString::from(response.message.clone()), cx))
                .into_any_element(),
            IndexAnswer::Incremental(response) => surface(dense, cx)
                .child(stat_line(
                    t("index.filesIndexed", locale),
                    response.files_indexed.to_string(),
                    cx,
                ))
                .child(stat_line(
                    t("index.filesRemoved", locale),
                    response.files_removed.to_string(),
                    cx,
                ))
                .child(stat_line(
                    t("index.entities", locale),
                    response.total_entities.to_string(),
                    cx,
                ))
                .child(stat_line(
                    t("index.elapsed", locale),
                    format!("{} ms", response.elapsed_ms),
                    cx,
                ))
                .into_any_element(),
            IndexAnswer::Parse(response) => {
                let columns = [
                    Column::grow(t("index.colName", locale)),
                    Column::fixed(t("index.colKind", locale), 120.0),
                    Column::fixed(t("index.colLines", locale), 110.0),
                ];
                let rows: Vec<Vec<Cell>> = response
                    .entities
                    .iter()
                    .map(|entity| {
                        vec![
                            Cell::new(SharedString::from(entity.name.clone())),
                            Cell::toned(SharedString::from(entity.kind.clone()), Tone::Info),
                            Cell::muted(SharedString::from(format!(
                                "{}-{}",
                                entity.start_line, entity.end_line
                            ))),
                        ]
                    })
                    .collect();
                if rows.is_empty() {
                    return empty_state(
                        t("index.noEntities", locale),
                        Some(SharedString::from(response.file_path.clone())),
                        None,
                        cx,
                    )
                    .into_any_element();
                }
                surface(dense, cx)
                    .child(hint(
                        SharedString::from(format!(
                            "{} · {} · {} {}",
                            response.file_path,
                            response.language,
                            response.entities.len(),
                            t("index.entities", locale)
                        )),
                        cx,
                    ))
                    .child(data_grid(
                        PanelKind::Index,
                        &columns,
                        &rows,
                        dense,
                        &mut *cx,
                    ))
                    .into_any_element()
            }
            IndexAnswer::Summary(response) => {
                let columns = [
                    Column::grow(t("index.colFile", locale)),
                    Column::fixed(t("index.colEntities", locale), 90.0),
                    Column::fixed(t("index.colImportance", locale), 110.0),
                ];
                let rows: Vec<Vec<Cell>> = response
                    .summaries
                    .iter()
                    .map(|summary| {
                        vec![
                            Cell::new(SharedString::from(summary.file_path.clone())),
                            Cell::muted(SharedString::from(summary.entity_count.to_string())),
                            Cell::toned(
                                SharedString::from(summary.importance_level.clone()),
                                Tone::Info,
                            ),
                        ]
                    })
                    .collect();
                if rows.is_empty() {
                    return empty_state(
                        t("index.noSummaries", locale),
                        Some(SharedString::from(format!(
                            "{} {}",
                            response.success_count,
                            t("index.succeeded", locale)
                        ))),
                        None,
                        cx,
                    )
                    .into_any_element();
                }
                surface(dense, cx)
                    .child(hint(
                        SharedString::from(format!(
                            "{} / {} · {} ms",
                            response.success_count, response.total_files, response.elapsed_ms
                        )),
                        cx,
                    ))
                    .child(data_grid(
                        PanelKind::Index,
                        &columns,
                        &rows,
                        dense,
                        &mut *cx,
                    ))
                    .into_any_element()
            }
            IndexAnswer::Stats(response) => surface(dense, cx)
                .child(stat_line(
                    t("index.files", locale),
                    response.statistics.total_files.to_string(),
                    cx,
                ))
                .child(stat_line(
                    t("index.entities", locale),
                    response.statistics.total_entities.to_string(),
                    cx,
                ))
                .child(stat_line(
                    t("index.relations", locale),
                    response.statistics.total_relations.to_string(),
                    cx,
                ))
                .child(stat_line(
                    t("index.vectors", locale),
                    response.statistics.total_vectors.to_string(),
                    cx,
                ))
                .child(stat_line(
                    t("index.bm25Docs", locale),
                    response.statistics.total_bm25_documents.to_string(),
                    cx,
                ))
                .into_any_element(),
        }
    }
}

impl Render for IndexView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = locale(cx);

        let tabs = choice_row(
            "index-tab",
            &INDEX_TABS,
            self.tab,
            |this: &mut Self, index| {
                this.tab = index;
                this.answer = Load::idle();
            },
            cx,
        );

        let inputs = match self.tab {
            0 => v_flex()
                .gap_2()
                .child(field(t("index.path", locale), Input::new(&self.path), cx))
                .child(field(
                    t("index.extensions", locale),
                    Input::new(&self.extensions),
                    cx,
                ))
                .child(hint(t("index.pathOptionalHint", locale), cx))
                .into_any_element(),
            1 => v_flex()
                .gap_3()
                .child(field(
                    t("index.addFiles", locale),
                    Input::new(&self.add_files),
                    cx,
                ))
                .child(field(
                    t("index.removeFiles", locale),
                    Input::new(&self.remove_files),
                    cx,
                ))
                .child(
                    Button::new("index-force")
                        .small()
                        .label(t("index.forceReindex", locale))
                        .selected(self.force)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.force = !this.force;
                            cx.notify();
                        })),
                )
                .into_any_element(),
            2 => field(t("index.file", locale), Input::new(&self.file), cx).into_any_element(),
            3 => v_flex()
                .gap_3()
                .child(field(t("index.files", locale), Input::new(&self.file), cx))
                .child(field(
                    t("index.directories", locale),
                    Input::new(&self.directories),
                    cx,
                ))
                .child(
                    Button::new("index-recursive")
                        .small()
                        .label(t("index.recursive", locale))
                        .selected(self.recursive)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.recursive = !this.recursive;
                            cx.notify();
                        })),
                )
                .into_any_element(),
            _ => hint(t("index.statsHint", locale), cx).into_any_element(),
        };

        let form = field_group(cx).child(tabs).child(inputs).child(
            h_flex().gap_3().items_center().child(
                Button::new("index-run")
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
                    Button::new(format!("index-history-{entry}"))
                        .small()
                        .ghost()
                        .label(SharedString::from(entry.clone()))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let target = if this.tab == 1 {
                                &this.add_files
                            } else {
                                &this.path
                            };
                            target
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
                t("index.emptyTitle", locale),
                Some(SharedString::from(t("index.emptyHint", locale))),
                None,
                cx,
            )
            .into_any_element(),
            Load::Loading => loading_row(t("index.loading", locale), cx).into_any_element(),
            Load::Failed(failure) => failure_block(failure, locale, cx).into_any_element(),
            Load::Ready(answer) => self.render_answer(answer, cx),
        };

        page_body("index")
            .child(panel_header(
                t("index.title", locale),
                Some(SharedString::from(t("index.subtitle", locale))),
                None,
                cx,
            ))
            .child(stack().child(form).children(history_row).child(answer))
    }
}
