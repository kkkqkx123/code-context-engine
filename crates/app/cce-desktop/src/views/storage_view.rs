//! Storage panel: backend status, index statistics and removal of indexed
//! content.
//!
//! Every removal path here is destructive and immediate: the panel asks for an
//! explicit selection first, so a click can never clear storage by accident.

use cce_api::models::{IndexStatsResponse, StorageStatusResponse};
use gpui_kit::component::{
    Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Window,
};

use crate::components::{
    Cell, Column, Tone, actions, data_grid, failure_block, field, field_group, hint, loading_row,
    notice, page_body, panel_header, stack, stat_line, surface,
};
use crate::connection::ClientStore;
use crate::i18n::{locale, t};
use crate::services::request::{self, Load};
use crate::services::storage_service::{self, ClearScope};
use crate::state::AppState;

pub struct StorageView {
    file: Entity<InputState>,
    entity_id: Entity<InputState>,
    batch_files: Entity<InputState>,
    batch_entities: Entity<InputState>,
    status: Load<StorageStatusResponse>,
    stats: Load<IndexStatsResponse>,
    scope: ClearScope,
    outcome: Option<(bool, &'static str)>,
}

impl StorageView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let locale = crate::i18n::locale(cx);
        let file = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("storage.filePlaceholder", locale))
        });
        let entity_id = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("storage.entityPlaceholder", locale))
        });
        let batch_files = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("storage.batchFilesPlaceholder", locale))
        });
        let batch_entities = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("storage.batchEntitiesPlaceholder", locale))
        });
        Self {
            file,
            entity_id,
            batch_files,
            batch_entities,
            status: Load::idle(),
            stats: Load::idle(),
            scope: ClearScope {
                vectors: false,
                bm25: false,
                relations: false,
                cache: false,
            },
            outcome: None,
        }
    }

    fn active_project(cx: &Context<Self>) -> Option<i64> {
        AppState::global(cx).read(cx).active_project_id()
    }

    fn load_status(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.status = Load::Failed(request::Failure::disconnected());
            cx.notify();
            return;
        };
        self.status.begin();
        cx.notify();
        let task = storage_service::status(cx, client);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.status = Load::settle(outcome);
            cx.notify();
        });
    }

    fn load_stats(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.stats = Load::Failed(request::Failure::disconnected());
            cx.notify();
            return;
        };
        let Some(project_id) = Self::active_project(cx) else {
            self.stats = Load::Failed(request::Failure {
                key: "error.noProject",
                detail: SharedString::default(),
            });
            cx.notify();
            return;
        };
        self.stats.begin();
        cx.notify();
        let task = storage_service::stats(cx, client, project_id);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.stats = Load::settle(outcome);
            cx.notify();
        });
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.outcome = Some((false, "error.notConnected"));
            cx.notify();
            return;
        };
        let Some(project_id) = Self::active_project(cx) else {
            self.outcome = Some((false, "error.noProject"));
            cx.notify();
            return;
        };
        if self.scope.is_empty() {
            self.outcome = Some((false, "storage.selectScope"));
            cx.notify();
            return;
        }
        let scope = self.scope;
        self.outcome = None;
        cx.notify();
        let task = storage_service::clear(cx, client, project_id, scope);
        request::spawn_into(cx, task, move |this, outcome, cx| {
            match outcome {
                Ok(response) => {
                    this.outcome = Some((true, "storage.cleared"));
                    log::info!(
                        "cleared storage for project {project_id}: {}",
                        response.message
                    );
                    this.load_stats(cx);
                }
                Err(err) => {
                    log::warn!("clearing storage failed: {err}");
                    this.outcome = Some((false, "storage.clearFailed"));
                }
            }
            cx.notify();
        });
    }

    fn delete_file(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.outcome = Some((false, "error.notConnected"));
            cx.notify();
            return;
        };
        let Some(project_id) = Self::active_project(cx) else {
            self.outcome = Some((false, "error.noProject"));
            cx.notify();
            return;
        };
        let file = self.file.read(cx).value().trim().to_string();
        if file.is_empty() {
            self.outcome = Some((false, "storage.fileRequired"));
            cx.notify();
            return;
        }
        self.outcome = None;
        cx.notify();
        let task = storage_service::delete_file(cx, client, project_id, file);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.outcome = Some(match outcome {
                Ok(_) => (true, "storage.deleted"),
                Err(_) => (false, "storage.deleteFailed"),
            });
            cx.notify();
        });
    }

    fn delete_entity(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.outcome = Some((false, "error.notConnected"));
            cx.notify();
            return;
        };
        let Some(project_id) = Self::active_project(cx) else {
            self.outcome = Some((false, "error.noProject"));
            cx.notify();
            return;
        };
        let entity_id = self.entity_id.read(cx).value().trim().parse::<u64>().ok();
        let Some(entity_id) = entity_id else {
            self.outcome = Some((false, "storage.entityRequired"));
            cx.notify();
            return;
        };
        self.outcome = None;
        cx.notify();
        let task = storage_service::delete_entity(cx, client, project_id, entity_id);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.outcome = Some(match outcome {
                Ok(_) => (true, "storage.deleted"),
                Err(_) => (false, "storage.deleteFailed"),
            });
            cx.notify();
        });
    }

    fn batch_delete(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.outcome = Some((false, "error.notConnected"));
            cx.notify();
            return;
        };
        let Some(project_id) = Self::active_project(cx) else {
            self.outcome = Some((false, "error.noProject"));
            cx.notify();
            return;
        };
        let files = crate::services::split_list(&self.batch_files.read(cx).value());
        let entities: Vec<u64> = crate::services::split_list(&self.batch_entities.read(cx).value())
            .iter()
            .filter_map(|entry| entry.parse::<u64>().ok())
            .collect();
        if files.is_empty() && entities.is_empty() {
            self.outcome = Some((false, "storage.batchRequired"));
            cx.notify();
            return;
        }
        self.outcome = None;
        cx.notify();
        let task = storage_service::batch_delete(cx, client, project_id, files, entities);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.outcome = Some(match outcome {
                Ok(_) => (true, "storage.deleted"),
                Err(_) => (false, "storage.deleteFailed"),
            });
            cx.notify();
        });
    }

    fn render_status(&self, cx: &Context<Self>) -> AnyElement {
        let locale = locale(cx);
        let dense = AppState::global(cx).read(cx).density.is_dense();
        match &self.status {
            Load::Idle => hint(t("storage.statusIdle", locale), cx).into_any_element(),
            Load::Loading => loading_row(t("storage.loading", locale), cx).into_any_element(),
            Load::Failed(failure) => failure_block(failure, locale, cx).into_any_element(),
            Load::Ready(response) => {
                let columns = [
                    Column::fixed(t("storage.colBackend", locale), 120.0),
                    Column::fixed(t("storage.colConnected", locale), 110.0),
                    Column::fixed(t("storage.colItems", locale), 110.0),
                    Column::fixed(t("storage.colDisk", locale), 110.0),
                    Column::grow(t("storage.colError", locale)),
                ];
                let status = &response.status;
                let components = [
                    ("vector", &status.vector_storage),
                    ("bm25", &status.bm25_storage),
                    ("relation", &status.relation_storage),
                ];
                let rows: Vec<Vec<Cell>> = components
                    .iter()
                    .map(|(name, component)| {
                        vec![
                            Cell::new(SharedString::from(*name)),
                            Cell::toned(
                                SharedString::from(component.backend.clone()),
                                if component.connected {
                                    Tone::Success
                                } else {
                                    Tone::Danger
                                },
                            ),
                            Cell::muted(SharedString::from(component.item_count.to_string())),
                            Cell::muted(SharedString::from(format!(
                                "{:.1} MB",
                                component.disk_usage_mb
                            ))),
                            Cell::muted(SharedString::from(
                                component.last_error.clone().unwrap_or_default(),
                            )),
                        ]
                    })
                    .collect();
                surface(dense, cx)
                    .child(stat_line(
                        t("storage.totalDisk", locale),
                        format!("{:.1} MB", status.total_disk_usage_mb),
                        cx,
                    ))
                    .child(data_grid(&columns, &rows, dense, cx))
                    .into_any_element()
            }
        }
    }

    fn render_stats(&self, cx: &Context<Self>) -> AnyElement {
        let locale = locale(cx);
        let dense = AppState::global(cx).read(cx).density.is_dense();
        match &self.stats {
            Load::Idle => hint(t("storage.statsIdle", locale), cx).into_any_element(),
            Load::Loading => loading_row(t("storage.loading", locale), cx).into_any_element(),
            Load::Failed(failure) => failure_block(failure, locale, cx).into_any_element(),
            Load::Ready(response) => surface(dense, cx)
                .child(stat_line(
                    t("storage.files", locale),
                    response.statistics.total_files.to_string(),
                    cx,
                ))
                .child(stat_line(
                    t("storage.entities", locale),
                    response.statistics.total_entities.to_string(),
                    cx,
                ))
                .child(stat_line(
                    t("storage.relations", locale),
                    response.statistics.total_relations.to_string(),
                    cx,
                ))
                .child(stat_line(
                    t("storage.vectors", locale),
                    response.statistics.total_vectors.to_string(),
                    cx,
                ))
                .child(stat_line(
                    t("storage.bm25Docs", locale),
                    response.statistics.total_bm25_documents.to_string(),
                    cx,
                ))
                .into_any_element(),
        }
    }
}

impl Render for StorageView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = locale(cx);

        let header_actions = actions()
            .child(
                Button::new("storage-status")
                    .small()
                    .label(t("storage.refreshStatus", locale))
                    .on_click(cx.listener(|this, _, _, cx| this.load_status(cx))),
            )
            .child(
                Button::new("storage-stats")
                    .small()
                    .label(t("storage.refreshStats", locale))
                    .on_click(cx.listener(|this, _, _, cx| this.load_stats(cx))),
            );

        let mut scope_row = h_flex().gap_2().flex_wrap();
        for (index, key) in ["vectors", "bm25", "relations", "cache"].iter().enumerate() {
            let selected = match index {
                0 => self.scope.vectors,
                1 => self.scope.bm25,
                2 => self.scope.relations,
                _ => self.scope.cache,
            };
            scope_row = scope_row.child(
                Button::new(format!("storage-scope-{index}"))
                    .small()
                    .label(SharedString::from(*key))
                    .selected(selected)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let slot = match index {
                            0 => &mut this.scope.vectors,
                            1 => &mut this.scope.bm25,
                            2 => &mut this.scope.relations,
                            _ => &mut this.scope.cache,
                        };
                        *slot = !*slot;
                        cx.notify();
                    })),
            );
        }

        let clear_group = field_group(cx)
            .child(hint(t("storage.clearHint", locale), cx))
            .child(scope_row)
            .child(
                Button::new("storage-clear")
                    .small()
                    .danger()
                    .label(t("storage.clear", locale))
                    .on_click(cx.listener(|this, _, _, cx| this.clear(cx))),
            );

        let single_group = field_group(cx)
            .child(
                h_flex()
                    .gap_3()
                    .items_end()
                    .child(field(t("storage.file", locale), Input::new(&self.file), cx))
                    .child(
                        Button::new("storage-delete-file")
                            .small()
                            .label(t("storage.deleteFile", locale))
                            .on_click(cx.listener(|this, _, _, cx| this.delete_file(cx))),
                    )
                    .child(field(
                        t("storage.entityId", locale),
                        Input::new(&self.entity_id),
                        cx,
                    ))
                    .child(
                        Button::new("storage-delete-entity")
                            .small()
                            .label(t("storage.deleteEntity", locale))
                            .on_click(cx.listener(|this, _, _, cx| this.delete_entity(cx))),
                    ),
            )
            .child(
                h_flex()
                    .gap_3()
                    .items_end()
                    .child(field(
                        t("storage.batchFiles", locale),
                        Input::new(&self.batch_files),
                        cx,
                    ))
                    .child(field(
                        t("storage.batchEntities", locale),
                        Input::new(&self.batch_entities),
                        cx,
                    ))
                    .child(
                        Button::new("storage-batch-delete")
                            .small()
                            .label(t("storage.batchDelete", locale))
                            .on_click(cx.listener(|this, _, _, cx| this.batch_delete(cx))),
                    ),
            );

        let outcome_row = self
            .outcome
            .map(|(positive, key)| notice(t(key, locale), positive, cx));

        page_body("storage")
            .child(panel_header(
                t("storage.title", locale),
                Some(SharedString::from(t("storage.subtitle", locale))),
                Some(header_actions.into_any_element()),
                cx,
            ))
            .child(
                stack()
                    .children(outcome_row)
                    .child(self.render_status(cx))
                    .child(self.render_stats(cx))
                    .child(clear_group)
                    .child(single_group)
                    .child(notice(t("storage.warningHint", locale), false, cx)),
            )
    }
}
