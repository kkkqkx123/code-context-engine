//! Version-aware epoch view for tool queries.
//!
//! A published generation owns its data through a parent chain; the visible
//! data of a generation is `own rows ∪ parent rows − overridden files`.
//! [`EpochView`] carries this complete view so every tool resolves the same
//! versioned data without depending on the orchestration layer.

use cce_storage_common::MetadataStorage;
use cce_storage_metadb_sqlite::SqliteClient;
use std::sync::Arc;

use crate::SymbolLookupError;

/// Epoch view of the active publication: the own data epoch, the inherited
/// parent epoch (when the generation is not full), and the files whose
/// parent-generation rows must stay hidden.
#[derive(Debug, Clone, Default)]
pub struct EpochView {
    own_epoch: i64,
    parent_epoch: Option<i64>,
    excluded_files: Vec<String>,
}

impl EpochView {
    /// Create a full-generation view (no parent inheritance).
    pub fn new(own_epoch: i64) -> Result<Self, SymbolLookupError> {
        Self::inherited(own_epoch, None, Vec::new())
    }

    /// Create an epoch view from a manifest-derived inheritance chain.
    pub fn inherited(
        own_epoch: i64,
        parent_epoch: Option<i64>,
        excluded_files: Vec<String>,
    ) -> Result<Self, SymbolLookupError> {
        if own_epoch < 0 {
            return Err(SymbolLookupError::Internal(format!(
                "epoch must be non-negative, got {own_epoch}"
            )));
        }
        if let Some(parent) = parent_epoch {
            if parent < 0 {
                return Err(SymbolLookupError::Internal(format!(
                    "parent epoch must be non-negative, got {parent}"
                )));
            }
            if parent >= own_epoch {
                return Err(SymbolLookupError::Internal(format!(
                    "parent epoch {parent} must be older than own epoch {own_epoch}"
                )));
            }
        }
        Ok(Self {
            own_epoch,
            parent_epoch,
            excluded_files,
        })
    }

    /// Own data epoch of the active generation.
    pub fn epoch_value(&self) -> i64 {
        self.own_epoch
    }

    /// The inherited generation, when the view is not a full generation.
    pub fn parent_epoch(&self) -> Option<i64> {
        self.parent_epoch
    }

    /// Files whose parent-generation rows are excluded from the view.
    pub fn excluded_files(&self) -> &[String] {
        &self.excluded_files
    }

    /// All visible generations, ascending (`[parent, own]`; `[own]` for full
    /// generations).
    pub fn epochs(&self) -> Vec<i64> {
        let mut epochs = self
            .parent_epoch
            .map_or_else(Vec::new, |parent| vec![parent]);
        epochs.push(self.own_epoch);
        epochs
    }
}

/// Derive the [`EpochView`] of the active publication through the
/// backend-neutral relation contract.
///
/// Reads the active manifest together with its inheritance link and the
/// generation overrides of its own epoch. Projects never published through a
/// manifest fall back to the legacy `project_meta.active_epoch` key with a
/// full-generation view; a missing row is the legitimate default 0, while an
/// unparseable stored value is propagated instead of silently downgrading.
pub async fn load_epoch_view(
    store: &Arc<SqliteClient>,
    project_id: i64,
) -> Result<EpochView, SymbolLookupError> {
    if let Some(manifest) = store.manifest_active(project_id).await.map_err(|error| {
        SymbolLookupError::Internal(format!("Failed to read active index manifest: {error}"))
    })? {
        let excluded_files = store
            .overrides_for_generation(project_id, manifest.data_epoch)
            .await
            .map_err(|error| {
                SymbolLookupError::Internal(format!("Failed to read generation overrides: {error}"))
            })?
            .into_iter()
            .map(|override_entry| override_entry.file_path)
            .collect();
        return EpochView::inherited(
            manifest.data_epoch,
            manifest.parent_data_epoch,
            excluded_files,
        );
    }
    let epoch =
        MetadataStorage::project_meta_get_int_optional(store.as_ref(), project_id, "active_epoch")
            .await
            .map_err(|error| {
                SymbolLookupError::Internal(format!(
                    "Failed to read active_epoch for project {project_id}: {error}"
                ))
            })?
            .unwrap_or(0);
    EpochView::new(epoch)
}
