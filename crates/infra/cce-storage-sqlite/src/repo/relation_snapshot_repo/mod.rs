//! Normalized persistence for final canonical relationship epochs.

mod snapshot_delta;
mod snapshot_reader;
mod snapshot_writer;

use std::collections::HashMap;

use cce_types::{
    RELATION_PARSER_VERSION, RELATION_PATH_NORMALIZATION_VERSION, RELATION_RESOLVER_VERSION,
    RELATION_SNAPSHOT_SCHEMA_VERSION, StorageError,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Serialize, de::DeserializeOwned};

pub use cce_types::relation::{RelationSnapshotManifest, RelationSnapshotState};

pub struct RelationSnapshotRepository;

impl RelationSnapshotRepository {
    pub fn get_manifest_by_operation(
        conn: &Connection,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Option<RelationSnapshotManifest>, StorageError> {
        let epoch = conn
            .query_row(
                "SELECT relation_epoch FROM relation_snapshot_manifest
                 WHERE project_id = ?1 AND operation_id = ?2",
                params![project_id, operation_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(query_error)?;
        epoch
            .map(|value| Self::get_manifest(conn, project_id, value))
            .transpose()
            .map(Option::flatten)
    }

    pub fn allocate_building(
        tx: &Transaction<'_>,
        project_id: i64,
        operation_id: &str,
        config_fingerprint: &str,
    ) -> Result<i64, StorageError> {
        let existing = tx
            .query_row(
                "SELECT relation_epoch, state FROM relation_snapshot_manifest
                 WHERE project_id = ?1 AND operation_id = ?2",
                params![project_id, operation_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(query_error)?;
        if let Some((epoch, state)) = existing {
            if state == RelationSnapshotState::Failed.as_str() {
                tx.execute(
                    "UPDATE relation_snapshot_manifest
                     SET state = 'building', input_fingerprint = NULL,
                         snapshot_fingerprint = NULL, file_count = NULL,
                         entity_count = NULL, relation_count = NULL,
                         dependency_count = NULL, validated_at = NULL,
                         activated_at = NULL, failure_reason = NULL,
                         symbol_key_conflict_count = 0,
                         symbol_key_conflict_samples_json = NULL
                     WHERE project_id = ?1 AND relation_epoch = ?2",
                    params![project_id, epoch],
                )
                .map_err(query_error)?;
            }
            return Ok(epoch);
        }
        let epoch = tx
            .query_row(
                "SELECT MAX(value) FROM (
                    SELECT COALESCE(MAX(relation_epoch), 0) AS value
                    FROM relation_snapshot_manifest WHERE project_id = ?1
                    UNION ALL
                    SELECT COALESCE(MAX(CAST(value AS INTEGER)), 0) AS value
                    FROM project_meta
                    WHERE project_id = ?1 AND key = 'active_relation_epoch'
                )",
                params![project_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(query_error)?
            + 1;
        tx.execute(
            "INSERT INTO relation_snapshot_manifest (
                project_id, relation_epoch, operation_id, state, schema_version,
                parser_version, resolver_version, path_normalization_version,
                config_fingerprint, created_at
             ) VALUES (?1, ?2, ?3, 'building', ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                project_id,
                epoch,
                operation_id,
                RELATION_SNAPSHOT_SCHEMA_VERSION,
                RELATION_PARSER_VERSION,
                RELATION_RESOLVER_VERSION,
                RELATION_PATH_NORMALIZATION_VERSION,
                config_fingerprint,
                chrono::Utc::now().timestamp()
            ],
        )
        .map_err(query_error)?;
        Ok(epoch)
    }

    pub fn activate(tx: &Transaction<'_>, project_id: i64, epoch: i64) -> Result<(), StorageError> {
        let now = chrono::Utc::now().timestamp();
        let current_state = tx
            .query_row(
                "SELECT state FROM relation_snapshot_manifest
                 WHERE project_id = ?1 AND relation_epoch = ?2",
                params![project_id, epoch],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(query_error)?;
        match current_state.as_deref() {
            Some("active") => return Ok(()),
            Some("delta") => {
                // Delta epochs stay as 'delta' — just update project_meta
            }
            Some("ready") => {
                let changed = tx
                    .execute(
                        "UPDATE relation_snapshot_manifest SET state = 'active', activated_at = ?3
                         WHERE project_id = ?1 AND relation_epoch = ?2 AND state = 'ready'",
                        params![project_id, epoch, now],
                    )
                    .map_err(query_error)?;
                if changed != 1 {
                    return Err(StorageError::Transaction(format!(
                        "epoch {epoch} is not ready"
                    )));
                }
                tx.execute(
                    "UPDATE relation_snapshot_manifest SET state = 'ready'
                     WHERE project_id = ?1 AND relation_epoch <> ?2 AND state = 'active'",
                    params![project_id, epoch],
                )
                .map_err(query_error)?;
            }
            _ => {
                return Err(StorageError::Transaction(format!(
                    "epoch {epoch} is not activatable (state: {:?})",
                    current_state
                )));
            }
        }
        tx.execute(
            "INSERT OR REPLACE INTO project_meta (project_id, key, value, created_at, updated_at)
             VALUES (?1, 'active_relation_epoch', ?2,
                COALESCE((SELECT created_at FROM project_meta
                          WHERE project_id = ?1 AND key = 'active_relation_epoch'), ?3), ?3)",
            params![project_id, epoch.to_string(), now],
        )
        .map_err(query_error)?;
        Ok(())
    }

    pub fn mark_failed(
        tx: &Transaction<'_>,
        project_id: i64,
        epoch: i64,
        reason: &str,
    ) -> Result<(), StorageError> {
        tx.execute(
            "UPDATE relation_snapshot_manifest SET state = 'failed', failure_reason = ?3
             WHERE project_id = ?1 AND relation_epoch = ?2 AND state <> 'active'",
            params![project_id, epoch, reason],
        )
        .map_err(query_error)?;
        Ok(())
    }

    pub fn retry_failed(
        tx: &Transaction<'_>,
        project_id: i64,
        epoch: i64,
    ) -> Result<(), StorageError> {
        let changed = tx
            .execute(
                "UPDATE relation_snapshot_manifest SET
                    state = 'building', input_fingerprint = NULL,
                    snapshot_fingerprint = NULL, file_count = NULL,
                    entity_count = NULL, relation_count = NULL,
                    dependency_count = NULL, validated_at = NULL,
                    activated_at = NULL, failure_reason = NULL,
                    symbol_key_conflict_count = 0,
                    symbol_key_conflict_samples_json = NULL
                 WHERE project_id = ?1 AND relation_epoch = ?2 AND state = 'failed'",
                params![project_id, epoch],
            )
            .map_err(query_error)?;
        if changed != 1 {
            return Err(StorageError::Transaction(format!(
                "epoch {epoch} is not retryable"
            )));
        }
        Ok(())
    }

    /// Retain the active epoch, one rollback-ready epoch, and every epoch
    /// referenced by an unfinished operation. Stale failed/building candidates
    /// are removed only after the supplied cutoff.
    pub fn collect_garbage(
        tx: &Transaction<'_>,
        project_id: i64,
        stale_before: i64,
    ) -> Result<usize, StorageError> {
        tx.execute(
            "DELETE FROM relation_snapshot_manifest
             WHERE project_id = ?1
               AND state <> 'active'
               AND operation_id NOT IN (
                    SELECT operation_id FROM checkpoint
                    WHERE project_id = ?1 AND status <> 'completed'
               )
               AND (
                    (state IN ('failed', 'building') AND created_at < ?2)
                    OR (
                        state = 'ready' AND relation_epoch NOT IN (
                            SELECT relation_epoch FROM relation_snapshot_manifest
                            WHERE project_id = ?1 AND state = 'ready'
                            ORDER BY relation_epoch DESC LIMIT 1
                        )
                    )
               )",
            params![project_id, stale_before],
        )
        .map_err(query_error)
    }

    /// Delete all manifests (and their cascade rows) for a project except
    /// the given `keep_epoch`. Used during compaction to clean up old
    /// full snapshots and delta chains after a new base is activated.
    pub fn delete_manifests_except(
        tx: &Transaction<'_>,
        project_id: i64,
        keep_epoch: i64,
    ) -> Result<usize, StorageError> {
        let deleted = tx
            .execute(
                "DELETE FROM relation_snapshot_manifest
                 WHERE project_id = ?1 AND relation_epoch <> ?2",
                params![project_id, keep_epoch],
            )
            .map_err(query_error)?;
        Ok(deleted)
    }

    pub fn get_manifest(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<RelationSnapshotManifest>, StorageError> {
        let raw = conn
            .query_row(
                "SELECT project_id, relation_epoch, operation_id, state, schema_version,
                        parser_version, resolver_version, path_normalization_version,
                        config_fingerprint, input_fingerprint, snapshot_fingerprint,
                        file_count, entity_count, relation_count, dependency_count, failure_reason,
                        symbol_key_conflict_count, symbol_key_conflict_samples_json
                 FROM relation_snapshot_manifest WHERE project_id = ?1 AND relation_epoch = ?2",
                params![project_id, epoch],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, Option<String>>(9)?,
                        row.get::<_, Option<String>>(10)?,
                        row.get::<_, Option<i64>>(11)?,
                        row.get::<_, Option<i64>>(12)?,
                        row.get::<_, Option<i64>>(13)?,
                        row.get::<_, Option<i64>>(14)?,
                        row.get::<_, Option<String>>(15)?,
                        row.get::<_, i64>(16)?,
                        row.get::<_, Option<String>>(17)?,
                    ))
                },
            )
            .optional()
            .map_err(query_error)?;
        raw.map(|record| {
            let symbol_key_conflict_samples = match record.17 {
                Some(json) => match serde_json::from_str(&json) {
                    Ok(samples) => samples,
                    Err(error) => {
                        tracing::warn!(
                            project_id,
                            epoch,
                            "symbol key conflict samples JSON is corrupt; falling back to empty list: {error}"
                        );
                        Vec::new()
                    }
                },
                None => Vec::new(),
            };
            Ok(RelationSnapshotManifest {
                project_id: record.0,
                relation_epoch: record.1,
                operation_id: record.2,
                state: RelationSnapshotState::parse(&record.3)?,
                schema_version: record.4 as u32,
                parser_version: record.5 as u32,
                resolver_version: record.6 as u32,
                path_normalization_version: record.7 as u32,
                config_fingerprint: record.8,
                input_fingerprint: record.9,
                snapshot_fingerprint: record.10,
                file_count: record.11.map(|value| value as usize),
                entity_count: record.12.map(|value| value as usize),
                relation_count: record.13.map(|value| value as usize),
                dependency_count: record.14.map(|value| value as usize),
                failure_reason: record.15,
                symbol_key_conflict_count: record.16 as u64,
                symbol_key_conflict_samples,
            })
        })
        .transpose()
    }
}

fn require_state(
    tx: &Transaction<'_>,
    project_id: i64,
    epoch: i64,
    expected: RelationSnapshotState,
) -> Result<(), StorageError> {
    let state: String = tx
        .query_row(
            "SELECT state FROM relation_snapshot_manifest
             WHERE project_id = ?1 AND relation_epoch = ?2",
            params![project_id, epoch],
            |row| row.get(0),
        )
        .map_err(query_error)?;
    if state != expected.as_str() {
        return Err(StorageError::Transaction(format!(
            "epoch {epoch} is {state}, expected {}",
            expected.as_str()
        )));
    }
    Ok(())
}

fn required<'a, K, V>(values: &'a HashMap<K, V>, key: &K, role: &str) -> Result<&'a V, StorageError>
where
    K: Eq + std::hash::Hash,
{
    values
        .get(key)
        .ok_or_else(|| StorageError::Validation(format!("missing {role}")))
}

fn to_json<T: Serialize>(value: &T) -> Result<String, StorageError> {
    serde_json::to_string(value).map_err(|error| StorageError::Validation(error.to_string()))
}

fn optional_json<T: Serialize>(value: &Option<T>) -> Result<Option<String>, StorageError> {
    value.as_ref().map(to_json).transpose()
}

fn from_json<T: DeserializeOwned>(value: &str) -> Result<T, StorageError> {
    serde_json::from_str(value).map_err(|error| StorageError::Validation(error.to_string()))
}

fn optional_from_json<T: DeserializeOwned>(
    value: Option<String>,
) -> Result<Option<T>, StorageError> {
    value.map(|json| from_json(&json)).transpose()
}

fn query_error(error: rusqlite::Error) -> StorageError {
    StorageError::Query(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteClient;

    fn insert_project(client: &SqliteClient) {
        let conn = client
            .write_connection()
            .expect("test connection should open");
        conn.execute(
            "INSERT INTO projects
                (id, name, root_path, config_file_path, created_at, updated_at)
             VALUES (1, 'test', '/tmp/test', '.cce/config.json', 1, 1)",
            [],
        )
        .expect("test project should be inserted");
    }

    #[test]
    fn normalized_snapshot_does_not_store_canonical_blob() {
        use cce_types::CanonicalRelationSnapshot;

        let client = SqliteClient::in_memory().expect("in-memory database should open");
        insert_project(&client);
        let snapshot = CanonicalRelationSnapshot::new("config".to_string());
        let epoch = client
            .with_transaction(|tx| {
                RelationSnapshotRepository::allocate_building(tx, 1, "operation", "config")
            })
            .expect("epoch should allocate");
        client
            .with_transaction(|tx| {
                RelationSnapshotRepository::write_snapshot_and_mark_ready(
                    tx,
                    1,
                    epoch,
                    &snapshot,
                    &snapshot.input_fingerprint(),
                    &snapshot.fingerprint(),
                )
            })
            .expect("snapshot should persist");
        let conn = client
            .write_connection()
            .expect("test connection should open");
        let payload_table: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name = 'relation_snapshot_payload'",
                [],
                |row| row.get(0),
            )
            .expect("schema query should succeed");
        assert_eq!(payload_table, 0);
    }

    /// Conflict diagnostics must survive the full persistence round trip
    /// (write → manifest → read_snapshot) without altering the fingerprint.
    #[test]
    fn manifest_conflict_metadata_round_trips_through_persistence() {
        use cce_types::{CanonicalRelationSnapshot, EntityKind, SymbolKeyConflictRecord};

        let client = SqliteClient::in_memory().expect("in-memory database should open");
        insert_project(&client);
        let mut snapshot = CanonicalRelationSnapshot::new("config".to_string());
        snapshot.build_metadata.symbol_key_conflict_count = 2;
        snapshot.build_metadata.symbol_key_conflict_samples = vec![
            SymbolKeyConflictRecord {
                file_path: "a.rs".to_string(),
                scoped_name: "dup".to_string(),
                kind: EntityKind::Function,
                kept_entity: 1,
                rejected_entity: 2,
            },
            SymbolKeyConflictRecord {
                file_path: "b.rs".to_string(),
                scoped_name: "other".to_string(),
                kind: EntityKind::Function,
                kept_entity: 3,
                rejected_entity: 4,
            },
        ];
        let epoch = client
            .with_transaction(|tx| {
                RelationSnapshotRepository::allocate_building(tx, 1, "operation", "config")
            })
            .expect("epoch should allocate");
        client
            .with_transaction(|tx| {
                RelationSnapshotRepository::write_snapshot_and_mark_ready(
                    tx,
                    1,
                    epoch,
                    &snapshot,
                    &snapshot.input_fingerprint(),
                    &snapshot.fingerprint(),
                )
            })
            .expect("snapshot should persist");

        let conn = client
            .read_connection()
            .expect("test connection should open");
        let manifest = RelationSnapshotRepository::get_manifest(&conn, 1, epoch)
            .expect("manifest should load")
            .expect("manifest should exist");
        assert_eq!(manifest.symbol_key_conflict_count, 2);
        assert_eq!(manifest.symbol_key_conflict_samples.len(), 2);
        assert_eq!(manifest.symbol_key_conflict_samples[0].scoped_name, "dup");

        let reloaded = RelationSnapshotRepository::read_snapshot(&conn, &manifest)
            .expect("snapshot should reload");
        assert_eq!(reloaded.build_metadata.symbol_key_conflict_count, 2);
        assert_eq!(reloaded.build_metadata.symbol_key_conflict_samples.len(), 2);
        assert_eq!(
            reloaded.build_metadata.symbol_key_conflict_samples[1].rejected_entity,
            4
        );
        assert_eq!(
            reloaded.fingerprint(),
            snapshot.fingerprint(),
            "conflict diagnostics must not influence the fingerprint"
        );
    }
}
