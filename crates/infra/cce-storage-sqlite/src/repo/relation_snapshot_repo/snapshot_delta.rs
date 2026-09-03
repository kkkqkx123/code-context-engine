use cce_types::{SnapshotDelta, StorageError};
use rusqlite::{Connection, OptionalExtension, params};

use super::query_error;
use super::{RelationSnapshotRepository, RelationSnapshotState};

impl RelationSnapshotRepository {
    /// Read a single delta by epoch.
    pub fn read_delta(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<SnapshotDelta>, StorageError> {
        let data: Option<Vec<u8>> = conn
            .query_row(
                "SELECT delta_data FROM relation_snapshot_deltas
                 WHERE project_id = ?1 AND delta_epoch = ?2",
                params![project_id, epoch],
                |row| row.get(0),
            )
            .optional()
            .map_err(query_error)?;

        match data {
            Some(compressed) => {
                let decompressed = zstd::decode_all(&*compressed)
                    .map_err(|e| StorageError::Validation(format!("delta decompression: {e}")))?;
                let delta: SnapshotDelta = serde_json::from_slice(&decompressed)
                    .map_err(|e| StorageError::Validation(format!("delta deserialization: {e}")))?;
                Ok(Some(delta))
            }
            None => Ok(None),
        }
    }

    /// Read all deltas between `after_epoch` (exclusive) and `up_to_epoch`
    /// (inclusive), ordered by epoch ascending.
    pub fn get_delta_chain(
        conn: &Connection,
        project_id: i64,
        after_epoch: i64,
        up_to_epoch: i64,
    ) -> Result<Vec<SnapshotDelta>, StorageError> {
        let mut statement = conn
            .prepare(
                "SELECT delta_data FROM relation_snapshot_deltas
                 WHERE project_id = ?1
                   AND delta_epoch > ?2
                   AND delta_epoch <= ?3
                 ORDER BY delta_epoch ASC",
            )
            .map_err(query_error)?;

        let rows = statement
            .query_map(params![project_id, after_epoch, up_to_epoch], |row| {
                row.get::<_, Vec<u8>>(0)
            })
            .map_err(query_error)?;

        let mut deltas = Vec::new();
        for row in rows {
            let compressed = row.map_err(query_error)?;
            let decompressed = zstd::decode_all(&*compressed)
                .map_err(|e| StorageError::Validation(format!("delta decompression: {e}")))?;
            let delta: SnapshotDelta = serde_json::from_slice(&decompressed)
                .map_err(|e| StorageError::Validation(format!("delta deserialization: {e}")))?;
            deltas.push(delta);
        }
        Ok(deltas)
    }

    /// Walk the delta chain backwards from `delta_epoch` to find the nearest
    /// base epoch with state = Active (a full snapshot). Returns `None` if the
    /// epoch is already a full snapshot or no base is found.
    pub fn find_base_epoch(
        conn: &Connection,
        project_id: i64,
        delta_epoch: i64,
    ) -> Result<Option<i64>, StorageError> {
        let mut current = delta_epoch;
        loop {
            let manifest = Self::get_manifest(conn, project_id, current)?;
            match manifest {
                Some(m) if m.state == RelationSnapshotState::Active => {
                    return Ok(Some(current));
                }
                Some(m) if m.state == RelationSnapshotState::Delta => {
                    let base: Option<i64> = conn
                        .query_row(
                            "SELECT base_epoch FROM relation_snapshot_deltas
                             WHERE project_id = ?1 AND delta_epoch = ?2",
                            params![project_id, current],
                            |row| row.get(0),
                        )
                        .optional()
                        .map_err(query_error)?;
                    match base {
                        Some(base_epoch) => {
                            if base_epoch == current || base_epoch < 0 {
                                return Err(StorageError::Validation(format!(
                                    "delta chain broken at epoch {current}: invalid base_epoch {base_epoch}"
                                )));
                            }
                            current = base_epoch;
                        }
                        None => {
                            return Err(StorageError::Validation(format!(
                                "delta chain broken at epoch {current}: missing base_epoch reference"
                            )));
                        }
                    }
                }
                Some(_) => {
                    return Ok(Some(current));
                }
                None => {
                    return Ok(None);
                }
            }
        }
    }

    /// Return (chain_length, cumulative_delta_size_bytes, base_size_bytes)
    /// for the delta chain that builds on top of the active base epoch.
    pub fn get_delta_chain_info(
        conn: &Connection,
        project_id: i64,
        active_epoch: i64,
    ) -> Result<(usize, i64, i64), StorageError> {
        let base_epoch =
            Self::find_base_epoch(conn, project_id, active_epoch)?.unwrap_or(active_epoch);
        if base_epoch >= active_epoch {
            return Ok((0, 0, 0));
        }
        let (count, total_size): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(size_bytes), 0)
                 FROM relation_snapshot_deltas
                 WHERE project_id = ?1 AND delta_epoch > ?2
                   AND delta_epoch <= ?3",
                params![project_id, base_epoch, active_epoch],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(query_error)?;

        let base_size = [
            "SELECT COALESCE(SUM(
                COALESCE(length(path), 0) + COALESCE(length(language), 0) +
                COALESCE(length(input_hash), 0) + COALESCE(length(imports_json), 0)
             ), 0) FROM relation_snapshot_files
             WHERE project_id = ?1 AND relation_epoch = ?2",
            "SELECT COALESCE(SUM(
                COALESCE(length(scoped_name), 0) + COALESCE(length(kind_json), 0) +
                COALESCE(length(name), 0) + COALESCE(length(signature), 0) +
                COALESCE(length(parameters_json), 0) + COALESCE(length(return_type), 0) +
                COALESCE(length(span_json), 0) + COALESCE(length(doc_comment), 0) +
                COALESCE(length(modifiers_json), 0) + COALESCE(length(attributes_json), 0) +
                COALESCE(length(metadata_json), 0) + COALESCE(length(subtype), 0)
             ), 0) FROM relation_snapshot_entities
             WHERE project_id = ?1 AND relation_epoch = ?2",
            "SELECT COALESCE(SUM(
                COALESCE(length(raw_target), 0) + COALESCE(length(relation_type_json), 0) +
                COALESCE(length(span_json), 0) + COALESCE(length(external_type_json), 0) +
                COALESCE(length(unresolved_reason), 0) +
                COALESCE(length(stdlib_category_json), 0)
             ), 0) FROM relation_snapshot_relations
             WHERE project_id = ?1 AND relation_epoch = ?2",
            "SELECT COALESCE(SUM(
                COALESCE(length(export_type), 0)
             ), 0) FROM relation_snapshot_exports
             WHERE project_id = ?1 AND relation_epoch = ?2",
            "SELECT COALESCE(SUM(
                COALESCE(length(target_path), 0) + COALESCE(length(source), 0)
             ), 0) FROM relation_snapshot_dependencies
             WHERE project_id = ?1 AND relation_epoch = ?2",
        ];
        let base_size = base_size.iter().try_fold(0i64, |total, sql| {
            conn.query_row(sql, params![project_id, base_epoch], |row| {
                row.get::<_, i64>(0)
            })
            .map(|size| total.saturating_add(size))
            .map_err(query_error)
        })?;
        Ok((count as usize, total_size, base_size.max(1)))
    }
}
