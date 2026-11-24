//! SQLite adapter for the relation snapshot read port.

use cce_types::relation::RelationSnapshotStore;
use cce_types::{
    CanonicalRelationSnapshot, RelationShardComponents, RelationSnapshotManifest, ShardDirectory,
    SnapshotDelta, StorageError,
};

use crate::client::SqliteClient;
use crate::repo::relation_snapshot_repo::RelationSnapshotRepository;

/// Adapter exposing SQLite-backed relation snapshot reads through the core
/// [`RelationSnapshotStore`] port.
#[derive(Clone)]
pub struct SqliteSnapshotStore {
    sqlite: SqliteClient,
}

impl SqliteSnapshotStore {
    pub fn new(sqlite: SqliteClient) -> Self {
        Self { sqlite }
    }
}

impl RelationSnapshotStore for SqliteSnapshotStore {
    fn get_manifest(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<RelationSnapshotManifest>, StorageError> {
        let conn = self.sqlite.read_connection()?;
        RelationSnapshotRepository::get_manifest(&conn, project_id, epoch)
    }

    fn read_snapshot(
        &self,
        manifest: &RelationSnapshotManifest,
    ) -> Result<CanonicalRelationSnapshot, StorageError> {
        let conn = self.sqlite.read_connection()?;
        RelationSnapshotRepository::read_snapshot(&conn, manifest)
    }

    fn read_shard_directory(
        &self,
        project_id: i64,
        relation_epoch: i64,
    ) -> Result<ShardDirectory, StorageError> {
        let conn = self.sqlite.read_connection()?;
        RelationSnapshotRepository::read_shard_directory(&conn, project_id, relation_epoch)
    }

    fn read_shard_components(
        &self,
        project_id: i64,
        relation_epoch: i64,
        shard_id: i64,
    ) -> Result<Option<RelationShardComponents>, StorageError> {
        let conn = self.sqlite.read_connection()?;
        let manifest = RelationSnapshotRepository::get_manifest(&conn, project_id, relation_epoch)?
            .ok_or_else(|| {
                StorageError::validation(format!("relation epoch {relation_epoch} has no manifest"))
            })?;
        RelationSnapshotRepository::read_shard_components(&conn, &manifest, shard_id)
    }

    fn find_base_epoch(
        &self,
        project_id: i64,
        delta_epoch: i64,
    ) -> Result<Option<i64>, StorageError> {
        let conn = self.sqlite.read_connection()?;
        RelationSnapshotRepository::find_base_epoch(&conn, project_id, delta_epoch)
    }

    fn get_delta_chain(
        &self,
        project_id: i64,
        after_epoch: i64,
        up_to_epoch: i64,
    ) -> Result<Vec<SnapshotDelta>, StorageError> {
        let conn = self.sqlite.read_connection()?;
        RelationSnapshotRepository::get_delta_chain(&conn, project_id, after_epoch, up_to_epoch)
    }
}

impl RelationSnapshotStore for SqliteClient {
    fn get_manifest(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<RelationSnapshotManifest>, StorageError> {
        SqliteSnapshotStore::new(self.clone()).get_manifest(project_id, epoch)
    }

    fn read_snapshot(
        &self,
        manifest: &RelationSnapshotManifest,
    ) -> Result<CanonicalRelationSnapshot, StorageError> {
        SqliteSnapshotStore::new(self.clone()).read_snapshot(manifest)
    }

    fn read_shard_directory(
        &self,
        project_id: i64,
        relation_epoch: i64,
    ) -> Result<ShardDirectory, StorageError> {
        SqliteSnapshotStore::new(self.clone()).read_shard_directory(project_id, relation_epoch)
    }

    fn read_shard_components(
        &self,
        project_id: i64,
        relation_epoch: i64,
        shard_id: i64,
    ) -> Result<Option<RelationShardComponents>, StorageError> {
        SqliteSnapshotStore::new(self.clone()).read_shard_components(
            project_id,
            relation_epoch,
            shard_id,
        )
    }

    fn find_base_epoch(
        &self,
        project_id: i64,
        delta_epoch: i64,
    ) -> Result<Option<i64>, StorageError> {
        SqliteSnapshotStore::new(self.clone()).find_base_epoch(project_id, delta_epoch)
    }

    fn get_delta_chain(
        &self,
        project_id: i64,
        after_epoch: i64,
        up_to_epoch: i64,
    ) -> Result<Vec<SnapshotDelta>, StorageError> {
        SqliteSnapshotStore::new(self.clone()).get_delta_chain(project_id, after_epoch, up_to_epoch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::SqliteClient;
    use crate::repo::relation_snapshot_repo::RelationSnapshotRepository;
    use cce_types::RelationSnapshotState;

    fn test_sqlite() -> SqliteClient {
        let sqlite = SqliteClient::in_memory().expect("in-memory database should open");
        {
            let conn = sqlite
                .read_connection()
                .expect("test connection should open");
            conn.execute(
                "INSERT INTO projects
                    (id, name, root_path, config_file_path, created_at, updated_at)
                 VALUES (1, 'test', '/path/that/does/not/exist', '.cce/config.json', 1, 1)",
                [],
            )
            .expect("test project should be inserted");
        }
        sqlite
    }

    fn test_snapshot() -> CanonicalRelationSnapshot {
        CanonicalRelationSnapshot::new("config".to_string())
    }

    fn write_active(sqlite: &SqliteClient, project_id: i64) -> i64 {
        let epoch = sqlite
            .with_transaction(|tx| {
                RelationSnapshotRepository::allocate_building(tx, project_id, "operation", "config")
            })
            .expect("epoch should allocate");
        let snapshot = test_snapshot();
        let artifact =
            cce_types::CanonicalSnapshotArtifact::from_snapshot(&snapshot, Default::default())
                .expect("artifact should build");
        sqlite
            .with_transaction(|tx| {
                RelationSnapshotRepository::write_snapshot_and_mark_ready(
                    tx, project_id, epoch, &artifact,
                )?;
                RelationSnapshotRepository::activate(tx, project_id, epoch)
            })
            .expect("epoch should activate");
        epoch
    }

    #[test]
    fn adapter_reads_active_snapshot_written_by_repo() {
        let sqlite = test_sqlite();
        let epoch = write_active(&sqlite, 1);
        let store = SqliteSnapshotStore::new(sqlite);

        let manifest = store
            .get_manifest(1, epoch)
            .expect("lookup should not fail")
            .expect("manifest should exist");
        assert_eq!(manifest.state, RelationSnapshotState::Active);
        assert_eq!(manifest.schema_version, test_snapshot().schema_version);

        let loaded = store
            .read_snapshot(&manifest)
            .expect("snapshot should read back");
        assert_eq!(loaded.fingerprint(), test_snapshot().fingerprint());

        assert_eq!(
            store
                .find_base_epoch(1, epoch)
                .expect("lookup should not fail"),
            Some(epoch)
        );
    }

    #[test]
    fn adapter_reads_delta_chain() {
        let sqlite = test_sqlite();
        let base_epoch = write_active(&sqlite, 1);
        let delta_epoch = sqlite
            .with_transaction(|tx| {
                RelationSnapshotRepository::allocate_building(tx, 1, "operation-delta", "config")
            })
            .expect("delta epoch should allocate");
        let delta = SnapshotDelta {
            epoch: delta_epoch,
            base_epoch,
            config_fingerprint: "config".to_string(),
            removed_files: Vec::new(),
            added_files: Vec::new(),
            removed_entities: Vec::new(),
            added_entities: Vec::new(),
            removed_relations: Vec::new(),
            added_relations: Vec::new(),
            import_diffs: Vec::new(),
            export_diffs: Vec::new(),
            file_relation_diffs: Vec::new(),
            relation_edges_dropped_unbounded: 0,
            renamed_entities: Vec::new(),
            dependency_diffs: Vec::new(),
        };
        sqlite
            .with_transaction(|tx| {
                RelationSnapshotRepository::write_delta(tx, 1, delta_epoch, &delta)
            })
            .expect("delta should write");

        let store = SqliteSnapshotStore::new(sqlite);
        let manifest = store
            .get_manifest(1, delta_epoch)
            .expect("lookup should not fail")
            .expect("delta manifest should exist");
        assert_eq!(manifest.state, RelationSnapshotState::Delta);

        assert_eq!(
            store
                .find_base_epoch(1, delta_epoch)
                .expect("lookup should not fail"),
            Some(base_epoch)
        );

        let chain = store
            .get_delta_chain(1, base_epoch, delta_epoch)
            .expect("delta chain should read");
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].epoch, delta_epoch);
        assert_eq!(chain[0].base_epoch, base_epoch);
    }

    #[test]
    fn adapter_reads_shard_directory_and_components() {
        use cce_types::{
            CanonicalEntity, CanonicalFile, CanonicalRelation, CanonicalRelationTarget, EntityKind,
            RelationType, SnapshotArtifactOptions, StableSymbolKey,
        };

        let sqlite = test_sqlite();
        let mut snapshot = CanonicalRelationSnapshot::new("config".to_string());
        for path in ["a.rs", "b.rs"] {
            snapshot.files.push(CanonicalFile {
                path: path.to_string(),
                language: "rust".to_string(),
                input_hash: format!("hash-{path}"),
                file_size: 1,
                imports: Vec::new(),
                exports: Vec::new(),
            });
        }
        let key = |path: &str, name: &str| {
            StableSymbolKey::new(path, name, EntityKind::Function, &format!("fn {name}()"))
        };
        let entity = |path: &str, name: &str, id: u64| CanonicalEntity {
            key: key(path, name),
            entity_id: Some(id),
            name: name.to_string(),
            signature: format!("fn {name}()"),
            parameters: Vec::new(),
            return_type: None,
            span: Default::default(),
            depth: 0,
            parent: None,
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: Default::default(),
            metadata: Default::default(),
            is_stdlib: false,
            stdlib_category: None,
            subtype: None,
        };
        let a_key = key("a.rs", "alpha");
        let b_key = key("b.rs", "beta");
        snapshot.entities.push(entity("a.rs", "alpha", 1));
        snapshot.entities.push(entity("b.rs", "beta", 2));
        snapshot.relations.push(CanonicalRelation {
            caller: a_key.clone(),
            target: CanonicalRelationTarget::Internal { key: b_key },
            raw_target: "beta".to_string(),
            relation_type: RelationType::DirectCall,
            span: Default::default(),
            stdlib_category: None,
            overload_signature: None,
            callee_symbol: None,
            owner_type: None,
            call_context: Default::default(),
            call_frequency: 1,
            cfg_condition: None,
        });

        let epoch = sqlite
            .with_transaction(|tx| {
                RelationSnapshotRepository::allocate_building(tx, 1, "sharded", "config")
            })
            .expect("epoch should allocate");
        let artifact = cce_types::CanonicalSnapshotArtifact::from_snapshot(
            &snapshot,
            SnapshotArtifactOptions::default().with_shard_items(1),
        )
        .expect("artifact should build");
        sqlite
            .with_transaction(|tx| {
                RelationSnapshotRepository::write_snapshot_and_mark_ready(tx, 1, epoch, &artifact)
            })
            .expect("snapshot should persist");

        let store = SqliteSnapshotStore::new(sqlite);
        let directory = store
            .read_shard_directory(1, epoch)
            .expect("shard directory should read");
        assert!(
            directory.descriptors.len() >= 2,
            "small budget must split shards"
        );
        assert_eq!(directory.file_shards.len(), 2);
        assert_eq!(directory.entity_locations.len(), 2);
        assert_eq!(directory.target_callers.len(), 1);

        let mut assembled = CanonicalRelationSnapshot::new("config".to_string());
        for descriptor in &directory.descriptors {
            let components = store
                .read_shard_components(1, epoch, descriptor.shard_id)
                .expect("shard should read")
                .expect("shard should exist");
            assembled.files.extend(components.files);
            assembled.entities.extend(components.entities);
            assembled.relations.extend(components.relations);
            assembled.dependencies.extend(components.dependencies);
        }
        assembled.normalize();
        assert_eq!(assembled.fingerprint(), snapshot.fingerprint());
        let _ = a_key;
    }

    #[test]
    fn adapter_reports_missing_manifest_as_none() {
        let store = SqliteSnapshotStore::new(test_sqlite());
        assert!(
            store
                .get_manifest(1, 999)
                .expect("lookup should not fail")
                .is_none()
        );
    }
}
