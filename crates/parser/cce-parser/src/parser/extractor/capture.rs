//! Capture module: pure tree-sitter capture → entity data extraction
//!
//! This module provides functions that extract typed data from tree-sitter query
//! matches without modifying any entity state. It represents the "pure capture"
//! layer between raw tree-sitter output and the entity model.

pub mod kind_mapper;
pub mod parser;

pub use kind_mapper::determine_entity_kind;
pub use parser::*;
