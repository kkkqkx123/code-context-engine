//! In-memory input collection for a full relation build.
//!
//! A complete relation graph needs multiple passes over parsed files. This
//! collection gathers the operation-local slim copies ([`RelBuildEntry`])
//! once per batch, so the finalize passes read directly from memory without
//! a disk round-trip.

use std::collections::HashMap;

use cce_types::import::ReexportRecord;
use cce_types::{Entity, EntityId, ImportTable, Language, ParsedFile, RawRelationData};
use cce_relation::symbol_table::ProjectSymbolTable;

/// Slim representation of a parsed file for relation construction.
///
/// Only the fields consumed by `register_file_entities`,
/// `resolve_file_relations`, and the plugin replay paths are retained;
/// NL/export-oriented sidecars (behavior, control flow, embedded blocks,
/// block relations, doc comments) are intentionally dropped.
#[derive(Debug, Clone)]
pub(crate) struct RelBuildEntry {
    pub path: String,
    pub language: Language,
    pub source: String,
    pub entities: Vec<Entity>,
    pub local_symbols: HashMap<String, Vec<EntityId>>,
    pub raw_relations: Vec<RawRelationData>,
    pub import_table: Option<ImportTable>,
    pub reexports: Vec<ReexportRecord>,
    pub file_hash: Option<String>,
}

impl RelBuildEntry {
    fn from_parsed(parsed: &ParsedFile) -> Self {
        Self {
            path: parsed.path.clone(),
            language: parsed.language,
            source: parsed.source.to_string(),
            entities: parsed.entities.clone(),
            local_symbols: parsed.local_symbols.clone(),
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
            entities: self.entities.clone(),
            local_symbols: self.local_symbols.clone(),
            raw_relations: self.raw_relations.clone(),
            import_table: self.import_table.clone(),
            reexports: self.reexports.clone(),
            file_hash: self.file_hash.clone(),
            ..Default::default()
        }
    }
}

/// Operation-local parsed-file collection for relation construction.
///
/// Entries are retained in insertion order so all later passes are
/// deterministic.
pub(crate) struct RelationBuildInputs {
    entries: Vec<RelBuildEntry>,
    project_symbols: ProjectSymbolTable,
}

impl RelationBuildInputs {
    /// Create an empty collection with the already initialized project symbol
    /// table.
    pub(crate) fn new(project_symbols: ProjectSymbolTable) -> Self {
        Self {
            entries: Vec::new(),
            project_symbols,
        }
    }

    /// Retain one parsed file for later passes.
    pub(crate) fn append(&mut self, parsed: &ParsedFile) {
        self.entries.push(RelBuildEntry::from_parsed(parsed));
    }

    /// Replay every collected parsed file in insertion order; returns the
    /// number of visited files.
    pub(crate) fn for_each(&self, mut visit: impl FnMut(&ParsedFile)) -> usize {
        let mut visited = 0usize;
        for entry in &self.entries {
            let parsed = entry.to_parsed();
            visit(&parsed);
            visited += 1;
        }
        visited
    }

    /// Access the project-wide symbols accumulated while inputs were gathered.
    pub(crate) fn project_symbols(&self) -> &ProjectSymbolTable {
        &self.project_symbols
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::{EntityKind, RelationType, Span};
    use std::path::PathBuf;

    fn sample_entry() -> RelBuildEntry {
        RelBuildEntry {
            path: "src/lib.rs".to_string(),
            language: Language::Rust,
            source: "fn alpha() {} pub fn beta() {}".to_string(),
            entities: vec![Entity::new(
                EntityId(0),
                EntityKind::Function,
                "alpha".to_string(),
                Span::default(),
            )],
            local_symbols: HashMap::from([("alpha".to_string(), vec![EntityId(0)])]),
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
    fn replays_inputs_in_insertion_order_with_slim_fields() {
        let inputs = RelationBuildInputs::new(ProjectSymbolTable::new(PathBuf::from(".")));
        let mut first = sample_entry();
        first.path = "first.rs".to_string();
        let mut second = sample_entry();
        second.path = "second.rs".to_string();
        let mut inputs = inputs;
        inputs.append(&first.to_parsed());
        inputs.append(&second.to_parsed());

        let mut paths = Vec::new();
        let count = inputs.for_each(|parsed| paths.push(parsed.path.clone()));

        assert_eq!(count, 2);
        assert_eq!(paths, ["first.rs", "second.rs"]);
    }

    #[test]
    fn empty_collection_visits_nothing() {
        let inputs = RelationBuildInputs::new(ProjectSymbolTable::new(PathBuf::from(".")));
        assert_eq!(inputs.for_each(|_| ()), 0);
    }
}
