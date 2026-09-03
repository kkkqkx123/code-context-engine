//! Entity preprocessing module for optimization before AST to NL conversion
//!
//! This module handles entity preprocessing between the Parser and AstToNl stages.
//! It improves the quality of natural language conversion by:
//!
//! - Merging simple repeated function calls
//! - Associating classes with their methods based on size
//! - Grouping test suites with their test cases
//! - Merging simple getters/setters into their class
//!
//! # Architecture
//!
//! ```text
//! Parser → [PreprocessingPipeline] → AstToNl → ...
//!                  ↓
//!           Vec<EntityGroup>
//! ```
//!
//! # Pipeline Stages
//!
//! The preprocessing pipeline consists of four stages:
//! 1. **Call Merging** - Merge repeated simple calls
//! 2. **Test Suite Grouping** - Group test suites with test cases
//! 3. **Class-Method Association** - Associate small classes with methods
//! 4. **Source Generation** - Generate combined source for groups
//!
//! # Components
//!
//! - `pipeline` - Main preprocessing pipeline coordinator
//! - `types` - Shared type definitions (EntityGroup, PatternInfo, StdlibCategory)
//! - `recognizers` - Entity recognition (test suites, standard library)
//! - `processors` - Entity processing (call merging, grouping)
//! - `config` - Configuration for the pipeline
//! - `context` - Context structures for processing
//! - `metrics` - Metrics collection for pipeline operations
//!
//! # Pattern Detection Policy
//!
//! Framework and design pattern recognition (DTO, Repository, EventHandler,
//! Builder, Singleton, etc.) is not built in: name/path heuristics lose member
//! information and produce false positives. Domain-specific recognition is
//! delegated to the plugin system.

// Internal implementation modules (crate-only access)

pub mod builtin_stages;
pub(crate) mod context;
pub(crate) mod language_patterns;
pub mod metadata;
pub mod pipeline;
pub mod plugin_grouping;
pub(crate) mod processors;
pub(crate) mod recognizers;
#[cfg(test)]
pub(crate) mod test_utils;
pub mod types;

// Re-export main types
pub use cce_config::NestProcessorConfig;
pub use context::FileProcessingContext;
pub use pipeline::{PipelineBuilder, PreprocessingPipeline};

// Re-export types
pub use types::{
    EntityGroup, EntityMeta, GetterSetterSummary, GroupRole, GroupType, MemberRole,
    MemberRolesBuilder, PatternInfo, ProcessingResult, ProcessingStats, SpanError, ValidationError,
};

// Re-export recognizers
pub use recognizers::{StdlibEntityDetector, StdlibEntityInfo};

// Re-export processors
pub use processors::{
    CallMerger, ClassMethodProcessor, EntityCallMergeExt, GetterSetterDetectionConfig,
    GetterSetterDetector, MergedCallInfo, MethodType, ParameterPattern, SemanticRole, ValuePattern,
};
