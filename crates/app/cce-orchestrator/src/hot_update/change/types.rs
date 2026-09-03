//! Change detection types for hot update
//!
//! This module provides types for detecting and tracking file changes,
//! entity changes, and generating change statistics.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use cce_parser::summary::FileSummary;
use cce_types::ContentRoute;
use cce_types::entity::{Entity, EntityId, ParsedFile};

/// File change type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FileChangeType {
    /// New file added
    Added,
    /// File modified (content changed)
    Modified,
    /// File deleted
    Deleted,
    /// File renamed/moved (same content, different path)
    Renamed,
}

impl std::fmt::Display for FileChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileChangeType::Added => write!(f, "added"),
            FileChangeType::Modified => write!(f, "modified"),
            FileChangeType::Deleted => write!(f, "deleted"),
            FileChangeType::Renamed => write!(f, "renamed"),
        }
    }
}

/// File change information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    /// File path
    pub path: PathBuf,
    /// Type of change
    pub change_type: FileChangeType,
    /// Previous path (for renamed files)
    pub previous_path: Option<PathBuf>,
    /// File content SHA256 hash
    pub content_hash: String,
    /// File size in bytes
    pub size: u64,
    /// Last modification time
    pub modified: DateTime<Utc>,
}

impl FileChange {
    /// Create a new file change
    pub fn new(
        path: PathBuf,
        change_type: FileChangeType,
        content_hash: String,
        size: u64,
        modified: DateTime<Utc>,
    ) -> Self {
        Self {
            path,
            change_type,
            previous_path: None,
            content_hash,
            size,
            modified,
        }
    }

    /// Set previous path (for renamed files)
    pub fn with_previous_path(mut self, path: PathBuf) -> Self {
        self.previous_path = Some(path);
        self
    }

    /// Check if this is an add operation
    pub fn is_added(&self) -> bool {
        matches!(self.change_type, FileChangeType::Added)
    }

    /// Check if this is a modification
    pub fn is_modified(&self) -> bool {
        matches!(self.change_type, FileChangeType::Modified)
    }

    /// Check if this is a deletion
    pub fn is_deleted(&self) -> bool {
        matches!(self.change_type, FileChangeType::Deleted)
    }

    /// Check if this is a rename
    pub fn is_renamed(&self) -> bool {
        matches!(self.change_type, FileChangeType::Renamed)
    }
}

/// Entity change type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityChangeType {
    /// New entity added
    Added,
    /// Entity modified (signature, body changed)
    Modified,
    /// Entity deleted
    Deleted,
    /// Entity unchanged (for reference)
    Unchanged,
}

impl std::fmt::Display for EntityChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityChangeType::Added => write!(f, "added"),
            EntityChangeType::Modified => write!(f, "modified"),
            EntityChangeType::Deleted => write!(f, "deleted"),
            EntityChangeType::Unchanged => write!(f, "unchanged"),
        }
    }
}

/// Entity change information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityChange {
    /// Entity ID
    pub entity_id: EntityId,
    /// Entity name
    pub entity_name: String,
    /// Type of change
    pub change_type: EntityChangeType,
    /// Entity data (for added/modified)
    pub entity: Option<Entity>,
    /// Previous entity data (for modified/deleted)
    pub previous_entity: Option<Entity>,
}

impl EntityChange {
    /// Create a new entity change
    pub fn new(entity_id: EntityId, entity_name: String, change_type: EntityChangeType) -> Self {
        Self {
            entity_id,
            entity_name,
            change_type,
            entity: None,
            previous_entity: None,
        }
    }

    /// Set current entity
    pub fn with_entity(mut self, entity: Entity) -> Self {
        self.entity = Some(entity);
        self
    }

    /// Set previous entity
    pub fn with_previous_entity(mut self, entity: Entity) -> Self {
        self.previous_entity = Some(entity);
        self
    }

    /// Check if this is an add operation
    pub fn is_added(&self) -> bool {
        matches!(self.change_type, EntityChangeType::Added)
    }

    /// Check if this is a modification
    pub fn is_modified(&self) -> bool {
        matches!(self.change_type, EntityChangeType::Modified)
    }

    /// Check if this is a deletion
    pub fn is_deleted(&self) -> bool {
        matches!(self.change_type, EntityChangeType::Deleted)
    }
}

/// Parse result with change information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseResultWithChanges {
    /// File path
    pub file_path: PathBuf,
    /// Parsed file content
    pub parsed_file: ParsedFile,
    /// Pipeline routing decided at parse time.
    ///
    /// Document variants (`Documentation`/`Config`/`PlainText`) mark non-AST
    /// placeholder parse results (empty entities by design, chunks produced
    /// later through the document pipeline); `Ast` marks a real tree-sitter
    /// parse. Downstream stages must branch on this marker instead of
    /// re-detecting the file type from the path.
    pub content_route: ContentRoute,
    /// File change type
    pub file_change_type: FileChangeType,
    /// Entity changes
    pub entity_changes: Vec<EntityChange>,
    /// Is this a new file (no previous version in storage)
    pub is_new_file: bool,
    /// SHA-256 of the file content this parse result was produced from.
    ///
    /// Populated by `FileProcessor` on every parse; consumers can compare it
    /// against the committed file hash (or the relation base snapshot's file
    /// hash) to detect content drift between change detection and processing
    /// (TOCTOU), e.g. when a dependent file is reparsed mid-operation.
    #[serde(skip)]
    pub content_hash: Option<String>,
    /// File-level summary (generated during hot update)
    #[serde(skip)]
    pub file_summary: Option<FileSummary>,
    /// Set when recovery found an already-exported NL document for this file,
    /// allowing the export processor to skip re-exporting it.
    #[serde(skip)]
    pub already_exported: bool,
    /// Render fingerprint persisted at the last export, used to validate the
    /// `already_exported` skip on recovery. `None` (pre-fingerprint checkpoint)
    /// forces a conservative re-export.
    #[serde(skip)]
    pub stored_render_fingerprint: Option<String>,
    /// Summary configuration fingerprint that produced the persisted summary,
    /// used to decide whether recovery must regenerate it.
    #[serde(skip)]
    pub stored_summary_fingerprint: Option<String>,
    /// Content hash the persisted summary was generated from, used together
    /// with `stored_summary_fingerprint` to decide whether the persisted
    /// summary is still valid for the current file content.
    #[serde(skip)]
    pub stored_content_hash: Option<String>,
    /// Summary configuration fingerprint that produced the current
    /// `file_summary` value (persisted into the checkpoint after generation).
    #[serde(skip)]
    pub summary_fingerprint: Option<String>,
    /// Per-module completion markers restored from the checkpoint on recovery
    /// (module name → fingerprint of the inputs that produced the stored data).
    #[serde(skip)]
    pub module_progress: std::collections::HashMap<String, String>,
}

impl ParseResultWithChanges {
    /// Create a new parse result with changes
    ///
    /// The content route is derived once from the file path here; every
    /// consumer reads `content_route` afterwards.
    pub fn new(
        file_path: PathBuf,
        parsed_file: ParsedFile,
        file_change_type: FileChangeType,
        is_new_file: bool,
    ) -> Self {
        Self {
            content_route: ContentRoute::detect_from_path(&file_path.to_string_lossy()),
            file_path,
            parsed_file,
            file_change_type,
            entity_changes: Vec::new(),
            is_new_file,
            content_hash: None,
            file_summary: None,
            already_exported: false,
            stored_render_fingerprint: None,
            stored_summary_fingerprint: None,
            stored_content_hash: None,
            summary_fingerprint: None,
            module_progress: std::collections::HashMap::new(),
        }
    }

    /// Mark this file as already exported (used during checkpoint recovery).
    pub fn with_already_exported(mut self) -> Self {
        self.already_exported = true;
        self
    }

    /// Attach the render fingerprint persisted at the last export.
    pub fn with_stored_render_fingerprint(mut self, fingerprint: Option<String>) -> Self {
        self.stored_render_fingerprint = fingerprint;
        self
    }

    /// Attach the summary configuration fingerprint persisted with the summary.
    pub fn with_stored_summary_fingerprint(mut self, fingerprint: Option<String>) -> Self {
        self.stored_summary_fingerprint = fingerprint;
        self
    }

    /// Attach the content hash the persisted summary was generated from.
    pub fn with_stored_content_hash(mut self, content_hash: Option<String>) -> Self {
        self.stored_content_hash = content_hash;
        self
    }

    /// Set the file summary
    pub fn with_file_summary(mut self, summary: Option<FileSummary>) -> Self {
        self.file_summary = summary;
        self
    }

    /// Add an entity change
    pub fn add_entity_change(&mut self, change: EntityChange) {
        self.entity_changes.push(change);
    }

    /// Get added entities
    pub fn added_entities(&self) -> Vec<&EntityChange> {
        self.entity_changes
            .iter()
            .filter(|c| c.is_added())
            .collect()
    }

    /// Get modified entities
    pub fn modified_entities(&self) -> Vec<&EntityChange> {
        self.entity_changes
            .iter()
            .filter(|c| c.is_modified())
            .collect()
    }

    /// Get deleted entities
    pub fn deleted_entities(&self) -> Vec<&EntityChange> {
        self.entity_changes
            .iter()
            .filter(|c| c.is_deleted())
            .collect()
    }

    /// Check if there are any entity changes
    pub fn has_entity_changes(&self) -> bool {
        self.entity_changes
            .iter()
            .any(|c| !matches!(c.change_type, EntityChangeType::Unchanged))
    }
}

/// Batch change result for multiple files
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatchChangeResult {
    /// Changed files
    pub file_changes: Vec<FileChange>,
    /// Parse results with changes
    pub parse_results: Vec<ParseResultWithChanges>,
    /// Files that failed to process
    pub failed_files: Vec<(PathBuf, String)>,
    /// Rename mappings: (old_path, new_path) for tracking file renames
    pub rename_mappings: Vec<(PathBuf, PathBuf)>,
}

impl BatchChangeResult {
    /// Create a new batch change result
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a file change
    pub fn add_file_change(&mut self, change: FileChange) {
        self.file_changes.push(change);
    }

    /// Add a parse result
    pub fn add_parse_result(&mut self, result: ParseResultWithChanges) {
        self.parse_results.push(result);
    }

    /// Add a failed file
    pub fn add_failed(&mut self, path: PathBuf, error: String) {
        self.failed_files.push((path, error));
    }

    /// Get count of changed files
    pub fn changed_file_count(&self) -> usize {
        self.file_changes.len()
    }

    /// Get count of successfully processed files
    pub fn processed_count(&self) -> usize {
        self.parse_results.len()
    }

    /// Get count of failed files
    pub fn failed_count(&self) -> usize {
        self.failed_files.len()
    }

    /// Check if batch is empty (no changes)
    pub fn is_empty(&self) -> bool {
        self.file_changes.is_empty() && self.parse_results.is_empty()
    }

    /// Check if there are any changes
    pub fn has_changes(&self) -> bool {
        !self.is_empty()
    }

    /// Get all entity changes across all files
    pub fn all_entity_changes(&self) -> Vec<&EntityChange> {
        self.parse_results
            .iter()
            .flat_map(|r| r.entity_changes.iter())
            .collect()
    }

    /// Add a rename mapping
    pub fn add_rename_mapping(&mut self, from: PathBuf, to: PathBuf) {
        self.rename_mappings.push((from, to));
    }

    /// Check if there are any rename mappings
    pub fn has_rename_mappings(&self) -> bool {
        !self.rename_mappings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;
    use cce_types::entity::EntityKind;

    fn create_test_entity(id: u32, name: &str) -> Entity {
        Entity::new(
            EntityId(id.into()),
            EntityKind::Function,
            name.to_string(),
            Span::default(),
        )
    }

    #[test]
    fn test_file_change_creation() {
        let change = FileChange::new(
            PathBuf::from("test.rs"),
            FileChangeType::Added,
            "abc123".to_string(),
            100,
            Utc::now(),
        );

        assert!(change.is_added());
        assert!(!change.is_modified());
        assert!(!change.is_deleted());
    }

    #[test]
    fn test_entity_change_creation() {
        let entity = create_test_entity(1, "test_func");
        let change = EntityChange::new(
            EntityId(1),
            "test_func".to_string(),
            EntityChangeType::Added,
        )
        .with_entity(entity);

        assert!(change.is_added());
        assert!(change.entity.is_some());
    }

    #[test]
    fn test_batch_change_result() {
        let mut batch = BatchChangeResult::new();

        let file_change = FileChange::new(
            PathBuf::from("test.rs"),
            FileChangeType::Added,
            "abc".to_string(),
            100,
            Utc::now(),
        );
        batch.add_file_change(file_change);

        assert_eq!(batch.changed_file_count(), 1);
        assert!(!batch.is_empty());
    }

    #[test]
    fn test_file_change_with_previous_path() {
        let change = FileChange::new(
            PathBuf::from("new_name.rs"),
            FileChangeType::Renamed,
            "abc123".to_string(),
            100,
            Utc::now(),
        )
        .with_previous_path(PathBuf::from("old_name.rs"));

        assert!(change.is_renamed());
        assert!(change.previous_path.is_some());
        assert_eq!(
            change.previous_path.as_ref().unwrap(),
            &PathBuf::from("old_name.rs")
        );
    }

    #[test]
    fn test_file_change_type_display() {
        assert_eq!(format!("{}", FileChangeType::Added), "added");
        assert_eq!(format!("{}", FileChangeType::Modified), "modified");
        assert_eq!(format!("{}", FileChangeType::Deleted), "deleted");
        assert_eq!(format!("{}", FileChangeType::Renamed), "renamed");
    }

    #[test]
    fn test_entity_change_type_display() {
        assert_eq!(format!("{}", EntityChangeType::Added), "added");
        assert_eq!(format!("{}", EntityChangeType::Modified), "modified");
        assert_eq!(format!("{}", EntityChangeType::Deleted), "deleted");
        assert_eq!(format!("{}", EntityChangeType::Unchanged), "unchanged");
    }

    #[test]
    fn test_batch_change_result_operations() {
        let mut batch = BatchChangeResult::new();

        assert!(batch.is_empty());
        assert!(!batch.has_changes());
        assert_eq!(batch.changed_file_count(), 0);
        assert_eq!(batch.processed_count(), 0);
        assert_eq!(batch.failed_count(), 0);

        // Add file change
        let file_change = FileChange::new(
            PathBuf::from("test.rs"),
            FileChangeType::Added,
            "abc".to_string(),
            100,
            Utc::now(),
        );
        batch.add_file_change(file_change);

        assert!(!batch.is_empty());
        assert!(batch.has_changes());
        assert_eq!(batch.changed_file_count(), 1);

        // Add failed file
        batch.add_failed(PathBuf::from("error.rs"), "Parse error".to_string());
        assert_eq!(batch.failed_count(), 1);
    }
}
