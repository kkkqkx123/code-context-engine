//! Extraction traits
//!
//! Defines the core interfaces for symbol extraction across all languages.

use cce_plugin::{CodePlugin, PluginCapability, PluginRegistry};
use cce_types::language::Language;
use std::sync::Arc;
use tree_sitter::Tree;

use super::common::{
    ExtractionContext, ExtractionError, ExtractionResult, ImportClassification, ImportClassifier,
    StandardizedExport, StandardizedImport,
};

/// Core symbol extraction trait
///
/// Implement this trait for each language to extract import/export information.
pub trait SymbolExtractor: Send + Sync {
    /// Extract imports from a syntax tree
    ///
    /// # Arguments
    /// * `tree` - The parsed syntax tree
    /// * `source` - The source code text
    ///
    /// # Returns
    /// A vector of standardized import descriptors
    fn extract_imports(&self, tree: &Tree, source: &str) -> Vec<StandardizedImport>;

    /// Extract imports with context
    ///
    /// This method provides context information for resolving relative imports
    /// and classifying imports as internal/external.
    ///
    /// # Arguments
    /// * `tree` - The parsed syntax tree
    /// * `source` - The source code text
    /// * `context` - Extraction context with file and project information
    ///
    /// # Returns
    /// A vector of standardized import descriptors
    fn extract_imports_with_context(
        &self,
        tree: &Tree,
        source: &str,
        context: &ExtractionContext,
    ) -> Vec<StandardizedImport> {
        // Default implementation: extract imports and resolve relative imports
        let mut imports = self.extract_imports(tree, source);

        // Resolve relative imports if context is available
        for import in &mut imports {
            if import.is_relative {
                if let Some(resolved) =
                    context.resolve_relative_import(&import.source, self.language())
                {
                    import.source = resolved;
                    import.is_relative = false;
                }
            }
        }

        imports
    }

    /// Extract exports from a syntax tree
    ///
    /// # Arguments
    /// * `tree` - The parsed syntax tree
    /// * `source` - The source code text
    ///
    /// # Returns
    /// A vector of standardized export descriptors
    fn extract_exports(&self, tree: &Tree, source: &str) -> Vec<StandardizedExport>;

    /// Get the language this extractor supports
    fn language(&self) -> Language;

    /// Extract package declaration from source file
    ///
    /// This is used for languages like Java, Go, Kotlin, etc. that have explicit
    /// package declarations. The package declaration is used to determine internal
    /// imports (same package or subpackage) vs external imports.
    ///
    /// # Arguments
    /// * `tree` - The parsed syntax tree
    /// * `source` - The source code text
    ///
    /// # Returns
    /// The package declaration string, or None if not present or not applicable
    fn extract_package_declaration(&self, _tree: &Tree, _source: &str) -> Option<String> {
        None
    }

    /// Classify imports using the unified classifier
    ///
    /// # Arguments
    /// * `imports` - The imports to classify
    /// * `context` - Extraction context
    ///
    /// # Returns
    /// A vector of import classifications
    fn classify_imports(
        &self,
        imports: &[StandardizedImport],
        context: &ExtractionContext,
    ) -> Vec<ImportClassification> {
        ImportClassifier::classify_batch(imports, context, self.language())
    }
}

/// Create an extractor for a specific language
///
/// Returns `None` if the language is not supported.
pub fn create_extractor(language: Language) -> Option<Box<dyn SymbolExtractor>> {
    use super::*;

    match language {
        // C family
        Language::C => Some(Box::new(c::CExtractor::new())),
        Language::Cpp => Some(Box::new(cpp::CppExtractor::new())),
        Language::CSharp => Some(Box::new(csharp::CSharpExtractor::new())),

        // Web languages
        Language::JavaScript => Some(Box::new(javascript::JavaScriptExtractor::new())),
        Language::TypeScript => Some(Box::new(typescript::TypeScriptExtractor::new())),
        Language::Jsx => Some(Box::new(javascript::JavaScriptExtractor::new())),
        Language::Tsx => Some(Box::new(typescript::TypeScriptExtractor::new())),

        // Systems languages
        Language::Rust => Some(Box::new(rust::RustExtractor::new())),
        Language::Go => Some(Box::new(go::GoExtractor::new())),

        // Scripting languages
        Language::Python => Some(Box::new(python::PythonExtractor::new())),
        Language::Ruby => Some(Box::new(ruby::RubyExtractor::new())),
        Language::Php => Some(Box::new(php::PhpExtractor::new())),
        Language::Dart => Some(Box::new(dart::DartExtractor::new())),

        // Shell/Scripting languages
        Language::Bash => Some(Box::new(bash::BashExtractor::new())),
        Language::Lua => Some(Box::new(lua::LuaExtractor::new())),

        // JVM languages
        Language::Java => Some(Box::new(java::JavaExtractor::new())),
        Language::Kotlin => Some(Box::new(kotlin::KotlinExtractor::new())),
        Language::Scala => Some(Box::new(scala::ScalaExtractor::new())),

        // Frontend languages
        Language::Vue => None,    // Not implemented yet
        Language::Svelte => None, // Not implemented yet
        Language::Html => None,   // Not implemented yet
        Language::Css => None,    // Not implemented yet
        Language::Scss => None,   // Not implemented yet
        Language::Less => None,   // Not implemented yet

        // Unknown
        Language::Unknown => None,

        // Data formats - not applicable for symbol extraction
        Language::Json => None,
        Language::Yaml => None,
        Language::Toml => None,
        Language::Xml => None,
        // Custom languages use the generic entity/relationship fallback.
        Language::Custom(_) => None,
    }
}

/// Create an extractor for a language, consulting the plugin registry for
/// custom languages.
///
/// Built-in languages use [`create_extractor`]. For `Language::Custom(_)`,
/// the registry is queried for a `SymbolExtract`-capable plugin matching the
/// file path and detected language name; all matches are wrapped in a
/// [`super::plugin_extractor::PluginSymbolExtractorChain`] in priority order,
/// so a declined or failed plugin falls through to the next (override-tier
/// semantics). Returns `None` when no built-in extractor or matching plugin
/// exists.
pub fn create_extractor_with_registry(
    language: Language,
    registry: Option<&PluginRegistry>,
    file_path: &str,
    language_str: &str,
) -> Option<Box<dyn SymbolExtractor>> {
    if let Language::Custom(_) = language {
        let registry = registry?;
        let plugins: Vec<Arc<dyn CodePlugin>> = registry
            .get_plugins(
                PluginCapability::SymbolExtract,
                Some(file_path),
                Some(language_str),
            )
            .into_iter()
            .map(Arc::clone)
            .collect();
        if plugins.is_empty() {
            return None;
        }
        let extractor: Box<dyn SymbolExtractor> =
            Box::new(super::plugin_extractor::PluginSymbolExtractorChain::new(
                plugins,
                file_path.to_string(),
                language,
            ));
        return Some(extractor);
    }
    create_extractor(language)
}

/// Extract imports and exports from a file
///
/// Convenience function that uses the appropriate extractor for the file's language.
pub fn extract_from_file(
    tree: &Tree,
    source: &str,
    language: Language,
) -> ExtractionResult<(Vec<StandardizedImport>, Vec<StandardizedExport>)> {
    create_extractor(language)
        .map(|extractor| {
            let imports = extractor.extract_imports(tree, source);
            let exports = extractor.extract_exports(tree, source);
            (imports, exports)
        })
        .ok_or_else(|| ExtractionError::unsupported("symbol extraction", language.to_string()))
}

/// Extract package declaration from a file
///
/// Returns the package declaration for languages that support it (Java, Go, Kotlin, etc.).
/// Returns None for languages that don't have package declarations.
pub fn extract_package_from_file(tree: &Tree, source: &str, language: Language) -> Option<String> {
    create_extractor(language)
        .and_then(|extractor| extractor.extract_package_declaration(tree, source))
}

/// Create extraction context with package declaration
///
/// This function creates an ExtractionContext and populates the package_declaration
/// field by extracting it from the source file.
pub fn create_context_with_package(
    file_path: std::path::PathBuf,
    project_root: std::path::PathBuf,
    language: Language,
    tree: &Tree,
    source: &str,
) -> ExtractionContext {
    let mut context = ExtractionContext::from_file(file_path, project_root, language);
    if let Some(package) = extract_package_from_file(tree, source, language) {
        context.package_declaration = Some(package);
    }
    context
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::language::Language;

    #[test]
    fn test_create_extractor() {
        assert!(create_extractor(Language::Rust).is_some());
        assert!(create_extractor(Language::JavaScript).is_some());
        assert!(create_extractor(Language::Unknown).is_none());
    }
}
