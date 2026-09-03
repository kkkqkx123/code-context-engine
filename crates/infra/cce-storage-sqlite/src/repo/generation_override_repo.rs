//! Per-generation file overrides for zero-copy candidate generations.
//!
//! Under the inheritance model a candidate generation resolves unchanged files
//! against its parent epoch (see `ProjectIndexManifest::parent_data_epoch`).
//! Only files touched by the hot update differ: their new data is written to
//! the candidate's own epoch, or the file was deleted outright. This table
//! records those exceptions so read-side resolution can exclude them from the
//! parent lookup.

use rusqlite::{Connection, Transaction, params};

use cce_types::StorageError;

/// Why a file must not resolve against the parent generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideDisposition {
    /// The file has new rows in this generation; only they are visible.
    Replaced,
    /// The file was deleted in this generation; it is invisible everywhere.
    Deleted,
}

impl OverrideDisposition {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Replaced => "replaced",
            Self::Deleted => "deleted",
        }
    }

    fn parse(value: &str) -> Result<Self, StorageError> {
        match value {
            "replaced" => Ok(Self::Replaced),
            "deleted" => Ok(Self::Deleted),
            other => Err(StorageError::Query(format!(
                "invalid generation override disposition: {other}"
            ))),
        }
    }
}

/// A single file-level exception to parent inheritance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationOverride {
    pub file_path: String,
    pub disposition: OverrideDisposition,
}

/// Repository for `generation_overrides`.
pub struct GenerationOverrideRepository;

impl GenerationOverrideRepository {
    /// Upsert an override. A later call for the same (project, epoch, file)
    /// replaces the earlier disposition.
    pub fn upsert(
        tx: &Transaction<'_>,
        project_id: i64,
        epoch: i64,
        file_path: &str,
        disposition: OverrideDisposition,
    ) -> Result<(), StorageError> {
        tx.execute(
            "INSERT INTO generation_overrides (project_id, epoch, file_path, disposition)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(project_id, epoch, file_path) DO UPDATE SET
                disposition = excluded.disposition",
            params![project_id, epoch, file_path, disposition.as_str()],
        )
        .map_err(|error| StorageError::Insert(error.to_string()))?;
        Ok(())
    }

    /// List all overrides of one generation.
    pub fn list_for_generation(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<GenerationOverride>, StorageError> {
        let mut stmt = conn
            .prepare(
                "SELECT file_path, disposition FROM generation_overrides
                 WHERE project_id = ?1 AND epoch = ?2
                 ORDER BY file_path",
            )
            .map_err(|error| StorageError::Query(error.to_string()))?;
        let rows = stmt
            .query_map(params![project_id, epoch], |row| {
                Ok(GenerationOverride {
                    file_path: row.get(0)?,
                    disposition: match OverrideDisposition::parse(&row.get::<_, String>(1)?) {
                        Ok(disposition) => disposition,
                        Err(_) => return Err(rusqlite::Error::InvalidQuery),
                    },
                })
            })
            .map_err(|error| StorageError::Query(error.to_string()))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| StorageError::Query(error.to_string()))
    }

    /// Drop every override of one generation. Called when the generation is
    /// materialized (its own rows become complete) or garbage collected.
    pub fn clear_generation(
        tx: &Transaction<'_>,
        project_id: i64,
        epoch: i64,
    ) -> Result<(), StorageError> {
        tx.execute(
            "DELETE FROM generation_overrides WHERE project_id = ?1 AND epoch = ?2",
            params![project_id, epoch],
        )
        .map_err(|error| StorageError::Delete(error.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(
            "CREATE TABLE projects (id INTEGER PRIMARY KEY);
             CREATE TABLE generation_overrides (
                project_id INTEGER NOT NULL,
                epoch INTEGER NOT NULL,
                file_path TEXT NOT NULL,
                disposition TEXT NOT NULL CHECK(disposition IN ('replaced', 'deleted')),
                PRIMARY KEY (project_id, epoch, file_path),
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
             );
             INSERT INTO projects (id) VALUES (1);",
        )
        .expect("seed schema");
        conn
    }

    #[test]
    fn upsert_and_list_roundtrip() {
        let mut conn = setup();
        let tx = conn.transaction().expect("begin tx");
        GenerationOverrideRepository::upsert(&tx, 1, 3, "src/a.rs", OverrideDisposition::Replaced)
            .expect("upsert replaced");
        GenerationOverrideRepository::upsert(&tx, 1, 3, "src/b.rs", OverrideDisposition::Deleted)
            .expect("upsert deleted");
        tx.commit().expect("commit");

        let overrides =
            GenerationOverrideRepository::list_for_generation(&conn, 1, 3).expect("list overrides");
        assert_eq!(
            overrides,
            vec![
                GenerationOverride {
                    file_path: "src/a.rs".to_string(),
                    disposition: OverrideDisposition::Replaced,
                },
                GenerationOverride {
                    file_path: "src/b.rs".to_string(),
                    disposition: OverrideDisposition::Deleted,
                },
            ]
        );

        // Other generations are untouched by the filter.
        assert!(
            GenerationOverrideRepository::list_for_generation(&conn, 1, 4)
                .expect("list other generation")
                .is_empty()
        );
    }

    #[test]
    fn upsert_replaces_disposition_and_clear_is_scoped() {
        let mut conn = setup();
        {
            let tx = conn.transaction().expect("begin tx");
            GenerationOverrideRepository::upsert(
                &tx,
                1,
                2,
                "src/x.rs",
                OverrideDisposition::Deleted,
            )
            .expect("initial insert");
            GenerationOverrideRepository::upsert(
                &tx,
                1,
                2,
                "src/x.rs",
                OverrideDisposition::Replaced,
            )
            .expect("disposition upgrade");
            tx.commit().expect("commit");
        }

        let overrides =
            GenerationOverrideRepository::list_for_generation(&conn, 1, 2).expect("list");
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].disposition, OverrideDisposition::Replaced);

        {
            let tx = conn.transaction().expect("begin tx");
            GenerationOverrideRepository::clear_generation(&tx, 1, 2).expect("clear");
            GenerationOverrideRepository::upsert(
                &tx,
                1,
                9,
                "src/keep.rs",
                OverrideDisposition::Deleted,
            )
            .expect("other generation marker");
            tx.commit().expect("commit");
        }
        assert!(
            GenerationOverrideRepository::list_for_generation(&conn, 1, 2)
                .expect("relist")
                .is_empty(),
            "clear must remove exactly the target generation"
        );
        assert_eq!(
            GenerationOverrideRepository::list_for_generation(&conn, 1, 9)
                .expect("list survivor")
                .len(),
            1
        );
    }
}
