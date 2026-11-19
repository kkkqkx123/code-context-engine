//! Storage panel: backend status, index statistics, removal of indexed
//! content and the managed vector backend's lifecycle.
//!
//! Every removal path here is destructive, so each one is armed first: the
//! browser turns into a confirm/cancel pair carrying the exact scope about to
//! be dropped. The managed backend's stop/restart actions are armed the same
//! way, because they interrupt every query in flight.

use cce_api::models::{IndexStatsResponse, QdrantProcessStatusResponse, StorageStatusResponse};
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
    Cell, Column, Confirm, Tone, actions, confirm_bar, data_grid, failure_block, field,
    field_group, hint, loading_row, notice, page_body, panel_header, stack, stat_line, surface,
    toast,
};
use crate::connection::ClientStore;
use crate::i18n::{locale, t};
use crate::services::refresh;
use crate::services::request::{self, Load};
use crate::services::storage_service::{self, ClearScope, QdrantAction};
use crate::state::AppState;
use crate::views::PanelKind;

/// Confirmation slots; one per destructive action in this panel.
const CONFIRM_CLEAR: usize = 0;
const CONFIRM_DELETE_FILE: usize = 1;
const CONFIRM_DELETE_ENTITY: usize = 2;
const CONFIRM_BATCH_DELETE: usize = 3;
const CONFIRM_QDRANT_STOP: usize = 4;
const CONFIRM_QDRANT_RESTART: usize = 5;

pub struct StorageView {
    file: Entity<InputState>,
    entity_id: Entity<InputState>,
    batch_files: Entity<InputState>,
    batch_entities: Entity<InputState>,
    status: Load<StorageStatusResponse>,
    stats: Load<IndexStatsResponse>,
    qdrant: Load<QdrantProcessStatusResponse>,
    scope: ClearScope,
    confirm: Confirm,
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
            qdrant: Load::idle(),
            confirm: Confirm::none(),
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
                candidates: Vec::new(),
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

    fn load_qdrant(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.qdrant = Load::Failed(request::Failure::disconnected());
            cx.notify();
            return;
        };
        self.qdrant.begin();
        cx.notify();
        let task = storage_service::qdrant_status(cx, client);
        request::spawn_into(cx, task, |this, outcome, cx| {
            this.qdrant = Load::settle(outcome);
            cx.notify();
        });
    }

    /// Re-read the snapshots that are already on screen.
    pub fn replay(&mut self, cx: &mut Context<Self>) {
        refresh::touch(PanelKind::Storage, cx);
        if !matches!(self.status, Load::Idle) {
            self.load_status(cx);
        }
        if !matches!(self.stats, Load::Idle) {
            self.load_stats(cx);
        }
        if !matches!(self.qdrant, Load::Idle) {
            self.load_qdrant(cx);
        }
    }

    fn qdrant(&mut self, action: QdrantAction, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.outcome = Some((false, "error.notConnected"));
            cx.notify();
            return;
        };
        self.outcome = None;
        cx.notify();
        let task = storage_service::qdrant_action(cx, client, action);
        request::spawn_into(cx, task, move |this, outcome, cx| {
            match outcome {
                Ok(response) => {
                    this.outcome = Some((true, action.key()));
                    log::info!("qdrant {}: {}", action.key(), response.message);
                    toast::push(
                        SharedString::from(response.message),
                        response.success,
                        &mut *cx,
                    );
                    this.load_qdrant(cx);
                }
                Err(err) => {
                    log::warn!("qdrant {} failed: {err}", action.key());
                    this.outcome = Some((false, "storage.qdrantFailed"));
                    this.load_qdrant(cx);
                }
            }
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
                    toast::push_key("storage.cleared", true, cx);
                    log::info!(
                        "cleared storage for project {project_id}: {}",
                        response.message
                    );
                    this.load_stats(cx);
                }
                Err(err) => {
                    log::warn!("clearing storage failed: {err}");
                    this.outcome = Some((false, "storage.clearFailed"));
                    toast::push_key("storage.clearFailed", false, cx);
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
            let result = match outcome {
                Ok(_) => (true, "storage.deleted"),
                Err(_) => (false, "storage.deleteFailed"),
            };
            this.outcome = Some(result);
            toast::push_key(result.1, result.0, cx);
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
            let result = match outcome {
                Ok(_) => (true, "storage.deleted"),
                Err(_) => (false, "storage.deleteFailed"),
            };
            this.outcome = Some(result);
            toast::push_key(result.1, result.0, cx);
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
            let result = match outcome {
                Ok(_) => (true, "storage.deleted"),
                Err(_) => (false, "storage.deleteFailed"),
            };
            this.outcome = Some(result);
            toast::push_key(result.1, result.0, cx);
            cx.notify();
        });
    }

    /// Managed vector backend: state plus the three lifecycle actions.
    ///
    /// Status is polled while the panel is open, so a backend that crashed on
    /// its own shows up without a manual refresh.
    fn render_qdrant(&self, cx: &mut Context<Self>) -> AnyElement {
        let locale = locale(cx);
        let dense = AppState::global(cx).read(cx).density.is_dense();
        let body = match &self.qdrant {
            Load::Idle => hint(t("storage.qdrantIdle", locale), cx).into_any_element(),
            Load::Loading => loading_row(t("storage.qdrantLoading", locale), cx).into_any_element(),
            Load::Failed(failure) => failure_block(failure, locale, cx).into_any_element(),
            Load::Ready(response) => surface(dense, cx)
                .child(stat_line(
                    t("storage.qdrantState", locale),
                    response.status.to_string(),
                    cx,
                ))
                .child(stat_line(
                    t("storage.qdrantManagedLabel", locale),
                    match response.managed {
                        true => t("storage.qdrantManaged", locale).to_string(),
                        false => t("storage.qdrantExternal", locale).to_string(),
                    },
                    cx,
                ))
                .into_any_element(),
        };

        field_group(cx)
            .child(hint(t("storage.qdrantHint", locale), cx))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("storage-qdrant-refresh")
                            .small()
                            .label(t("storage.qdrantRefresh", locale))
                            .on_click(cx.listener(|this, _, _, cx| this.load_qdrant(cx))),
                    )
                    .child(
                        Button::new("storage-qdrant-start")
                            .small()
                            .label(t("storage.qdrantStart", locale))
                            .on_click(
                                cx.listener(|this, _, _, cx| this.qdrant(QdrantAction::Start, cx)),
                            ),
                    )
                    .child(
                        Button::new("storage-qdrant-stop")
                            .small()
                            .selected(self.confirm.is_armed(CONFIRM_QDRANT_STOP))
                            .label(t("storage.qdrantStop", locale))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.confirm.request(CONFIRM_QDRANT_STOP) {
                                    this.qdrant(QdrantAction::Stop, cx);
                                } else {
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        Button::new("storage-qdrant-restart")
                            .small()
                            .label(t("storage.qdrantRestart", locale))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.confirm.request(CONFIRM_QDRANT_RESTART) {
                                    this.qdrant(QdrantAction::Restart, cx);
                                } else {
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .child(body)
            .into_any_element()
    }

    fn render_status(&self, cx: &mut Context<Self>) -> AnyElement {
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
                    .child(data_grid(
                        PanelKind::Storage,
                        &columns,
                        &rows,
                        dense,
                        &mut *cx,
                    ))
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
                    .selected(self.confirm.is_armed(CONFIRM_CLEAR))
                    .label(t("storage.clear", locale))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.confirm.request(CONFIRM_CLEAR) {
                            this.clear(cx);
                        } else {
                            cx.notify();
                        }
                    })),
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
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.confirm.request(CONFIRM_DELETE_FILE) {
                                    this.delete_file(cx);
                                } else {
                                    cx.notify();
                                }
                            })),
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
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.confirm.request(CONFIRM_DELETE_ENTITY) {
                                    this.delete_entity(cx);
                                } else {
                                    cx.notify();
                                }
                            })),
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
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.confirm.request(CONFIRM_BATCH_DELETE) {
                                    this.batch_delete(cx);
                                } else {
                                    cx.notify();
                                }
                            })),
                    ),
            );

        let confirm_block = match self.confirm.armed() {
            Some(CONFIRM_CLEAR) => Some(
                confirm_bar(
                    "confirm-clear",
                    SharedString::from(t("storage.confirmClear", locale)),
                    locale,
                    cx,
                    cx.listener(|this, _, _, cx| {
                        this.confirm.cancel();
                        this.clear(cx);
                    }),
                    cx.listener(|this, _, _, cx| {
                        this.confirm.cancel();
                        cx.notify();
                    }),
                )
                .into_any_element(),
            ),
            Some(CONFIRM_DELETE_FILE) => Some(
                confirm_bar(
                    "confirm-delete-file",
                    SharedString::from(format!(
                        "{}: {}",
                        t("storage.confirmDeleteFile", locale),
                        self.file.read(cx).value()
                    )),
                    locale,
                    cx,
                    cx.listener(|this, _, _, cx| {
                        this.confirm.cancel();
                        this.delete_file(cx);
                    }),
                    cx.listener(|this, _, _, cx| {
                        this.confirm.cancel();
                        cx.notify();
                    }),
                )
                .into_any_element(),
            ),
            Some(CONFIRM_DELETE_ENTITY) => Some(
                confirm_bar(
                    "confirm-delete-entity",
                    SharedString::from(format!(
                        "{}: {}",
                        t("storage.confirmDeleteEntity", locale),
                        self.entity_id.read(cx).value()
                    )),
                    locale,
                    cx,
                    cx.listener(|this, _, _, cx| {
                        this.confirm.cancel();
                        this.delete_entity(cx);
                    }),
                    cx.listener(|this, _, _, cx| {
                        this.confirm.cancel();
                        cx.notify();
                    }),
                )
                .into_any_element(),
            ),
            Some(CONFIRM_BATCH_DELETE) => Some(
                confirm_bar(
                    "confirm-batch-delete",
                    SharedString::from(t("storage.confirmBatchDelete", locale)),
                    locale,
                    cx,
                    cx.listener(|this, _, _, cx| {
                        this.confirm.cancel();
                        this.batch_delete(cx);
                    }),
                    cx.listener(|this, _, _, cx| {
                        this.confirm.cancel();
                        cx.notify();
                    }),
                )
                .into_any_element(),
            ),
            Some(CONFIRM_QDRANT_STOP) => Some(
                confirm_bar(
                    "confirm-qdrant-stop",
                    SharedString::from(t("storage.confirmQdrantStop", locale)),
                    locale,
                    cx,
                    cx.listener(|this, _, _, cx| {
                        this.confirm.cancel();
                        this.qdrant(QdrantAction::Stop, cx);
                    }),
                    cx.listener(|this, _, _, cx| {
                        this.confirm.cancel();
                        cx.notify();
                    }),
                )
                .into_any_element(),
            ),
            Some(CONFIRM_QDRANT_RESTART) => Some(
                confirm_bar(
                    "confirm-qdrant-restart",
                    SharedString::from(t("storage.confirmQdrantRestart", locale)),
                    locale,
                    cx,
                    cx.listener(|this, _, _, cx| {
                        this.confirm.cancel();
                        this.qdrant(QdrantAction::Restart, cx);
                    }),
                    cx.listener(|this, _, _, cx| {
                        this.confirm.cancel();
                        cx.notify();
                    }),
                )
                .into_any_element(),
            ),
            _ => None,
        };

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
                    .children(confirm_block)
                    .child(self.render_qdrant(cx))
                    .child(clear_group)
                    .child(single_group)
                    .child(notice(t("storage.warningHint", locale), false, cx)),
            )
    }
}
