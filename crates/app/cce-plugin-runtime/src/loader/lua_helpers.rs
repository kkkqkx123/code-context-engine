use mlua::Table;

use crate::error::PluginError;
use crate::lua_mapping::lua_table_to_plugin_entity;
use cce_types::PluginEntity;

/// Read the `plugin.priority` metadata field.
///
/// A missing field defaults to `0`; a field of the wrong type or an
/// overflowing value is a load-time error (instead of being silently
/// treated as `0`). Negative values are valid: they place the plugin below
/// the built-in implementation (fallback tier).
pub(crate) fn read_lua_priority(plugin_table: &Table) -> Result<i32, PluginError> {
    use mlua::Value;
    let raw = plugin_table
        .get::<Value>("priority")
        .map_err(|e| PluginError::ScriptError(format!("Failed to read plugin.priority: {e}")))?;
    match raw {
        Value::Nil => Ok(0),
        Value::Integer(i) => i32::try_from(i).map_err(|_| {
            PluginError::ScriptError(format!("plugin.priority must be a 32-bit integer, got {i}"))
        }),
        Value::Number(n) if n.is_finite() && n.fract() == 0.0 => {
            i32::try_from(n as i64).map_err(|_| {
                PluginError::ScriptError(format!(
                    "plugin.priority must be a 32-bit integer, got {n}"
                ))
            })
        }
        _ => Err(PluginError::ScriptError(
            "plugin.priority must be a 32-bit integer".to_string(),
        )),
    }
}

/// Parse a query-type string key from a `LanguageRemap` plugin's
/// `query_schemes` table (e.g. `"entity"` → [`QueryType::Entity`]).
pub(crate) fn parse_query_type(key: &str) -> Option<cce_types::QueryType> {
    use cce_types::QueryType;
    QueryType::ALL
        .iter()
        .find(|qt| qt.to_string().eq_ignore_ascii_case(key) || qt.as_u32().to_string() == key)
        .copied()
}

/// Map a plugin operation name to its capability-facet metric label.
pub(crate) fn capability_label(operation: &str) -> &'static str {
    match operation {
        "generate_bm25"
        | "generate_embedding"
        | "generate_bm25_batch"
        | "generate_embedding_batch" => "text_gen",
        "parse_document" => "format_parse",
        "extract_entities" => "entity_extract",
        "post_group" => "group",
        "group" => "group_override",
        "chunk" => "chunk",
        "rerank" => "rerank",
        "extract_symbols" | "extract_relations" => "relation_extract",
        "extract_imports" | "extract_exports" => "symbol_extract",
        "rewrite_query" => "query_rewrite",
        "fusion_weights" => "fusion",
        "filter_results" => "result_filter",
        "filter_file" => "file_filter",
        _ => "unknown",
    }
}

/// Parse a 1-indexed Lua array of entity tables.
pub(crate) fn parse_entity_array(table: &Table) -> Vec<PluginEntity> {
    let mut out = Vec::new();
    for pair in table.pairs::<mlua::Value, mlua::Value>() {
        if let Ok((_, mlua::Value::Table(t))) = pair {
            if let Ok(e) = lua_table_to_plugin_entity(&t) {
                out.push(e);
            }
        }
    }
    out
}

/// Read a string field from a Lua table (nil-tolerant).
pub(crate) fn get_string_field(table: &Table, key: &str) -> Option<String> {
    match table.get::<mlua::Value>(key).ok()? {
        mlua::Value::String(s) => Some(s.to_string_lossy().to_string()),
        _ => None,
    }
}
