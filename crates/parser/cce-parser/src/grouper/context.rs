//! Context structures for pre-processor strategies
//!
//! This module provides typed context objects for strategy operations,
//! improving code clarity and reducing parameter passing complexity.

use cce_config::NestProcessorConfig;
use cce_types::entity::{Entity, ParsedFile};
use cce_types::language::Language;

/// File-level processing context
///
/// Provides access to file-level information needed by first-level strategies
/// (ClassMethodAssociator, CallMergeStrategy, UtilityMarkStrategy).
pub struct FileProcessingContext<'a> {
    /// All entities in the file
    pub entities: &'a [Entity],

    /// Parsed file information (contains language, local_calls, etc.)
    pub parsed_file: &'a ParsedFile,

    /// Configuration for processing
    pub config: &'a NestProcessorConfig,
}

impl<'a> FileProcessingContext<'a> {
    /// Create a new file processing context
    pub fn new(
        entities: &'a [Entity],
        parsed_file: &'a ParsedFile,
        config: &'a NestProcessorConfig,
    ) -> Self {
        Self {
            entities,
            parsed_file,
            config,
        }
    }

    /// Get the language for this file
    pub fn language(&self) -> &Language {
        &self.parsed_file.language
    }

    /// Get the language governing language-dispatched analysis for one
    /// entity: the innermost embedded block containing it, else the file
    /// language. File-level decisions (path rules, whole-file heuristics)
    /// keep using [`language`](Self::language).
    pub fn language_for(&self, entity: &Entity) -> Language {
        crate::parser::embedded_types::resolve_entity_language(
            &self.parsed_file.embedded_blocks,
            self.parsed_file.language,
            &entity.span,
        )
    }

    /// Find an entity by ID
    pub fn find_entity(&self, entity_id: cce_types::entity::EntityId) -> Option<&Entity> {
        self.entities.iter().find(|e| e.id == entity_id)
    }
}
