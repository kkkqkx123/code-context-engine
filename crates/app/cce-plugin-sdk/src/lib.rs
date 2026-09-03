//! CCE Native Plugin SDK
//!
//! This crate provides everything needed to build native plugins for the
//! [Code Context Engine (CCE)](https://github.com/atomgit/code-context-engine).
//!
//! # Quick Start
//!
//! ```ignore
//! use cce_plugin_sdk::{declare_plugin, FfiPlugin, PluginMetadata};
//!
//! struct MyPlugin;
//!
//! impl FfiPlugin for MyPlugin {
//!     fn metadata(&self) -> PluginMetadata {
//!         PluginMetadata {
//!             id: "my-plugin".into(),
//!             name: "My Plugin".into(),
//!             version: "0.1.0".into(),
//!             priority: 10,
//!             description: Some("Custom BM25 text generator".into()),
//!         }
//!     }
//! }
//!
//! declare_plugin!(MyPlugin);
//! ```
//!
//! Then in `Cargo.toml`:
//! ```toml
//! [package]
//! name = "my-cce-plugin"
//! version = "0.1.0"
//! edition = "2021"
//!
//! [lib]
//! crate-type = ["cdylib"]
//!
//! [dependencies]
//! cce-plugin-sdk = "0.1"
//! ```
//!
//! Build with `cargo build --release` — the resulting `.so` / `.dylib` / `.dll`
//! can be loaded by CCE via the `plugins.json` registry.
//!
//! # ABI contract
//!
//! The authoritative definition of the exported C ABI lives in
//! `plugin-sdk/include/cce_plugin.h`. The `declare_plugin!` macro generates
//! exactly those symbols; the host loader
//! (`cce_infrastructure::plugin::native`) consumes them. Keep the three in
//! sync when changing the ABI.

// ═══════════════════════════════════════════════════════════════════════
// Re-exports from cce_core (single source of truth)
// ═══════════════════════════════════════════════════════════════════════

pub use cce_plugin::{PluginCapability, PluginError, PluginMetadata};

// Contract types the plugin-extension capabilities exchange across the FFI
// (JSON strings). Re-exported so plugins can deserialize them without
// declaring a direct dependency on cce-core beyond the SDK.
pub use cce_types::grouper::EntityGroup;
pub use cce_types::plugin::{
    FileFilterDecision, FusionWeights, GroupPluginContext, PluginDocument, PluginEntity,
    PluginExport, PluginImport, PluginRelation, PluginSymbol, QueryRewriteResult,
    ResultFilterEntry,
};
pub use cce_types::{ChunkedResult, GroupConversions, RerankCandidate, RerankResult};

// Re-exports so plugins can parse `group_json` / `groups_json` without
// declaring a direct dependency, and so the `declare_plugin!` macro can
// reference serde_json via `$crate::` (macro hygiene).
pub use serde_json;

pub mod abi;
#[doc(hidden)]
pub mod ffi;
pub mod types;

pub use abi::CCE_ABI_VERSION;
pub use types::FfiPlugin;

#[cfg(test)]
mod tests {
    use super::*;

    /// A test plugin used in unit tests.
    struct TestPlugin;

    impl Default for TestPlugin {
        fn default() -> Self {
            TestPlugin
        }
    }

    impl FfiPlugin for TestPlugin {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata {
                id: "test/test-plugin".to_string(),
                name: "Test Plugin".to_string(),
                version: "0.1.0".to_string(),
                priority: 5,
                capability_priorities: std::collections::HashMap::new(),
                description: Some("A test plugin for unit tests".to_string()),
                capabilities: Vec::new(),
            }
        }
    }

    declare_plugin!(TestPlugin);

    /// Helper: call an exported function returning a C string and read it.
    unsafe fn c_string_to_string(ptr: *mut std::ffi::c_char) -> String {
        assert!(!ptr.is_null());
        let s = unsafe { std::ffi::CStr::from_ptr(ptr) }
            .to_str()
            .unwrap()
            .to_string();
        unsafe { cce_plugin_free_string(ptr) };
        s
    }

    #[test]
    fn test_abi_version() {
        assert_eq!(cce_plugin_abi_version(), CCE_ABI_VERSION);
        assert_eq!(CCE_ABI_VERSION, 1);
    }

    #[test]
    fn test_has_bm25_generation_default() {
        assert!(!cce_plugin_has_bm25_generation());
    }

    #[test]
    fn test_has_embedding_generation_default() {
        assert!(!cce_plugin_has_embedding_generation());
    }

    #[test]
    fn test_lifecycle_default() {
        assert!(!cce_plugin_has_lifecycle());
        assert!(cce_plugin_create().is_null());
        // Must not crash
        unsafe { cce_plugin_destroy(std::ptr::null_mut()) };
    }

    #[test]
    fn test_metadata_returns_valid_json() {
        let ptr = cce_plugin_metadata();
        let json_str = unsafe { c_string_to_string(ptr) };
        let meta: PluginMetadata = serde_json::from_str(&json_str).unwrap();
        assert_eq!(meta.id, "test/test-plugin");
        assert_eq!(meta.name, "Test Plugin");
        assert_eq!(meta.version, "0.1.0");
        assert_eq!(meta.priority, 5);
        assert_eq!(
            meta.description.as_deref(),
            Some("A test plugin for unit tests")
        );
    }

    #[test]
    fn test_free_string_null() {
        unsafe { cce_plugin_free_string(std::ptr::null_mut()) };
    }

    #[test]
    fn test_bm25_default_returns_none() {
        let group_json = std::ffi::CString::new(r#"{"group_id":"g1"}"#).unwrap();
        let ptr = unsafe { cce_plugin_generate_bm25(std::ptr::null_mut(), group_json.as_ptr()) };
        let s = unsafe { c_string_to_string(ptr) };
        let result: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(result["result"], "none");
    }

    #[test]
    fn test_embedding_default_returns_none() {
        let group_json = std::ffi::CString::new(r#"{"group_id":"g1"}"#).unwrap();
        let ptr =
            unsafe { cce_plugin_generate_embedding(std::ptr::null_mut(), group_json.as_ptr()) };
        let s = unsafe { c_string_to_string(ptr) };
        let result: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(result["result"], "none");
    }

    #[test]
    fn test_bm25_batch_default_returns_all_none() {
        let groups_json =
            std::ffi::CString::new(r#"[{"group_id":"g1"},{"group_id":"g2"}]"#).unwrap();
        let ptr =
            unsafe { cce_plugin_generate_bm25_batch(std::ptr::null_mut(), groups_json.as_ptr()) };
        let s = unsafe { c_string_to_string(ptr) };
        let result: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(result["result"], "ok");
        let value = result["value"].as_array().unwrap();
        assert_eq!(value.len(), 2);
        assert!(value.iter().all(|v| v.is_null()));
    }

    #[test]
    fn test_embedding_batch_default_returns_all_none() {
        let groups_json =
            std::ffi::CString::new(r#"[{"group_id":"g1"},{"group_id":"g2"}]"#).unwrap();
        let ptr = unsafe {
            cce_plugin_generate_embedding_batch(std::ptr::null_mut(), groups_json.as_ptr())
        };
        let s = unsafe { c_string_to_string(ptr) };
        let result: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(result["result"], "ok");
        let value = result["value"].as_array().unwrap();
        assert_eq!(value.len(), 2);
        assert!(value.iter().all(|v| v.is_null()));
    }

    /// The header is the authoritative ABI definition; the generated symbols
    /// must stay in sync with it. This test greps the header for every symbol
    /// the macro generates.
    #[test]
    fn test_header_symbol_consistency() {
        const HEADER: &str = include_str!("../include/cce_plugin.h");
        let expected_symbols = [
            "CCE_PLUGIN_ABI_VERSION",
            "cce_plugin_abi_version",
            "cce_plugin_metadata",
            "cce_plugin_has_bm25_generation",
            "cce_plugin_has_embedding_generation",
            "cce_plugin_has_lifecycle",
            "cce_plugin_create",
            "cce_plugin_destroy",
            "cce_plugin_free_string",
            "cce_plugin_generate_bm25",
            "cce_plugin_generate_embedding",
            "cce_plugin_generate_bm25_batch",
            "cce_plugin_generate_embedding_batch",
            "cce_plugin_has_parse",
            "cce_plugin_has_extract",
            "cce_plugin_has_group",
            "cce_plugin_has_chunk",
            "cce_plugin_has_rerank",
            "cce_plugin_has_ast_language",
            "cce_plugin_parse_document",
            "cce_plugin_extract_entities",
            "cce_plugin_post_group",
            "cce_plugin_chunk",
            "cce_plugin_rerank",
            "cce_plugin_query_scheme",
            "cce_plugin_tree_sitter_language",
            "cce_plugin_language_name",
            "cce_plugin_language_extensions",
            "cce_plugin_has_language_remap",
            "cce_plugin_remap_grammar_language",
            "cce_plugin_has_stdlib_heuristic",
            "cce_plugin_has_test_file_heuristic",
            "cce_plugin_has_entity_kind_heuristic",
            "cce_plugin_classify_stdlib",
            "cce_plugin_is_test_file",
            "cce_plugin_entity_kind",
            "cce_plugin_has_group_override",
            "cce_plugin_has_relation_extract",
            "cce_plugin_has_query_rewrite",
            "cce_plugin_has_fusion",
            "cce_plugin_has_result_filter",
            "cce_plugin_has_file_filter",
            "cce_plugin_group",
            "cce_plugin_extract_symbols",
            "cce_plugin_extract_relations",
            "cce_plugin_rewrite_query",
            "cce_plugin_fusion_weights",
            "cce_plugin_filter_results",
            "cce_plugin_filter_file",
        ];
        for symbol in expected_symbols {
            assert!(
                HEADER.contains(symbol),
                "header must declare symbol {symbol}"
            );
        }
        // The header must document the JSON result protocol.
        assert!(HEADER.contains(r#""result":"ok""#));
        assert!(HEADER.contains(r#""result":"none""#));
        assert!(HEADER.contains(r#""result":"error""#));
        assert!(HEADER.contains("error_type"));
    }

    #[test]
    fn test_ffi_read_c_str() {
        let c_str = std::ffi::CString::new("hello world").unwrap();
        let result = unsafe { ffi::read_c_str(c_str.as_ptr()) };
        assert_eq!(result.unwrap(), "hello world");
    }

    #[test]
    fn test_ffi_read_c_str_null() {
        let result = unsafe { ffi::read_c_str(std::ptr::null()) };
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Null pointer"));
    }

    #[test]
    fn test_error_json_includes_error_type() {
        let json = ffi::error_json(&PluginError::Timeout);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["result"], "error");
        assert_eq!(parsed["error_type"], "timeout");

        let json = ffi::error_json(&PluginError::ScriptError("boom".to_string()));
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["error_type"], "script");
    }

    #[test]
    fn test_result_to_c_string_ok() {
        let r: Result<Option<String>, PluginError> = Ok(Some("hello".to_string()));
        let ptr = ffi::result_to_c_string::<String>(&r);
        let s = unsafe { c_string_to_string(ptr) };
        let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed["result"], "ok");
        assert_eq!(parsed["value"], "hello");
    }

    #[test]
    fn test_vec_result_to_c_string_ok() {
        let r: Result<Vec<Option<String>>, PluginError> =
            Ok(vec![Some("a".to_string()), None, Some("c".to_string())]);
        let ptr = ffi::vec_result_to_c_string(&r);
        let s = unsafe { c_string_to_string(ptr) };
        let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed["result"], "ok");
        assert_eq!(parsed["value"][0], "a");
        assert!(parsed["value"][1].is_null());
        assert_eq!(parsed["value"][2], "c");
    }

    #[test]
    fn test_error_to_c_string_contains_type() {
        let ptr = ffi::error_to_c_string(&PluginError::Timeout);
        let s = unsafe { c_string_to_string(ptr) };
        let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed["result"], "error");
        assert_eq!(parsed["error_type"], "timeout");
    }
}
