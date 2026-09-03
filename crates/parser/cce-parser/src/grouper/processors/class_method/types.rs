use smallvec::SmallVec;

use crate::grouper::types::PatternInfo;
use crate::grouper::types::pattern::MemberRole;
use cce_config::NestProcessorConfig;
use cce_types::entity::{Entity, EntityId, ParsedFile};
use cce_types::language::Language;

/// Result of pattern processing for a class
///
/// Contains filtered methods and pattern detection information
/// to be stored in EntityGroup for later use in conversion.
#[derive(Debug, Clone)]
pub struct PatternProcessingResult {
    /// Filtered methods to include in the group
    pub methods: Vec<Entity>,
    /// Pattern information (Builder, Factory, Getter/Setter)
    pub pattern_info: PatternInfo,
    /// Member roles (significant vs boilerplate)
    pub member_roles: SmallVec<[(EntityId, MemberRole); 8]>,
}

/// Context for extracting nested entity groups
///
/// Encapsulates all parameters needed for recursive nested group extraction.
#[derive(Debug)]
pub struct NestedExtractionContext<'a> {
    /// All entities in the file
    pub all_entities: &'a [Entity],
    /// Remaining nesting depth
    pub max_depth: usize,
    /// Minimum size for nested entities
    pub min_nested_size: usize,
    /// Already processed entity IDs
    pub processed_ids: &'a mut std::collections::HashSet<EntityId>,
    /// Language of the source code
    pub language: &'a Language,
    /// Parsed file containing source code
    pub parsed_file: &'a ParsedFile,
    /// Configuration for nest processor
    pub config: &'a NestProcessorConfig,
}
