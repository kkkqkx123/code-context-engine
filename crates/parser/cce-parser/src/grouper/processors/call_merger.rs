//! Call merge functionality
//!
//! This module provides functionality to detect and merge consecutive identical
//! simple function calls within a function body. This improves the quality of
//! natural language conversion by abstracting repetitive patterns.
//!
//! # Features
//!
//! - Generic call merging: Detect and merge consecutive identical calls
//! - Standard library specialization: Enhanced merging for stdlib calls with semantic information
//!
//! # Example
//!
//! ```ignore
//! fn debug_logs() {
//!     logger.debug("step 1");
//!     logger.debug("step 2");
//!     logger.debug("step 3");
//! }
//! ```
//!
//! After merging, this will be represented as:
//! "A function that performs 3 consecutive calls to logger.debug"

use crate::grouper::context::FileProcessingContext;
use crate::grouper::metadata;
use cce_config::NestProcessorConfig;
use serde::{Deserialize, Serialize};

use crate::grouper::types::StdlibCategory;
use cce_types::entity::Entity;
use cce_types::language::Language;

/// Parameter pattern for merged calls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterPattern {
    /// Parameter type
    pub param_type: String,
    /// Value pattern
    pub value_pattern: ValuePattern,
    /// Whether this parameter is optional
    pub is_optional: bool,
    /// Semantic role of the parameter
    pub semantic_role: SemanticRole,
}

impl Default for ParameterPattern {
    fn default() -> Self {
        Self {
            param_type: String::new(),
            value_pattern: ValuePattern::Default,
            is_optional: false,
            semantic_role: SemanticRole::Input,
        }
    }
}

/// Value pattern type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValuePattern {
    /// Literal value pattern
    Literal(String),
    /// Variable pattern
    Variable(String),
    /// Expression pattern
    Expression(String),
    /// Default value
    Default,
}

/// Semantic role of a parameter
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum SemanticRole {
    /// Input parameter
    #[default]
    Input,
    /// Output parameter
    Output,
    /// Configuration parameter
    Config,
    /// Callback parameter
    Callback,
    /// Context parameter
    Context,
}

/// Information about merged consecutive calls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergedCallInfo {
    /// Name of the callee function/method
    pub callee_name: String,
    /// Number of consecutive calls
    pub count: usize,
    /// Position of the first call
    pub first_call_order: usize,
    /// Position of the last call
    pub last_call_order: usize,
    /// Whether this is a standard library call
    pub is_stdlib: bool,
    /// Standard library category (if stdlib)
    pub stdlib_category: Option<StdlibCategory>,
    /// Semantic pattern description (if stdlib)
    pub semantic_pattern: Option<String>,
    /// Parameter patterns
    pub parameter_patterns: Vec<ParameterPattern>,
}

impl MergedCallInfo {
    /// Create a human-readable description of the merged calls
    pub fn to_description(&self) -> String {
        if self.is_stdlib {
            if let Some(ref semantic) = self.semantic_pattern {
                format!(
                    "{} consecutive calls to {} (stdlib: {})",
                    self.count, self.callee_name, semantic
                )
            } else {
                format!(
                    "{} consecutive calls to {} (stdlib)",
                    self.count, self.callee_name
                )
            }
        } else {
            format!("{} consecutive calls to {}", self.count, self.callee_name)
        }
    }
}

/// Call merger for merging simple repeated calls
///
/// Identifies consecutive identical simple calls and marks them for merging.
/// The actual merging is represented by adding metadata to the entity's
/// doc_comment and storing structured merge information.
pub struct CallMerger {}

impl Default for CallMerger {
    fn default() -> Self {
        Self::new()
    }
}

impl CallMerger {
    /// Create a new call merger
    pub fn new() -> Self {
        Self {}
    }

    /// Merge simple repeated calls
    ///
    /// Returns a tuple of (merged_entities, merge_count)
    ///
    /// # Arguments
    /// * `ctx` - File processing context containing entities and raw relations
    ///
    /// # Returns
    /// * Tuple of (processed entities, number of merge patterns found)
    pub fn merge(&self, ctx: FileProcessingContext) -> (Vec<Entity>, usize) {
        let mut merged_entities = Vec::new();
        let mut total_merge_count = 0;

        // Get language from parsed file
        let language = ctx.parsed_file.language;

        // Resolve local calls from raw_relations
        let config = cce_parser_core::LocalCallResolverConfig {
            enable_signature_matching: true,
            ..Default::default()
        };
        let local_call_resolver = cce_parser_core::LocalCallResolver::with_config(config);
        let local_calls = local_call_resolver.resolve_from_parsed_file(ctx.parsed_file);

        // Iterate through each function
        for entity in ctx.entities {
            if !entity.kind.is_function_like() {
                merged_entities.push(entity.clone());
                continue;
            }

            // Find all calls within this function, sorted by call_order
            let mut calls: Vec<_> = local_calls
                .iter()
                .filter(|c| c.caller == entity.id)
                .cloned()
                .collect();

            // Sort by call_order to ensure correct sequence
            calls.sort_by_key(|c| c.call_order);

            // Detect consecutive repeated calls
            let consecutive_patterns = self.detect_consecutive_calls(&calls, ctx.config, &language);

            if consecutive_patterns.is_empty() {
                // No merge patterns found, keep entity as-is
                merged_entities.push(entity.clone());
            } else {
                // Create merged entity with metadata
                let merged_entity = self.create_merged_entity(entity, &consecutive_patterns);
                merged_entities.push(merged_entity);
                total_merge_count += consecutive_patterns.len();
            }
        }

        (merged_entities, total_merge_count)
    }

    /// Detect consecutive repeated calls
    ///
    /// Analyzes the call sequence to find patterns of consecutive identical calls.
    /// When stdlib merge is enabled, also detects and enhances stdlib call information.
    ///
    /// Note: Standard library calls are skipped from merge processing. While stdlib relations
    /// are preserved for data integrity (see resolver.rs), they should not be merged as they
    /// do not represent business logic dependencies and their relations are marked as external.
    ///
    /// # Arguments
    /// * `calls` - List of calls (should be pre-sorted by call_order)
    /// * `config` - Configuration including merge threshold
    /// * `language` - Programming language for stdlib detection
    ///
    /// # Returns
    /// * List of detected consecutive patterns that meet the threshold
    fn detect_consecutive_calls(
        &self,
        calls: &[cce_parser_core::LocalCall],
        config: &NestProcessorConfig,
        _language: &Language,
    ) -> Vec<MergedCallInfo> {
        let mut patterns = Vec::new();
        let mut current_pattern: Option<MergedCallInfo> = None;

        for call in calls {
            // Use stdlib_category from LocalCall (set during Parser phase)
            // This eliminates duplicate stdlib detection
            let is_stdlib = call.stdlib_category.is_some();
            let stdlib_category = call.stdlib_category;

            // Skip stdlib calls: they don't represent business logic relationships and should
            // not be merged. Stdlib relations are preserved with external_type = StandardLibrary
            // for data integrity, but downstream processing avoids merging them.
            if is_stdlib {
                // Finalize current pattern if we were building one
                if let Some(p) = current_pattern.take() {
                    if p.count >= config.simple_call_merge_threshold {
                        patterns.push(p);
                    }
                }
                // Skip this stdlib call
                continue;
            }

            let pattern = match &mut current_pattern {
                Some(p)
                    if p.callee_name == call.callee_name
                        && p.last_call_order + 1 == call.call_order =>
                {
                    // Consecutive repetition, update existing pattern
                    p.count += 1;
                    p.last_call_order = call.call_order;
                    current_pattern.clone()
                }
                _ => {
                    // Not consecutive or new pattern - save previous if valid
                    if let Some(p) = current_pattern.take() {
                        if p.count >= config.simple_call_merge_threshold {
                            patterns.push(p);
                        }
                    }
                    // Start new pattern
                    Some(MergedCallInfo {
                        callee_name: call.callee_name.clone(),
                        count: 1,
                        first_call_order: call.call_order,
                        last_call_order: call.call_order,
                        is_stdlib,
                        stdlib_category,
                        semantic_pattern: None,
                        parameter_patterns: Vec::new(),
                    })
                }
            };

            current_pattern = pattern;
        }

        // Handle the last pattern
        if let Some(p) = current_pattern {
            if p.count >= config.simple_call_merge_threshold {
                patterns.push(p);
            }
        }

        patterns
    }

    /// Create merged entity with call merge metadata
    ///
    /// Enhances the entity with information about merged consecutive calls.
    /// This information is used in the NL conversion phase to generate
    /// better descriptions.
    ///
    /// **IMPORTANT**: This function preserves the original entity ID to maintain
    /// relationship integrity. The merged entity has the same ID as the input entity.
    ///
    /// # Arguments
    /// * `entity` - Original entity
    /// * `patterns` - Detected consecutive call patterns
    ///
    /// # Returns
    /// * Enhanced entity with merge metadata (same ID as input)
    fn create_merged_entity(&self, entity: &Entity, patterns: &[MergedCallInfo]) -> Entity {
        // Clone preserves the original entity ID - this is critical for
        // maintaining relationship integrity in the inverted index
        let mut merged = entity.clone();

        // Verify ID preservation (debug assertion)
        debug_assert_eq!(
            merged.id, entity.id,
            "Entity ID must be preserved during merge"
        );

        // Build merge information descriptions
        let merge_descriptions: Vec<String> = patterns.iter().map(|p| p.to_description()).collect();

        let merge_info = merge_descriptions.join("; ");

        // Update doc_comment with merge information
        let existing_doc = merged.doc_comment.as_deref().unwrap_or("");
        let new_doc = if existing_doc.is_empty() {
            format!("[Call merge: {}]", merge_info)
        } else {
            format!("{}\n\n[Call merge: {}]", existing_doc, merge_info)
        };
        merged.doc_comment = Some(new_doc);

        // Store structured merge info in metadata field (cleaner approach)
        let merge_json = match serde_json::to_string(patterns) {
            Ok(json) => json,
            Err(e) => {
                tracing::warn!("Failed to serialize merged call info: {}", e);
                String::new()
            }
        };
        merged
            .metadata
            .insert("merged_calls".to_string(), merge_json);

        merged
    }
}

/// Trait extension for Entity to extract merged call information
pub trait EntityCallMergeExt {
    /// Extract merged call information from entity
    ///
    /// Returns None if no merge information was stored
    fn get_merged_call_info(&self) -> Option<Vec<MergedCallInfo>>;

    /// Check if this entity has merged calls
    fn has_merged_calls(&self) -> bool;

    /// Get the count of merged call patterns
    fn merged_call_count(&self) -> usize;
}

impl EntityCallMergeExt for Entity {
    fn get_merged_call_info(&self) -> Option<Vec<MergedCallInfo>> {
        // Parse merge info from metadata field (cleaner approach)
        self.metadata
            .get(metadata::MERGED_CALLS)
            .and_then(|json_str| serde_json::from_str(json_str).ok())
    }

    fn has_merged_calls(&self) -> bool {
        self.metadata.contains_key(metadata::MERGED_CALLS)
    }

    fn merged_call_count(&self) -> usize {
        self.get_merged_call_info().map(|v| v.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::ParsedFile;
    use cce_types::Span;
    use cce_types::entity::{EntityId, EntityKind};

    #[test]
    fn test_call_merger() {
        let merger = CallMerger::new();
        let config = NestProcessorConfig::default();

        let entity = Entity::new(
            EntityId(0),
            EntityKind::Function,
            "test".to_string(),
            Span::default(),
        );

        let parsed_file = ParsedFile::new(
            cce_types::language::Language::Rust,
            "test.rs".to_string(),
            "",
        );
        let entities = &[entity];
        let ctx = FileProcessingContext::new(entities, &parsed_file, &config);
        let (merged, count) = merger.merge(ctx);
        assert_eq!(merged.len(), 1);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_call_merger_default() {
        let merger = CallMerger::new();
        let config = NestProcessorConfig::default();

        let entity = Entity::new(
            EntityId(0),
            EntityKind::Function,
            "test".to_string(),
            Span::default(),
        );

        let parsed_file = ParsedFile::new(
            cce_types::language::Language::Rust,
            "test.rs".to_string(),
            "",
        );
        let entities = &[entity];
        let ctx = FileProcessingContext::new(entities, &parsed_file, &config);
        let (merged, count) = merger.merge(ctx);
        assert_eq!(merged.len(), 1);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_merged_call_info_description() {
        let info = MergedCallInfo {
            callee_name: "println!".to_string(),
            count: 3,
            first_call_order: 1,
            last_call_order: 3,
            is_stdlib: true,
            stdlib_category: Some(StdlibCategory::Macro),
            semantic_pattern: Some("Standard macro: println!".to_string()),
            parameter_patterns: Vec::new(),
        };

        let desc = info.to_description();
        assert!(desc.contains("stdlib"));
        assert!(desc.contains("println!"));
    }

    #[test]
    fn test_parameter_pattern() {
        let pattern = ParameterPattern {
            param_type: "String".to_string(),
            value_pattern: ValuePattern::Literal("test".to_string()),
            is_optional: false,
            semantic_role: SemanticRole::Input,
        };

        assert_eq!(pattern.param_type, "String");
        assert!(!pattern.is_optional);
    }
}
