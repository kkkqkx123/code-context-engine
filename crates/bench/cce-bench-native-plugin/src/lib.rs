//! Minimal native plugin used only by the plugin overhead benchmark.

use cce_plugin_sdk::{FfiPlugin, PluginError, PluginMetadata, declare_plugin};

#[derive(Default)]
struct BenchPlugin;

impl FfiPlugin for BenchPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "bench/native-plugin".to_string(),
            name: "Benchmark Native Plugin".to_string(),
            version: "1.0.0".to_string(),
            priority: 1000,
            description: Some("Native load overhead fixture".to_string()),
            ..Default::default()
        }
    }

    fn supports_bm25(&self) -> bool {
        true
    }

    fn generate_bm25(
        &self,
        _ctx: *mut std::ffi::c_void,
        group_json: &str,
    ) -> Result<Option<String>, PluginError> {
        Ok(Some(format!("native bm25:{}", group_json.len())))
    }
}

declare_plugin!(BenchPlugin);
