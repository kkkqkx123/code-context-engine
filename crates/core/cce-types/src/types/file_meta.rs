//! File metadata types
//!
//! This module provides types for file-level metadata management including:
//! - File information (path, language, timestamps, statistics)
//! - Import tables (unified import representation across languages)
//!
//! These types are used by the file state manager and relation index.

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

use crate::types::import::{ImportKind, StandardizedImport, StandardizedImportTable};

#[derive(
    Debug, Clone, Serialize, Deserialize, RkyvSerialize, RkyvDeserialize, Archive, Default,
)]
pub struct ImportTable {
    /// File ID
    pub file_id: String,
    /// Provides a unified representation that works across all supported languages.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub standardized_imports: Vec<StandardizedImport>,
    /// Import source statistics
    #[serde(default)]
    pub source_stats: crate::types::import::ImportSourceStats,
}

impl ImportTable {
    /// Add a standardized import
    pub fn add_standardized_import(&mut self, import: StandardizedImport) {
        self.standardized_imports.push(import);
    }

    /// Get standardized imports filtered by kind
    pub fn standardized_imports_by_kind(&self, kind: ImportKind) -> Vec<&StandardizedImport> {
        self.standardized_imports
            .iter()
            .filter(|i| i.kind == kind)
            .collect()
    }

    /// Get wildcard standardized imports
    pub fn standardized_wildcard_imports(&self) -> Vec<&StandardizedImport> {
        self.standardized_imports
            .iter()
            .filter(|i| i.is_wildcard)
            .collect()
    }

    /// Convert from StandardizedImportTable
    pub fn from_standardized(table: &StandardizedImportTable) -> Self {
        Self {
            file_id: table.file_id.clone(),
            standardized_imports: table.imports.clone(),
            source_stats: crate::types::import::ImportSourceStats::default(),
        }
    }

    /// Convert to StandardizedImportTable
    pub fn to_standardized(&self) -> StandardizedImportTable {
        StandardizedImportTable {
            file_id: self.file_id.clone(),
            imports: self.standardized_imports.clone(),
        }
    }

    /// Get total import count
    pub fn import_count(&self) -> usize {
        self.standardized_imports.len()
    }

    /// Get all imports as StandardizedImport
    pub fn all_standardized_imports(&self) -> &[StandardizedImport] {
        &self.standardized_imports
    }
}

/// File information with parsing metadata
///
/// Combines basic file info with parsing state and statistics.
/// Managed by FileStateManager for project-wide coordination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    // === Basic Information ===
    /// File ID (usually file path)
    pub id: String,
    /// File path
    pub path: String,
    /// Programming language
    pub language: String,

    // === File Metadata ===
    /// File content hash (SHA256)
    pub file_hash: String,
    /// File size in bytes
    pub file_size: u64,
    /// Last modified time (timestamp)
    pub modified_time: u64,

    // === Parse Status ===
    /// Current parse status
    pub parse_status: crate::types::entity::ParseStatus,
    /// Parse errors list
    pub parse_errors: Vec<String>,
    /// Parse version (incremented on each re-parse)
    pub parse_version: u64,

    // === Statistics ===
    /// Number of entities in parsed file
    pub entity_count: usize,
    /// Number of relations in parsed file
    pub relation_count: usize,
    /// Number of exports
    pub export_count: usize,
    /// Number of imports
    pub import_count: usize,

    // === Dependencies ===
    /// Files this file depends on (outgoing dependencies)
    /// Note: Incoming dependencies (depended_by) are tracked by FileDependencyGraph
    /// to avoid data redundancy and ensure consistency
    pub depends_on: Vec<String>,
}

impl Default for FileInfo {
    fn default() -> Self {
        Self {
            id: String::new(),
            path: String::new(),
            language: String::new(),
            file_hash: String::new(),
            file_size: 0,
            modified_time: 0,
            parse_status: crate::types::entity::ParseStatus::Pending,
            parse_errors: Vec::new(),
            parse_version: 0,
            entity_count: 0,
            relation_count: 0,
            export_count: 0,
            import_count: 0,
            depends_on: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_info_default() {
        let info = FileInfo::default();
        assert!(info.id.is_empty());
        assert!(info.path.is_empty());
        assert_eq!(info.file_size, 0);
        assert!(info.depends_on.is_empty());
    }

    #[test]
    fn test_import_table_count() {
        let table = ImportTable::default();
        assert_eq!(table.import_count(), 0);
    }
}
