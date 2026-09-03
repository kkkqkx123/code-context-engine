use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use tracing::warn;

use cce_plugin::{PluginBundle, PluginError, PluginSource};

use crate::loader::LuaPlugin;
use crate::native::NativePlugin;
use crate::types::{PluginEntry, PluginRegistryFile, PluginType};
use cce_metrics::PluginMetrics;

/// A plugin source that loads from a `plugins.json` registry file.
///
/// This is the default source — it reads the project's `.cce/plugins.json`,
/// parses entries, and loads each plugin (Lua script or native library).
pub struct FilePluginSource {
    registry_path: std::path::PathBuf,
    metrics: Option<Arc<PluginMetrics>>,
}

impl FilePluginSource {
    /// Create a source pointing at the given registry file path.
    pub fn new(registry_path: std::path::PathBuf) -> Self {
        Self {
            registry_path,
            metrics: None,
        }
    }

    /// Return the registry file path this source reads from.
    pub fn registry_path(&self) -> &std::path::Path {
        &self.registry_path
    }

    /// Attach a metrics sink; load/execution events are recorded into it.
    pub fn with_metrics(mut self, metrics: Arc<PluginMetrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Create a source from project root and configuration.
    ///
    /// `registry_file` is resolved relative to the **project root**.
    /// The default location is `<project_root>/.cce/plugins.json`.
    pub fn from_project(project_root: &Path, registry_file: Option<&str>) -> Self {
        let file = registry_file.unwrap_or(".cce/plugins.json");
        let path = project_root.join(file);
        Self::new(path)
    }

    fn load_one(&self, entry: &PluginEntry) -> Result<PluginBundle, PluginError> {
        let plugin_path = self
            .registry_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&entry.path);

        if !plugin_path.exists() {
            return Err(PluginError::ResourceError(format!(
                "Plugin path does not exist: {}",
                plugin_path.display()
            )));
        }

        // Digest of the load artifact (script source or library bytes) so
        // downstream caches can detect plugin content changes that do not
        // bump the declared version.
        let content_digest = {
            let bytes = std::fs::read(&plugin_path).map_err(|e| {
                PluginError::ResourceError(format!(
                    "Failed to read plugin file {}: {e}",
                    plugin_path.display()
                ))
            })?;
            cce_utils::hash::calculate_hash(&bytes)
        };

        let plugin: Arc<dyn cce_plugin::CodePlugin> = match entry.plugin_type {
            PluginType::Lua => {
                let script = std::fs::read_to_string(&plugin_path).map_err(|e| {
                    PluginError::ExecutionFailed(format!(
                        "Failed to read Lua script {}: {e}",
                        plugin_path.display()
                    ))
                })?;
                let lua = LuaPlugin::from_script(&script)?;
                match &self.metrics {
                    Some(m) => Arc::new(lua.with_metrics(m.clone())),
                    None => Arc::new(lua),
                }
            }
            PluginType::Native => {
                let native = NativePlugin::load(&plugin_path)?;
                match &self.metrics {
                    Some(m) => Arc::new(native.with_metrics(m.clone())),
                    None => Arc::new(native),
                }
            }
        };

        let mut bundle = PluginBundle::new(plugin)
            .with_content_digest(content_digest)
            .with_file_patterns(entry.file_patterns.clone().unwrap_or_default())
            .with_languages(entry.languages.clone().unwrap_or_default());
        if let Some(capabilities) = &entry.capabilities {
            if !capabilities.is_empty() {
                bundle = bundle.with_capabilities(capabilities.clone());
            }
        }
        if let Some(priority) = entry.priority {
            bundle = bundle.with_priority(priority);
        }
        if let Some(priorities) = &entry.capability_priorities {
            if !priorities.is_empty() {
                bundle = bundle.with_capability_priorities(priorities.clone());
            }
        }

        if let Some(m) = &self.metrics {
            m.record_load();
        }
        Ok(bundle)
    }
}

impl PluginSource for FilePluginSource {
    fn collect(&self) -> Result<Vec<PluginBundle>, PluginError> {
        if !self.registry_path.exists() {
            warn!(
                "Plugin registry file not found at {}",
                self.registry_path.display()
            );
            return Ok(Vec::new());
        }

        let content = std::fs::read_to_string(&self.registry_path).map_err(|e| {
            PluginError::ExecutionFailed(format!("Failed to read plugin registry: {e}"))
        })?;

        let registry: PluginRegistryFile = serde_json::from_str(&content).map_err(|e| {
            PluginError::ExecutionFailed(format!("Failed to parse plugin registry JSON: {e}"))
        })?;

        let mut bundles = Vec::with_capacity(registry.plugins.len());
        // Duplicate plugin metadata ids across entries are skipped to avoid
        // silently replacing an already loaded plugin in the registry.
        let mut seen_ids: HashSet<String> = HashSet::new();
        for entry in &registry.plugins {
            if !entry.enabled {
                continue;
            }
            match self.load_one(entry) {
                Ok(bundle) => {
                    let id = bundle.plugin.metadata().id.clone();
                    if !seen_ids.insert(id.clone()) {
                        warn!("Skipping duplicate plugin id '{}' (already loaded)", id);
                        if let Some(m) = &self.metrics {
                            m.record_load_failure();
                        }
                        continue;
                    }
                    bundles.push(bundle);
                }
                Err(e) => {
                    warn!("Failed to load plugin {}: {}", entry.id, e);
                    if let Some(m) = &self.metrics {
                        m.record_load_failure();
                    }
                }
            }
        }

        Ok(bundles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_project_default_path() {
        let source = FilePluginSource::from_project(Path::new("/project"), None);
        assert_eq!(
            source.registry_path(),
            Path::new("/project/.cce/plugins.json")
        );
    }

    #[test]
    fn test_from_project_custom_path_relative_to_root() {
        let source =
            FilePluginSource::from_project(Path::new("/project"), Some("plugins/registry.json"));
        assert_eq!(
            source.registry_path(),
            Path::new("/project/plugins/registry.json")
        );
    }

    #[test]
    fn test_from_project_explicit_cce_path() {
        let source =
            FilePluginSource::from_project(Path::new("/project"), Some(".cce/plugins.json"));
        assert_eq!(
            source.registry_path(),
            Path::new("/project/.cce/plugins.json")
        );
    }
}
