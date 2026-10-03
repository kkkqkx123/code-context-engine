//! Low-level Lua table field accessors.
//!
//! Best-effort readers that coerce Lua values into the Rust types used by the
//! mapping modules. Missing or mismatched fields fall back to defaults.

use mlua::{Table, Value};

/// Read an optional string field from a Lua table.
pub(super) fn get_string(table: &Table, key: &str) -> Option<String> {
    match table.get::<Value>(key).ok()? {
        Value::String(s) => Some(s.to_string_lossy().to_string()),
        Value::Integer(i) => Some(i.to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Read a string→string map field from a Lua table.
pub(super) fn get_string_map(
    table: &Table,
    key: &str,
) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    if let Ok(Some(map)) = table.get::<Option<Table>>(key) {
        for (k, v) in map.pairs::<String, Value>().flatten() {
            let value = match v {
                Value::String(s) => s.to_string_lossy().to_string(),
                Value::Integer(i) => i.to_string(),
                Value::Number(n) => n.to_string(),
                Value::Boolean(b) => b.to_string(),
                _ => continue,
            };
            out.insert(k, value);
        }
    }
    out
}

/// Read an optional array-of-strings field from a Lua table.
pub(super) fn get_string_array(table: &Table, key: &str) -> Option<Vec<String>> {
    let array = table.get::<Option<Table>>(key).ok()??;
    let mut out = Vec::new();
    for pair in array.pairs::<Value, Value>() {
        if let Ok((_, Value::String(s))) = pair {
            out.push(s.to_string_lossy().to_string());
        }
    }
    Some(out)
}

/// Read an optional u64 field.
pub(super) fn get_u64(table: &Table, key: &str) -> u64 {
    table.get::<Option<u64>>(key).ok().flatten().unwrap_or(0)
}

/// Read an optional f32 field.
pub(super) fn get_f32(table: &Table, key: &str) -> Option<f32> {
    table.get::<Option<f32>>(key).ok().flatten()
}

/// Read an optional i32 field.
pub(super) fn get_i32(table: &Table, key: &str) -> Option<i32> {
    table.get::<Option<i32>>(key).ok().flatten()
}
