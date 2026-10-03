//! Lua mapping utilities for converting Rust types to Lua tables.
//!
//! This module provides helper functions to convert EntityGroup and related types
//! into Lua-compatible table structures for plugin interaction. Responsibilities
//! are split across submodules:
//!
//! - [`table_accessors`] — low-level Lua table field readers
//! - [`entity_mapping`] — EntityGroup / GroupedEntity conversions
//! - [`contract_mapping`] — plugin extension contract conversions

mod contract_mapping;
mod entity_mapping;
pub(crate) mod table_accessors;

pub use contract_mapping::{
    group_conversions_to_lua_table, group_plugin_context_to_lua_table, lua_table_to_chunked_result,
    lua_table_to_filter_entries, lua_table_to_plugin_document, lua_table_to_plugin_entity,
    lua_table_to_plugin_exports, lua_table_to_plugin_imports, lua_table_to_plugin_relations,
    lua_table_to_plugin_symbols, lua_table_to_rerank_result,
};
pub use entity_mapping::{
    entity_group_to_lua_table, grouped_entity_to_lua_table, lua_table_to_entity_group,
};
