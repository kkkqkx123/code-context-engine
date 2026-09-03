//! Project-level publication manifest for atomically selecting index generations.

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use cce_types::StorageError;

use crate::repo::project_repo::ProjectRepository;
use crate::repo::relation_snapshot_repo::RelationSnapshotRepository;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectIndexManifestState {
    Building,
    Active,
    Failed,
}

impl ProjectIndexManifestState {
    fn parse(value: &str) -> Result<Self, StorageError> {
        match value {
            "building" => Ok(Self::Building),
            "active" => Ok(Self::Active),
            "failed" => Ok(Self::Failed),
            _ => Err(StorageError::Query(format!(
                "invalid project index manifest state: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProjectIndexManifest {
    pub project_id: i64,
    pub publication_epoch: i64,
    pub data_epoch: i64,
    pub relation_epoch: i64,
    pub operation_id: String,
    pub state: ProjectIndexManifestState,
    pub input_fingerprint: Option<String>,
    /// Set once the inheritance registration of a zero-copy candidate
    /// generation is complete (parent link + residual cleanup committed).
    /// Recovery may only reuse a building candidate when this flag is set; a
    /// crash before registration leaves it false and forces a fresh
    /// registration.
    pub candidate_ready: bool,
    /// Generation this one inherits from under zero-copy candidate building
    /// (see the epoch-clone design). `None` for full generations that own all
    /// of their data. Inheritance is single-parent by construction.
    pub parent_data_epoch: Option<i64>,
}

/// Epochs that are safe to remove from the local and external generations.
///
/// The plan is calculated from the durable manifest state before external
/// storage is touched. Callers should remove external epochs first and then
/// apply the SQLite part of the plan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GenerationGcPlan {
    pub stale_publication_epochs: Vec<i64>,
    pub stale_data_epochs: Vec<i64>,
    pub stale_relation_epochs: Vec<i64>,
    pub protected_data_epochs: Vec<i64>,
    pub protected_relation_epochs: Vec<i64>,
}

/// Repository for the single project-visible index generation.
pub struct ProjectIndexManifestRepository;

impl ProjectIndexManifestRepository {
    pub fn begin_building(
        tx: &Transaction<'_>,
        project_id: i64,
        data_epoch: i64,
        operation_id: &str,
        input_fingerprint: Option<&str>,
    ) -> Result<ProjectIndexManifest, StorageError> {
        if let Some(existing) = tx
            .query_row(
                "SELECT project_id, publication_epoch, data_epoch, relation_epoch,
                        operation_id, state, input_fingerprint, candidate_ready,
                        parent_data_epoch
                 FROM project_index_manifests
                 WHERE project_id = ?1 AND operation_id = ?2",
                params![project_id, operation_id],
                Self::from_row,
            )
            .optional()
            .map_err(|error| StorageError::Query(error.to_string()))?
        {
            if existing.data_epoch != data_epoch {
                return Err(StorageError::Validation(format!(
                    "operation {operation_id} already targets data epoch {} instead of {data_epoch}",
                    existing.data_epoch
                )));
            }
            match existing.state {
                ProjectIndexManifestState::Building | ProjectIndexManifestState::Active => {
                    return Ok(existing);
                }
                ProjectIndexManifestState::Failed => {
                    tx.execute(
                        "UPDATE project_index_manifests
                         SET state = 'building', relation_epoch = 0,
                             activated_at = NULL, failure_reason = NULL,
                             candidate_ready = 0,
                             created_at = ?1
                         WHERE project_id = ?2 AND operation_id = ?3",
                        params![chrono::Utc::now().timestamp(), project_id, operation_id],
                    )
                    .map_err(|error| StorageError::Transaction(error.to_string()))?;
                    return Ok(ProjectIndexManifest {
                        state: ProjectIndexManifestState::Building,
                        relation_epoch: 0,
                        candidate_ready: false,
                        parent_data_epoch: None,
                        ..existing
                    });
                }
            }
        }

        let publication_epoch = tx
            .query_row(
                "SELECT COALESCE(MAX(publication_epoch), 0) + 1
                 FROM project_index_manifests WHERE project_id = ?1",
                params![project_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| StorageError::Query(error.to_string()))?;
        let now = chrono::Utc::now().timestamp();
        tx.execute(
            "INSERT INTO project_index_manifests
                (project_id, publication_epoch, data_epoch, relation_epoch,
                 operation_id, state, input_fingerprint, created_at)
             VALUES (?1, ?2, ?3, 0, ?4, 'building', ?5, ?6)",
            params![
                project_id,
                publication_epoch,
                data_epoch,
                operation_id,
                input_fingerprint,
                now,
            ],
        )
        .map_err(|error| StorageError::Insert(error.to_string()))?;
        Ok(ProjectIndexManifest {
            project_id,
            publication_epoch,
            data_epoch,
            relation_epoch: 0,
            operation_id: operation_id.to_string(),
            state: ProjectIndexManifestState::Building,
            input_fingerprint: input_fingerprint.map(ToString::to_string),
            candidate_ready: false,
            parent_data_epoch: None,
        })
    }

    /// Record the inheritance link for a zero-copy candidate generation.
    ///
    /// Called once when a hot-update candidate is created: from then on the
    /// candidate resolves any file not in its own rows (and not overridden)
    /// against `parent_data_epoch`. Full generations never call this.
    pub fn set_parent_data_epoch(
        tx: &Transaction<'_>,
        project_id: i64,
        operation_id: &str,
        parent_data_epoch: Option<i64>,
    ) -> Result<(), StorageError> {
        tx.execute(
            "UPDATE project_index_manifests
             SET parent_data_epoch = ?1
             WHERE project_id = ?2 AND operation_id = ?3",
            params![parent_data_epoch, project_id, operation_id],
        )
        .map_err(|error| StorageError::Transaction(error.to_string()))?;
        Ok(())
    }

    /// Mark a building candidate generation as fully registered (inheritance
    /// chain + residual cleanup committed), so a recovered operation can reuse
    /// it instead of re-registering the active generation.
    pub fn mark_candidate_ready(
        tx: &Transaction<'_>,
        project_id: i64,
        operation_id: &str,
    ) -> Result<(), StorageError> {
        tx.execute(
            "UPDATE project_index_manifests
             SET candidate_ready = 1
             WHERE project_id = ?1 AND operation_id = ?2 AND state = 'building'",
            params![project_id, operation_id],
        )
        .map_err(|error| StorageError::Transaction(error.to_string()))?;
        Ok(())
    }

    /// Load a building candidate manifest for a specific operation, if one
    /// exists. Recovery uses it to adopt an already-prepared candidate.
    pub fn get_building_by_operation(
        conn: &Connection,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Option<ProjectIndexManifest>, StorageError> {
        conn.query_row(
            "SELECT project_id, publication_epoch, data_epoch, relation_epoch,
                    operation_id, state, input_fingerprint, candidate_ready,
                    parent_data_epoch
             FROM project_index_manifests
             WHERE project_id = ?1 AND operation_id = ?2 AND state = 'building'",
            params![project_id, operation_id],
            Self::from_row,
        )
        .optional()
        .map_err(|error| StorageError::Query(error.to_string()))
    }

    /// Highest `data_epoch` among currently-building candidate manifests for a
    /// project, if any.
    ///
    /// Hot-update recovery re-parses files against the building candidate
    /// epoch, whose data an interrupted run may already have written; the
    /// entity-ID seed must sit above that epoch's maximum so a resumed re-parse
    /// never collides with the entity IDs reused from checkpoint envelopes.
    pub fn get_building_max_epoch(
        conn: &Connection,
        project_id: i64,
    ) -> Result<Option<i64>, StorageError> {
        conn.query_row(
            "SELECT MAX(data_epoch) FROM project_index_manifests
             WHERE project_id = ?1 AND state = 'building'",
            params![project_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .map_err(|error| StorageError::Query(error.to_string()))
    }

    pub fn get_active(
        conn: &Connection,
        project_id: i64,
    ) -> Result<Option<ProjectIndexManifest>, StorageError> {
        conn.query_row(
            "SELECT project_id, publication_epoch, data_epoch, relation_epoch,
                    operation_id, state, input_fingerprint, candidate_ready,
                    parent_data_epoch
             FROM project_index_manifests
             WHERE project_id = ?1 AND state = 'active'
             ORDER BY publication_epoch DESC LIMIT 1",
            params![project_id],
            Self::from_row,
        )
        .optional()
        .map_err(|error| StorageError::Query(error.to_string()))
    }

    pub fn activate(
        tx: &Transaction<'_>,
        project_id: i64,
        data_epoch: i64,
        relation_epoch: i64,
        operation_id: &str,
        input_fingerprint: Option<&str>,
    ) -> Result<ProjectIndexManifest, StorageError> {
        let now = chrono::Utc::now().timestamp();
        if relation_epoch > 0 {
            let relation_state = tx
                .query_row(
                    "SELECT state FROM relation_snapshot_manifest
                     WHERE project_id = ?1 AND relation_epoch = ?2",
                    params![project_id, relation_epoch],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|error| StorageError::Query(error.to_string()))?
                .ok_or_else(|| {
                    StorageError::Validation(format!(
                        "relation epoch {relation_epoch} does not exist for project {project_id}"
                    ))
                })?;
            if !matches!(relation_state.as_str(), "ready" | "active" | "delta") {
                return Err(StorageError::Validation(format!(
                    "relation epoch {relation_epoch} is not publishable ({relation_state})"
                )));
            }
            // Relation activation is part of the same SQLite transaction as
            // project-manifest activation. This prevents the relation meta
            // epoch from becoming visible before the data generation.
            RelationSnapshotRepository::activate(tx, project_id, relation_epoch)?;
        }
        let building =
            Self::begin_building(tx, project_id, data_epoch, operation_id, input_fingerprint)?;
        if building.state == ProjectIndexManifestState::Active {
            if building.relation_epoch == relation_epoch {
                return Ok(building);
            }
            return Err(StorageError::Validation(format!(
                "operation {operation_id} is already active at relation epoch {}",
                building.relation_epoch
            )));
        }
        let changed = tx
            .execute(
                "UPDATE project_index_manifests
             SET relation_epoch = ?1, state = 'active', activated_at = ?2,
                 input_fingerprint = COALESCE(?3, input_fingerprint), failure_reason = NULL
             WHERE project_id = ?4 AND publication_epoch = ?5 AND state = 'building'",
                params![
                    relation_epoch,
                    now,
                    input_fingerprint,
                    project_id,
                    building.publication_epoch,
                ],
            )
            .map_err(|error| StorageError::Transaction(error.to_string()))?;
        if changed != 1 {
            return Err(StorageError::Transaction(format!(
                "manifest for operation {operation_id} is not building"
            )));
        }

        for (key, value) in [
            ("epoch", data_epoch),
            ("active_epoch", data_epoch),
            ("active_relation_epoch", relation_epoch),
            ("epoch_ready", 1),
        ] {
            tx.execute(
                "INSERT INTO project_meta (project_id, key, value, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)
                 ON CONFLICT(project_id, key) DO UPDATE SET
                    value = excluded.value, updated_at = excluded.updated_at",
                params![project_id, key, value.to_string(), now],
            )
            .map_err(|error| StorageError::Transaction(error.to_string()))?;
        }

        Ok(ProjectIndexManifest {
            project_id,
            publication_epoch: building.publication_epoch,
            data_epoch,
            relation_epoch,
            operation_id: operation_id.to_string(),
            state: ProjectIndexManifestState::Active,
            input_fingerprint: input_fingerprint.map(ToString::to_string),
            candidate_ready: building.candidate_ready,
            parent_data_epoch: building.parent_data_epoch,
        })
    }

    /// Read the parent link of a data epoch (any manifest row carrying it).
    ///
    /// A missing or epoch-less row means the generation is parent-free.
    pub fn parent_data_epoch_of(
        conn: &Connection,
        project_id: i64,
        data_epoch: i64,
    ) -> Result<Option<i64>, StorageError> {
        conn.query_row(
            "SELECT parent_data_epoch FROM project_index_manifests
             WHERE project_id = ?1 AND data_epoch = ?2
             ORDER BY publication_epoch DESC LIMIT 1",
            params![project_id, data_epoch],
            |row| row.get(0),
        )
        .optional()
        .map(|value| value.flatten())
        .map_err(|error| StorageError::Query(error.to_string()))
    }

    /// Build a generation cleanup plan from durable publication state.
    pub fn generation_gc_plan(
        conn: &Connection,
        project_id: i64,
        keep_active_generations: usize,
        stale_before: i64,
    ) -> Result<GenerationGcPlan, StorageError> {
        let keep_active_generations = keep_active_generations.max(1);
        let mut statement = conn
            .prepare(
                "SELECT publication_epoch, data_epoch, relation_epoch, state,
                        operation_id, created_at, parent_data_epoch
                 FROM project_index_manifests
                 WHERE project_id = ?1
                 ORDER BY publication_epoch DESC",
            )
            .map_err(|error| StorageError::Query(error.to_string()))?;
        let mut rows = statement
            .query(params![project_id])
            .map_err(|error| StorageError::Query(error.to_string()))?;

        let mut plan = GenerationGcPlan::default();
        let mut active_count = 0usize;
        while let Some(row) = rows
            .next()
            .map_err(|error| StorageError::Query(error.to_string()))?
        {
            let publication_epoch: i64 = row
                .get(0)
                .map_err(|error| StorageError::Query(error.to_string()))?;
            let data_epoch: i64 = row
                .get(1)
                .map_err(|error| StorageError::Query(error.to_string()))?;
            let relation_epoch: i64 = row
                .get(2)
                .map_err(|error| StorageError::Query(error.to_string()))?;
            let state: String = row
                .get(3)
                .map_err(|error| StorageError::Query(error.to_string()))?;
            let operation_id: String = row
                .get(4)
                .map_err(|error| StorageError::Query(error.to_string()))?;
            let created_at: i64 = row
                .get(5)
                .map_err(|error| StorageError::Query(error.to_string()))?;
            let parent_data_epoch: Option<i64> = row
                .get(6)
                .map_err(|error| StorageError::Query(error.to_string()))?;

            let checkpoint_active: bool = conn
                .query_row(
                    "SELECT EXISTS(
                         SELECT 1 FROM checkpoint
                         WHERE project_id = ?1 AND operation_id = ?2
                           AND status = 'in_progress'
                     )",
                    params![project_id, operation_id],
                    |value| value.get::<_, i64>(0),
                )
                .map(|value| value != 0)
                .map_err(|error| StorageError::Query(error.to_string()))?;

            let keep = match state.as_str() {
                "active" => {
                    let keep = active_count < keep_active_generations;
                    active_count += 1;
                    keep
                }
                "building" => checkpoint_active || created_at >= stale_before,
                "failed" => created_at >= stale_before,
                _ => created_at >= stale_before,
            };

            if keep {
                if data_epoch > 0 && !plan.protected_data_epochs.contains(&data_epoch) {
                    plan.protected_data_epochs.push(data_epoch);
                }
                // Inheritance-chain protection: a kept generation resolves its
                // unchanged files against its ancestors, so every parent (up to
                // the depth-2 chain bound) must stay alive as well. Conservative
                // by construction — an over-protected epoch is merely retained
                // until a later GC run.
                let mut ancestor = parent_data_epoch;
                let mut depth = 0usize;
                while let Some(epoch) = ancestor
                    && depth < 2
                {
                    if epoch <= 0 {
                        break;
                    }
                    if !plan.protected_data_epochs.contains(&epoch) {
                        plan.protected_data_epochs.push(epoch);
                    }
                    ancestor = Self::parent_data_epoch_of(conn, project_id, epoch)?;
                    depth += 1;
                }
                if relation_epoch > 0 && !plan.protected_relation_epochs.contains(&relation_epoch) {
                    plan.protected_relation_epochs.push(relation_epoch);
                }
            } else {
                plan.stale_publication_epochs.push(publication_epoch);
                if data_epoch > 0 && !plan.stale_data_epochs.contains(&data_epoch) {
                    plan.stale_data_epochs.push(data_epoch);
                }
                if relation_epoch > 0 && !plan.stale_relation_epochs.contains(&relation_epoch) {
                    plan.stale_relation_epochs.push(relation_epoch);
                }
            }
        }
        drop(rows);
        drop(statement);

        for key in ["active_epoch", "epoch"] {
            let value = ProjectRepository::meta_get_int_optional(conn, project_id, key)?;
            if let Some(value) = value {
                if value > 0 && !plan.protected_data_epochs.contains(&value) {
                    plan.protected_data_epochs.push(value);
                }
            }
        }

        let active_relation_epoch =
            ProjectRepository::meta_get_int_optional(conn, project_id, "active_relation_epoch")?;
        if let Some(active_relation_epoch) = active_relation_epoch
            && active_relation_epoch > 0
        {
            let mut current = active_relation_epoch;
            loop {
                if !plan.protected_relation_epochs.contains(&current) {
                    plan.protected_relation_epochs.push(current);
                }
                let base = conn
                    .query_row(
                        "SELECT base_epoch FROM relation_snapshot_deltas
                         WHERE project_id = ?1 AND delta_epoch = ?2",
                        params![project_id, current],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()
                    .map_err(|error| StorageError::Query(error.to_string()))?;
                let Some(base) = base else { break };
                if base <= 0 || base == current {
                    break;
                }
                current = base;
            }
        }

        let mut relation_statement = conn
            .prepare(
                "SELECT relation_epoch, created_at FROM relation_snapshot_manifest
                 WHERE project_id = ?1",
            )
            .map_err(|error| StorageError::Query(error.to_string()))?;
        let mut relation_rows = relation_statement
            .query(params![project_id])
            .map_err(|error| StorageError::Query(error.to_string()))?;
        while let Some(row) = relation_rows
            .next()
            .map_err(|error| StorageError::Query(error.to_string()))?
        {
            let epoch: i64 = row
                .get(0)
                .map_err(|error| StorageError::Query(error.to_string()))?;
            let created_at: i64 = row
                .get(1)
                .map_err(|error| StorageError::Query(error.to_string()))?;
            if epoch > 0
                && !plan.protected_relation_epochs.contains(&epoch)
                && created_at < stale_before
                && !plan.stale_relation_epochs.contains(&epoch)
            {
                plan.stale_relation_epochs.push(epoch);
            }
        }

        plan.stale_data_epochs
            .retain(|epoch| !plan.protected_data_epochs.contains(epoch));
        plan.stale_relation_epochs
            .retain(|epoch| !plan.protected_relation_epochs.contains(epoch));
        Ok(plan)
    }

    /// Apply the SQLite portion of a previously calculated cleanup plan.
    pub fn apply_generation_gc(
        tx: &Transaction<'_>,
        project_id: i64,
        plan: &GenerationGcPlan,
    ) -> Result<(), StorageError> {
        for epoch in &plan.stale_data_epochs {
            tx.execute(
                "DELETE FROM file_summaries
                 WHERE epoch = ?2 AND file_id IN
                     (SELECT id FROM files WHERE project_id = ?1 AND epoch = ?2)",
                params![project_id, epoch],
            )
            .map_err(|error| StorageError::Delete(error.to_string()))?;
            for table in ["entity_detail_mappings", "chunks", "entities", "files"] {
                let sql = format!("DELETE FROM {table} WHERE project_id = ?1 AND epoch = ?2");
                tx.execute(&sql, params![project_id, epoch])
                    .map_err(|error| StorageError::Delete(error.to_string()))?;
            }
            // Overrides die with their generation; a recycled epoch number
            // must never inherit a stale exclusion set.
            crate::repo::generation_override_repo::GenerationOverrideRepository::clear_generation(
                tx, project_id, *epoch,
            )?;
        }
        for publication_epoch in &plan.stale_publication_epochs {
            tx.execute(
                "DELETE FROM project_index_manifests
                 WHERE project_id = ?1 AND publication_epoch = ?2",
                params![project_id, publication_epoch],
            )
            .map_err(|error| StorageError::Delete(error.to_string()))?;
        }
        for epoch in &plan.stale_relation_epochs {
            tx.execute(
                "DELETE FROM relation_snapshot_manifest
                 WHERE project_id = ?1 AND relation_epoch = ?2",
                params![project_id, epoch],
            )
            .map_err(|error| StorageError::Delete(error.to_string()))?;
        }
        Ok(())
    }

    pub fn mark_failed(
        tx: &Transaction<'_>,
        project_id: i64,
        operation_id: &str,
        reason: &str,
    ) -> Result<(), StorageError> {
        tx.execute(
            "UPDATE project_index_manifests
             SET state = 'failed', failure_reason = ?1
             WHERE project_id = ?2 AND operation_id = ?3 AND state = 'building'",
            params![reason, project_id, operation_id],
        )
        .map_err(|error| StorageError::Transaction(error.to_string()))?;
        Ok(())
    }

    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectIndexManifest> {
        let state: String = row.get(5)?;
        let state =
            ProjectIndexManifestState::parse(&state).map_err(|_| rusqlite::Error::InvalidQuery)?;
        Ok(ProjectIndexManifest {
            project_id: row.get(0)?,
            publication_epoch: row.get(1)?,
            data_epoch: row.get(2)?,
            relation_epoch: row.get(3)?,
            operation_id: row.get(4)?,
            state,
            input_fingerprint: row.get(6)?,
            candidate_ready: row.get::<_, i64>(7)? != 0,
            parent_data_epoch: row.get(8)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_selects_the_latest_generation() {
        let client = crate::SqliteClient::in_memory().expect("in-memory database should open");
        client
            .with_transaction(|tx| {
                tx.execute(
                    "INSERT INTO projects (id, name, root_path, config_file_path, created_at, updated_at)
                     VALUES (1, 'test', '/tmp/test', '.cce/config.json', 1, 1)",
                    [],
                )
                .map_err(|error| StorageError::Insert(error.to_string()))?;
                ProjectIndexManifestRepository::activate(tx, 1, 3, 0, "operation-1", None)?;
                ProjectIndexManifestRepository::activate(
                    tx,
                    1,
                    4,
                    0,
                    "operation-2",
                    Some("input"),
                )?;
                Ok(())
            })
            .expect("manifest activation should succeed");
        let conn = client.write_connection().expect("connection should open");
        let active = ProjectIndexManifestRepository::get_active(&conn, 1)
            .expect("active manifest should load")
            .expect("active manifest should exist");
        assert_eq!(active.data_epoch, 4);
        assert_eq!(active.relation_epoch, 0);
        drop(conn);
        assert_eq!(
            client
                .project_meta_get_int(1, "epoch")
                .expect("epoch metadata should be updated"),
            4
        );
    }

    #[test]
    fn failed_manifest_is_reopened_as_building_for_retry() {
        let client = crate::SqliteClient::in_memory().expect("in-memory database should open");
        client
            .with_transaction(|tx| {
                tx.execute(
                    "INSERT INTO projects (id, name, root_path, config_file_path, created_at, updated_at)
                     VALUES (1, 'test', '/tmp/test', '.cce/config.json', 1, 1)",
                    [],
                )
                .map_err(|error| StorageError::Insert(error.to_string()))?;
                let manifest = ProjectIndexManifestRepository::begin_building(
                    tx, 1, 2, "operation", None,
                )?;
                assert_eq!(manifest.state, ProjectIndexManifestState::Building);
                ProjectIndexManifestRepository::mark_failed(tx, 1, "operation", "injected")?;
                let retried = ProjectIndexManifestRepository::begin_building(
                    tx, 1, 2, "operation", None,
                )?;
                assert_eq!(retried.state, ProjectIndexManifestState::Building);
                assert_eq!(retried.relation_epoch, 0);
                Ok(())
            })
            .expect("failed manifest should be retryable");
    }

    /// Seed one project and activate a generation chain
    /// `1 → 2 → ...` where every generation after the first inherits from its
    /// predecessor (zero-copy candidates that were published).
    fn seed_inherited_chain(
        tx: &Transaction<'_>,
        project_id: i64,
        depth: usize,
    ) -> Result<(), StorageError> {
        tx.execute(
            "INSERT INTO projects (id, name, root_path, config_file_path, created_at, updated_at)
             VALUES (?1, 'test', '/tmp/test', '.cce/config.json', 1, 1)",
            params![project_id],
        )
        .map_err(|error| StorageError::Insert(error.to_string()))?;
        for epoch in 1..=depth as i64 {
            let operation = format!("operation-{epoch}");
            if epoch > 1 {
                ProjectIndexManifestRepository::begin_building(
                    tx, project_id, epoch, &operation, None,
                )?;
                ProjectIndexManifestRepository::set_parent_data_epoch(
                    tx,
                    project_id,
                    &operation,
                    Some(epoch - 1),
                )?;
            }
            ProjectIndexManifestRepository::activate(tx, project_id, epoch, 0, &operation, None)
                .map(|_| ())?;
        }
        Ok(())
    }

    #[test]
    fn generation_gc_protects_parent_chain_of_kept_active_generation() {
        let client = crate::SqliteClient::in_memory().expect("in-memory database should open");
        client
            .with_transaction(|tx| seed_inherited_chain(tx, 1, 2))
            .expect("inherited chain should be created");

        // Retention keeps only the newest active generation; its inherited
        // parent must still be protected even beyond the retention window.
        let conn = client.write_connection().expect("connection should open");
        let plan = ProjectIndexManifestRepository::generation_gc_plan(&conn, 1, 1, i64::MAX)
            .expect("generation GC plan should be generated");
        assert_eq!(plan.protected_data_epochs, vec![2, 1]);
        assert!(
            !plan.stale_data_epochs.contains(&1),
            "the parent of a kept inherited generation must not be collected"
        );
    }

    #[test]
    fn generation_gc_walks_the_full_depth_two_chain() {
        let client = crate::SqliteClient::in_memory().expect("in-memory database should open");
        client
            .with_transaction(|tx| seed_inherited_chain(tx, 1, 3))
            .expect("three-generation chain should be created");

        // Chain bound: the kept generation protects parent AND grandparent.
        let conn = client.write_connection().expect("connection should open");
        let plan = ProjectIndexManifestRepository::generation_gc_plan(&conn, 1, 1, i64::MAX)
            .expect("generation GC plan should be generated");
        assert_eq!(plan.protected_data_epochs, vec![3, 2, 1]);
        assert!(plan.stale_data_epochs.is_empty());
    }

    #[test]
    fn generation_gc_protects_parents_of_fresh_building_and_failed_generations() {
        let client = crate::SqliteClient::in_memory().expect("in-memory database should open");
        client
            .with_transaction(|tx| {
                tx.execute(
                    "INSERT INTO projects (id, name, root_path, config_file_path, created_at, updated_at)
                     VALUES (1, 'test', '/tmp/test', '.cce/config.json', 1, 1)",
                    [],
                )
                .map_err(|error| StorageError::Insert(error.to_string()))?;
                ProjectIndexManifestRepository::activate(tx, 1, 1, 0, "operation-1", None)
                    .map(|_| ())?;

                // Fresh building candidate inheriting from the active generation.
                ProjectIndexManifestRepository::begin_building(tx, 1, 2, "building", None)?;
                ProjectIndexManifestRepository::set_parent_data_epoch(tx, 1, "building", Some(1))?;
                ProjectIndexManifestRepository::mark_candidate_ready(tx, 1, "building")?;

                // Fresh failed candidate also linked to the active generation.
                ProjectIndexManifestRepository::begin_building(tx, 1, 3, "failed", None)?;
                ProjectIndexManifestRepository::set_parent_data_epoch(tx, 1, "failed", Some(1))?;
                ProjectIndexManifestRepository::mark_failed(tx, 1, "failed", "injected")?;
                Ok(())
            })
            .expect("building and failed manifests should be created");

        // Everything is fresh (stale_before in the past keeps both
        // non-active manifests), so their shared parent must be protected.
        let now = chrono::Utc::now().timestamp();
        let conn = client.write_connection().expect("connection should open");
        let plan = ProjectIndexManifestRepository::generation_gc_plan(&conn, 1, 1, now - 60)
            .expect("generation GC plan should be generated");
        assert!(plan.protected_data_epochs.contains(&2));
        assert!(plan.stale_data_epochs.is_empty());

        // Once stale, the building/failed generations lose protection — but
        // the active generation itself remains protected by retention.
        let plan = ProjectIndexManifestRepository::generation_gc_plan(&conn, 1, 1, i64::MAX)
            .expect("second GC plan should be generated");
        assert!(!plan.protected_data_epochs.contains(&2));
        assert!(!plan.protected_data_epochs.contains(&3));
        assert!(plan.stale_data_epochs.contains(&2));
        assert!(plan.stale_data_epochs.contains(&3));
    }

    #[test]
    fn apply_generation_gc_clears_overrides_of_stale_generations() {
        use crate::GenerationOverrideRepository;
        use crate::OverrideDisposition;

        let client = crate::SqliteClient::in_memory().expect("in-memory database should open");
        client
            .with_transaction(|tx| {
                tx.execute(
                    "INSERT INTO projects (id, name, root_path, config_file_path, created_at, updated_at)
                     VALUES (1, 'test', '/tmp/test', '.cce/config.json', 1, 1)",
                    [],
                )
                .map_err(|error| StorageError::Insert(error.to_string()))?;
                for (epoch, operation) in [(1, "operation-1"), (2, "operation-2")] {
                    ProjectIndexManifestRepository::activate(tx, 1, epoch, 0, operation, None)
                        .map(|_| ())?;
                }
                GenerationOverrideRepository::upsert(
                    tx,
                    1,
                    1,
                    "src/gone.rs",
                    OverrideDisposition::Deleted,
                )?;
                Ok(())
            })
            .expect("generations and override should be created");

        let conn = client.write_connection().expect("connection should open");
        let plan = ProjectIndexManifestRepository::generation_gc_plan(&conn, 1, 1, i64::MAX)
            .expect("generation GC plan should be generated");
        assert_eq!(plan.stale_data_epochs, vec![1]);
        drop(conn);

        client
            .with_transaction(|tx| {
                ProjectIndexManifestRepository::apply_generation_gc(tx, 1, &plan)
            })
            .expect("generation GC should apply");
        let conn = client.read_connection().expect("connection should open");
        assert!(
            GenerationOverrideRepository::list_for_generation(&conn, 1, 1)
                .expect("overrides should list")
                .is_empty(),
            "a recycled epoch number must never inherit a stale exclusion set"
        );
    }

    #[test]
    fn generation_gc_keeps_the_active_retention_window() {
        let client = crate::SqliteClient::in_memory().expect("in-memory database should open");
        client
            .with_transaction(|tx| {
                tx.execute(
                    "INSERT INTO projects (id, name, root_path, config_file_path, created_at, updated_at)
                     VALUES (1, 'test', '/tmp/test', '.cce/config.json', 1, 1)",
                    [],
                )
                .map_err(|error| StorageError::Insert(error.to_string()))?;
                for (epoch, operation) in [(1, "operation-1"), (2, "operation-2"), (3, "operation-3")] {
                    ProjectIndexManifestRepository::activate(tx, 1, epoch, 0, operation, None)?;
                }
                Ok(())
            })
            .expect("manifests should activate");

        let conn = client.write_connection().expect("connection should open");
        let plan = ProjectIndexManifestRepository::generation_gc_plan(&conn, 1, 2, i64::MAX)
            .expect("generation GC plan should be generated");
        assert_eq!(plan.stale_data_epochs, vec![1]);
        assert_eq!(plan.protected_data_epochs, vec![3, 2]);
        drop(conn);

        client
            .with_transaction(|tx| {
                ProjectIndexManifestRepository::apply_generation_gc(tx, 1, &plan)
            })
            .expect("generation GC should apply");
        let conn = client.write_connection().expect("connection should open");
        let manifest_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM project_index_manifests WHERE project_id = 1",
                [],
                |row| row.get(0),
            )
            .expect("manifest count should be queryable");
        assert_eq!(manifest_count, 2);
    }

    #[test]
    fn generation_gc_plan_rejects_unparseable_active_epoch_meta() {
        let client = crate::SqliteClient::in_memory().expect("in-memory database should open");
        client
            .with_transaction(|tx| {
                tx.execute(
                    "INSERT INTO projects (id, name, root_path, config_file_path, created_at, updated_at)
                     VALUES (1, 'test', '/tmp/test', '.cce/config.json', 1, 1)",
                    [],
                )
                .map_err(|error| StorageError::Insert(error.to_string()))?;
                ProjectIndexManifestRepository::activate(tx, 1, 3, 0, "operation-1", None)?;
                Ok(())
            })
            .expect("manifest activation should succeed");
        let conn = client.write_connection().expect("connection should open");
        conn.execute(
            "UPDATE project_meta SET value = 'corrupt'
             WHERE project_id = 1 AND key = 'active_epoch'",
            [],
        )
        .expect("active_epoch meta should be corrupted");
        let result = ProjectIndexManifestRepository::generation_gc_plan(&conn, 1, 2, i64::MAX);
        assert!(
            matches!(result, Err(StorageError::Query(_))),
            "corrupt active_epoch meta must fail GC instead of protecting nothing"
        );
    }

    #[test]
    fn generation_gc_plan_rejects_unparseable_active_relation_epoch_meta() {
        let client = crate::SqliteClient::in_memory().expect("in-memory database should open");
        client
            .with_transaction(|tx| {
                tx.execute(
                    "INSERT INTO projects (id, name, root_path, config_file_path, created_at, updated_at)
                     VALUES (1, 'test', '/tmp/test', '.cce/config.json', 1, 1)",
                    [],
                )
                .map_err(|error| StorageError::Insert(error.to_string()))?;
                ProjectIndexManifestRepository::activate(tx, 1, 3, 0, "operation-1", None)?;
                Ok(())
            })
            .expect("manifest activation should succeed");
        let conn = client.write_connection().expect("connection should open");
        conn.execute(
            "UPDATE project_meta SET value = 'corrupt'
             WHERE project_id = 1 AND key = 'active_relation_epoch'",
            [],
        )
        .expect("active_relation_epoch meta should be corrupted");
        let result = ProjectIndexManifestRepository::generation_gc_plan(&conn, 1, 2, i64::MAX);
        assert!(
            matches!(result, Err(StorageError::Query(_))),
            "corrupt active_relation_epoch meta must fail GC instead of skipping chain protection"
        );
    }

    #[test]
    fn generation_gc_plan_accepts_missing_meta_rows() {
        let client = crate::SqliteClient::in_memory().expect("in-memory database should open");
        client
            .with_transaction(|tx| {
                tx.execute(
                    "INSERT INTO projects (id, name, root_path, config_file_path, created_at, updated_at)
                     VALUES (1, 'test', '/tmp/test', '.cce/config.json', 1, 1)",
                    [],
                )
                .map_err(|error| StorageError::Insert(error.to_string()))?;
                ProjectIndexManifestRepository::begin_building(tx, 1, 2, "operation-1", None)?;
                Ok(())
            })
            .expect("manifest begin should succeed");
        let conn = client.write_connection().expect("connection should open");
        // No activation means no project_meta rows for the epoch keys. The GC
        // plan must still be computable with nothing protected.
        let plan = ProjectIndexManifestRepository::generation_gc_plan(&conn, 1, 2, i64::MAX)
            .expect("plan should be generated without meta rows");
        assert!(plan.protected_data_epochs.is_empty());
    }

    #[test]
    fn parent_data_epoch_persists_through_inheritance_link() {
        let client = crate::SqliteClient::in_memory().expect("in-memory database should open");
        client
            .with_transaction(|tx| {
                tx.execute(
                    "INSERT INTO projects (id, name, root_path, config_file_path, created_at, updated_at)
                     VALUES (1, 'test', '/tmp/test', '.cce/config.json', 1, 1)",
                    [],
                )
                .map_err(|error| StorageError::Insert(error.to_string()))?;
                ProjectIndexManifestRepository::begin_building(tx, 1, 3, "operation-1", None)?;
                ProjectIndexManifestRepository::set_parent_data_epoch(tx, 1, "operation-1", Some(2))?;
                Ok(())
            })
            .expect("candidate with parent link should persist");
        let conn = client.write_connection().expect("connection should open");
        let building =
            ProjectIndexManifestRepository::get_building_by_operation(&conn, 1, "operation-1")
                .expect("building manifest should load")
                .expect("building manifest should exist");
        assert_eq!(building.parent_data_epoch, Some(2));
        drop(conn);

        // Clearing the link (e.g. candidate reset) round-trips to None.
        client
            .with_transaction(|tx| {
                ProjectIndexManifestRepository::set_parent_data_epoch(tx, 1, "operation-1", None)
            })
            .expect("clearing the link should succeed");
        let conn = client.write_connection().expect("connection should reopen");
        let building =
            ProjectIndexManifestRepository::get_building_by_operation(&conn, 1, "operation-1")
                .expect("building manifest should reload")
                .expect("building manifest should exist");
        assert_eq!(building.parent_data_epoch, None);
    }

    #[test]
    fn candidate_ready_marks_the_building_manifest_for_resume() {
        let client = crate::SqliteClient::in_memory().expect("in-memory database should open");
        client
            .with_transaction(|tx| {
                tx.execute(
                    "INSERT INTO projects (id, name, root_path, config_file_path, created_at, updated_at)
                     VALUES (1, 'test', '/tmp/test', '.cce/config.json', 1, 1)",
                    [],
                )
                .map_err(|error| StorageError::Insert(error.to_string()))?;
                ProjectIndexManifestRepository::begin_building(tx, 1, 2, "operation-1", None)?;
                Ok(())
            })
            .expect("manifest begin should succeed");
        let conn = client.write_connection().expect("connection should open");
        let building =
            ProjectIndexManifestRepository::get_building_by_operation(&conn, 1, "operation-1")
                .expect("building manifest should load")
                .expect("building manifest should exist");
        assert!(!building.candidate_ready);
        assert_eq!(building.data_epoch, 2);
        drop(conn);

        client
            .with_transaction(|tx| {
                ProjectIndexManifestRepository::mark_candidate_ready(tx, 1, "operation-1")
            })
            .expect("candidate should be marked ready");

        let conn = client.write_connection().expect("connection should open");
        let ready =
            ProjectIndexManifestRepository::get_building_by_operation(&conn, 1, "operation-1")
                .expect("ready manifest should load")
                .expect("ready manifest should exist");
        assert!(ready.candidate_ready, "candidate_ready must be persisted");
        // A different operation has no building manifest.
        assert!(
            ProjectIndexManifestRepository::get_building_by_operation(&conn, 1, "other-operation")
                .expect("query should succeed")
                .is_none()
        );
    }
}
