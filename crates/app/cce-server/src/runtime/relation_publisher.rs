//! Server-side publisher for complete relation snapshots.

use std::sync::Arc;

use async_trait::async_trait;
use cce_orchestrator::index::{
    RelationPublication, RelationSnapshotPublisher, ResolutionPipelineService,
};
use cce_relation::index::RelationIndexView;
use cce_relation::index::core::RelationIndex;
use cce_relation::index::snapshot_index::{LayeredSnapshotIndex, RelationSnapshotIndex};
use cce_relation::index::snapshot_loader::RelationSnapshotLoader;
use cce_storage_sqlite::SqliteClient;
use cce_storage_sqlite::repo::{ProjectIndexManifestRepository, RelationSnapshotRepository};
use cce_storage_sqlite::snapshot_store::SqliteSnapshotStore;
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
            writer: ResolutionPipelineService::new(sqlite.clone()),
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

    fn has_project_publication_candidate(&self, project_id: i64) -> Result<bool, StorageError> {
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
                snapshot_index,
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
                    snapshot_index,
                    epoch,
                    integrity,
                    Some(operation_id.to_string()),
                )
                .await;
        }

        Ok(RelationPublication {
            relation_epoch: epoch,
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
        let scoped_base: LayeredSnapshotIndex;
        match base {
            Some(view) => {
                // Hot path: reuse the in-process base cache instead of
                // re-reading the full base from SQLite. The base was validated
                // when it was published, so only the delta's own surface is
                // validated here (scoped validation).
                //
                // Zero-copy pass-through: the view already shares the cached
                // materialized base's maps and carries the accumulated chain,
                // so no conversion or re-cloning happens here.
                scoped_base = view;
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
                scoped_base = LayeredSnapshotIndex::new(Arc::new(
                    RelationSnapshotIndex::from_index_shared(&loaded),
                ));
            }
        }

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
                    layered,
                    epoch,
                    SnapshotIntegrity::Full,
                    Some(operation_id.to_string()),
                )
                .await;
        }

        Ok(RelationPublication {
            relation_epoch: epoch,
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

#[cfg(test)]
mod tests {
    use super::*;
    use cce_relation::index::snapshot_query::{
        SnapshotEntityQueryOps, SnapshotFileQueryOps, SnapshotRelationQueryOps,
        SnapshotSymbolQueryOps,
    };
    use cce_relation::index::{LayeredSnapshotIndex, RelationSnapshotIndex};
    use cce_storage_sqlite::ProjectRepository;
    use cce_storage_sqlite::repo::ProjectIndexManifestRepository;
    use cce_storage_sqlite::types::NewProjectRecord;
    use cce_types::relation::CallContext;
    use cce_types::{
        CanonicalEntity, CanonicalFile, CanonicalRelation, CanonicalRelationTarget, EntityKind,
        RelationType, Span, StableSymbolKey,
    };

    fn insert_project(sqlite: &SqliteClient) {
        sqlite
            .with_transaction(|tx| {
                ProjectRepository::insert(
                    tx,
                    &NewProjectRecord::new("test".to_string(), "/tmp/test".to_string()),
                )
                .map(|_| ())
            })
            .expect("test project should be inserted");
    }

    fn snapshot() -> CanonicalRelationSnapshot {
        let caller =
            StableSymbolKey::new("src/main.rs", "caller", EntityKind::Function, "fn caller()");
        let callee =
            StableSymbolKey::new("src/lib.rs", "callee", EntityKind::Function, "fn callee()");
        let mut snapshot = CanonicalRelationSnapshot::new("test-config".to_string());
        snapshot.files = vec![
            CanonicalFile {
                path: "src/main.rs".to_string(),
                language: "rust".to_string(),
                input_hash: "main".to_string(),
                file_size: 10,
                imports: Vec::new(),
                exports: Vec::new(),
            },
            CanonicalFile {
                path: "src/lib.rs".to_string(),
                language: "rust".to_string(),
                input_hash: "lib".to_string(),
                file_size: 10,
                imports: Vec::new(),
                exports: Vec::new(),
            },
        ];
        snapshot.entities = vec![
            CanonicalEntity {
                key: caller.clone(),
                entity_id: Some(1),
                name: "caller".to_string(),
                signature: "fn caller()".to_string(),
                parameters: Vec::new(),
                return_type: None,
                span: Span::default(),
                depth: 0,
                parent: None,
                doc_comment: None,
                modifiers: Vec::new(),
                attributes: Default::default(),
                metadata: Default::default(),
                is_stdlib: false,
                stdlib_category: None,
                subtype: None,
            },
            CanonicalEntity {
                key: callee.clone(),
                entity_id: Some(2),
                name: "callee".to_string(),
                signature: "fn callee()".to_string(),
                parameters: Vec::new(),
                return_type: None,
                span: Span::default(),
                depth: 0,
                parent: None,
                doc_comment: None,
                modifiers: Vec::new(),
                attributes: Default::default(),
                metadata: Default::default(),
                is_stdlib: false,
                stdlib_category: None,
                subtype: None,
            },
        ];
        snapshot.relations.push(CanonicalRelation {
            caller,
            target: CanonicalRelationTarget::Internal { key: callee },
            raw_target: "callee".to_string(),
            relation_type: RelationType::DirectCall,
            span: Span::default(),
            stdlib_category: None,
        });
        snapshot
    }

    /// Build the in-memory index matching `snapshot()`.
    fn snapshot_index(snapshot: &CanonicalRelationSnapshot) -> cce_relation::index::RelationIndex {
        RelationSnapshotLoader::load_canonical(snapshot).expect("test snapshot should load")
    }

    /// Layered base view over `snapshot`'s in-memory index (empty chain).
    fn test_layered_base(snapshot: &CanonicalRelationSnapshot) -> LayeredSnapshotIndex {
        LayeredSnapshotIndex::new(Arc::new(RelationSnapshotIndex::from_index_shared(
            &snapshot_index(snapshot),
        )))
    }

    /// Layered base view over an already-built in-memory index (empty chain).
    fn test_layered_base_from_index(
        index: &cce_relation::index::RelationIndex,
    ) -> LayeredSnapshotIndex {
        LayeredSnapshotIndex::new(Arc::new(RelationSnapshotIndex::from_index_shared(index)))
    }

    #[tokio::test]
    async fn cas_rejects_concurrent_publish_with_stale_base_epoch() {
        let sqlite = SqliteClient::in_memory().expect("in-memory SQLite should open");
        insert_project(&sqlite);
        let runtime = Arc::new(RelationRuntime::new(1));
        let publisher = ServerRelationSnapshotPublisher::new(sqlite.clone(), runtime.clone());

        // First publish (no CAS) — succeeds.
        let first_snapshot = snapshot();
        let first = publisher
            .publish(
                1,
                "operation-a",
                first_snapshot.clone(),
                &snapshot_index(&first_snapshot),
            )
            .await
            .expect("first snapshot should publish");

        // Second publish with a stale base_epoch — must be rejected.
        let mut stale = snapshot();
        stale.base_relation_epoch = Some(0); // active_epoch is now first.relation_epoch
        let err = publisher
            .publish(1, "operation-b", stale.clone(), &snapshot_index(&stale))
            .await
            .expect_err("stale base_epoch should be rejected");
        assert!(
            err.to_string().contains("Epoch conflict"),
            "error should mention epoch conflict, got: {err}"
        );

        // Active epoch must still point to the first publish.
        assert_eq!(
            sqlite
                .project_meta_get_int(1, "active_relation_epoch")
                .expect("active epoch should be readable"),
            first.relation_epoch
        );
    }

    #[tokio::test]
    async fn publish_is_restart_equivalent_and_idempotent() {
        let sqlite = SqliteClient::in_memory().expect("in-memory SQLite should open");
        insert_project(&sqlite);
        let runtime = Arc::new(RelationRuntime::new(1));
        let publisher = ServerRelationSnapshotPublisher::new(sqlite.clone(), runtime.clone());

        let first_snapshot = snapshot();
        let first = publisher
            .publish(
                1,
                "operation-1",
                first_snapshot.clone(),
                &snapshot_index(&first_snapshot),
            )
            .await
            .expect("complete snapshot should publish");
        let second_snapshot = snapshot();
        let second = publisher
            .publish(
                1,
                "operation-1",
                second_snapshot.clone(),
                &snapshot_index(&second_snapshot),
            )
            .await
            .expect("same operation should be idempotent");
        assert_eq!(first.relation_epoch, second.relation_epoch);
        assert_eq!(
            sqlite
                .project_meta_get_int(1, "active_relation_epoch")
                .expect("active epoch should be readable"),
            first.relation_epoch
        );
        assert_eq!(runtime.get_relation_epoch().await, first.relation_epoch);

        let live = runtime
            .get_snapshot()
            .await
            .expect("runtime snapshot should be published")
            .index
            .clone();
        let cold = RelationSnapshotLoader::load(
            &SqliteSnapshotStore::new(sqlite.clone()),
            1,
            first.relation_epoch,
        )
        .expect("active epoch should cold-load");
        assert_eq!(live.function_count(), cold.function_count());
        assert_eq!(
            live.resolved_relation_count(),
            cold.resolved_relation_count()
        );
        assert_eq!(live.compute_fingerprint(), cold.compute_fingerprint());

        let callee = live
            .get_entity_id_by_symbol_key(&StableSymbolKey::new(
                "src/lib.rs",
                "callee",
                EntityKind::Function,
                "fn callee()",
            ))
            .expect("callee should exist");
        assert_eq!(
            live.get_callers_by_callee_entity(callee),
            cold.get_callers_by_callee_entity(callee)
        );
    }

    #[tokio::test]
    async fn delta_publish_is_immediately_visible_to_runtime_queries() {
        let sqlite = SqliteClient::in_memory().expect("in-memory SQLite should open");
        insert_project(&sqlite);
        let runtime = Arc::new(RelationRuntime::new(1));
        let publisher = ServerRelationSnapshotPublisher::new(sqlite.clone(), runtime.clone());

        let base_snapshot = snapshot();
        let base = publisher
            .publish(
                1,
                "operation-base",
                base_snapshot.clone(),
                &snapshot_index(&base_snapshot),
            )
            .await
            .expect("base snapshot should publish");

        // Build a delta: remove the existing internal edge, add a new entity
        // in a new file plus an edge from the existing caller to it.
        let new_entity_id = cce_types::EntityId(3);
        let new_key = StableSymbolKey::new(
            "src/extra.rs",
            "extra_fn",
            EntityKind::Function,
            "fn extra_fn()",
        );
        let delta = SnapshotDelta {
            epoch: base.relation_epoch + 1,
            base_epoch: base.relation_epoch,
            config_fingerprint: "test-config".to_string(),
            removed_files: Vec::new(),
            added_files: vec![cce_types::FileInfo {
                id: "src/extra.rs".to_string(),
                path: "src/extra.rs".to_string(),
                language: "rust".to_string(),
                file_hash: "extra".to_string(),
                file_size: 5,
                modified_time: 0,
                parse_status: cce_types::entity::ParseStatus::Success,
                parse_errors: Vec::new(),
                parse_version: 1,
                entity_count: 1,
                relation_count: 1,
                export_count: 0,
                import_count: 0,
                depends_on: Vec::new(),
            }],
            removed_entities: Vec::new(),
            added_entities: vec![cce_types::AddedEntity {
                entity: cce_types::Entity {
                    id: new_entity_id,
                    kind: EntityKind::Function,
                    name: "extra_fn".to_string(),
                    signature: "fn extra_fn()".to_string(),
                    parameters: Vec::new(),
                    return_type: None,
                    span: Span::default(),
                    depth: 0,
                    parent: None,
                    children: Vec::new(),
                    doc_comment: None,
                    modifiers: Vec::new(),
                    attributes: Default::default(),
                    metadata: Default::default(),
                    is_stdlib: false,
                    stdlib_category: None,
                    subtype: None,
                },
                symbol_key: new_key.clone(),
                file_path: "src/extra.rs".to_string(),
            }],
            removed_relations: vec![cce_types::ResolvedRelation {
                caller: cce_types::EntityId(1),
                callee_id: Some(cce_types::EntityId(2)),
                callee_name: "callee".to_string(),
                relation_type: RelationType::DirectCall,
                span: Span::default(),
                is_external: false,
                external_type: None,
                callee_symbol: None,
                stdlib_category: None,
                owner_type: None,
                call_context: CallContext::Direct,
            }],
            added_relations: vec![cce_types::ResolvedRelation {
                caller: cce_types::EntityId(1),
                callee_id: Some(new_entity_id),
                callee_name: "extra_fn".to_string(),
                relation_type: RelationType::DirectCall,
                span: Span::default(),
                is_external: false,
                external_type: None,
                callee_symbol: None,
                stdlib_category: None,
                owner_type: None,
                call_context: CallContext::Direct,
            }],
            import_diffs: Vec::new(),
            export_diffs: Vec::new(),
            file_relation_diffs: Vec::new(),
            relation_edges_dropped_unbounded: 0,
            dependency_diffs: Vec::new(),
            renamed_entities: Vec::new(),
        };
        let publication = publisher
            .publish_delta(
                1,
                "operation-delta",
                delta.clone(),
                // Hot path: supply the in-process layered base (as the
                // hot-update relation base cache would) so the publisher skips
                // the SQLite reload and validates scoped.
                Some(test_layered_base(&base_snapshot)),
            )
            .await
            .expect("delta should publish");

        // Immediately query through the runtime: the delta must be visible.
        let runtime_snapshot = runtime
            .get_snapshot()
            .await
            .expect("runtime snapshot should be published");
        assert_eq!(runtime_snapshot.relation_epoch, publication.relation_epoch);
        let live = runtime_snapshot.index.clone();

        // Old edge gone, new entity + edge visible with full identity.
        assert_eq!(
            live.get_callers_by_callee_entity(cce_types::EntityId(2)),
            Vec::<cce_types::EntityId>::new()
        );
        assert_eq!(
            live.get_callers_by_callee_entity(new_entity_id),
            vec![cce_types::EntityId(1)]
        );
        assert_eq!(
            live.get_entity_id_by_stable_symbol_id(&new_key.stable_id().0),
            Some(new_entity_id)
        );
        assert_eq!(
            live.get_file_path_by_entity(new_entity_id),
            Some("src/extra.rs".to_string())
        );
        assert!(live.contains_file("src/extra.rs"));
        // Delta chain replay from SQLite reconstructs the same merged graph.
        let cold = RelationSnapshotLoader::load(
            &SqliteSnapshotStore::new(sqlite.clone()),
            1,
            publication.relation_epoch,
        )
        .expect("delta chain should cold-load");
        assert_eq!(live.compute_fingerprint(), cold.compute_fingerprint());

        // A delta that leaves a dangling internal target is rejected before
        // persisting: publish a fresh base (caller -> callee edge intact),
        // then remove the callee entity WITHOUT removing the edge.
        let base2_snapshot = snapshot();
        let base2 = publisher
            .publish(
                1,
                "operation-base-2",
                base2_snapshot.clone(),
                &snapshot_index(&base2_snapshot),
            )
            .await
            .expect("second base snapshot should publish");
        assert!(base2.relation_epoch > publication.relation_epoch);
        let dangling = SnapshotDelta {
            epoch: base2.relation_epoch + 1,
            base_epoch: base2.relation_epoch,
            config_fingerprint: "test-config".to_string(),
            removed_files: vec!["src/lib.rs".to_string()],
            added_files: Vec::new(),
            removed_entities: vec![cce_types::EntityId(2)],
            added_entities: Vec::new(),
            removed_relations: Vec::new(),
            added_relations: Vec::new(),
            import_diffs: Vec::new(),
            export_diffs: Vec::new(),
            file_relation_diffs: Vec::new(),
            relation_edges_dropped_unbounded: 0,
            dependency_diffs: Vec::new(),
            renamed_entities: Vec::new(),
        };
        let error = publisher
            .publish_delta(1, "operation-dangling", dangling, None)
            .await
            .expect_err("dangling delta must be rejected");
        assert!(
            error.to_string().contains("failed scoped validation"),
            "error should mention scoped validation, got: {error}"
        );
        // Active epoch must remain on the previous valid publication.
        assert_eq!(
            sqlite
                .project_meta_get_int(1, "active_relation_epoch")
                .expect("active epoch should be readable"),
            base2.relation_epoch
        );
    }

    #[tokio::test]
    async fn cold_path_delta_publish_is_scoped_without_full_merge() {
        let sqlite = SqliteClient::in_memory().expect("in-memory SQLite should open");
        insert_project(&sqlite);
        let runtime = Arc::new(RelationRuntime::new(1));
        let publisher = ServerRelationSnapshotPublisher::new(sqlite.clone(), runtime.clone());

        let base_snapshot = snapshot();
        let base = publisher
            .publish(
                1,
                "operation-base",
                base_snapshot.clone(),
                &snapshot_index(&base_snapshot),
            )
            .await
            .expect("base snapshot should publish");

        // A delta with both an addition and a removal, published through the
        // cold path (base=None, base only available from SQLite). The cold
        // path must NOT merge the full graph via `detached_clone` + `apply_delta`
        // just to re-validate it — scoped validation against the read-only
        // layered base covers every invariant a delta can violate  This
        // pins the "no detached clone on the publish path" guarantee end-to-end.
        let new_entity_id = cce_types::EntityId(3);
        let new_key = StableSymbolKey::new(
            "src/extra.rs",
            "extra_fn",
            EntityKind::Function,
            "fn extra_fn()",
        );
        let delta = SnapshotDelta {
            epoch: base.relation_epoch + 1,
            base_epoch: base.relation_epoch,
            config_fingerprint: "test-config".to_string(),
            removed_files: Vec::new(),
            added_files: vec![cce_types::FileInfo {
                id: "src/extra.rs".to_string(),
                path: "src/extra.rs".to_string(),
                language: "rust".to_string(),
                file_hash: "extra".to_string(),
                file_size: 5,
                modified_time: 0,
                parse_status: cce_types::entity::ParseStatus::Success,
                parse_errors: Vec::new(),
                parse_version: 1,
                entity_count: 1,
                relation_count: 1,
                export_count: 0,
                import_count: 0,
                depends_on: Vec::new(),
            }],
            removed_entities: Vec::new(),
            added_entities: vec![cce_types::AddedEntity {
                entity: cce_types::Entity {
                    id: new_entity_id,
                    kind: EntityKind::Function,
                    name: "extra_fn".to_string(),
                    signature: "fn extra_fn()".to_string(),
                    parameters: Vec::new(),
                    return_type: None,
                    span: Span::default(),
                    depth: 0,
                    parent: None,
                    children: Vec::new(),
                    doc_comment: None,
                    modifiers: Vec::new(),
                    attributes: Default::default(),
                    metadata: Default::default(),
                    is_stdlib: false,
                    stdlib_category: None,
                    subtype: None,
                },
                symbol_key: new_key.clone(),
                file_path: "src/extra.rs".to_string(),
            }],
            removed_relations: vec![cce_types::ResolvedRelation {
                caller: cce_types::EntityId(1),
                callee_id: Some(cce_types::EntityId(2)),
                callee_name: "callee".to_string(),
                relation_type: RelationType::DirectCall,
                span: Span::default(),
                is_external: false,
                external_type: None,
                callee_symbol: None,
                stdlib_category: None,
                owner_type: None,
                call_context: CallContext::Direct,
            }],
            added_relations: vec![cce_types::ResolvedRelation {
                caller: cce_types::EntityId(1),
                callee_id: Some(new_entity_id),
                callee_name: "extra_fn".to_string(),
                relation_type: RelationType::DirectCall,
                span: Span::default(),
                is_external: false,
                external_type: None,
                callee_symbol: None,
                stdlib_category: None,
                owner_type: None,
                call_context: CallContext::Direct,
            }],
            import_diffs: Vec::new(),
            export_diffs: Vec::new(),
            file_relation_diffs: Vec::new(),
            relation_edges_dropped_unbounded: 0,
            dependency_diffs: Vec::new(),
            renamed_entities: Vec::new(),
        };
        let publication = publisher
            .publish_delta(1, "operation-cold-delta", delta, None)
            .await
            .expect("cold-path delta should publish");

        let runtime_snapshot = runtime
            .get_snapshot()
            .await
            .expect("runtime snapshot should be published");
        assert_eq!(runtime_snapshot.relation_epoch, publication.relation_epoch);
        let live = runtime_snapshot.index.clone();

        // The merged projection (base + delta) is fully queryable: old edge
        // gone, new entity + edge present.
        assert_eq!(
            live.get_callers_by_callee_entity(cce_types::EntityId(2)),
            Vec::<cce_types::EntityId>::new()
        );
        assert_eq!(
            live.get_callers_by_callee_entity(new_entity_id),
            vec![cce_types::EntityId(1)]
        );
        assert_eq!(
            live.get_entity_id_by_stable_symbol_id(&new_key.stable_id().0),
            Some(new_entity_id)
        );
        assert!(live.contains_file("src/extra.rs"));
        // Replaying from SQLite reconstructs the same merged graph.
        let cold = RelationSnapshotLoader::load(
            &SqliteSnapshotStore::new(sqlite.clone()),
            1,
            publication.relation_epoch,
        )
        .expect("delta chain should cold-load");
        assert_eq!(live.compute_fingerprint(), cold.compute_fingerprint());
    }

    #[tokio::test]
    async fn chain_hot_publish_merges_accumulated_deltas_for_runtime() {
        let sqlite = SqliteClient::in_memory().expect("in-memory SQLite should open");
        insert_project(&sqlite);
        let runtime = Arc::new(RelationRuntime::new(1));
        let publisher = ServerRelationSnapshotPublisher::new(sqlite.clone(), runtime.clone());

        let base_snapshot = snapshot();
        let base = publisher
            .publish(
                1,
                "operation-base",
                base_snapshot.clone(),
                &snapshot_index(&base_snapshot),
            )
            .await
            .expect("base snapshot should publish");

        // Delta 1: add extra.rs / extra_fn, rewire the caller edge to it.
        let extra_id = cce_types::EntityId(3);
        let extra_key = StableSymbolKey::new(
            "src/extra.rs",
            "extra_fn",
            EntityKind::Function,
            "fn extra_fn()",
        );
        let delta1 = SnapshotDelta {
            epoch: base.relation_epoch + 1,
            base_epoch: base.relation_epoch,
            config_fingerprint: "test-config".to_string(),
            removed_files: Vec::new(),
            added_files: vec![cce_types::FileInfo {
                id: "src/extra.rs".to_string(),
                path: "src/extra.rs".to_string(),
                language: "rust".to_string(),
                file_hash: "extra".to_string(),
                file_size: 5,
                modified_time: 0,
                parse_status: cce_types::entity::ParseStatus::Success,
                parse_errors: Vec::new(),
                parse_version: 1,
                entity_count: 1,
                relation_count: 1,
                export_count: 0,
                import_count: 0,
                depends_on: Vec::new(),
            }],
            removed_entities: Vec::new(),
            added_entities: vec![cce_types::AddedEntity {
                entity: cce_types::Entity {
                    id: extra_id,
                    kind: EntityKind::Function,
                    name: "extra_fn".to_string(),
                    signature: "fn extra_fn()".to_string(),
                    parameters: Vec::new(),
                    return_type: None,
                    span: Span::default(),
                    depth: 0,
                    parent: None,
                    children: Vec::new(),
                    doc_comment: None,
                    modifiers: Vec::new(),
                    attributes: Default::default(),
                    metadata: Default::default(),
                    is_stdlib: false,
                    stdlib_category: None,
                    subtype: None,
                },
                symbol_key: extra_key.clone(),
                file_path: "src/extra.rs".to_string(),
            }],
            removed_relations: vec![cce_types::ResolvedRelation {
                caller: cce_types::EntityId(1),
                callee_id: Some(cce_types::EntityId(2)),
                callee_name: "callee".to_string(),
                relation_type: RelationType::DirectCall,
                span: Span::default(),
                is_external: false,
                external_type: None,
                callee_symbol: None,
                stdlib_category: None,
                owner_type: None,
                call_context: CallContext::Direct,
            }],
            added_relations: vec![cce_types::ResolvedRelation {
                caller: cce_types::EntityId(1),
                callee_id: Some(extra_id),
                callee_name: "extra_fn".to_string(),
                relation_type: RelationType::DirectCall,
                span: Span::default(),
                is_external: false,
                external_type: None,
                callee_symbol: None,
                stdlib_category: None,
                owner_type: None,
                call_context: CallContext::Direct,
            }],
            import_diffs: Vec::new(),
            export_diffs: Vec::new(),
            file_relation_diffs: Vec::new(),
            relation_edges_dropped_unbounded: 0,
            dependency_diffs: Vec::new(),
            renamed_entities: Vec::new(),
        };

        // Publish delta1 for real: this advances the active epoch to 2, so the
        // layered base below can legally serve a follow-up delta at epoch 3.
        publisher
            .publish_delta(
                1,
                "operation-delta-1",
                delta1.clone(),
                Some(test_layered_base(&base_snapshot)),
            )
            .await
            .expect("delta1 should publish");

        // The in-process base cache has already accumulated delta1: the
        // layered base handed to the publisher carries a non-empty chain
        // (R9: 6.4b chain non-empty merge path).
        let mut layered = test_layered_base(&base_snapshot);
        layered.deltas.push(Arc::new(delta1.clone()));

        // Delta 2: drop extra_fn, add extra2_fn in a new file.
        let extra2_id = cce_types::EntityId(4);
        let extra2_key = StableSymbolKey::new(
            "src/extra2.rs",
            "extra2_fn",
            EntityKind::Function,
            "fn extra2_fn()",
        );
        let delta2 = SnapshotDelta {
            epoch: delta1.epoch + 1,
            base_epoch: delta1.epoch,
            config_fingerprint: "test-config".to_string(),
            removed_files: vec!["src/extra.rs".to_string()],
            added_files: vec![cce_types::FileInfo {
                id: "src/extra2.rs".to_string(),
                path: "src/extra2.rs".to_string(),
                language: "rust".to_string(),
                file_hash: "extra2".to_string(),
                file_size: 5,
                modified_time: 0,
                parse_status: cce_types::entity::ParseStatus::Success,
                parse_errors: Vec::new(),
                parse_version: 1,
                entity_count: 1,
                relation_count: 1,
                export_count: 0,
                import_count: 0,
                depends_on: Vec::new(),
            }],
            removed_entities: vec![extra_id],
            added_entities: vec![cce_types::AddedEntity {
                entity: cce_types::Entity {
                    id: extra2_id,
                    kind: EntityKind::Function,
                    name: "extra2_fn".to_string(),
                    signature: "fn extra2_fn()".to_string(),
                    parameters: Vec::new(),
                    return_type: None,
                    span: Span::default(),
                    depth: 0,
                    parent: None,
                    children: Vec::new(),
                    doc_comment: None,
                    modifiers: Vec::new(),
                    attributes: Default::default(),
                    metadata: Default::default(),
                    is_stdlib: false,
                    stdlib_category: None,
                    subtype: None,
                },
                symbol_key: extra2_key.clone(),
                file_path: "src/extra2.rs".to_string(),
            }],
            removed_relations: vec![cce_types::ResolvedRelation {
                caller: cce_types::EntityId(1),
                callee_id: Some(extra_id),
                callee_name: "extra_fn".to_string(),
                relation_type: RelationType::DirectCall,
                span: Span::default(),
                is_external: false,
                external_type: None,
                callee_symbol: None,
                stdlib_category: None,
                owner_type: None,
                call_context: CallContext::Direct,
            }],
            added_relations: vec![cce_types::ResolvedRelation {
                caller: cce_types::EntityId(1),
                callee_id: Some(extra2_id),
                callee_name: "extra2_fn".to_string(),
                relation_type: RelationType::DirectCall,
                span: Span::default(),
                is_external: false,
                external_type: None,
                callee_symbol: None,
                stdlib_category: None,
                owner_type: None,
                call_context: CallContext::Direct,
            }],
            import_diffs: Vec::new(),
            export_diffs: Vec::new(),
            file_relation_diffs: Vec::new(),
            relation_edges_dropped_unbounded: 0,
            dependency_diffs: Vec::new(),
            renamed_entities: Vec::new(),
        };

        let publication = publisher
            .publish_delta(1, "operation-chain-delta", delta2.clone(), Some(layered))
            .await
            .expect("chain hot publish should merge the accumulated deltas");

        // The runtime projection reflects base + delta1 + delta2.
        let runtime_snapshot = runtime
            .get_snapshot()
            .await
            .expect("runtime snapshot should be published");
        assert_eq!(runtime_snapshot.relation_epoch, publication.relation_epoch);
        let live = runtime_snapshot.index.clone();
        assert_eq!(
            live.get_callers_by_callee_entity(extra_id),
            Vec::<cce_types::EntityId>::new()
        );
        assert_eq!(
            live.get_callers_by_callee_entity(extra2_id),
            vec![cce_types::EntityId(1)]
        );
        assert!(!live.contains_file("src/extra.rs"));
        assert!(live.contains_file("src/extra2.rs"));

        // Full chain replay reconstructs the same merged graph.
        let cold = RelationSnapshotLoader::load(
            &SqliteSnapshotStore::new(sqlite.clone()),
            1,
            publication.relation_epoch,
        )
        .expect("delta chain should cold-load");
        assert_eq!(live.compute_fingerprint(), cold.compute_fingerprint());
    }

    #[tokio::test]
    async fn delta_with_provided_base_is_rejected_by_scoped_validation() {
        let sqlite = SqliteClient::in_memory().expect("in-memory SQLite should open");
        insert_project(&sqlite);
        let runtime = Arc::new(RelationRuntime::new(1));
        let publisher = ServerRelationSnapshotPublisher::new(sqlite.clone(), runtime.clone());

        let base_snapshot = snapshot();
        let base_index = Arc::new(snapshot_index(&base_snapshot));
        let base = publisher
            .publish(1, "operation-base", base_snapshot.clone(), &base_index)
            .await
            .expect("base snapshot should publish");

        // Remove the callee entity without scheduling the caller's edge for
        // removal: the scoped validation (hot path with a provided base) must
        // catch the dangling reference before persisting.
        let dangling = SnapshotDelta {
            epoch: base.relation_epoch + 1,
            base_epoch: base.relation_epoch,
            config_fingerprint: "test-config".to_string(),
            removed_files: Vec::new(),
            added_files: Vec::new(),
            removed_entities: vec![cce_types::EntityId(2)],
            added_entities: Vec::new(),
            removed_relations: Vec::new(),
            added_relations: Vec::new(),
            import_diffs: Vec::new(),
            export_diffs: Vec::new(),
            file_relation_diffs: Vec::new(),
            relation_edges_dropped_unbounded: 0,
            dependency_diffs: Vec::new(),
            renamed_entities: Vec::new(),
        };
        let error = publisher
            .publish_delta(
                1,
                "operation-dangling-hot",
                dangling,
                Some(test_layered_base_from_index(&base_index)),
            )
            .await
            .expect_err("dangling delta with provided base must be rejected");
        assert!(
            error.to_string().contains("failed scoped validation"),
            "error should mention scoped validation, got: {error}"
        );
        // A delta with an added relation targeting a nonexistent entity is
        // likewise rejected by the scoped validation.
        let missing_target = SnapshotDelta {
            epoch: base.relation_epoch + 1,
            base_epoch: base.relation_epoch,
            config_fingerprint: "test-config".to_string(),
            removed_files: Vec::new(),
            added_files: Vec::new(),
            removed_entities: Vec::new(),
            added_entities: Vec::new(),
            removed_relations: Vec::new(),
            added_relations: vec![cce_types::ResolvedRelation {
                caller: cce_types::EntityId(1),
                callee_id: Some(cce_types::EntityId(99)),
                callee_name: "ghost".to_string(),
                relation_type: RelationType::DirectCall,
                span: Span::default(),
                is_external: false,
                external_type: None,
                callee_symbol: None,
                stdlib_category: None,
                owner_type: None,
                call_context: CallContext::Direct,
            }],
            import_diffs: Vec::new(),
            export_diffs: Vec::new(),
            file_relation_diffs: Vec::new(),
            relation_edges_dropped_unbounded: 0,
            dependency_diffs: Vec::new(),
            renamed_entities: Vec::new(),
        };
        let error = publisher
            .publish_delta(1, "operation-ghost", missing_target, None)
            .await
            .expect_err("added relation with missing internal target must be rejected");
        assert!(
            error.to_string().contains("failed scoped validation"),
            "error should mention scoped validation, got: {error}"
        );
        assert_eq!(
            sqlite
                .project_meta_get_int(1, "active_relation_epoch")
                .expect("active epoch should be readable"),
            base.relation_epoch
        );
    }

    /// Append `count` pure-add deltas on top of `base`, one new file and
    /// entity per delta, chaining epochs.
    async fn publish_add_deltas(
        publisher: &ServerRelationSnapshotPublisher,
        base: &RelationPublication,
        count: i64,
    ) {
        for i in 0..count {
            let path = format!("src/extra{i}.rs");
            let name = format!("extra{i}_fn");
            let delta = SnapshotDelta {
                epoch: base.relation_epoch + 1 + i,
                base_epoch: base.relation_epoch + i,
                config_fingerprint: "test-config".to_string(),
                removed_files: Vec::new(),
                added_files: vec![cce_types::FileInfo {
                    id: path.clone(),
                    path: path.clone(),
                    language: "rust".to_string(),
                    file_hash: format!("extra{i}"),
                    file_size: 5,
                    modified_time: 0,
                    parse_status: cce_types::entity::ParseStatus::Success,
                    parse_errors: Vec::new(),
                    parse_version: 1,
                    entity_count: 1,
                    relation_count: 0,
                    export_count: 0,
                    import_count: 0,
                    depends_on: Vec::new(),
                }],
                removed_entities: Vec::new(),
                added_entities: vec![cce_types::AddedEntity {
                    entity: cce_types::Entity {
                        id: cce_types::EntityId(3 + i as u64),
                        kind: EntityKind::Function,
                        name: name.clone(),
                        signature: format!("fn {name}()"),
                        parameters: Vec::new(),
                        return_type: None,
                        span: Span::default(),
                        depth: 0,
                        parent: None,
                        children: Vec::new(),
                        doc_comment: None,
                        modifiers: Vec::new(),
                        attributes: Default::default(),
                        metadata: Default::default(),
                        is_stdlib: false,
                        stdlib_category: None,
                        subtype: None,
                    },
                    symbol_key: StableSymbolKey::new(&path, &name, EntityKind::Function, "fn f()"),
                    file_path: path.clone(),
                }],
                removed_relations: Vec::new(),
                added_relations: Vec::new(),
                import_diffs: Vec::new(),
                export_diffs: Vec::new(),
                file_relation_diffs: Vec::new(),
                relation_edges_dropped_unbounded: 0,
                dependency_diffs: Vec::new(),
                renamed_entities: Vec::new(),
            };
            publisher
                .publish_delta(1, &format!("operation-delta-{i}"), delta, None)
                .await
                .expect("add delta should publish");
        }
    }

    #[tokio::test]
    async fn maybe_compact_merges_delta_chain_at_threshold() {
        let sqlite = SqliteClient::in_memory().expect("in-memory SQLite should open");
        insert_project(&sqlite);
        let runtime = Arc::new(RelationRuntime::new(1));
        let publisher = ServerRelationSnapshotPublisher::new(sqlite.clone(), runtime.clone());

        let base_snapshot = snapshot();
        let base = publisher
            .publish(
                1,
                "operation-base",
                base_snapshot.clone(),
                &snapshot_index(&base_snapshot),
            )
            .await
            .expect("base snapshot should publish");

        // Cross max_chain_length (10) of the default compaction config.
        publish_add_deltas(&publisher, &base, 10).await;
        assert!(
            publisher
                .needs_compaction(1, &CompactionConfig::default())
                .expect("threshold check should read the chain"),
            "chain must cross the compaction threshold"
        );

        let active_before = sqlite
            .project_meta_get_int(1, "active_relation_epoch")
            .expect("active epoch should be readable");
        publisher
            .maybe_compact(1)
            .await
            .expect("compaction should succeed");
        let active_after = sqlite
            .project_meta_get_int(1, "active_relation_epoch")
            .expect("active epoch should be readable");
        assert!(
            active_after > active_before,
            "compaction must advance the epoch"
        );
        assert_eq!(runtime.get_relation_epoch().await, active_after);

        // The chain is reset by the merge: a second call is a no-op.
        assert!(
            !publisher
                .needs_compaction(1, &CompactionConfig::default())
                .expect("threshold check should read the chain"),
            "compaction must reset the delta chain"
        );
        publisher
            .maybe_compact(1)
            .await
            .expect("no-op compaction should succeed");

        // Cold load reconstructs the merged graph including all added files.
        let cold = RelationSnapshotLoader::load(
            &SqliteSnapshotStore::new(sqlite.clone()),
            1,
            active_after,
        )
        .expect("compacted epoch should cold-load");
        let live = runtime
            .get_snapshot()
            .await
            .expect("runtime snapshot should be published")
            .index
            .clone();
        assert_eq!(live.compute_fingerprint(), cold.compute_fingerprint());
        for i in 0..10 {
            assert!(
                cold.contains_file(&format!("src/extra{i}.rs")),
                "compacted snapshot must retain extra{i}.rs"
            );
        }
    }

    #[tokio::test]
    async fn maybe_compact_defers_while_candidate_building() {
        let sqlite = SqliteClient::in_memory().expect("in-memory SQLite should open");
        insert_project(&sqlite);
        let runtime = Arc::new(RelationRuntime::new(1));
        let publisher = ServerRelationSnapshotPublisher::new(sqlite.clone(), runtime.clone());

        let base_snapshot = snapshot();
        let base = publisher
            .publish(
                1,
                "operation-base",
                base_snapshot.clone(),
                &snapshot_index(&base_snapshot),
            )
            .await
            .expect("base snapshot should publish");
        publish_add_deltas(&publisher, &base, 10).await;

        // An in-flight publication candidate (state = building) must defer
        // the compaction: the candidate's uncommitted manifest references
        // the same epochs, and `delete_manifests_except` would purge it.
        sqlite
            .with_transaction(|tx| {
                ProjectIndexManifestRepository::begin_building(tx, 1, 0, "operation-inflight", None)
                    .map(|_| ())
            })
            .expect("building manifest should insert");
        assert!(
            publisher
                .has_project_publication_candidate(1)
                .expect("candidate check"),
            "the building manifest must be visible"
        );

        let active_before = sqlite
            .project_meta_get_int(1, "active_relation_epoch")
            .expect("active epoch should be readable");
        publisher
            .maybe_compact(1)
            .await
            .expect("deferred compaction should not fail");
        let active_after = sqlite
            .project_meta_get_int(1, "active_relation_epoch")
            .expect("active epoch should be readable");
        assert_eq!(
            active_after, active_before,
            "compaction must defer while a candidate is building"
        );
        assert!(
            publisher
                .needs_compaction(1, &CompactionConfig::default())
                .expect("threshold check should read the chain"),
            "the chain must be left untouched for the next attempt"
        );
    }
}
