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

    /// Find an entity by ID
    pub fn find_entity(&self, entity_id: cce_types::entity::EntityId) -> Option<&Entity> {
        self.entities.iter().find(|e| e.id == entity_id)
    }
}
