//! Server-side publisher for complete relation snapshots.

use std::sync::Arc;

use async_trait::async_trait;
use cce_codegraph::index::RelationIndexView;
use cce_codegraph::index::core::RelationIndex;
use cce_codegraph::index::snapshot_index::{LayeredSnapshotIndex, RelationSnapshotIndex};
use cce_codegraph::index::snapshot_loader::RelationSnapshotLoader;
use cce_orchestrator::index::{
    RelationPublication, RelationSnapshotPublisher, ResolutionPipelineService,
};
use cce_storage_metadb_sqlite::SqliteClient;
use cce_storage_metadb_sqlite::repo::{ProjectIndexManifestRepository, RelationSnapshotRepository};
use cce_storage_metadb_sqlite::snapshot_store::SqliteSnapshotStore;
use cce_types::{CanonicalRelationSnapshot, SnapshotDelta, StorageError};
use rusqlite::OptionalExtension;
use tokio::sync::Mutex;

use super::{RelationRuntime, SnapshotIntegrity};

/// Configuration for delta chain compaction.
pub struct CompactionConfig {
    /// Maximum number of deltas in a chain before compaction is triggered.
    pub max_chain_length: usize,
    /// Maximum cumulative delta size as a fraction of base snapshot size.
    /// Compaction is triggered when `cumulative_delta_size > max_delta_ratio * base_size`.
    pub max_delta_ratio: f64,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            max_chain_length: 10,
            max_delta_ratio: 0.2,
        }
    }
}

/// Publishes SQLite's canonical relation epoch and its process-local runtime
/// projection as one serialized operation.
pub struct ServerRelationSnapshotPublisher {
    sqlite: SqliteClient,
    runtime: Arc<RelationRuntime>,
    writer: ResolutionPipelineService,
    publish_lock: Arc<Mutex<()>>,
}

impl ServerRelationSnapshotPublisher {
    pub fn new(sqlite: SqliteClient, runtime: Arc<RelationRuntime>) -> Self {
        Self {
            writer: ResolutionPipelineService::new_local(sqlite.clone()),
            sqlite,
            publish_lock: runtime.publication_lock(),
            runtime,
        }
    }

    fn integrity(snapshot: &CanonicalRelationSnapshot) -> SnapshotIntegrity {
        if snapshot.files.is_empty()
            && snapshot.entities.is_empty()
            && snapshot.relations.is_empty()
            && snapshot.dependencies.is_empty()
        {
            SnapshotIntegrity::Empty
        } else {
            SnapshotIntegrity::Full
        }
    }

    /// Scoped validation for an incremental delta against its (already
    /// validated) base. Cost is bounded by the delta's own surface instead of
    /// the project size, covering every failure class a delta can introduce:
    ///
    /// 1. Added relations: the caller and any internal target must resolve to
    ///    an entity of the base or of the delta's added entities (no dangling
    ///    internal targets).
    /// 2. Removed entities: every surviving edge pointing at a removed entity
    ///    must be scheduled for removal in `removed_relations`, so the merged
    ///    graph has no dangling references. `compute_delta`'s reverse-index
    ///    pass guarantees this contract; this check pins it.
    ///
    /// The base itself is trusted (it was validated when it was published), so
    /// the untouched portion of the graph is not re-walked. `base` may be the
    /// concrete materialized index or the layered view of the cached state at
    /// `delta.base_epoch` (base + accumulated deltas); both expose the same
    /// entity/relation surface through [`RelationIndexView`].
    fn validate_delta_scoped<V: RelationIndexView>(
        base: &V,
        delta: &SnapshotDelta,
    ) -> Result<(), String> {
        use std::collections::HashSet;

        let added_ids: HashSet<cce_types::EntityId> = delta
            .added_entities
            .iter()
            .map(|added| added.entity.id)
            .collect();

        for relation in &delta.added_relations {
            if !base.function_contains(relation.caller) && !added_ids.contains(&relation.caller) {
                return Err(format!(
                    "delta added relation has unknown caller {}",
                    relation.caller.0
                ));
            }
            if let Some(callee_id) = relation.callee_id
                && !base.function_contains(callee_id)
                && !added_ids.contains(&callee_id)
            {
                return Err(format!(
                    "delta added relation references missing internal target {}",
                    callee_id.0
                ));
            }
        }

        let removed_set: HashSet<cce_types::EntityId> =
            delta.removed_entities.iter().copied().collect();
        let scheduled: HashSet<(cce_types::EntityId, cce_types::EntityId)> = delta
            .removed_relations
            .iter()
            .filter_map(|relation| relation.callee_id.map(|callee| (relation.caller, callee)))
            .collect();
        for removed_id in &delta.removed_entities {
            for relation in Self::relations_to_entity(base, *removed_id) {
                if removed_set.contains(&relation.caller)
                    || scheduled.contains(&(relation.caller, *removed_id))
                {
                    continue;
                }
                return Err(format!(
                    "removed entity {} is still referenced by caller {} without a scheduled removal",
                    removed_id.0, relation.caller.0
                ));
            }
        }

        Ok(())
    }

    /// Resolved relations targeting `callee` through the view's merged state
    /// (reverse-lookup equivalent of `RelationQueryOps::get_relations_to_entity`
    /// over a [`RelationIndexView`]).
    fn relations_to_entity<V: RelationIndexView>(
        base: &V,
        callee: cce_types::EntityId,
    ) -> Vec<cce_types::ResolvedRelation> {
        let mut result = Vec::new();
        for caller in base.callers_of(callee) {
            if let Some(relations) = base.relations_of(caller) {
                for relation in relations {
                    if relation.callee_id == Some(callee) {
                        result.push(relation);
                    }
                }
            }
        }
        result
    }

    fn mark_candidate_failed(&self, project_id: i64, epoch: i64, error: &StorageError) {
        if let Err(mark_error) = self
            .writer
            .mark_failed(project_id, epoch, &error.to_string())
        {
            tracing::warn!(
                project_id,
                epoch,
                error = %mark_error,
                "Failed to mark relation publication candidate as failed"
            );
        }
    }

    pub(crate) fn has_project_publication_candidate(
        &self,
        project_id: i64,
    ) -> Result<bool, StorageError> {
        let conn = self.sqlite.read_connection()?;
        let exists: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM project_index_manifests
                WHERE project_id = ?1 AND state = 'building'
                 LIMIT 1",
                rusqlite::params![project_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| StorageError::Query(error.to_string()))?;
        Ok(exists.is_some())
    }

    /// Resolve the current data generation epoch, preferring the durable
    /// project manifest over the legacy `active_epoch` meta.
    ///
    /// A missing manifest with a missing legacy meta row is the legitimate
    /// default 0 (never published); real DB failures are propagated instead
    /// of being silently downgraded.
    fn active_data_epoch(&self, project_id: i64) -> Result<i64, StorageError> {
        let manifest = self
            .sqlite
            .with_transaction(|tx| ProjectIndexManifestRepository::get_active(tx, project_id))?;
        match manifest {
            Some(manifest) => Ok(manifest.data_epoch),
            None => self
                .sqlite
                .project_meta_get_int_optional(project_id, "active_epoch")
                .map(|value| value.unwrap_or(0)),
        }
    }

    /// Check whether the delta chain for a project exceeds compaction thresholds.
    pub fn needs_compaction(
        &self,
        project_id: i64,
        config: &CompactionConfig,
    ) -> Result<bool, StorageError> {
        if !config.max_delta_ratio.is_finite() || config.max_delta_ratio < 0.0 {
            return Err(StorageError::Validation(
                "max_delta_ratio must be finite and non-negative".to_string(),
            ));
        }
        let active_epoch = self
            .sqlite
            .project_meta_get_int(project_id, "active_relation_epoch")
            .map_err(|error| StorageError::Query(error.to_string()))?;
        if active_epoch <= 0 {
            return Ok(false);
        }
        let conn = self.sqlite.read_connection()?;
        let (chain_len, cumul_size, base_size) =
            RelationSnapshotRepository::get_delta_chain_info(&conn, project_id, active_epoch)?;
        drop(conn);

        if chain_len == 0 {
            return Ok(false);
        }
        if chain_len >= config.max_chain_length {
            return Ok(true);
        }
        Ok((cumul_size as f64 / base_size as f64) > config.max_delta_ratio)
    }

    /// Perform a full compaction: merge all deltas into a new base snapshot.
    ///
    /// This loads the base + all deltas, produces a fresh full snapshot,
    /// writes it as a new Active epoch, and GCs the old delta chain.
    pub async fn compact_project(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<RelationPublication, StorageError> {
        let _guard = self.publish_lock.lock().await;

        let active_epoch = self
            .sqlite
            .project_meta_get_int(project_id, "active_relation_epoch")
            .map_err(|error| {
                StorageError::Validation(format!("cannot read active epoch: {error}"))
            })?;
        if active_epoch <= 0 {
            return Err(StorageError::Validation(
                "no active epoch to compact".to_string(),
            ));
        }

        self.runtime.set_updating().await;

        // Load current full state (base + deltas) from SQLite
        let full_index = match RelationSnapshotLoader::load(
            &SqliteSnapshotStore::new(self.sqlite.clone()),
            project_id,
            active_epoch,
        ) {
            Ok(index) => index,
            Err(error) => {
                self.runtime.report_failure(error.to_string()).await;
                return Err(error);
            }
        };

        // Export as full canonical snapshot
        let config_fingerprint = {
            let manifest = self
                .sqlite
                .with_transaction(|tx| {
                    RelationSnapshotRepository::get_manifest(tx, project_id, active_epoch)
                })
                .map_err(|error| {
                    StorageError::Validation(format!("cannot read manifest: {error}"))
                })?
                .ok_or_else(|| {
                    StorageError::Validation("active epoch manifest missing".to_string())
                })?;
            manifest.config_fingerprint
        };

        let snapshot = match full_index.to_canonical_snapshot(config_fingerprint) {
            Ok(s) => s,
            Err(error) => {
                self.runtime.report_failure(error.clone()).await;
                return Err(StorageError::Validation(error));
            }
        };
        // The merged snapshot is a full base; `base_relation_epoch` is only
        // meaningful for `publish()`'s CAS check and is not persisted, so
        // leaving it unset keeps the written fingerprint identical to the
        // one recomputed on load.

        // Publish as a full snapshot (allocates new epoch, writes full tables)
        let integrity = Self::integrity(&snapshot);
        let epoch = match self
            .writer
            .allocate_and_write(project_id, operation_id, &snapshot)
        {
            Ok(epoch) => epoch,
            Err(error) => {
                self.runtime.report_failure(error.to_string()).await;
                return Err(error);
            }
        };

        let data_epoch = self.active_data_epoch(project_id)?;
        if let Err(error) = self.sqlite.with_transaction(|tx| {
            ProjectIndexManifestRepository::activate(
                tx,
                project_id,
                data_epoch,
                epoch,
                operation_id,
                None,
            )
            .map(|_| ())
        }) {
            self.mark_candidate_failed(project_id, epoch, &error);
            self.runtime.report_failure(error.to_string()).await;
            return Err(error);
        }

        // GC old delta chain
        let _gc_count = self
            .sqlite
            .with_transaction(|tx| {
                RelationSnapshotRepository::delete_manifests_except(tx, project_id, epoch)
            })
            .unwrap_or(0);

        // Publish to runtime as base-only snapshot (no delta). The merged
        // `full_index` is local to this compaction and dropped afterwards, so
        // the snapshot can share its maps zero-copy
        let snapshot_index = Arc::new(RelationSnapshotIndex::from_index_shared(&full_index));
        self.runtime
            .publish_snapshot(
                Arc::clone(&snapshot_index),
                epoch,
                integrity,
                Some(format!("{operation_id}-compaction")),
            )
            .await;

        tracing::info!(
            project_id,
            epoch,
            "Compaction complete: delta chain merged into new base snapshot"
        );

        Ok(RelationPublication {
            relation_epoch: epoch,
            snapshot: Arc::new(LayeredSnapshotIndex::new(snapshot_index)),
        })
    }
}

#[async_trait]
impl RelationSnapshotPublisher for ServerRelationSnapshotPublisher {
    async fn publish(
        &self,
        project_id: i64,
        operation_id: &str,
        snapshot: CanonicalRelationSnapshot,
        index: &RelationIndex,
    ) -> Result<RelationPublication, StorageError> {
        let _guard = self.publish_lock.lock().await;

        // Compare-and-swap: if a base_epoch was supplied, verify the active
        // epoch has not advanced past it. This prevents concurrent hot-update
        // candidates built from the same baseline from silently overwriting
        // each other.
        if let Some(base_epoch) = snapshot.base_relation_epoch {
            let active_epoch = self
                .sqlite
                .project_meta_get_int(project_id, "active_relation_epoch")
                .map_err(|error| {
                    StorageError::Validation(format!("cannot read active epoch for CAS: {error}"))
                })?;
            if active_epoch != base_epoch {
                return Err(StorageError::epoch_conflict(active_epoch, base_epoch));
            }
        }

        self.runtime.set_updating().await;
        let integrity = Self::integrity(&snapshot);

        // Build the runtime projection from the in-memory index BEFORE the
        // SQLite write  so publication does not re-read what it just
        // persisted. Validation replaces the previous load-back as the
        // integrity check.
        if let Err(error) = index.validate_snapshot() {
            self.runtime.report_failure(error.clone()).await;
            return Err(StorageError::Validation(error));
        }
        // this path keeps a deep copy on purpose. `index` is the
        // long-lived builder index of the full-index orchestrator: it is
        // cleared (`builder.clear()`) and rebuilt on the next index run, so a
        // zero-copy share would corrupt the published snapshot. The
        // SQLite cold-start/compaction paths use `from_index_shared` instead.
        let snapshot_index = Arc::new(RelationSnapshotIndex::from_index(index));

        let epoch = match self
            .writer
            .allocate_and_write(project_id, operation_id, &snapshot)
        {
            Ok(epoch) => epoch,
            Err(error) => {
                self.runtime.report_failure(error.to_string()).await;
                return Err(error);
            }
        };

        let project_candidate = self.has_project_publication_candidate(project_id)?;
        if !project_candidate {
            let data_epoch = self.active_data_epoch(project_id)?;
            if let Err(error) = self.sqlite.with_transaction(|tx| {
                ProjectIndexManifestRepository::activate(
                    tx,
                    project_id,
                    data_epoch,
                    epoch,
                    operation_id,
                    None,
                )
                .map(|_| ())
            }) {
                self.mark_candidate_failed(project_id, epoch, &error);
                self.runtime.report_failure(error.to_string()).await;
                return Err(error);
            }
            self.runtime
                .publish_snapshot(
                    Arc::clone(&snapshot_index),
                    epoch,
                    integrity,
                    Some(operation_id.to_string()),
                )
                .await;
        }

        Ok(RelationPublication {
            relation_epoch: epoch,
            snapshot: Arc::new(LayeredSnapshotIndex::new(snapshot_index)),
        })
    }

    async fn publish_delta(
        &self,
        project_id: i64,
        operation_id: &str,
        delta: SnapshotDelta,
        base: Option<LayeredSnapshotIndex>,
    ) -> Result<RelationPublication, StorageError> {
        let _guard = self.publish_lock.lock().await;

        let active_epoch = self
            .sqlite
            .project_meta_get_int(project_id, "active_relation_epoch")
            .map_err(|error| {
                StorageError::Validation(format!("cannot read active epoch for CAS: {error}"))
            })?;

        if active_epoch != delta.base_epoch {
            return Err(StorageError::epoch_conflict(active_epoch, delta.base_epoch));
        }

        self.runtime.set_updating().await;

        // The runtime projection IS the supplied layered view on the hot
        // path (base + accumulated chain, zero conversions); the cold path
        // assembles it from a full SQLite load.
        let scoped_base: LayeredSnapshotIndex = match base {
            Some(view) => {
                // Hot path: reuse the in-process base cache instead of
                // re-reading the full base from SQLite. The base was validated
                // when it was published, so only the delta's own surface is
                // validated here (scoped validation).
                //
                // Zero-copy pass-through: the view already shares the cached
                // materialized base's maps and carries the accumulated chain,
                // so no conversion or re-cloning happens here.
                view
            }
            None => {
                // Cold path: validate the delta's own surface against the
                // loaded base read-only instead of merging the full graph just
                // to re-validate it. The base was validated when it was
                // persisted (a relation epoch must be independently loadable),
                // and `validate_delta_scoped` below pins every invariant a
                // delta can violate (added edges referencing missing entities;
                // removed entities still referenced without a scheduled
                // removal). No detached O(project) clone is performed here;
                // `detached_clone` is reserved for scenarios that genuinely
                // need an independent mutable copy (e.g. compaction)
                let loaded = match RelationSnapshotLoader::load(
                    &SqliteSnapshotStore::new(self.sqlite.clone()),
                    project_id,
                    delta.base_epoch,
                ) {
                    Ok(index) => index,
                    Err(error) => {
                        self.runtime.report_failure(error.to_string()).await;
                        return Err(error);
                    }
                };
                LayeredSnapshotIndex::new(Arc::new(RelationSnapshotIndex::from_index_shared(
                    &loaded,
                )))
            }
        };

        if let Err(error) = Self::validate_delta_scoped(&scoped_base, &delta) {
            self.runtime.report_failure(error.clone()).await;
            return Err(StorageError::Validation(format!(
                "relation delta {project_id}@{active_epoch} failed scoped validation: {error}"
            )));
        }

        let epoch = match self.sqlite.with_transaction(|tx| {
            let epoch = RelationSnapshotRepository::allocate_building(
                tx,
                project_id,
                operation_id,
                &delta.config_fingerprint,
            )?;
            RelationSnapshotRepository::write_delta(tx, project_id, epoch, &delta)?;
            Ok(epoch)
        }) {
            Ok(epoch) => epoch,
            Err(error) => {
                self.runtime.report_failure(error.to_string()).await;
                return Err(error);
            }
        };

        // Zero-copy runtime projection: share the base's maps into the
        // snapshot view instead of deep-copying them (the caller keeps the
        // canonical base alive in the relation base cache). The accumulated
        // delta chain from the base cache is passed through as-is, so the
        // runtime `LayeredSnapshotIndex` traverses the full chain at query
        // time without an O(project) merge.
        let delta_arc = Arc::new(delta);
        let layered = if scoped_base.deltas.is_empty() {
            Arc::new(LayeredSnapshotIndex::with_delta(
                Arc::clone(&scoped_base.base),
                delta_arc,
            ))
        } else {
            let mut deltas = scoped_base.deltas.clone();
            deltas.push(delta_arc);
            Arc::new(LayeredSnapshotIndex::with_deltas(
                Arc::clone(&scoped_base.base),
                deltas,
            ))
        };

        let project_candidate = self.has_project_publication_candidate(project_id)?;
        if !project_candidate {
            let data_epoch = self.active_data_epoch(project_id)?;
            if let Err(error) = self.sqlite.with_transaction(|tx| {
                ProjectIndexManifestRepository::activate(
                    tx,
                    project_id,
                    data_epoch,
                    epoch,
                    operation_id,
                    None,
                )
                .map(|_| ())
            }) {
                self.mark_candidate_failed(project_id, epoch, &error);
                self.runtime.report_failure(error.to_string()).await;
                return Err(error);
            }
            self.runtime
                .publish_layered_snapshot(
                    Arc::clone(&layered),
                    epoch,
                    SnapshotIntegrity::Full,
                    Some(operation_id.to_string()),
                )
                .await;
        }

        Ok(RelationPublication {
            relation_epoch: epoch,
            snapshot: layered,
        })
    }

    async fn maybe_compact(&self, project_id: i64) -> Result<(), StorageError> {
        // Thresholds are the defaults configured in `CompactionConfig`
        // (chain length 10 / cumulative delta volume ratio 0.2).
        if !self.needs_compaction(project_id, &CompactionConfig::default())? {
            return Ok(());
        }
        // A data-side building candidate means another operation is in
        // flight; `compact_project` retires all non-active relation manifests
        // (`delete_manifests_except`), which would destroy an in-flight
        // operation's building delta. Defer compaction to the next call.
        if self.has_project_publication_candidate(project_id)? {
            tracing::debug!(
                project_id,
                "Relation delta-chain compaction deferred: another publication candidate is in flight"
            );
            return Ok(());
        }
        let active_epoch = self
            .sqlite
            .project_meta_get_int(project_id, "active_relation_epoch")
            .map_err(|error| {
                StorageError::Validation(format!(
                    "cannot read active epoch for compaction: {error}"
                ))
            })?;
        // Deterministic operation id per base epoch keeps a retried
        // compaction idempotent (the same building manifest is reused).
        let operation_id = format!("auto-compaction-{active_epoch}");
        let publication = self.compact_project(project_id, &operation_id).await?;
        tracing::info!(
            project_id,
            base_epoch = active_epoch,
            compacted_epoch = publication.relation_epoch,
            "Relation delta chain compacted after activation"
        );
        Ok(())
    }
}
