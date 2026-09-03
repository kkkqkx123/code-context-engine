//! Disk-backed input spool for a full relation build.
//!
//! A complete relation graph needs multiple passes over parsed files. Keeping
//! every `ParsedFile` in memory defeats batch processing, so this spool owns
//! the operation-local durable copy used by those passes.
//!
//! Entries persist only the fields relation construction needs
//! ([`RelBuildEntrySnapshot`], serialized with rkyv + zstd) and live under the
//! project data directory at a fixed path, so a crashed run cannot leave orphan
//! directories in the system temp dir. A new full index for the same project
//! supersedes any previous spool.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use cce_relation::symbol_table::ProjectSymbolTable;
use cce_types::import::ReexportRecord;
use cce_types::serialization::{deserialize_from_cache, serialize_for_cache};
use cce_types::{EntityId, EntitySnapshot, ImportTable, Language, ParsedFile, RawRelationData};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use thiserror::Error;

/// A recoverable error while storing or replaying relation build inputs.
#[derive(Debug, Error)]
pub(crate) enum RelationBuildSpoolError {
    #[error("relation build spool I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("relation build spool serialization error: {0}")]
    Serialization(String),
}

/// rkyv-safe snapshot of [`ParsedFile`] for disk spooling.
///
/// `HashMap` fields are replaced by `Vec` tuples to satisfy rkyv 0.8 trait
/// bounds (`ArchivedHashMap` does not implement `Hash`/`Eq`).  Only the
/// fields consumed by `register_file_entities`, `resolve_file_relations`,
/// and the plugin replay paths are retained; NL/export-oriented sidecars
/// (behavior, control flow, embedded blocks, block relations, doc comments)
/// are intentionally dropped.
#[derive(Debug, Clone, Archive, RkyvSerialize, RkyvDeserialize)]
pub(crate) struct RelBuildEntrySnapshot {
    pub path: String,
    pub language: Language,
    pub source: String,
    pub entities: Vec<EntitySnapshot>,
    pub local_symbols: Vec<(String, Vec<EntityId>)>,
    pub raw_relations: Vec<RawRelationData>,
    pub import_table: Option<ImportTable>,
    pub reexports: Vec<ReexportRecord>,
    pub file_hash: Option<String>,
}

impl RelBuildEntrySnapshot {
    fn from_parsed(parsed: &ParsedFile) -> Self {
        Self {
            path: parsed.path.clone(),
            language: parsed.language,
            source: parsed.source.to_string(),
            entities: parsed.entities.iter().map(EntitySnapshot::from).collect(),
            local_symbols: parsed
                .local_symbols
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            raw_relations: parsed.raw_relations.clone(),
            import_table: parsed.import_table.clone(),
            reexports: parsed.reexports.clone(),
            file_hash: parsed.file_hash.clone(),
        }
    }

    fn to_parsed(&self) -> ParsedFile {
        ParsedFile {
            language: self.language,
            path: self.path.clone(),
            source: self.source.clone().into(),
            entities: self.entities.iter().cloned().map(Into::into).collect(),
            local_symbols: self.local_symbols.iter().cloned().collect(),
            raw_relations: self.raw_relations.clone(),
            import_table: self.import_table.clone(),
            reexports: self.reexports.clone(),
            file_hash: self.file_hash.clone(),
            ..Default::default()
        }
    }
}

/// Operation-local, disk-backed parsed-file storage for relation construction.
///
/// Entries are retained in insertion order so all later passes are
/// deterministic. The directory is removed when the spool is dropped.
pub(crate) struct RelationBuildSpool {
    directory: PathBuf,
    entries: Vec<PathBuf>,
    project_symbols: ProjectSymbolTable,
}

impl RelationBuildSpool {
    /// Create an empty spool with the already initialized project symbol table.
    ///
    /// The spool lives at `<project_root>/.cce/relation-build-{project_id}`.
    /// A leftover directory from a crashed previous run is removed first
    /// (full indexes are exclusive per project, so a fresh spool supersedes
    /// any earlier in-flight one).
    pub(crate) fn new(
        project_id: i64,
        project_root: &Path,
        project_symbols: ProjectSymbolTable,
    ) -> Result<Self, RelationBuildSpoolError> {
        let directory = project_root
            .join(".cce")
            .join(format!("relation-build-{project_id}"));
        // Clean up an orphaned spool left behind by a crashed run.
        if directory.exists() {
            if let Err(error) = fs::remove_dir_all(&directory) {
                tracing::warn!(
                    path = %directory.display(),
                    %error,
                    "Failed to remove orphaned relation build spool"
                );
            }
        }
        fs::create_dir_all(&directory)?;

        Ok(Self {
            directory,
            entries: Vec::new(),
            project_symbols,
        })
    }

    /// Persist one parsed file for later replay.
    pub(crate) fn append(&mut self, parsed: &ParsedFile) -> Result<(), RelationBuildSpoolError> {
        let entry_path = self
            .directory
            .join(format!("{:020}.bin", self.entries.len()));
        let snapshot = RelBuildEntrySnapshot::from_parsed(parsed);
        let (encoded, _original_size, _compressed_size) = serialize_for_cache(&snapshot)
            .map_err(|e| RelationBuildSpoolError::Serialization(e.to_string()))?;
        let file = File::create(&entry_path)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(&encoded)?;
        writer.flush()?;
        self.entries.push(entry_path);
        Ok(())
    }

    /// Replay every parsed file without retaining the complete input set.
    pub(crate) fn for_each(
        &self,
        mut visit: impl FnMut(&ParsedFile),
    ) -> Result<usize, RelationBuildSpoolError> {
        for entry_path in &self.entries {
            let encoded = fs::read(entry_path)?;
            let snapshot: RelBuildEntrySnapshot = deserialize_from_cache(&encoded)
                .map_err(|e| RelationBuildSpoolError::Serialization(e.to_string()))?;
            let parsed = snapshot.to_parsed();
            visit(&parsed);
        }
        Ok(self.entries.len())
    }

    /// Number of spooled entries.
    pub(crate) fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Estimated decoded memory usage.
    ///
    /// Uses a conservative 4x decompression factor over the on-disk size.
    /// Caller can use this to decide whether to use the in-memory cache path.
    pub(crate) fn estimated_decoded_bytes(&self) -> u64 {
        self.total_encoded_bytes().saturating_mul(4)
    }

    /// Collect all parsed files into memory.
    ///
    /// Used for the memory-cache optimization when the estimated size fits
    /// within the configured limit. The caller must ensure the spool is not
    /// too large before invoking this to avoid OOM.
    pub(crate) fn collect_all(&self) -> Result<Vec<ParsedFile>, RelationBuildSpoolError> {
        let mut files = Vec::with_capacity(self.entries.len());
        for entry_path in &self.entries {
            let encoded = fs::read(entry_path)?;
            let snapshot: RelBuildEntrySnapshot = deserialize_from_cache(&encoded)
                .map_err(|e| RelationBuildSpoolError::Serialization(e.to_string()))?;
            files.push(snapshot.to_parsed());
        }
        Ok(files)
    }

    /// Total on-disk encoded bytes (for spool replay observability).
    pub(crate) fn total_encoded_bytes(&self) -> u64 {
        self.entries
            .iter()
            .filter_map(|p| std::fs::metadata(p).ok().map(|m| m.len()))
            .sum()
    }

    /// Access the project-wide symbols accumulated while inputs were spooled.
    pub(crate) fn project_symbols(&self) -> &ProjectSymbolTable {
        &self.project_symbols
    }

    #[cfg(test)]
    fn directory(&self) -> &Path {
        &self.directory
    }

    fn cleanup(&mut self) {
        if self.directory.exists()
            && let Err(error) = fs::remove_dir_all(&self.directory)
        {
            tracing::warn!(
                path = %self.directory.display(),
                %error,
                "Failed to remove relation build spool"
            );
        }
        self.entries.clear();
    }
}

impl Drop for RelationBuildSpool {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::{EntityKind, RelationType, Span};
    use std::path::PathBuf;

    fn sample_snapshot() -> RelBuildEntrySnapshot {
        let entity = cce_types::Entity::new(
            EntityId(0),
            EntityKind::Function,
            "alpha".to_string(),
            Span::default(),
        );
        RelBuildEntrySnapshot {
            path: "src/lib.rs".to_string(),
            language: Language::Rust,
            source: "fn alpha() {} pub fn beta() {}".to_string(),
            entities: vec![EntitySnapshot::from(&entity)],
            local_symbols: vec![("alpha".to_string(), vec![EntityId(0)])],
            raw_relations: vec![RawRelationData {
                src: EntityId(0),
                level: cce_types::RelationLevel::Entity,
                dst_name: "beta".to_string(),
                relation_type: RelationType::DirectCall,
                span: Span::default(),
                stdlib_category: None,
            }],
            import_table: None,
            reexports: vec![ReexportRecord::new("beta", "alpha", "beta")],
            file_hash: None,
        }
    }

    #[test]
    fn entry_round_trips_through_cache_format() {
        let snapshot = sample_snapshot();
        let (encoded, _, _) = serialize_for_cache(&snapshot).expect("entry should serialize");
        let decoded: RelBuildEntrySnapshot =
            deserialize_from_cache(&encoded).expect("entry should deserialize");
        assert_eq!(decoded.path, snapshot.path);
        assert_eq!(decoded.source, snapshot.source);
        assert_eq!(decoded.entities.len(), 1);
        assert_eq!(decoded.raw_relations.len(), 1);
        assert_eq!(decoded.local_symbols[0].1, vec![EntityId(0)]);
    }

    #[test]
    fn replays_inputs_in_insertion_order_and_cleans_up() {
        let root = tempfile::tempdir().expect("temp dir");
        let project_root = root.path().join("project");
        let project_symbols = ProjectSymbolTable::new(PathBuf::from("."));
        let mut spool = RelationBuildSpool::new(1, &project_root, project_symbols)
            .expect("relation build spool should be created");
        let directory = spool.directory().to_path_buf();

        let mut first = sample_snapshot().to_parsed();
        first.path = "first.rs".to_string();
        let mut second = sample_snapshot().to_parsed();
        second.path = "second.rs".to_string();
        spool
            .append(&first)
            .expect("first parsed file should be stored");
        spool
            .append(&second)
            .expect("second parsed file should be stored");

        let mut paths = Vec::new();
        let count = spool
            .for_each(|parsed| paths.push(parsed.path.clone()))
            .expect("spool should replay stored parsed files");

        assert_eq!(count, 2);
        assert_eq!(paths, ["first.rs", "second.rs"]);
        drop(spool);
        assert!(!directory.exists());
    }

    #[test]
    fn new_spool_removes_orphaned_directory_from_crashed_run() {
        let root = tempfile::tempdir().expect("temp dir");
        let project_root = root.path().join("project");
        let orphan = project_root.join(".cce").join("relation-build-1");
        fs::create_dir_all(&orphan).expect("orphan dir should be created");
        fs::write(orphan.join("00000000000000000000.bin"), b"stale")
            .expect("orphan entry should be written");

        let project_symbols = ProjectSymbolTable::new(PathBuf::from("."));
        let spool = RelationBuildSpool::new(1, &project_root, project_symbols)
            .expect("new spool should clean the orphan");
        let directory = spool.directory();
        assert!(directory.exists());
        let remaining = fs::read_dir(directory)
            .expect("spool dir should be readable")
            .count();
        assert_eq!(remaining, 0, "orphaned entries must be cleaned up");
    }
}
