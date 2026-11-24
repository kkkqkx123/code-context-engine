//! Disk-backed canonical snapshot artifact used by snapshot publication.
//!
//! The artifact is the only output of canonical snapshot construction. It
//! replaces a fully materialized `CanonicalRelationSnapshot` with bounded
//! write batches, an external-sort fingerprint, and on-disk component rows.
//! The temporary directory is removed when the artifact is dropped.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use cce_utils::ExternalSorter;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::canonical::{relation_sort_key, stable_json};
use super::{
    CanonicalDependency, CanonicalEntity, CanonicalFile, CanonicalRelation, SnapshotBuildMetadata,
    normalize_project_path,
};
use crate::types::{CanonicalRelationSnapshot, StorageError};

pub const DEFAULT_SNAPSHOT_BATCH_ITEMS: usize = 512;
pub const DEFAULT_FINGERPRINT_CHUNK_BYTES: usize = 8 * 1024 * 1024;
pub const DEFAULT_ARTIFACT_MAX_SHARD_ITEMS: usize =
    super::snapshot_shard::DEFAULT_MAX_SHARD_ITEMS;

static ARTIFACT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Version and build information stored alongside the artifact rows.
#[derive(Debug, Clone)]
pub struct SnapshotArtifactHeader {
    pub schema_version: u32,
    pub parser_version: u32,
    pub resolver_version: u32,
    pub path_normalization_version: u32,
    pub config_fingerprint: String,
    pub base_relation_epoch: Option<i64>,
    pub build_metadata: SnapshotBuildMetadata,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SnapshotArtifactCounts {
    pub files: usize,
    pub entities: usize,
    pub relations: usize,
    pub dependencies: usize,
}

/// Bounded row batch handed to the persistence layer.
#[derive(Debug)]
pub enum CanonicalComponentBatch {
    Files(Vec<CanonicalFile>),
    Entities(Vec<CanonicalEntity>),
    Relations(Vec<CanonicalRelation>),
    Dependencies(Vec<CanonicalDependency>),
}

/// Canonical entity plus the persistent entity ID that is deliberately
/// excluded from the fingerprint payload.
#[derive(Serialize, Deserialize)]
struct ArtifactEntity {
    #[serde(flatten)]
    entity: CanonicalEntity,
    entity_id: Option<u64>,
}

impl ArtifactEntity {
    fn into_canonical(mut self) -> CanonicalEntity {
        self.entity.entity_id = self.entity_id;
        self.entity
    }
}

/// Reads canonical rows from an artifact in bounded batches.
pub struct SnapshotBatchReader {
    directory: PathBuf,
    batch_items: usize,
    stage: usize,
    reader: Option<BufReader<File>>,
    stage_limit: usize,
    failed: bool,
}

impl SnapshotBatchReader {
    fn new(artifact: &CanonicalSnapshotArtifact, batch_items: usize) -> Self {
        Self {
            directory: artifact.directory.clone(),
            batch_items,
            stage: 0,
            reader: None,
            stage_limit: 4,
            failed: false,
        }
    }

    fn next_stage(&mut self) {
        self.reader = None;
        self.stage += 1;
    }
}

impl Iterator for SnapshotBatchReader {
    type Item = Result<CanonicalComponentBatch, StorageError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }
        loop {
            if self.stage >= self.stage_limit {
                return None;
            }
            if self.reader.is_none() {
                let name = match self.stage {
                    0 => "files.jsonl",
                    1 => "entities.jsonl",
                    2 => "relations.jsonl",
                    _ => "dependencies.jsonl",
                };
                match File::open(self.directory.join(name)) {
                    Ok(file) => self.reader = Some(BufReader::new(file)),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        self.next_stage();
                        continue;
                    }
                    Err(error) => {
                        self.failed = true;
                        return Some(Err(error.into()));
                    }
                }
            }
            let mut lines = Vec::with_capacity(self.batch_items);
            let mut read_error = None;
            {
                let reader = self.reader.as_mut().expect("reader is initialized");
                for _ in 0..self.batch_items {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => lines.push(line),
                        Err(error) => {
                            read_error = Some(error.into());
                            break;
                        }
                    }
                }
            }
            if lines.is_empty() {
                if let Some(error) = read_error {
                    self.failed = true;
                    return Some(Err(error));
                }
                self.next_stage();
                continue;
            }
            let result = match self.stage {
                0 => deserialize_batch(&lines).map(CanonicalComponentBatch::Files),
                1 => deserialize_entities(&lines).map(CanonicalComponentBatch::Entities),
                2 => deserialize_batch(&lines).map(CanonicalComponentBatch::Relations),
                _ => deserialize_batch(&lines).map(CanonicalComponentBatch::Dependencies),
            };
            match result {
                Ok(batch) => return Some(Ok(batch)),
                Err(error) => {
                    self.failed = true;
                    return Some(Err(error));
                }
            }
        }
    }
}

fn deserialize_entities(lines: &[String]) -> Result<Vec<CanonicalEntity>, StorageError> {
    lines
        .iter()
        .map(|line| {
            serde_json::from_str::<ArtifactEntity>(line)
                .map(ArtifactEntity::into_canonical)
                .map_err(|error| vec_error("entity", error))
        })
        .collect()
}

fn deserialize_batch<T: serde::de::DeserializeOwned>(
    lines: &[String],
) -> Result<Vec<T>, StorageError> {
    lines
        .iter()
        .map(|line| serde_json::from_str(line).map_err(|error| vec_error("row", error)))
        .collect()
}

fn vec_error(kind: &str, error: impl std::fmt::Display) -> StorageError {
    StorageError::validation(format!("invalid artifact {kind} row: {error}"))
}

/// Completed canonical snapshot artifact.
pub struct CanonicalSnapshotArtifact {
    directory: PathBuf,
    header: SnapshotArtifactHeader,
    counts: SnapshotArtifactCounts,
    input_fingerprint: String,
    snapshot_fingerprint: String,
    write_batches: u64,
    buffer_peak_bytes: u64,
    options: SnapshotArtifactOptions,
}

impl Drop for CanonicalSnapshotArtifact {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

impl CanonicalSnapshotArtifact {
    pub fn header(&self) -> &SnapshotArtifactHeader {
        &self.header
    }

    pub fn counts(&self) -> SnapshotArtifactCounts {
        self.counts
    }

    pub fn input_fingerprint(&self) -> &str {
        &self.input_fingerprint
    }

    pub fn snapshot_fingerprint(&self) -> &str {
        &self.snapshot_fingerprint
    }

    pub fn write_batches(&self) -> u64 {
        self.write_batches
    }

    pub fn buffer_peak_bytes(&self) -> u64 {
        self.buffer_peak_bytes
    }

    pub fn options(&self) -> SnapshotArtifactOptions {
        self.options
    }

    pub fn batches(&self, batch_items: usize) -> SnapshotBatchReader {
        SnapshotBatchReader::new(self, batch_items.max(1))
    }

    /// File-only batches, for writes that need both file and entity rows.
    pub fn file_batches(
        &self,
        batch_items: usize,
    ) -> impl Iterator<Item = Result<Vec<CanonicalFile>, StorageError>> + '_ {
        let mut reader = SnapshotBatchReader::new(self, batch_items.max(1));
        reader.stage_limit = 1;
        reader.map(|batch| match batch {
            Ok(CanonicalComponentBatch::Files(files)) => Ok(files),
            Ok(_) => Err(StorageError::validation("unexpected artifact batch stage")),
            Err(error) => Err(error),
        })
    }

    /// Stream file rows once more for writes that need both file and entity
    /// rows to exist (exports). No project-wide vector is retained.
    pub fn for_each_file(
        &self,
        mut visit: impl FnMut(CanonicalFile) -> Result<(), StorageError>,
    ) -> Result<(), StorageError> {
        let reader = BufReader::new(File::open(self.directory.join("files.jsonl"))?);
        for line in reader.lines() {
            let line = line?;
            visit(serde_json::from_str(&line).map_err(|error| vec_error("file", error))?)?;
        }
        Ok(())
    }

    /// Build an artifact from an already materialized snapshot.
    ///
    /// Callers that already own a canonical snapshot (compaction and tests)
    /// use this entry point; the full-index build path streams directly from
    /// the relation index through [`SnapshotArtifactBuilder`] instead.
    pub fn from_snapshot(
        snapshot: &CanonicalRelationSnapshot,
        options: SnapshotArtifactOptions,
    ) -> Result<Self, StorageError> {
        let mut builder = SnapshotArtifactBuilder::new(
            SnapshotArtifactHeader {
                schema_version: snapshot.schema_version,
                parser_version: snapshot.parser_version,
                resolver_version: snapshot.resolver_version,
                path_normalization_version: snapshot.path_normalization_version,
                config_fingerprint: snapshot.config_fingerprint.clone(),
                base_relation_epoch: snapshot.base_relation_epoch,
                build_metadata: snapshot.build_metadata.clone(),
            },
            options,
        )?;
        for mut file in snapshot.files.iter().cloned() {
            normalize_file(&mut file);
            builder.push_file(file)?;
        }
        for entity in &snapshot.entities {
            builder.push_entity(entity.clone())?;
        }
        for relation in &snapshot.relations {
            builder.push_relation(relation.clone())?;
        }
        for dependency in &snapshot.dependencies {
            builder.push_dependency(dependency.clone())?;
        }
        builder.finish()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SnapshotArtifactOptions {
    pub fingerprint_chunk_bytes: usize,
    /// Explicit item budget used to build the shard directory at write time.
    pub max_shard_items: usize,
}

impl SnapshotArtifactOptions {
    pub fn with_shard_items(mut self, max_shard_items: usize) -> Self {
        self.max_shard_items = max_shard_items;
        self
    }
}

impl Default for SnapshotArtifactOptions {
    fn default() -> Self {
        Self {
            fingerprint_chunk_bytes: DEFAULT_FINGERPRINT_CHUNK_BYTES,
            max_shard_items: DEFAULT_ARTIFACT_MAX_SHARD_ITEMS,
        }
    }
}

/// Streams canonical components into disk-backed rows while collecting the
/// external-sort inputs for both snapshot fingerprints.
pub struct SnapshotArtifactBuilder {
    directory: PathBuf,
    header: SnapshotArtifactHeader,
    counts: SnapshotArtifactCounts,
    files: BufWriter<File>,
    entities: BufWriter<File>,
    relations: BufWriter<File>,
    dependencies: BufWriter<File>,
    input_sort: ExternalSorter,
    file_sort: ExternalSorter,
    entity_sort: ExternalSorter,
    relation_sort: ExternalSorter,
    dependency_sort: ExternalSorter,
    buffer_peak_bytes: u64,
    options: SnapshotArtifactOptions,
}

impl SnapshotArtifactBuilder {
    pub fn new(
        header: SnapshotArtifactHeader,
        options: SnapshotArtifactOptions,
    ) -> Result<Self, StorageError> {
        let unique = ARTIFACT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("cce-snapshot-{}-{unique}", std::process::id()));
        fs::create_dir_all(&directory)?;
        let chunk = options.fingerprint_chunk_bytes.max(1024);
        Ok(Self {
            files: writer(&directory, "files.jsonl")?,
            entities: writer(&directory, "entities.jsonl")?,
            relations: writer(&directory, "relations.jsonl")?,
            dependencies: writer(&directory, "dependencies.jsonl")?,
            input_sort: ExternalSorter::new(&directory, "fingerprint-inputs", chunk),
            file_sort: ExternalSorter::new(&directory, "fingerprint-files", chunk),
            entity_sort: ExternalSorter::new(&directory, "fingerprint-entities", chunk),
            relation_sort: ExternalSorter::new(&directory, "fingerprint-relations", chunk),
            dependency_sort: ExternalSorter::new(&directory, "fingerprint-dependencies", chunk),
            directory,
            header,
            counts: SnapshotArtifactCounts::default(),
            buffer_peak_bytes: 0,
            options,
        })
    }

    pub fn push_file(&mut self, mut file: CanonicalFile) -> Result<(), StorageError> {
        normalize_file(&mut file);
        let json = serde_json::to_string(&file).map_err(serde_error)?;
        self.input_sort.push(
            format!("{}\0{}", file.path, file.input_hash).into_bytes(),
            serde_json::to_string(&(&file.path, &file.input_hash))
                .map_err(serde_error)?
                .into_bytes(),
        )?;
        self.file_sort
            .push(file.path.as_bytes().to_vec(), json.clone().into_bytes())?;
        write_row(&mut self.files, &json)?;
        self.counts.files += 1;
        self.record_buffer();
        Ok(())
    }

    pub fn push_entity(&mut self, entity: CanonicalEntity) -> Result<(), StorageError> {
        let sort_key = entity.key.sort_key();
        let fingerprint_json = serde_json::to_string(&entity).map_err(serde_error)?;
        let entity_id = entity.entity_id;
        let json =
            serde_json::to_string(&ArtifactEntity { entity, entity_id }).map_err(serde_error)?;
        self.entity_sort
            .push(sort_key.into_bytes(), fingerprint_json.into_bytes())?;
        write_row(&mut self.entities, &json)?;
        self.counts.entities += 1;
        self.record_buffer();
        Ok(())
    }

    pub fn push_relation(&mut self, mut relation: CanonicalRelation) -> Result<(), StorageError> {
        let json = serde_json::to_string(&relation).map_err(serde_error)?;
        // The volatile runtime symbol never participates in a fingerprint.
        relation.callee_symbol = None;
        let fingerprint_json = serde_json::to_string(&relation).map_err(serde_error)?;
        self.relation_sort.push(
            relation_sort_key(&relation).into_bytes(),
            fingerprint_json.into_bytes(),
        )?;
        write_row(&mut self.relations, &json)?;
        self.counts.relations += 1;
        self.record_buffer();
        Ok(())
    }

    pub fn push_dependency(&mut self, dependency: CanonicalDependency) -> Result<(), StorageError> {
        let json = serde_json::to_string(&dependency).map_err(serde_error)?;
        self.dependency_sort.push(
            format!(
                "{}\0{}\0{}",
                dependency.source_file, dependency.target_file, dependency.source
            )
            .into_bytes(),
            json.clone().into_bytes(),
        )?;
        write_row(&mut self.dependencies, &json)?;
        self.counts.dependencies += 1;
        self.record_buffer();
        Ok(())
    }

    fn record_buffer(&mut self) {
        let buffered = self.input_sort.buffered_bytes()
            + self.file_sort.buffered_bytes()
            + self.entity_sort.buffered_bytes()
            + self.relation_sort.buffered_bytes()
            + self.dependency_sort.buffered_bytes();
        self.buffer_peak_bytes = self.buffer_peak_bytes.max(buffered as u64);
    }

    pub fn finish(mut self) -> Result<CanonicalSnapshotArtifact, StorageError> {
        let batch_items = DEFAULT_SNAPSHOT_BATCH_ITEMS as u64;
        let counts = self.counts;
        let write_batches = counts.files.div_ceil(batch_items as usize) as u64
            + counts.entities.div_ceil(batch_items as usize) as u64
            + counts.relations.div_ceil(batch_items as usize) as u64
            + counts.dependencies.div_ceil(batch_items as usize) as u64
            // Exports are written from a second streaming pass over files.
            + counts.files.div_ceil(batch_items as usize) as u64;
        self.files.flush()?;
        self.entities.flush()?;
        self.relations.flush()?;
        self.dependencies.flush()?;
        let input_fingerprint = fingerprint_sorted(self.input_sort.into_sorted()?)?;
        let snapshot_fingerprint = fingerprint_components(
            &self.header,
            self.file_sort.into_sorted()?,
            self.entity_sort.into_sorted()?,
            self.relation_sort.into_sorted()?,
            self.dependency_sort.into_sorted()?,
        )?;
        Ok(CanonicalSnapshotArtifact {
            directory: self.directory,
            header: self.header,
            counts: self.counts,
            input_fingerprint,
            snapshot_fingerprint,
            write_batches,
            buffer_peak_bytes: self.buffer_peak_bytes,
            options: self.options,
        })
    }
}

fn writer(directory: &Path, name: &str) -> Result<BufWriter<File>, StorageError> {
    Ok(BufWriter::new(File::create(directory.join(name))?))
}

fn write_row(writer: &mut impl Write, json: &str) -> Result<(), StorageError> {
    writer.write_all(json.as_bytes())?;
    writer.write_all(b"\n")?;
    Ok(())
}

fn serde_error(error: impl std::fmt::Display) -> StorageError {
    StorageError::validation(format!("canonical snapshot serialization: {error}"))
}

fn normalize_file(file: &mut CanonicalFile) {
    file.path = normalize_project_path(&file.path);
    file.imports.sort_by_key(stable_json);
    file.exports
        .sort_by_key(|export| format!("{}\0{}", export.symbol.sort_key(), export.export_type));
}

#[derive(Serialize)]
struct FingerprintHeader<'a> {
    schema_version: u32,
    parser_version: u32,
    resolver_version: u32,
    path_normalization_version: u32,
    config_fingerprint: &'a str,
    base_relation_epoch: Option<i64>,
}

fn fingerprint_components(
    header: &SnapshotArtifactHeader,
    files: cce_utils::SortedRecords,
    entities: cce_utils::SortedRecords,
    relations: cce_utils::SortedRecords,
    dependencies: cce_utils::SortedRecords,
) -> Result<String, StorageError> {
    let prefix = serde_json::to_string(&FingerprintHeader {
        schema_version: header.schema_version,
        parser_version: header.parser_version,
        resolver_version: header.resolver_version,
        path_normalization_version: header.path_normalization_version,
        config_fingerprint: &header.config_fingerprint,
        base_relation_epoch: header.base_relation_epoch,
    })
    .map_err(serde_error)?;
    let mut hasher = Sha256::new();
    hasher.update(prefix.strip_suffix('}').unwrap_or(&prefix).as_bytes());
    hash_group(&mut hasher, "files", files)?;
    hash_group(&mut hasher, "entities", entities)?;
    hash_group(&mut hasher, "relations", relations)?;
    hash_group(&mut hasher, "dependencies", dependencies)?;
    hasher.update(b"}");
    Ok(hex::encode(hasher.finalize()))
}

fn hash_group(
    hasher: &mut Sha256,
    name: &str,
    records: cce_utils::SortedRecords,
) -> Result<(), StorageError> {
    hasher.update(format!(",\"{name}\":[").as_bytes());
    let mut first = true;
    for record in records {
        let (_, payload) = record?;
        if !first {
            hasher.update(b",");
        }
        first = false;
        hasher.update(&payload);
    }
    hasher.update(b"]");
    Ok(())
}

fn fingerprint_sorted(records: cce_utils::SortedRecords) -> Result<String, StorageError> {
    let mut hasher = Sha256::new();
    hasher.update(b"[");
    let mut first = true;
    for record in records {
        let (_, payload) = record?;
        if !first {
            hasher.update(b",");
        }
        first = false;
        hasher.update(&payload);
    }
    hasher.update(b"]");
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_fingerprint_matches_full_snapshot() {
        let mut snapshot = CanonicalRelationSnapshot::new("config".to_string());
        snapshot.files.push(CanonicalFile {
            path: "src/b.rs".to_string(),
            language: "rust".to_string(),
            input_hash: "bbb".to_string(),
            file_size: 1,
            imports: Vec::new(),
            exports: Vec::new(),
        });
        snapshot.files.push(CanonicalFile {
            path: "src/a.rs".to_string(),
            language: "rust".to_string(),
            input_hash: "aaa".to_string(),
            file_size: 1,
            imports: Vec::new(),
            exports: Vec::new(),
        });
        let key = crate::types::StableSymbolKey::new(
            "src/a.rs",
            "run",
            crate::types::EntityKind::Function,
            "fn run()",
        );
        snapshot.entities.push(CanonicalEntity {
            key: key.clone(),
            entity_id: None,
            name: "run".to_string(),
            signature: "fn run()".to_string(),
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
        });
        snapshot.relations.push(CanonicalRelation {
            caller: key,
            target: crate::types::CanonicalRelationTarget::Unresolved {
                reason: crate::types::UnresolvedReason::SymbolNotFound,
            },
            raw_target: "missing".to_string(),
            relation_type: crate::types::RelationType::DirectCall,
            span: Default::default(),
            stdlib_category: None,
            overload_signature: None,
            callee_symbol: None,
            owner_type: None,
            call_context: Default::default(),
            call_frequency: 1,
            cfg_condition: None,
        });
        let expected_fingerprint = snapshot.fingerprint();
        let expected_input = snapshot.input_fingerprint();
        let artifact = CanonicalSnapshotArtifact::from_snapshot(
            &snapshot,
            SnapshotArtifactOptions {
                fingerprint_chunk_bytes: 128,
                max_shard_items: 2,
            },
        )
        .expect("artifact should build");
        assert_eq!(artifact.snapshot_fingerprint(), &expected_fingerprint);
        assert_eq!(artifact.input_fingerprint(), &expected_input);
        assert_eq!(artifact.counts().files, 2);
        assert_eq!(artifact.counts().entities, 1);
        assert_eq!(artifact.counts().relations, 1);
    }
}
