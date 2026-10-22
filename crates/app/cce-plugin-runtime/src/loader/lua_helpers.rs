use mlua::Table;

use crate::error::PluginError;
use crate::loader::lua_mapping::lua_table_to_plugin_entity;
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
        "vocabulary" => "vocabulary",
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

/// Read the scope of a vocabulary entry.
///
/// The scope is a table with an `include` list, an `exclude` list and an
/// optional `priority`. A missing scope yields `None`.
///
/// A scope that cannot be read yields an error: the entry states a constraint
/// the host cannot honor, and honoring it as "applies everywhere" would put a
/// wrong meaning into the index.
pub(crate) fn read_vocabulary_scope(
    entry: &Table,
) -> Result<Option<cce_types::plugin::VocabularyScope>, String> {
    let raw: mlua::Value = entry
        .get("scope")
        .map_err(|error| format!("failed to read the scope: {error}"))?;
    let mlua::Value::Table(scope) = raw else {
        if matches!(raw, mlua::Value::Nil) {
            return Ok(None);
        }
        return Err("the scope must be a table".to_string());
    };
    let include = read_pattern_list(&scope, "include")?;
    let exclude = read_pattern_list(&scope, "exclude")?;
    let priority = read_scope_priority(&scope)?;
    Ok(Some(cce_types::plugin::VocabularyScope {
        include,
        exclude,
        priority,
    }))
}

/// Read one list of path patterns of a scope.
fn read_pattern_list(scope: &Table, key: &str) -> Result<Vec<String>, String> {
    let raw: mlua::Value = scope
        .get(key)
        .map_err(|error| format!("failed to read the scope {key}: {error}"))?;
    let mlua::Value::Table(list) = raw else {
        if matches!(raw, mlua::Value::Nil) {
            return Ok(Vec::new());
        }
        return Err(format!("the scope {key} must be a list of patterns"));
    };
    let mut patterns = Vec::new();
    for pair in list.pairs::<mlua::Value, mlua::Value>() {
        let (_, value) =
            pair.map_err(|error| format!("failed to read the scope {key}: {error}"))?;
        let mlua::Value::String(pattern) = value else {
            return Err(format!("the scope {key} must be a list of patterns"));
        };
        patterns.push(pattern.to_string_lossy().to_string());
    }
    Ok(patterns)
}

/// Read the priority of a scope.
fn read_scope_priority(scope: &Table) -> Result<i32, String> {
    let raw: mlua::Value = scope
        .get("priority")
        .map_err(|error| format!("failed to read the scope priority: {error}"))?;
    match raw {
        mlua::Value::Nil => Ok(0),
        mlua::Value::Integer(value) => i32::try_from(value)
            .map_err(|_| format!("the scope priority must be a 32-bit integer, got {value}")),
        mlua::Value::Number(value) if value.is_finite() && value.fract() == 0.0 => {
            i32::try_from(value as i64)
                .map_err(|_| format!("the scope priority must be a 32-bit integer, got {value}"))
        }
        _ => Err("the scope priority must be a 32-bit integer".to_string()),
    }
}
