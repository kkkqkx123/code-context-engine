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
pub mod native;
pub mod pattern;
pub mod registry;
pub mod types;
pub mod utils;

pub use loader::LuaPlugin;
pub use loader::lua_mapping::{
    entity_group_to_lua_table, group_conversions_to_lua_table, grouped_entity_to_lua_table,
    lua_table_to_chunked_result, lua_table_to_plugin_document, lua_table_to_plugin_entity,
    lua_table_to_rerank_result,
};
pub use native::NativePlugin;
pub use pattern::{CompiledPattern, PatternDeclaration, compile_patterns, extract_entities};
pub use registry::FilePluginSource;
pub use types::{PluginEntry, PluginRegistryFile, PluginType};
pub use utils::{CancellationToken, execute_with_timeout_blocking};

/// Default timeout for lightweight operations (filter_file, rewrite_query, etc.)
const LIGHTWEIGHT_TIMEOUT_MS: u64 = 1_000;
/// Default timeout for standard operations (single NL generation, parse, etc.)
const STANDARD_TIMEOUT_MS: u64 = 5_000;
/// Default timeout for heavyweight operations (batch generation, relation extract, etc.)
const HEAVYWEIGHT_TIMEOUT_MS: u64 = 30_000;

/// Return the default timeout in milliseconds for a given operation.
///
/// Three tiers:
/// - Lightweight (1s): filter_file, rewrite_query, classify_stdlib, is_test_file, entity_kind
/// - Standard (5s): generate_bm25, generate_embedding, parse_document, post_group, chunk, rerank, filter_results, fusion_weights
/// - Heavyweight (30s): generate_bm25_batch, generate_embedding_batch, extract_entities, extract_symbols, extract_relations, extract_imports, extract_exports
pub fn default_timeout_for(operation: &str) -> u64 {
    match operation {
        "filter_file" | "rewrite_query" | "classify_stdlib" | "is_test_file" | "entity_kind" => {
            LIGHTWEIGHT_TIMEOUT_MS
        }
        "generate_bm25_batch"
        | "generate_embedding_batch"
        | "extract_entities"
        | "extract_symbols"
        | "extract_relations"
        | "extract_imports"
        | "extract_exports" => HEAVYWEIGHT_TIMEOUT_MS,
        _ => STANDARD_TIMEOUT_MS,
    }
}
