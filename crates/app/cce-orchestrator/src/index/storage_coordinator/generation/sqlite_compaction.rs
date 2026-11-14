//! SQLite-side compaction: materializing inherited generations into full ones.
//!
//! The row copying itself runs inside the storage branch as one atomic
//! contract operation; this module keeps the scheduling-facing entry point.

use cce_storage_common::RelationStorage;

use crate::error::OrchestratorError;

use super::StorageCoordinator;

impl StorageCoordinator {
    /// Materialize an inherited generation into a complete one (compaction).
    ///
    /// Copies every parent-generation row that is not hidden by an override
    /// into `target_epoch`, merging with the rows the target already owns.
    /// Afterwards the target no longer depends on its parent and can be
    /// published as a full generation. Used only by compaction — candidate
    /// creation registers inheritance instead of copying.
    ///
    /// Idempotent under crash retry: rows of the copied source paths are
    /// deleted from the target inside the same transaction before the copy,
    /// so an interrupted attempt never leaves unique-constraint residue that
    /// would wedge every later compaction.
    ///
    /// `excluded_paths` lists the overridden files of the target; their
    /// parent rows must not be copied (replaced files already own newer
    /// rows, deleted files must stay invisible).
    pub(crate) async fn materialize_sqlite_generation(
        &self,
        source_epoch: i64,
        target_epoch: i64,
        excluded_paths: &[String],
    ) -> Result<(), OrchestratorError> {
        let Some(store) = self.metadata_store.as_ref() else {
            return Ok(());
        };
        store
            .generation_materialize(self.project_id, source_epoch, target_epoch, excluded_paths)
            .await
            .map_err(OrchestratorError::Storage)
    }
}

#[cfg(test)]
mod tests {
    use cce_storage_common::RelationStorage;
    use cce_storage_common::metadb::ChunkRecord;
    use cce_storage_metadb::RelationStore;

    use super::super::StorageCoordinator;

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(future)
    }

    /// A crashed earlier compaction attempt may have committed part of the
    /// copy. Re-running must succeed (no unique-constraint wedging), replace
    /// the residue with the source rows, and leave target-owned rows of
    /// overridden files untouched.
    #[test]
    fn materialize_is_restartable_after_partial_copy() {
        let database = RelationStore::in_memory().expect("in-memory store");
        block_on(database.ensure_project(1, "/tmp/test")).expect("project should be ensured");
        // Source generation 1: two files.
        block_on(database.files_record_hashes(
            1,
            1,
            &[
                ("src/keep.rs".to_string(), "hash-keep".to_string()),
                ("src/replaced.rs".to_string(), "hash-old".to_string()),
            ],
        ))
        .expect("source files should seed");
        let chunk = |path: &str, epoch: i64, content: &str| {
            ChunkRecord::new(
                format!("{path}::{epoch}"),
                path.to_string(),
                content.to_string(),
                0,
                5,
            )
            .with_epoch(epoch)
            .with_project_id(1)
        };
        block_on(database.chunks_upsert(&[
            chunk("src/keep.rs", 1, "keep-source-content"),
            chunk("src/replaced.rs", 1, "replaced-parent-content"),
        ]))
        .expect("source chunks should seed");
        // Target generation 2 owns a newer row for the replaced file…
        block_on(database.files_record_hashes(
            1,
            2,
            &[
                ("src/replaced.rs".to_string(), "hash-new".to_string()),
                ("src/keep.rs".to_string(), "hash-stale-residue".to_string()),
            ],
        ))
        .expect("target files should seed");
        block_on(database.chunks_upsert(&[
            chunk("src/replaced.rs", 2, "replaced-own-newer"),
            chunk("src/keep.rs", 2, "stale-partial-copy"),
        ]))
        .expect("target chunks should seed");

        let storage = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database.clone());
        block_on(storage.materialize_sqlite_generation(1, 2, &["src/replaced.rs".to_string()]))
            .expect("materialization over residue must succeed");

        // Copied rows keep their source chunk IDs (cross-epoch identity is
        // stable, like entity source IDs); assert by file and epoch.
        let chunk_content = |path: &str| -> String {
            let chunks = block_on(database.chunks_by_file_epoch(1, path, 2))
                .expect("target chunks should load");
            chunks
                .into_iter()
                .next()
                .expect("target chunk should exist")
                .content
        };
        assert_eq!(
            chunk_content("src/keep.rs"),
            "keep-source-content",
            "residue must be replaced by the source rows"
        );
        assert_eq!(
            chunk_content("src/replaced.rs"),
            "replaced-own-newer",
            "overridden files' own newer rows must survive"
        );
        let files = block_on(database.files_list(1, &[2])).expect("target files should load");
        assert_eq!(files.len(), 2, "no duplicate path rows may accumulate");

        // Re-running the whole compaction stays idempotent.
        block_on(storage.materialize_sqlite_generation(1, 2, &["src/replaced.rs".to_string()]))
            .expect("materialization must be restartable");
    }
}
