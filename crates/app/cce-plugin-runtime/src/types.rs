use serde::{Deserialize, Serialize};

pub use cce_plugin::PluginMetadata;

/// Type of plugin
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum PluginType {
    /// Lua script plugin (.lua)
    #[serde(rename = "lua")]
    #[default]
    Lua,
    /// Native dynamic library plugin (.so / .dll / .dylib)
    #[serde(rename = "native")]
    Native,
}

/// Entry for a plugin in the registry file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEntry {
    /// Unique identifier for the plugin
    pub id: String,
    /// Path to the plugin file (relative to the registry file's directory)
    pub path: String,
    /// Whether this plugin is currently enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Optional description
    pub description: Option<String>,

    /// Plugin type (lua or native)
    #[serde(default)]
    pub plugin_type: PluginType,

    // === File Filtering Fields ===
    /// Glob patterns to match files this plugin should apply to
    /// Examples: ["*.py", "src/views/*.py", "**/routes/*.py"]
    /// If empty or None, plugin applies to all files (backward compatible)
    #[serde(default)]
    pub file_patterns: Option<Vec<String>>,

    /// Optional: languages this plugin supports
    /// Language names should match Language enum variants (case-insensitive)
    /// Examples: ["python", "javascript", "typescript"]
    /// If empty or None, plugin applies to all languages
    #[serde(default)]
    pub languages: Option<Vec<String>>,

    /// Optional: declared capability facets (see `PluginCapability`).
    /// Overrides the plugin's own declaration; when absent the host probes
    /// the plugin's `supports_*` at runtime.
    /// Examples: ["text_gen", "entity_extract", "format_parse", "group",
    /// "chunk", "rerank", "ast_language"]
    #[serde(default)]
    pub capabilities: Option<Vec<String>>,

    /// Optional: host-side priority override (higher values are executed
    /// first). Overrides the plugin's own declared priority; when absent the
    /// plugin's metadata priority is used. Ties are resolved by the plugin
    /// declaration order in this file. Negative values place the plugin
    /// below the built-in implementation (fallback tier).
    #[serde(default)]
    pub priority: Option<i32>,

    /// Optional: per-capability priority overrides (capability name →
    /// priority). Overrides the plugin's own declared
    /// `capability_priorities`; capabilities not listed fall back to
    /// `priority` (or the plugin metadata). Valid capability names are the
    /// `PluginCapability` string forms, e.g. "text_gen", "fusion".
    /// Example: {"text_gen": 1000, "fusion": 10}
    #[serde(default)]
    pub capability_priorities: Option<std::collections::HashMap<String, i32>>,
}

fn default_enabled() -> bool {
    true
}

/// The structure of plugins.json
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginRegistryFile {
    pub plugins: Vec<PluginEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_metadata_default() {
        let meta = PluginMetadata::default();
        assert_eq!(meta.id, "unknown");
        assert_eq!(meta.name, "Unknown Plugin");
        assert_eq!(meta.version, "0.1.0");
        assert_eq!(meta.priority, 0);
        assert!(meta.description.is_none());
    }

    #[test]
    fn test_plugin_entry_default_enabled() {
        let entry = PluginEntry {
            id: "test".to_string(),
            path: "test.lua".to_string(),
            enabled: true,
            description: None,
            plugin_type: PluginType::Lua,
            file_patterns: None,
            languages: None,
            capabilities: None,
            priority: None,
            capability_priorities: None,
        };
        assert!(entry.enabled);
    }

    #[test]
    fn test_plugin_entry_deserialized_enabled() {
        let json = r#"{"id": "p1", "path": "p1.lua"}"#;
        let entry: PluginEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.id, "p1");
        assert!(entry.enabled);
        assert_eq!(entry.file_patterns, None);
        assert_eq!(entry.languages, None);
    }

    #[test]
    fn test_plugin_entry_deserialized_disabled() {
        let json = r#"{"id": "p1", "path": "p1.lua", "enabled": false}"#;
        let entry: PluginEntry = serde_json::from_str(json).unwrap();
        assert!(!entry.enabled);
    }

    #[test]
    fn test_plugin_entry_with_filters() {
        let json = r#"{
            "id": "p1",
            "path": "p1.lua",
            "file_patterns": ["*.py", "*.rs"],
            "languages": ["python", "rust"]
        }"#;
        let entry: PluginEntry = serde_json::from_str(json).unwrap();
        assert_eq!(
            entry.file_patterns,
            Some(vec!["*.py".into(), "*.rs".into()])
        );
        assert_eq!(entry.languages, Some(vec!["python".into(), "rust".into()]));
    }

    #[test]
    fn test_plugin_entry_priority_override() {
        let json = r#"{"id": "p1", "path": "p1.lua", "priority": 500}"#;
        let entry: PluginEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.priority, Some(500));
        let absent: PluginEntry =
            serde_json::from_str(r#"{"id": "p2", "path": "p2.lua"}"#).unwrap();
        assert_eq!(absent.priority, None);
    }

    #[test]
    fn test_plugin_registry_file_default() {
        let registry = PluginRegistryFile::default();
        assert!(registry.plugins.is_empty());
    }

    #[test]
    fn test_plugin_registry_file_deserialized() {
        let json = r#"{"plugins": [
            {"id": "a", "path": "a.lua"},
            {"id": "b", "path": "b.lua", "enabled": false}
        ]}"#;
        let registry: PluginRegistryFile = serde_json::from_str(json).unwrap();
        assert_eq!(registry.plugins.len(), 2);
        assert_eq!(registry.plugins[0].id, "a");
        assert!(registry.plugins[0].enabled);
        assert_eq!(registry.plugins[1].id, "b");
        assert!(!registry.plugins[1].enabled);
    }
}
