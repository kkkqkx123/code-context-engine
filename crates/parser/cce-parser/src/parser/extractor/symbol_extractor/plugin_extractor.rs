//! Plugin-backed symbol extraction for custom languages
//!
//! Wraps a [`cce_plugin::CodePlugin`] with the `SymbolExtract`
//! capability behind the [`SymbolExtractor`] trait, so custom languages
//! (`Language::Custom(_)`) obtain import/export extraction without a
//! built-in extractor.
//!
//! The plugin operates on raw source text (`content`), while the trait
//! receives a parsed tree + source. The tree is ignored; the source text
//! and the captured `file_path` / `language` strings are forwarded to the
//! plugin. Plugin results (`PluginImport` / `PluginExport`) are converted
//! into the standardized `StandardizedImport` / `StandardizedExport` forms.

use std::sync::Arc;

use cce_plugin::CodePlugin;
use cce_types::Language;

use super::common::StandardizedExport;
use super::common::StandardizedImport;
use super::traits::SymbolExtractor;
use tree_sitter::Tree;

/// Chain of [`PluginSymbolExtractor`]s tried in priority order.
///
/// Implements the override-tier semantics for `SymbolExtract`: the first
/// plugin returning a non-empty import/export list wins; declined or failed
/// plugins fall through to the next. Returns empty results only when every
/// plugin declined.
pub struct PluginSymbolExtractorChain {
    extractors: Vec<PluginSymbolExtractor>,
}

impl PluginSymbolExtractorChain {
    /// Build a chain from plugins (already priority-sorted by the registry).
    pub fn new(plugins: Vec<Arc<dyn CodePlugin>>, file_path: String, language: Language) -> Self {
        let language_str = language.to_string();
        Self {
            extractors: plugins
                .into_iter()
                .map(|plugin| {
                    PluginSymbolExtractor::new(
                        plugin,
                        file_path.clone(),
                        language,
                        language_str.clone(),
                    )
                })
                .collect(),
        }
    }

    fn try_imports(&self, source: &str) -> Vec<StandardizedImport> {
        for extractor in &self.extractors {
            let imports = extractor.imports_inner(source);
            if !imports.is_empty() {
                return imports;
            }
        }
        Vec::new()
    }

    fn try_exports(&self, source: &str) -> Vec<StandardizedExport> {
        for extractor in &self.extractors {
            let exports = extractor.exports_inner(source);
            if !exports.is_empty() {
                return exports;
            }
        }
        Vec::new()
    }
}

impl SymbolExtractor for PluginSymbolExtractorChain {
    fn extract_imports(&self, _tree: &Tree, source: &str) -> Vec<StandardizedImport> {
        self.try_imports(source)
    }

    fn extract_exports(&self, _tree: &Tree, source: &str) -> Vec<StandardizedExport> {
        self.try_exports(source)
    }

    fn language(&self) -> Language {
        self.extractors
            .first()
            .map(|e| e.language())
            .unwrap_or(Language::Unknown)
    }
}

/// [`SymbolExtractor`] wrapper for plugins with the `SymbolExtract` capability.
pub struct PluginSymbolExtractor {
    plugin: Arc<dyn CodePlugin>,
    file_path: String,
    language: Language,
    language_str: String,
}

impl PluginSymbolExtractor {
    /// Create a wrapper bound to a file path and detected language.
    pub fn new(
        plugin: Arc<dyn CodePlugin>,
        file_path: String,
        language: Language,
        language_str: String,
    ) -> Self {
        Self {
            plugin,
            file_path,
            language,
            language_str,
        }
    }

    /// Extract imports from a single plugin; `Ok(None)` / failure yield empty.
    fn imports_inner(&self, source: &str) -> Vec<StandardizedImport> {
        match self
            .plugin
            .extract_imports(source, &self.file_path, &self.language_str)
        {
            Ok(Some(imports)) => imports.into_iter().map(Into::into).collect(),
            Ok(None) => Vec::new(),
            Err(e) => {
                tracing::warn!(
                    plugin = %self.plugin.metadata().id,
                    error = %e,
                    "extract_imports failed, falling through to next plugin"
                );
                Vec::new()
            }
        }
    }

    /// Extract exports from a single plugin; `Ok(None)` / failure yield empty.
    fn exports_inner(&self, source: &str) -> Vec<StandardizedExport> {
        match self
            .plugin
            .extract_exports(source, &self.file_path, &self.language_str)
        {
            Ok(Some(exports)) => exports.into_iter().map(Into::into).collect(),
            Ok(None) => Vec::new(),
            Err(e) => {
                tracing::warn!(
                    plugin = %self.plugin.metadata().id,
                    error = %e,
                    "extract_exports failed, falling through to next plugin"
                );
                Vec::new()
            }
        }
    }
}

impl SymbolExtractor for PluginSymbolExtractor {
    fn extract_imports(&self, _tree: &Tree, source: &str) -> Vec<StandardizedImport> {
        self.imports_inner(source)
    }

    fn extract_exports(&self, _tree: &Tree, source: &str) -> Vec<StandardizedExport> {
        self.exports_inner(source)
    }

    fn language(&self) -> Language {
        self.language
    }
}
