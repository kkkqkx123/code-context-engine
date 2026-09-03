//! Unified query filter for version-aware queries
//!
//! This module provides a centralized mechanism for filtering queries based on
//! the *epoch view* of the published generation, ensuring consistency across
//! all query endpoints (Qdrant, BM25, SQLite).
//!
//! # Design
//!
//! A published generation under the inheritance model owns its data through a
//! parent chain: the visible data of a generation is
//!
//! ```text
//! visible = own rows ∪ parent rows − overridden files
//! ```
//!
//! [`QueryFilter`] carries this complete view (`own_epoch`, optional
//! `parent_epoch`, and the files whose parent-generation rows are hidden).
//! All queries must pass through it to ensure:
//! 1. Version isolation - queries see exactly the published generation view
//! 2. Consistency - all storage backends resolve the same view
//! 3. Maintainability - single point to update epoch filtering logic
//!
//! The view is derived from the active manifest on every query (two cheap
//! indexed lookups); no process-local cache is kept.
//!
//! # Integration
//!
//! - Searcher: derives the view once per request and passes it to strategies
//! - BM25 strategy: boolean epoch combination in the Tantivy index
//! - Qdrant strategy: should/must_not combination in the payload filter
//! - SQLite queries: two-stage "own first, miss → parent" resolution

use rusqlite::Connection;

use cce_storage_common::SearchFilter;
use cce_storage_sqlite::{GenerationOverrideRepository, ProjectIndexManifestRepository};

use crate::query::error::{QueryError, Result};

/// Error type for query filter operations
#[derive(Debug, thiserror::Error)]
pub enum FilterError {
    #[error("Invalid epoch: {0}")]
    InvalidEpoch(String),
}

/// Unified epoch-view filter for version-aware queries.
#[derive(Debug, Clone)]
pub struct QueryFilter {
    /// Data epoch written by the active generation itself.
    own_epoch: i64,
    /// Generation inherited from (its rows stay visible unless overridden).
    parent_epoch: Option<i64>,
    /// Files registered in `generation_overrides` for the own generation;
    /// their parent-generation rows are invisible (replaced) or hidden
    /// everywhere (deleted).
    excluded_files: Vec<String>,
}

impl QueryFilter {
    /// Create a full-generation view (no parent inheritance).
    ///
    /// # Errors
    /// Returns error if epoch is negative.
    pub fn new(own_epoch: i64) -> std::result::Result<Self, FilterError> {
        Self::inherited(own_epoch, None, Vec::new())
    }

    /// Create an epoch view from a manifest-derived inheritance chain.
    ///
    /// # Arguments
    /// * `own_epoch` - Data epoch of the active generation (must be >= 0)
    /// * `parent_epoch` - Inherited generation, when the generation is not full
    /// * `excluded_files` - Files whose parent rows must not be resolved
    ///
    /// # Errors
    /// Returns error if any epoch is negative or the parent is not older than
    /// the own epoch.
    pub fn inherited(
        own_epoch: i64,
        parent_epoch: Option<i64>,
        excluded_files: Vec<String>,
    ) -> std::result::Result<Self, FilterError> {
        if own_epoch < 0 {
            return Err(FilterError::InvalidEpoch(format!(
                "epoch must be non-negative, got {}",
                own_epoch
            )));
        }
        if let Some(parent) = parent_epoch {
            if parent < 0 {
                return Err(FilterError::InvalidEpoch(format!(
                    "parent epoch must be non-negative, got {}",
                    parent
                )));
            }
            if parent >= own_epoch {
                return Err(FilterError::InvalidEpoch(format!(
                    "parent epoch {} must be older than own epoch {}",
                    parent, own_epoch
                )));
            }
        }
        Ok(Self {
            own_epoch,
            parent_epoch,
            excluded_files,
        })
    }

    /// Own data epoch of the active generation (single-value cache keys etc.).
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
    /// generations). External filters match against these values.
    pub fn epochs(&self) -> Vec<i64> {
        let mut epochs = self
            .parent_epoch
            .map_or_else(Vec::new, |parent| vec![parent]);
        epochs.push(self.own_epoch);
        epochs
    }

    /// Create a SearchFilter carrying the complete epoch view for vector
    /// retrieval.
    pub fn to_search_filter(&self) -> SearchFilter {
        SearchFilter {
            epochs: self.epochs(),
            excluded_files: if self.excluded_files.is_empty() {
                None
            } else {
                Some(self.excluded_files.clone())
            },
            ..Default::default()
        }
    }

    /// Merge this epoch view into an existing SearchFilter.
    ///
    /// Any previously carried epoch view is overwritten.
    pub fn apply_to_search_filter(&self, mut filter: SearchFilter) -> SearchFilter {
        filter.epochs = self.epochs();
        filter.excluded_files = if self.excluded_files.is_empty() {
            None
        } else {
            Some(self.excluded_files.clone())
        };
        filter
    }
}

impl Default for QueryFilter {
    /// Default view uses epoch 0 (the initial full generation).
    fn default() -> Self {
        Self {
            own_epoch: 0,
            parent_epoch: None,
            excluded_files: Vec::new(),
        }
    }
}

/// Derive the [`QueryFilter`] of the active publication from SQLite.
///
/// Reads the active manifest together with its inheritance link and the
/// generation overrides of its own epoch. Projects never published through a
/// manifest fall back to the legacy `project_meta.active_epoch` key with a
/// full-generation view.
///
/// The view is recomputed on every call by design: readers must trust the
/// manifest, and keeping no process-local cache guarantees an adoption or
/// rollback becomes visible to the very next query.
pub(crate) fn load_active_query_filter(conn: &Connection, project_id: i64) -> Result<QueryFilter> {
    if let Some(manifest) =
        ProjectIndexManifestRepository::get_active(conn, project_id).map_err(|error| {
            QueryError::storage(&format!("Failed to read active index manifest: {error}"))
        })?
    {
        let excluded_files = GenerationOverrideRepository::list_for_generation(
            conn,
            project_id,
            manifest.data_epoch,
        )
        .map_err(|error| {
            QueryError::storage(&format!("Failed to read generation overrides: {error}"))
        })?
        .into_iter()
        .map(|override_entry| override_entry.file_path)
        .collect();
        return QueryFilter::inherited(
            manifest.data_epoch,
            manifest.parent_data_epoch,
            excluded_files,
        )
        .map_err(|error| QueryError::config(&format!("Invalid epoch view: {error}")));
    }
    let epoch = read_legacy_active_epoch(conn, project_id)?;
    QueryFilter::new(epoch).map_err(|error| QueryError::config(&format!("Invalid epoch: {error}")))
}

/// Read the legacy `project_meta.active_epoch` fallback.
///
/// Only a missing row is treated as the default epoch 0 (the meta key was
/// never written). Missing table, locked DB, I/O or unparseable stored values
/// are real failures and are propagated instead of silently downgrading to
/// epoch 0 — otherwise every storage backend would quietly query stale data.
pub(crate) fn read_legacy_active_epoch(conn: &Connection, project_id: i64) -> Result<i64> {
    match conn.query_row(
        "SELECT value FROM project_meta
         WHERE project_id = ?1 AND key = 'active_epoch'",
        rusqlite::params![project_id],
        |row| row.get::<_, String>(0),
    ) {
        Ok(value) => value.parse().map_err(|_| {
            QueryError::storage(&format!(
                "project_meta active_epoch for project {project_id} is not a valid integer: {value}"
            ))
        }),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0),
        Err(e) => Err(QueryError::storage(&format!(
            "Failed to read active_epoch for project {project_id}: {e}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_filter_creation() {
        let filter = QueryFilter::new(42).expect("Failed to create filter");
        assert_eq!(filter.epoch_value(), 42);
        assert_eq!(filter.parent_epoch(), None);
        assert!(filter.excluded_files().is_empty());
    }

    #[test]
    fn test_query_filter_creation_invalid() {
        assert!(QueryFilter::new(-1).is_err());
    }

    #[test]
    fn test_inherited_view_validation() {
        // Parent must be strictly older than the own epoch.
        assert!(QueryFilter::inherited(5, Some(5), Vec::new()).is_err());
        assert!(QueryFilter::inherited(5, Some(6), Vec::new()).is_err());
        assert!(QueryFilter::inherited(5, Some(-1), Vec::new()).is_err());
        let view = QueryFilter::inherited(5, Some(4), vec!["src/a.rs".to_string()])
            .expect("valid inherited view");
        assert_eq!(view.parent_epoch(), Some(4));
        assert_eq!(view.excluded_files(), ["src/a.rs"]);
    }

    #[test]
    fn test_default_filter() {
        let filter = QueryFilter::default();
        assert_eq!(filter.epoch_value(), 0);
        assert_eq!(filter.parent_epoch(), None);
    }

    #[test]
    fn test_epochs_listing_is_ascending() {
        let full = QueryFilter::new(7).expect("full view");
        assert_eq!(full.epochs(), vec![7]);

        let inherited = QueryFilter::inherited(7, Some(6), Vec::new()).expect("inherited view");
        assert_eq!(inherited.epochs(), vec![6, 7]);
    }

    #[test]
    fn test_to_search_filter() {
        let full = QueryFilter::new(10).expect("full view");
        let search_filter = full.to_search_filter();
        assert_eq!(search_filter.epochs, vec![10]);
        assert_eq!(search_filter.excluded_files, None);

        let inherited =
            QueryFilter::inherited(10, Some(9), vec!["a.rs".to_string()]).expect("view");
        let search_filter = inherited.to_search_filter();
        assert_eq!(search_filter.epochs, vec![9, 10]);
        assert_eq!(search_filter.excluded_files, Some(vec!["a.rs".to_string()]));
    }

    #[test]
    fn test_apply_to_search_filter() {
        let filter = QueryFilter::inherited(5, Some(4), Vec::new()).expect("view");
        let existing_filter = SearchFilter {
            group_id: Some("group1".to_string()),
            ..Default::default()
        };
        let result = filter.apply_to_search_filter(existing_filter);
        assert_eq!(result.epochs, vec![4, 5]);
        assert_eq!(result.group_id, Some("group1".to_string()));
    }

    #[test]
    fn test_multiple_filters_with_epochs() {
        for value in [0, 1, 100] {
            let filter = QueryFilter::new(value).expect("Failed to create filter");
            assert_eq!(filter.epoch_value(), value);
            assert_eq!(filter.epochs(), vec![value]);
        }
    }
}
