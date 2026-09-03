//! Common types used across the relation module
//!
//! This module provides shared type definitions that are used by multiple
//! sub-modules within the relation module. Types are organized by their usage:
//!
//! # Type Categories
//!
//! ## Export Types
//! - [`ExportInfo`]: Information about an exported symbol
//! - [`ExportType`]: Type of export (named, default, wildcard)
//!
//! ## Call Chain Types
//! - [`CallChainNode`]: A node in the call chain (function info + depth)
//! - [`CallChainPath`]: A path between two functions in the call graph
//!
//! # Module Organization
//!
//! These types are defined here because they are shared across multiple modules:
//! - `index/core.rs`: Re-exports `CallChainNode`, `CallChainPath`, `ExportInfo`, `ExportType`
//!
//! Types that are specific to a single module should be defined in that module
//! rather than here.

use cce_types::relation::CallContext;
use cce_types::{EntityId, RelationType};
use rkyv::{Archive, Deserialize, Serialize};
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

/// Export information
#[derive(Debug, Clone, SerdeSerialize, SerdeDeserialize, Archive, Serialize, Deserialize)]
pub struct ExportInfo {
    /// Function ID
    pub function_id: EntityId,
    /// Function name
    pub function_name: String,
    /// Export type
    pub export_type: ExportType,
}

/// Export type enumeration
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    SerdeSerialize,
    SerdeDeserialize,
    Default,
    Archive,
    Serialize,
    Deserialize,
)]
pub enum ExportType {
    /// Named export
    #[default]
    Named,
    /// Default export
    Default,
    /// Wildcard export
    Wildcard,
}

/// Call chain node representing a function in the call chain
#[derive(Debug, Clone, SerdeSerialize, SerdeDeserialize, Archive, Serialize, Deserialize)]
pub struct CallChainNode {
    /// Function ID
    pub function_id: EntityId,
    /// Function name
    pub function_name: String,
    /// File path
    pub file_path: String,
    /// Call depth from root
    pub depth: usize,
    /// Relation type (how this function was called)
    pub relation_type: RelationType,
    /// Call line number
    pub call_line: Option<usize>,
    /// Owner type for method/constructor calls (if resolved)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_type: Option<String>,
    /// Call context providing additional information about how the call is made
    #[serde(default)]
    pub call_context: CallContext,
}

/// Call chain path between two functions
#[derive(Debug, Clone, SerdeSerialize, SerdeDeserialize, Archive, Serialize, Deserialize)]
pub struct CallChainPath {
    /// Path nodes from source to target
    pub nodes: Vec<CallChainNode>,
    /// Path length
    pub length: usize,
}

/// Call graph with nodes and edges for visualization.
#[derive(Debug, Clone, Default)]
pub struct CallChainGraph {
    /// Nodes in the graph (BFS traversal result)
    pub nodes: Vec<CallChainNode>,
    /// Edges as (caller, callee, relation_type)
    pub edges: Vec<(EntityId, EntityId, RelationType)>,
}
