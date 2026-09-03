//! FFI scenario tests: an NL plugin implementing single + batch generation.
//!
//! Each integration test crate is its own binary, so this crate may declare
//! its own `cce_plugin_*` exports without colliding with the lib crate's
//! `#[cfg(test)]`-only `TestPlugin`.

use cce_plugin_sdk::{declare_plugin, FfiPlugin, PluginError, PluginMetadata};

struct NlPlugin;

impl Default for NlPlugin {
    fn default() -> Self {
        NlPlugin
    }
}

impl FfiPlugin for NlPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "test/nl-plugin".to_string(),
            name: "NL Plugin".to_string(),
            version: "0.2.0".to_string(),
            priority: 1,
            capability_priorities: std::collections::HashMap::new(),
            description: None,
            capabilities: Vec::new(),
        }
    }

    fn supports_bm25(&self) -> bool {
        true
    }

    fn supports_embedding(&self) -> bool {
        true
    }

    fn generate_bm25(
        &self,
        _ctx: *mut std::ffi::c_void,
        group_json: &str,
    ) -> Result<Option<String>, PluginError> {
        let v: serde_json::Value = serde_json::from_str(group_json).unwrap();
        Ok(Some(format!(
            "bm25:{}",
            v["group_id"].as_str().unwrap_or("?")
        )))
    }

    fn generate_embedding(
        &self,
        _ctx: *mut std::ffi::c_void,
        group_json: &str,
    ) -> Result<Option<String>, PluginError> {
        let v: serde_json::Value = serde_json::from_str(group_json).unwrap();
        Ok(Some(format!(
            "embedding:{}",
            v["group_id"].as_str().unwrap_or("?")
        )))
    }

    fn generate_bm25_batch(
        &self,
        _ctx: *mut std::ffi::c_void,
        groups_json: &str,
    ) -> Result<Vec<Option<String>>, PluginError> {
        let groups: Vec<serde_json::Value> = serde_json::from_str(groups_json).unwrap();
        Ok(groups
            .into_iter()
            .map(|g| {
                Some(format!(
                    "batch-bm25:{}",
                    g["group_id"].as_str().unwrap_or("?")
                ))
            })
            .collect())
    }
}

declare_plugin!(NlPlugin);

fn read(ptr: *mut std::ffi::c_char) -> String {
    assert!(!ptr.is_null());
    let s = unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_str()
        .unwrap()
        .to_string();
    unsafe { cce_plugin_free_string(ptr) };
    s
}

#[test]
fn test_metadata() {
    let json = read(cce_plugin_metadata());
    let meta: PluginMetadata = serde_json::from_str(&json).unwrap();
    assert_eq!(meta.id, "test/nl-plugin");
    assert_eq!(meta.name, "NL Plugin");
    assert_eq!(meta.version, "0.2.0");
    assert_eq!(meta.priority, 1);
}

#[test]
fn test_capabilities() {
    assert!(cce_plugin_has_bm25_generation());
    assert!(cce_plugin_has_embedding_generation());
    assert!(!cce_plugin_has_lifecycle());
}

#[test]
fn test_bm25_single() {
    let group_json = std::ffi::CString::new(r#"{"group_id":"g1"}"#).unwrap();
    let ptr = unsafe { cce_plugin_generate_bm25(std::ptr::null_mut(), group_json.as_ptr()) };
    let s = read(ptr);
    let result: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(result["result"], "ok");
    assert_eq!(result["value"], "bm25:g1");
}

#[test]
fn test_bm25_batch() {
    let groups_json = std::ffi::CString::new(r#"[{"group_id":"g1"},{"group_id":"g2"}]"#).unwrap();
    let ptr = unsafe { cce_plugin_generate_bm25_batch(std::ptr::null_mut(), groups_json.as_ptr()) };
    let s = read(ptr);
    let result: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(result["result"], "ok");
    assert_eq!(result["value"][0], "batch-bm25:g1");
    assert_eq!(result["value"][1], "batch-bm25:g2");
}

#[test]
fn test_embedding_batch_falls_back_to_single() {
    let groups_json = std::ffi::CString::new(r#"[{"group_id":"g1"},{"group_id":"g2"}]"#).unwrap();
    let ptr =
        unsafe { cce_plugin_generate_embedding_batch(std::ptr::null_mut(), groups_json.as_ptr()) };
    let s = read(ptr);
    let result: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(result["result"], "ok");
    assert_eq!(result["value"][0], "embedding:g1");
    assert_eq!(result["value"][1], "embedding:g2");
}

#[test]
fn test_lifecycle_null() {
    assert!(cce_plugin_create().is_null());
    unsafe { cce_plugin_destroy(std::ptr::null_mut()) };
}
