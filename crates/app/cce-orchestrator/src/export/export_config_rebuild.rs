//! Configuration rebuild logic for the export processor.
//!
//! Handles runtime configuration updates, reloading exporters and converters
//! from the current global configuration.

use std::path::Path;
use std::sync::Arc;

use cce_config::{AstToNlConfig, Settings};
use cce_parser::ast_to_nl::AstToNlConverter;
use cce_parser::ast_to_nl::chunker::GroupChunker;
use cce_plugin::PluginRegistry;

use crate::export::nl_exporter::NlDocumentExporter;
use crate::hot_update::Result;

/// Rebuild the internal exporter and converters from the current global
/// configuration, applying changed `export.include_summary` / relation
/// flags and AST-to-NL pipeline settings.
///
/// The plugin registry is carried over so the rebuilt converter keeps the
/// plugins that take part in text generation.
pub async fn rebuild_from_config(
    exporter: &Arc<tokio::sync::RwLock<Arc<NlDocumentExporter>>>,
    ast_converter: &Arc<tokio::sync::RwLock<Arc<AstToNlConverter>>>,
    chunker: &Arc<tokio::sync::RwLock<Arc<GroupChunker>>>,
    ast_to_nl_config: &Arc<tokio::sync::RwLock<AstToNlConfig>>,
    plugin_registry: Option<&Arc<PluginRegistry>>,
    relation_epoch_cache: &tokio::sync::Mutex<Option<i64>>,
    config_path: &Path,
) -> Result<()> {
    let current = exporter.read().await.config().clone();

    let mut export_config = current;
    if let Ok(global) = Settings::global() {
        export_config.include_summary = global.export.include_summary;
        export_config.enable_relation_enhancement = global.export.enable_relation_enhancement;
    }

    let new_exporter = Arc::new(NlDocumentExporter::new(export_config));
    *exporter.write().await = new_exporter;

    if let Ok(config) = Settings::ast_to_nl() {
        let converter = AstToNlConverter::with_config(&config);
        let converter = match plugin_registry {
            Some(registry) => converter.with_plugin_registry(registry.clone()),
            None => converter,
        };
        *ast_converter.write().await = Arc::new(converter);
        let new_chunker = GroupChunker::new(config.chunking.clone());
        let new_chunker = match plugin_registry {
            Some(registry) => new_chunker.with_plugin_registry(registry.clone()),
            None => new_chunker,
        };
        *chunker.write().await = Arc::new(new_chunker);
        *ast_to_nl_config.write().await = config;
    }

    *relation_epoch_cache.lock().await = None;

    tracing::info!(
        config = %config_path.display(),
        "NL document export processor reloaded configuration"
    );
    Ok(())
}
