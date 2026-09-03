//! Plugin system for Code Context Engine
//!
//! This module provides plugin loading implementations:
//! - **Lua scripts** — loaded via `mlua`, see [`loader`]
//! - **Native dynamic libraries** — loaded via `libloading`, see [`native`]
//! - **File-based source** — loads from `plugins.json`, see [`registry`]
//!
//! The pure in-memory registry and `PluginSource` trait live in
//! `cce_plugin`. This crate provides concrete sources and loaders.

mod error;
pub mod loader;
pub mod lua_mapping;
pub mod native;
pub mod pattern;
pub mod registry;
pub mod types;
pub mod utils;

pub use loader::LuaPlugin;
pub use lua_mapping::{
    entity_group_to_lua_table, group_conversions_to_lua_table, grouped_entity_to_lua_table,
    lua_table_to_chunked_result, lua_table_to_plugin_document, lua_table_to_plugin_entity,
    lua_table_to_rerank_result,
};
pub use native::NativePlugin;
pub use pattern::{CompiledPattern, PatternDeclaration, compile_patterns, extract_entities};
pub use registry::FilePluginSource;
pub use types::{PluginEntry, PluginRegistryFile, PluginType};
pub use utils::{CancellationToken, execute_with_timeout_blocking};
