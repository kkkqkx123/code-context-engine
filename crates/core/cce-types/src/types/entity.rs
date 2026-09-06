//! Entity types for semantic extraction
//!
//! Entity is not an AST node wrapper, but a **cross-language unified semantic abstraction**.
//!
//! # Design Principles
//!
//! - **Semantic Abstraction**: Entity represents semantic concepts (Function, Class) not syntax
//! - **Cross-Language Unified**: Different languages map to same EntityKind
//!   - Python def / Rust fn / Java method -> Function
//!   - Python class / Rust struct / Java class -> Class/Struct
//! - **Information Completeness**: All needed info extracted in one pass
//! - **Self-Contained**: No dependency on original AST after extraction
//!
//! # Module Organization
//!
//! - `id`: EntityId - file-local entity identifier
//! - `kind`: EntityKind - cross-language unified entity types
//! - `full`: Entity - complete entity with parent/children relationships (used during parsing)
//! - `grouped`: GroupedEntity - flattened entity representation (used after grouper stage)
//! - `file`: ParsedFile, relations - file-level parsed results
//! - `behavior`: Behavior sidecar facts keyed by entity ID
//! - `control_flow`: Control-flow sidecar facts keyed by entity ID
//! - `embedded_block`: EmbeddedBlock, BlockRelation - SFC embedded code block types

mod behavior;
mod control_flow;
mod embedded_block;
mod file;
mod full;
mod grouped;
mod id;
mod kind;
pub mod meta_keys;
pub use behavior::{BehaviorFact, BehaviorFactKind, BehaviorStore, EntityBehavior};
pub use control_flow::{
    ControlFlowFact, ControlFlowFactKind, ControlFlowStore, EntityControlFlow,
    find_outer_else_offset, has_outer_else_branch,
};
pub use embedded_block::{
    BlockRelation, BlockRelationType, BlockType, EmbeddedBlock, EmbeddedBlockSnapshot,
};
pub use file::{ParseStatus, ParsedFile, RawRelationData};
pub use full::{Entity, EntitySnapshot};
pub use grouped::GroupedEntity;
pub use id::{EntityId, FILE_DOC_SENTINEL_ID};
pub use kind::EntityKind;
