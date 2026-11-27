//! Server-side publisher for complete relation snapshots.

use std::sync::Arc;

use async_trait::async_trait;
use cce_codegraph::index::RelationIndexView;
use cce_codegraph::index::snapshot_index::{LayeredSnapshotIndex, RelationSnapshotIndex};
use cce_orchestrator::index::MetadataStore;
use cce_orchestrator::index::{
    RelationPublication, RelationRebuildPreparation, RelationSnapshotPublisher,
    ResolutionPipelineService,
};
use cce_storage_common::MetadataStorage;
use cce_types::relation::RelationSnapshotStore;
use cce_types::{CanonicalSnapshotArtifact, SnapshotDelta, StorageError};
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
    store: MetadataStore,
    runtime: Arc<RelationRuntime>,
    writer: ResolutionPipelineService,
    publish_lock: Arc<Mutex<()>>,
}

impl ServerRelationSnapshotPublisher {
    pub fn new(store: MetadataStore, runtime: Arc<RelationRuntime>) -> Self {
        Self {
            writer: ResolutionPipelineService::new(store.clone()),
            store,
            publish_lock: runtime.publication_lock(),
            runtime,
        }
    }

    fn integrity(artifact: &CanonicalSnapshotArtifact) -> SnapshotIntegrity {
        let counts = artifact.counts();
        if counts.files == 0
            && counts.entities == 0
            && counts.relations == 0
            && counts.dependencies == 0
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

    async fn mark_candidate_failed(&self, project_id: i64, epoch: i64, error: &StorageError) {
        if let Err(mark_error) = self
            .writer
            .mark_failed(project_id, epoch, &error.to_string())
            .await
        {
            tracing::warn!(
                project_id,
                epoch,
                error = %mark_error,
                "Failed to mark relation publication candidate as failed"
            );
        }
    }

    pub(crate) async fn has_project_publication_candidate(
        &self,
        project_id: i64,
    ) -> Result<bool, StorageError> {
        Ok(self
            .store
            .manifest_building_max_epoch(project_id)
            .await?
            .is_some())
    }

    /// Resolve the current data generation epoch, preferring the durable
    /// project manifest over the legacy `active_epoch` meta.
    ///
    /// A missing manifest with a missing legacy meta row is the legitimate
    /// default 0 (never published); real DB failures are propagated instead
    /// of being silently downgraded.
    async fn active_data_epoch(&self, project_id: i64) -> Result<i64, StorageError> {
        if let Some(manifest) = self.store.manifest_active(project_id).await? {
            return Ok(manifest.data_epoch);
        }
        Ok(self
            .store
            .project_meta_get_int_optional(project_id, "active_epoch")
            .await?
            .unwrap_or(0))
    }

    /// Check whether the delta chain for a project exceeds compaction thresholds.
    pub async fn needs_compaction(
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
            .store
            .project_meta_get_int(project_id, "active_relation_epoch")
            .await
            .map_err(|error| StorageError::Query(error.to_string()))?;
        if active_epoch <= 0 {
            return Ok(false);
        }
        let (chain_len, cumul_size, base_size) = self
            .store
            .snapshot_delta_chain_info(project_id, active_epoch)
            .await?;

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
            .store
            .project_meta_get_int(project_id, "active_relation_epoch")
            .await
            .map_err(|error| {
                StorageError::Validation(format!("cannot read active epoch: {error}"))
            })?;
        if active_epoch <= 0 {
            return Err(StorageError::Validation(
                "no active epoch to compact".to_string(),
            ));
        }

        self.runtime.set_updating().await;

        // Build the compacted rows from the published base shard and its
        // delta chain. No mutable full graph is constructed; the layered
        // view exports canonical rows through the same read surface used by
        // queries.
        let config_fingerprint = {
            let manifest = self
                .store
                .snapshot_manifest(project_id, active_epoch)
                .await
                .map_err(|error| {
                    StorageError::Validation(format!("cannot read manifest: {error}"))
                })?
                .ok_or_else(|| {
                    StorageError::Validation("active epoch manifest missing".to_string())
                })?;
            manifest.config_fingerprint
        };
        let base_epoch = self
            .store
            .find_base_epoch(project_id, active_epoch)
            .unwrap_or(Some(active_epoch))
            .unwrap_or(active_epoch);
        let base = match RelationSnapshotIndex::from_store(
            Arc::new(self.store.clone()),
            project_id,
            base_epoch,
            cce_types::ShardQueryConfig::default(),
        ) {
            Ok(base) => base,
            Err(error) => {
                self.runtime.report_failure(error.to_string()).await;
                return Err(error);
            }
        };
        let deltas = match self
            .store
            .get_delta_chain(project_id, base_epoch, active_epoch)
        {
            Ok(deltas) => deltas,
            Err(error) => {
                self.runtime.report_failure(error.to_string()).await;
                return Err(error);
            }
        };
        let layered = LayeredSnapshotIndex::with_deltas(
            Arc::new(base),
            deltas.into_iter().map(Arc::new).collect(),
        );
        let canonical = match layered.to_canonical_snapshot(config_fingerprint) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.runtime.report_failure(error.clone()).await;
                return Err(StorageError::Validation(error));
            }
        };
        let artifact = match cce_types::CanonicalSnapshotArtifact::from_snapshot(
            &canonical,
            Default::default(),
        ) {
            Ok(artifact) => artifact,
            Err(error) => {
                self.runtime.report_failure(error.to_string()).await;
                return Err(error);
            }
        };
        // The merged snapshot is a full base; `base_relation_epoch` is only
        // meaningful for `publish()`'s CAS check and is not persisted, so
        // leaving it unset keeps the written fingerprint identical to the
        // one recomputed on load.

        // Publish as a full snapshot (allocates new epoch, writes full tables)
        let integrity = Self::integrity(&artifact);
        let epoch = match self
            .writer
            .allocate_and_write_artifact(project_id, operation_id, &artifact)
            .await
        {
            Ok(epoch) => epoch,
            Err(error) => {
                self.runtime.report_failure(error.to_string()).await;
                return Err(error);
            }
        };

        let data_epoch = self.active_data_epoch(project_id).await?;
        if let Err(error) = self
            .store
            .manifest_activate(project_id, data_epoch, epoch, operation_id, None)
            .await
        {
            self.mark_candidate_failed(project_id, epoch, &error).await;
            self.runtime.report_failure(error.to_string()).await;
            return Err(error);
        }

        // GC old delta chain
        let _gc_count = self
            .store
            .snapshot_delete_manifests_except(project_id, epoch)
            .await
            .unwrap_or(0);

        // Publish the newly activated epoch as a sharded base-only snapshot.
        // The merged builder graph goes out of scope at the end of compaction.
        let snapshot_index = Arc::new(RelationSnapshotIndex::from_store(
            Arc::new(self.store.clone()),
            project_id,
            epoch,
            cce_types::ShardQueryConfig::default(),
        )?);
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
    async fn prepare_full_rebuild(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<RelationRebuildPreparation, StorageError> {
        let _guard = self.publish_lock.lock().await;
        let active_epoch = match self
            .store
            .project_meta_get_int(project_id, "active_relation_epoch")
            .await
        {
            Ok(epoch) => epoch,
            Err(error) => {
                tracing::debug!(%error, project_id, "No active relation epoch is recorded yet");
                0
            }
        };
        if active_epoch <= 0 {
            self.runtime.invalidate_snapshot().await;
            return Ok(RelationRebuildPreparation::default());
        }

        let already_disk_backed = self.runtime.get_snapshot().await.is_some_and(|snapshot| {
            snapshot.relation_epoch == active_epoch && snapshot.index.is_disk_backed()
        });
        if !already_disk_backed {
            let disk_index = RelationSnapshotIndex::from_store(
                Arc::new(self.store.clone()),
                project_id,
                active_epoch,
                cce_types::ShardQueryConfig::default(),
            );
            match disk_index {
                Ok(index) => {
                    let integrity = if index.function_count() == 0 {
                        SnapshotIntegrity::Empty
                    } else {
                        SnapshotIntegrity::Full
                    };
                    self.runtime
                        .publish_snapshot(Arc::new(index), active_epoch, integrity, None)
                        .await;
                }
                Err(error) => {
                    tracing::warn!(
                        project_id,
                        operation_id,
                        active_epoch,
                        %error,
                        "No disk-backed generation is available during relation rebuild"
                    );
                    self.runtime.invalidate_snapshot().await;
                    return Ok(RelationRebuildPreparation {
                        active_epoch,
                        disk_backed: false,
                    });
                }
            }
        }

        self.runtime.set_updating().await;
        Ok(RelationRebuildPreparation {
            active_epoch,
            disk_backed: true,
        })
    }

    async fn publish(
        &self,
        project_id: i64,
        operation_id: &str,
        artifact: &CanonicalSnapshotArtifact,
    ) -> Result<RelationPublication, StorageError> {
        let _guard = self.publish_lock.lock().await;

        // Compare-and-swap: if a base_epoch was supplied, verify the active
        // epoch has not advanced past it. This prevents concurrent hot-update
        // candidates built from the same baseline from silently overwriting
        // each other.
        if let Some(base_epoch) = artifact.header().base_relation_epoch {
            let active_epoch = self
                .store
                .project_meta_get_int(project_id, "active_relation_epoch")
                .await
                .map_err(|error| {
                    StorageError::Validation(format!("cannot read active epoch for CAS: {error}"))
                })?;
            if active_epoch != base_epoch {
                return Err(StorageError::epoch_conflict(active_epoch, base_epoch));
            }
        }

        self.runtime.set_updating().await;
        let integrity = Self::integrity(artifact);
        let shard_config = cce_types::ShardQueryConfig::default();

        // The artifact was produced by streaming canonical rows through the
        // external sort used for both fingerprints. Its counts and row shape
        // are the publication integrity check; no build graph is consulted.
        if let Err(error) = artifact.validate_rows() {
            self.runtime.report_failure(error.to_string()).await;
            return Err(error);
        }
        let epoch = match self
            .writer
            .allocate_and_write_artifact(project_id, operation_id, artifact)
            .await
        {
            Ok(epoch) => epoch,
            Err(error) => {
                self.runtime.report_failure(error.to_string()).await;
                return Err(error);
            }
        };

        let project_candidate = self.has_project_publication_candidate(project_id).await?;
        let published = if !project_candidate {
            let data_epoch = self.active_data_epoch(project_id).await?;
            if let Err(error) = self
                .store
                .manifest_activate(project_id, data_epoch, epoch, operation_id, None)
                .await
            {
                self.mark_candidate_failed(project_id, epoch, &error).await;
                self.runtime.report_failure(error.to_string()).await;
                return Err(error);
            }
            let published = Arc::new(RelationSnapshotIndex::from_store(
                Arc::new(self.store.clone()),
                project_id,
                epoch,
                shard_config,
            )?);
            self.runtime
                .publish_snapshot(
                    Arc::clone(&published),
                    epoch,
                    integrity,
                    Some(operation_id.to_string()),
                )
                .await;
            published
        } else {
            Arc::new(RelationSnapshotIndex::from_store(
                Arc::new(self.store.clone()),
                project_id,
                epoch,
                shard_config,
            )?)
        };
        Ok(RelationPublication {
            relation_epoch: epoch,
            snapshot: Arc::new(LayeredSnapshotIndex::new(published)),
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
            .store
            .project_meta_get_int(project_id, "active_relation_epoch")
            .await
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
                let base_epoch = self
                    .store
                    .find_base_epoch(project_id, delta.base_epoch)
                    .unwrap_or(Some(delta.base_epoch))
                    .unwrap_or(delta.base_epoch);
                let deltas = self
                    .store
                    .get_delta_chain(project_id, base_epoch, delta.base_epoch);
                match (
                    RelationSnapshotIndex::from_store(
                        Arc::new(self.store.clone()),
                        project_id,
                        base_epoch,
                        cce_types::ShardQueryConfig::default(),
                    ),
                    deltas,
                ) {
                    (Ok(base), Ok(deltas)) => LayeredSnapshotIndex::with_deltas(
                        Arc::new(base),
                        deltas.into_iter().map(Arc::new).collect(),
                    ),
                    (Err(error), _) | (_, Err(error)) => {
                        self.runtime.report_failure(error.to_string()).await;
                        return Err(error);
                    }
                }
            }
        };

        if let Err(error) = Self::validate_delta_scoped(&scoped_base, &delta) {
            self.runtime.report_failure(error.clone()).await;
            return Err(StorageError::Validation(format!(
                "relation delta {project_id}@{active_epoch} failed scoped validation: {error}"
            )));
        }

        let allocated = self
            .store
            .snapshot_allocate(project_id, operation_id, &delta.config_fingerprint)
            .await;
        let epoch = match allocated {
            Ok(epoch) => {
                self.store
                    .snapshot_write_delta(project_id, epoch, &delta)
                    .await?;
                epoch
            }
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

        let project_candidate = self.has_project_publication_candidate(project_id).await?;
        if !project_candidate {
            let data_epoch = self.active_data_epoch(project_id).await?;
            if let Err(error) = self
                .store
                .manifest_activate(project_id, data_epoch, epoch, operation_id, None)
                .await
            {
                self.mark_candidate_failed(project_id, epoch, &error).await;
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
        if !self
            .needs_compaction(project_id, &CompactionConfig::default())
            .await?
        {
            return Ok(());
        }
        // A data-side building candidate means another operation is in
        // flight; `compact_project` retires all non-active relation manifests
        // (`delete_manifests_except`), which would destroy an in-flight
        // operation's building delta. Defer compaction to the next call.
        if self.has_project_publication_candidate(project_id).await? {
            tracing::debug!(
                project_id,
                "Relation delta-chain compaction deferred: another publication candidate is in flight"
            );
            return Ok(());
        }
        let active_epoch = self
            .store
            .project_meta_get_int(project_id, "active_relation_epoch")
            .await
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
