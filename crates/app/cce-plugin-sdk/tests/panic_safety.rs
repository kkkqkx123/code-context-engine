//! Panic-safety tests: a plugin whose methods panic. Every `extern "C"`
//! export must contain the panic and report it as an FFI error result
//! instead of unwinding across the C ABI (which would be undefined
//! behaviour).

use cce_plugin_sdk::{declare_plugin, FfiPlugin, PluginError, PluginMetadata};

struct PanicPlugin;

impl Default for PanicPlugin {
    fn default() -> Self {
        PanicPlugin
    }
}

impl FfiPlugin for PanicPlugin {
    fn metadata(&self) -> PluginMetadata {
        panic!("metadata panicked");
    }

    fn supports_bm25(&self) -> bool {
        panic!("supports_bm25 panicked");
    }

    fn supports_embedding(&self) -> bool {
        panic!("supports_embedding panicked");
    }

    fn generate_bm25(
        &self,
        _ctx: *mut std::ffi::c_void,
        _group_json: &str,
    ) -> Result<Option<String>, PluginError> {
        panic!("generate_bm25 panicked");
    }

    fn generate_bm25_batch(
        &self,
        _ctx: *mut std::ffi::c_void,
        _groups_json: &str,
    ) -> Result<Vec<Option<String>>, PluginError> {
        panic!("generate_bm25_batch panicked");
    }
}

declare_plugin!(PanicPlugin);

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
fn test_panic_in_metadata_is_contained() {
    // The panic must not unwind through the extern "C" boundary.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cce_plugin_metadata()));
    let ptr = match result {
        Ok(p) => p,
        Err(_) => panic!("panic crossed the extern \"C\" boundary in metadata"),
    };
    let json = read(ptr);
    let meta: PluginMetadata = serde_json::from_str(&json).unwrap();
    // Fallback metadata is returned when serialization panics.
    assert_eq!(meta.id, "unknown");
}

#[test]
fn test_panic_in_supports_is_contained() {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cce_plugin_has_bm25_generation()
    }));
    let has = result.unwrap_or(true);
    assert!(!has, "panic in supports_bm25 must be contained to false");
}

#[test]
fn test_panic_in_generate_is_contained() {
    let group_json = std::ffi::CString::new(r#"{"group_id":"g1"}"#).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        cce_plugin_generate_bm25(std::ptr::null_mut(), group_json.as_ptr())
    }));
    let ptr = match result {
        Ok(p) => p,
        Err(_) => panic!("panic crossed the extern \"C\" boundary in generate_bm25"),
    };
    let s = read(ptr);
    let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(parsed["result"], "error");
    assert_eq!(parsed["error_type"], "execution_failed");
    assert!(parsed["message"].as_str().unwrap().contains("panicked"));
}

#[test]
fn test_panic_in_generate_batch_is_contained() {
    let groups_json = std::ffi::CString::new(r#"[{"group_id":"g1"},{"group_id":"g2"}]"#).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        cce_plugin_generate_bm25_batch(std::ptr::null_mut(), groups_json.as_ptr())
    }));
    let ptr = match result {
        Ok(p) => p,
        Err(_) => panic!("panic crossed the extern \"C\" boundary in generate_bm25_batch"),
    };
    let s = read(ptr);
    let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(parsed["result"], "error");
    assert_eq!(parsed["error_type"], "execution_failed");
    assert!(parsed["message"].as_str().unwrap().contains("panicked"));
}
