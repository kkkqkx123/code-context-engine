//! Process-internal cache of the materialized relation base index plus the
//! chain of deltas published on top of it.
//!
//! Hot updates used to reload the full base state from SQLite on every
//! candidate build and publication (two full loads per update), making the
//! hot-update cost O(project size) in the I/O dimension. This cache keeps the
//! current active epoch's state in process so hot updates reuse it instead of
//! rebuilding it.
//!
//! # Layered model
//!
//! The cached state is a materialized base snapshot (the chain head, held as
//! a zero-copy [`RelationSnapshotIndex`]) plus an ordered chain of
//! [`SnapshotDelta`]s published since the head was materialized.
//! [`RelationBaseCache::get_or_load`] hands out a read-only
//! [`LayeredSnapshotIndex`] view over that state (zero-copy base share,
//! shallow chain copy); [`RelationBaseCache::update`] appends a published
//! delta to the chain instead of re-materializing the whole project. Compaction is the
//! server-side `compact_project`'s job (`CompactionConfig` with
//! `max_chain_length=10` / `max_delta_ratio=0.2`); when it succeeds the
//! `notify_compaction` mechanism rebuilds the cached entry, so per-update
//! hot-update cost stays O(change size).
//!
//! # Ownership invariant
//!
//! The cached `base_index` is the canonical owner of its maps. It is never
//! handed out for mutation: views share it read-only, and the candidate build
//! works on an empty index with the view as the cross-file symbol context.
//! Compaction is the only place a full copy is produced (deliberately O(project)
//! once), and it happens inside the cache under the entry's exclusive access.
//! Violating this invariant corrupts the cache for all later updates.
//!
//! # CAS semantics
//!
//! `get_or_load` returns the cached entry only when its epoch (the chain tail)
//! matches the requested epoch. When another actor (another instance, a full
//! index in another process, or a manual operation) advances the active epoch,
//! the cache is stale and the base is rebuilt from SQLite exactly once,
//! replacing the cached entry. This matches the design goal: rebuild only on
//! CAS conflicts; single-instance hot updates hit the cache 100% of the time.
//!
//! `update` only appends when the published epoch continues the chain
//! (`epoch == cached.epoch + 1`); any discontinuity (a CAS retry that lost to
//! another actor) drops the entry so the next `get_or_load` rebuilds from the
//! store.

use std::sync::Arc;

use cce_relation::index::{LayeredSnapshotIndex, RelationIndex, RelationSnapshotIndex};
use cce_types::SnapshotDelta;
use cce_types::StorageError;
use cce_types::relation::RelationSnapshotStore;
use dashmap::DashMap;

/// One cached state: a materialized base plus the deltas published since.
#[derive(Debug)]
struct CachedBase {
    /// The epoch the chain tail materializes (the cached state's epoch).
    epoch: i64,
    /// Zero-copy snapshot of the materialized base index (chain head).
    /// Never handed out mutably.
    base_snapshot: Arc<RelationSnapshotIndex>,
    /// Chain of published deltas, epochs strictly ascending.
    deltas: Vec<Arc<SnapshotDelta>>,
}

/// Process-internal cache of relation base states, keyed by project ID.
/// Thread-safe; entries are updated atomically on publish.
#[derive(Debug, Default)]
pub struct RelationBaseCache {
    projects: DashMap<i64, CachedBase>,
}

impl RelationBaseCache {
    /// Create an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the read-only layered view of the state at `epoch`, loading the
    /// materialized base from `store` only when the cache misses or holds a
    /// stale epoch.
    ///
    /// The returned view shares the cached base zero-copy; it must only be
    /// used read-only (see the module documentation).
    pub fn get_or_load(
        &self,
        store: &dyn RelationSnapshotStore,
        project_id: i64,
        epoch: i64,
    ) -> Result<Arc<LayeredSnapshotIndex>, StorageError> {
        if let Some(cached) = self.projects.get(&project_id)
            && cached.epoch == epoch
        {
            return Ok(Arc::new(LayeredSnapshotIndex::with_deltas(
                Arc::clone(&cached.base_snapshot),
                cached.deltas.clone(),
            )));
        }
        let loaded = cce_relation::index::snapshot_loader::RelationSnapshotLoader::load(
            store, project_id, epoch,
        )?;
        // Zero-copy: the snapshot shares the loaded index's maps; the mutable
        // index itself is dropped immediately after.
        let base_snapshot = Arc::new(RelationSnapshotIndex::from_index_shared(&loaded));
        drop(loaded);
        self.projects.insert(
            project_id,
            CachedBase {
                epoch,
                base_snapshot: Arc::clone(&base_snapshot),
                deltas: Vec::new(),
            },
        );
        tracing::debug!(
            project_id,
            epoch,
            "Relation base cache miss: materialized base from store"
        );
        Ok(Arc::new(LayeredSnapshotIndex::new(base_snapshot)))
    }

    /// Append a successfully published delta to the chain for `project_id`,
    /// making `epoch` the new chain tail.
    ///
    /// When the entry is missing or `epoch` does not continue the chain
    /// (`epoch != cached.epoch + 1`, e.g. a CAS conflict lost to another
    /// actor), the entry is dropped and the next `get_or_load` rebuilds the
    /// state from the store.
    ///
    /// Compaction is NOT triggered here. The server-side `compact_project`
    /// is the sole materialization point (see `maybe_compact`).
    pub fn update(&self, project_id: i64, epoch: i64, delta: SnapshotDelta) {
        if let Some(mut cached) = self.projects.get_mut(&project_id) {
            if cached.epoch == epoch - 1 {
                cached.deltas.push(Arc::new(delta));
                cached.epoch = epoch;
                tracing::debug!(
                    project_id,
                    epoch,
                    chain_len = cached.deltas.len(),
                    "Relation base cache appended published delta"
                );
                return;
            }
            tracing::warn!(
                project_id,
                epoch,
                cached_epoch = cached.epoch,
                "Relation base cache update epoch discontinuity: dropping entry for cold reload"
            );
        }
        self.projects.remove(&project_id);
    }

    /// Drop the cached entry for a project (project deletion / runtime clear).
    pub fn remove(&self, project_id: i64) {
        self.projects.remove(&project_id);
    }

    /// Rebuild the materialized base from a compacted snapshot (called after
    /// server-side `compact_project` merges the chain into a new base).
    ///
    /// The caller (the server publisher) has already merged the chain and
    /// activated the new base in SQLite. This method replaces the cached
    /// entry with the compacted base and resets the chain.
    pub fn rebuild_from_compacted(
        &self,
        project_id: i64,
        epoch: i64,
        compacted_base: Arc<RelationIndex>,
    ) {
        let base_snapshot = Arc::new(RelationSnapshotIndex::from_index_shared(&compacted_base));
        self.projects.insert(
            project_id,
            CachedBase {
                epoch,
                base_snapshot,
                deltas: Vec::new(),
            },
        );
        tracing::debug!(
            project_id,
            epoch,
            "Relation base cache rebuilt from compacted snapshot"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_relation::index::{RelationDeltaOps, RelationIndexView, SnapshotFileQueryOps};
    use cce_types::{
        AddedEntity, CanonicalEntity, CanonicalFile, CanonicalRelationSnapshot, Entity, EntityId,
        EntityKind, RelationSnapshotManifest, RelationSnapshotState, Span, StableSymbolKey,
        StorageError,
    };
    use std::collections::HashMap;

    struct StubStore {
        snapshots: HashMap<i64, CanonicalRelationSnapshot>,
        loads: std::sync::atomic::AtomicUsize,
    }

    impl StubStore {
        fn manifest(&self, epoch: i64) -> RelationSnapshotManifest {
            let snapshot = self.snapshots.get(&epoch).expect("snapshot should exist");
            RelationSnapshotManifest {
                project_id: 1,
                relation_epoch: epoch,
                operation_id: "test".to_string(),
                state: RelationSnapshotState::Active,
                schema_version: snapshot.schema_version,
                parser_version: snapshot.parser_version,
                resolver_version: snapshot.resolver_version,
                path_normalization_version: snapshot.path_normalization_version,
                config_fingerprint: snapshot.config_fingerprint.clone(),
                input_fingerprint: Some(snapshot.input_fingerprint()),
                snapshot_fingerprint: Some(snapshot.fingerprint()),
                file_count: Some(snapshot.files.len()),
                entity_count: Some(snapshot.entities.len()),
                relation_count: Some(snapshot.relations.len()),
                dependency_count: Some(snapshot.dependencies.len()),
                failure_reason: None,
                symbol_key_conflict_count: snapshot.build_metadata.symbol_key_conflict_count,
                symbol_key_conflict_samples: snapshot
                    .build_metadata
                    .symbol_key_conflict_samples
                    .clone(),
            }
        }
    }

    impl RelationSnapshotStore for StubStore {
        fn get_manifest(
            &self,
            project_id: i64,
            epoch: i64,
        ) -> Result<Option<RelationSnapshotManifest>, StorageError> {
            Ok((project_id == 1).then(|| self.manifest(epoch)))
        }

        fn read_snapshot(
            &self,
            manifest: &RelationSnapshotManifest,
        ) -> Result<CanonicalRelationSnapshot, StorageError> {
            self.loads
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.snapshots
                .get(&manifest.relation_epoch)
                .cloned()
                .ok_or_else(|| {
                    StorageError::Query(format!(
                        "no snapshot for epoch {}",
                        manifest.relation_epoch
                    ))
                })
        }

        fn find_base_epoch(
            &self,
            project_id: i64,
            delta_epoch: i64,
        ) -> Result<Option<i64>, StorageError> {
            Ok((project_id == 1).then_some(delta_epoch))
        }

        fn get_delta_chain(
            &self,
            project_id: i64,
            after_epoch: i64,
            up_to_epoch: i64,
        ) -> Result<Vec<cce_types::SnapshotDelta>, StorageError> {
            let _ = (project_id, after_epoch, up_to_epoch);
            Ok(Vec::new())
        }
    }

    /// A base snapshot with several files/entities so the compaction volume
    /// ratio stays below the threshold across a full chain-length worth of
    /// small deltas (so pure chain-append and the length-branch compaction
    /// can be tested deterministically).
    fn snapshot_with_files(epoch: i64) -> CanonicalRelationSnapshot {
        let mut snapshot = CanonicalRelationSnapshot::new("config".to_string());
        let files: Vec<String> = (0..10)
            .map(|i| {
                if i == 0 {
                    "src/lib.rs".to_string()
                } else {
                    format!("src/f{i}.rs")
                }
            })
            .collect();
        for (i, file) in files.iter().enumerate() {
            let name = format!("fn_{i}");
            snapshot.files.push(CanonicalFile {
                path: file.clone(),
                language: "rust".to_string(),
                input_hash: format!("hash-{epoch}"),
                file_size: 1,
                imports: Vec::new(),
                exports: Vec::new(),
            });
            snapshot.entities.push(CanonicalEntity {
                key: StableSymbolKey::new(
                    file,
                    &name,
                    EntityKind::Function,
                    &format!("fn {name}()"),
                ),
                entity_id: Some(i as u64 + 1),
                name: name.clone(),
                signature: format!("fn {name}()"),
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
            });
        }
        snapshot
    }

    fn store_with(epochs: &[i64]) -> StubStore {
        let mut snapshots = HashMap::new();
        for &epoch in epochs {
            snapshots.insert(epoch, snapshot_with_files(epoch));
        }
        StubStore {
            snapshots,
            loads: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// A delta that adds one entity in a new file.
    fn delta_with_entity(
        epoch: i64,
        base_epoch: i64,
        id: u64,
        name: &str,
        file: &str,
    ) -> SnapshotDelta {
        let entity = Entity {
            id: EntityId(id),
            kind: EntityKind::Function,
            name: name.to_string(),
            signature: format!("fn {name}()"),
            ..Default::default()
        };
        SnapshotDelta {
            epoch,
            base_epoch,
            config_fingerprint: "config".to_string(),
            removed_files: vec![],
            added_files: vec![],
            removed_entities: vec![],
            added_entities: vec![AddedEntity {
                entity,
                symbol_key: StableSymbolKey::new(
                    file,
                    name,
                    EntityKind::Function,
                    &format!("fn {name}()"),
                ),
                file_path: file.to_string(),
            }],
            removed_relations: vec![],
            added_relations: vec![],
            import_diffs: vec![],
            export_diffs: vec![],
            file_relation_diffs: Vec::new(),
            relation_edges_dropped_unbounded: 0,
            dependency_diffs: vec![],
            renamed_entities: vec![],
        }
    }

    #[test]
    fn cache_hit_avoids_reload_and_matches_epoch() {
        let store = store_with(&[1]);
        let cache = RelationBaseCache::new();

        let first = cache
            .get_or_load(&store, 1, 1)
            .expect("load should succeed");
        assert!(first.file_contains("src/lib.rs"));
        assert!(first.function_contains(EntityId(1)));

        let second = cache
            .get_or_load(&store, 1, 1)
            .expect("cache hit should succeed");
        assert!(
            Arc::ptr_eq(&first.base, &second.base),
            "cache hit must share the base index"
        );
        assert_eq!(
            store.loads.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "cache hit must not reload from the store"
        );
    }

    #[test]
    fn stale_epoch_reloads_and_refreshes() {
        let store = store_with(&[1, 2]);
        let cache = RelationBaseCache::new();

        let epoch1 = cache
            .get_or_load(&store, 1, 1)
            .expect("epoch 1 should load");
        // A full publish advanced the epoch outside the cache: reload is required.
        let epoch2 = cache
            .get_or_load(&store, 1, 2)
            .expect("epoch 2 should load");
        assert!(!Arc::ptr_eq(&epoch1.base, &epoch2.base));

        // The refreshed entry serves the new epoch and its file hash.
        let epoch2_again = cache.get_or_load(&store, 1, 2).expect("epoch 2 hit");
        assert!(Arc::ptr_eq(&epoch2.base, &epoch2_again.base));
        let file = epoch2_again
            .base
            .get_file("src/lib.rs")
            .expect("file should exist");
        assert_eq!(file.file_hash, "hash-2");
    }

    #[test]
    fn update_appends_delta_chain() {
        let store = store_with(&[1]);
        let cache = RelationBaseCache::new();

        cache
            .get_or_load(&store, 1, 1)
            .expect("epoch 1 should load");

        // Simulate two successful delta publishes appending to the chain.
        let d2 = delta_with_entity(2, 1, 100, "beta", "src/beta.rs");
        let d3 = delta_with_entity(3, 2, 200, "gamma", "src/gamma.rs");
        cache.update(1, 2, d2.clone());
        cache.update(1, 3, d3.clone());

        let base = cache
            .get_or_load(&store, 1, 3)
            .expect("epoch 3 should hit cache");
        assert_eq!(base.deltas.len(), 2, "both deltas appended to the chain");
        assert!(base.file_contains("src/lib.rs"));
        assert!(base.function_contains(EntityId(1)));
        assert!(base.function_contains(EntityId(100)));
        assert!(base.function_contains(EntityId(200)));
        assert_eq!(
            base.entity_file_of(EntityId(200)).as_deref(),
            Some("src/gamma.rs")
        );
        // Only the initial epoch-1 load happened: the epoch-3 cache entry
        // served the update without any additional store read.
        assert_eq!(store.loads.load(std::sync::atomic::Ordering::Relaxed), 1);

        // Chain semantics match an independent materialization of base + deltas.
        let base_index =
            cce_relation::index::snapshot_loader::RelationSnapshotLoader::load(&store, 1, 1)
                .expect("base should load");
        let materialized = base_index.detached_clone();
        materialized.apply_delta(&d2);
        materialized.apply_delta(&d3);
        assert_eq!(
            base.function_contains(EntityId(1)),
            materialized.function_contains(EntityId(1))
        );
        assert_eq!(
            base.function_contains(EntityId(200)),
            materialized.function_contains(EntityId(200))
        );
        assert_eq!(
            base.entity_file_of(EntityId(200)),
            materialized.entity_file_of(EntityId(200))
        );
    }

    #[test]
    fn cache_epoch_mismatch_reloads() {
        let store = store_with(&[1, 2]);
        let cache = RelationBaseCache::new();

        let epoch1 = cache
            .get_or_load(&store, 1, 1)
            .expect("epoch 1 should load");
        assert_eq!(store.loads.load(std::sync::atomic::Ordering::Relaxed), 1);

        // Requesting an epoch that differs from the cached chain tail forces a
        // cold reload from the store and replaces the cached entry.
        let epoch2 = cache
            .get_or_load(&store, 1, 2)
            .expect("epoch 2 should load");
        assert_eq!(
            store.loads.load(std::sync::atomic::Ordering::Relaxed),
            2,
            "mismatched epoch must reload from the store"
        );
        assert!(!Arc::ptr_eq(&epoch1.base, &epoch2.base));
        assert!(
            epoch2.deltas.is_empty(),
            "cold reload starts an empty chain"
        );

        // The replaced entry serves the new epoch without another reload.
        let epoch2_again = cache.get_or_load(&store, 1, 2).expect("epoch 2 hit");
        assert_eq!(
            store.loads.load(std::sync::atomic::Ordering::Relaxed),
            2,
            "replaced entry must serve without reload"
        );
        assert!(Arc::ptr_eq(&epoch2.base, &epoch2_again.base));
    }

    #[test]
    fn update_with_discontinuous_epoch_drops_cache() {
        let store = store_with(&[1, 5]);
        let cache = RelationBaseCache::new();

        cache
            .get_or_load(&store, 1, 1)
            .expect("epoch 1 should load");
        // A delta whose epoch does not continue the chain (an abnormal
        // scenario): the entry is dropped so the next get_or_load cold-reloads.
        cache.update(1, 5, delta_with_entity(5, 4, 100, "beta", "src/beta.rs"));

        let view = cache
            .get_or_load(&store, 1, 5)
            .expect("epoch 5 should cold reload");
        assert_eq!(
            store.loads.load(std::sync::atomic::Ordering::Relaxed),
            2,
            "discontinuous update forces a cold reload"
        );
        // The store's epoch-5 snapshot has fn_0..fn_9 (id 1 = fn_0); the
        // dropped delta's entity must not leak into the reloaded state.
        assert!(view.function_contains(EntityId(1)));
        assert!(!view.function_contains(EntityId(100)));
    }

    #[test]
    fn rebuild_from_compacted_preserves_state() {
        let store = store_with(&[1]);
        let cache = RelationBaseCache::new();

        cache
            .get_or_load(&store, 1, 1)
            .expect("epoch 1 should load");

        // Append several deltas to the chain.
        for i in 0..5 {
            let epoch = 2 + i as i64;
            cache.update(
                1,
                epoch,
                delta_with_entity(
                    epoch,
                    epoch - 1,
                    100 + i as u64,
                    &format!("fn{i}"),
                    &format!("src/f{i}.rs"),
                ),
            );
        }

        // Simulate server-side compaction: materialize the base + chain.
        let base_index =
            cce_relation::index::snapshot_loader::RelationSnapshotLoader::load(&store, 1, 1)
                .expect("base should load");
        let merged = base_index.detached_clone();
        for d in cache
            .get_or_load(&store, 1, 6)
            .expect("epoch 6 should hit cache")
            .deltas
            .iter()
        {
            merged.apply_delta(d);
        }
        let compacted_arc = Arc::new(merged);

        // Rebuild the cache from the compacted snapshot.
        cache.rebuild_from_compacted(1, 6, Arc::clone(&compacted_arc));

        let view = cache
            .get_or_load(&store, 1, 6)
            .expect("epoch 6 should hit cache after compact");
        assert!(
            view.deltas.is_empty(),
            "chain must be empty after compact rebuild"
        );
        // State preserved: every added entity is visible through the base.
        for i in 0..5 {
            assert!(
                view.function_contains(EntityId(100 + i as u64)),
                "compacted entity {i} must remain visible"
            );
        }
        assert!(
            view.function_contains(EntityId(1)),
            "base entity must remain visible"
        );
        assert!(SnapshotFileQueryOps::contains_file(
            &*view.base,
            "src/lib.rs"
        ));

        // Updates continue on the compacted base.
        cache.update(1, 7, delta_with_entity(7, 6, 200, "after", "src/after.rs"));
        let view_after = cache
            .get_or_load(&store, 1, 7)
            .expect("epoch 7 should hit cache");
        assert_eq!(view_after.deltas.len(), 1);
        assert!(view_after.function_contains(EntityId(200)));
        assert!(
            view_after.function_contains(EntityId(100)),
            "compacted entity still visible after the next update"
        );
    }

    #[test]
    fn cache_update_shares_base_arc_until_compaction() {
        let store = store_with(&[1]);
        let cache = RelationBaseCache::new();

        let initial = cache
            .get_or_load(&store, 1, 1)
            .expect("epoch 1 should load");
        let base_arc = Arc::clone(&initial.base);

        // Several chain appends must not re-materialize the base: the Arc
        // pointer stays identical (zero full copies on the hot path).
        for i in 0..5 {
            let epoch = 2 + i as i64;
            cache.update(
                1,
                epoch,
                delta_with_entity(
                    epoch,
                    epoch - 1,
                    100 + i as u64,
                    &format!("fn{i}"),
                    &format!("src/f{i}.rs"),
                ),
            );
        }
        let after = cache
            .get_or_load(&store, 1, 6)
            .expect("epoch 6 should hit cache");
        assert!(
            Arc::ptr_eq(&base_arc, &after.base),
            "chain appends must not replace the base Arc (no full materialization)"
        );
        assert_eq!(after.deltas.len(), 5);
    }

    #[test]
    fn remove_drops_project_entry() {
        let store = store_with(&[1]);
        let cache = RelationBaseCache::new();

        cache
            .get_or_load(&store, 1, 1)
            .expect("epoch 1 should load");
        cache.remove(1);
        // After removal the next get_or_load reloads from the store.
        let reloaded = cache
            .get_or_load(&store, 1, 1)
            .expect("reload should succeed");
        assert!(reloaded.file_contains("src/lib.rs"));
        assert_eq!(
            store.loads.load(std::sync::atomic::Ordering::Relaxed),
            2,
            "removal forces a reload"
        );
    }
}
