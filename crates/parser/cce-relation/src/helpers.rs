//! Helper functions for relation extraction and processing
//!
//! This module contains utility functions for extracting file-level
//! relations (imports, exports, dependencies) from parsed AST data.

use crate::symbol::Visibility;
use crate::{ExportInfo, ExportType};
use cce_parser::parser::extractor::create_extractor_with_registry;
use cce_parser_core::ExtractionContext as ImportExtractionContext;
use cce_plugin::PluginRegistry;
use cce_types::import::{ReexportRecord, StandardizedImportTable};
use cce_types::language::Language;
use cce_types::{Entity, ImportTable, ParseError};
use tree_sitter::Tree;

/// Extract imports from a parsed file
///
/// This function uses the symbol extractor to get standardized imports
/// and converts them to an ImportTable.
///
/// # Arguments
///
/// * `tree` - Parsed AST tree
/// * `source` - Source code text
/// * `language` - Programming language
/// * `context` - Optional extraction context for relative import resolution
///
/// # Returns
///
/// * `ImportTable` - Table of imports for the file
pub fn extract_imports(
    tree: &Tree,
    source: &str,
    language: &Language,
    context: Option<ImportExtractionContext>,
) -> Result<ImportTable, ParseError> {
    extract_imports_with_registry(tree, source, language, context, None, "")
}

/// Extract imports with an optional plugin registry (used for custom
/// languages backed by a `SymbolExtract` plugin).
///
/// `file_path` is used to filter registry plugins by glob pattern; when an
/// extraction `context` is provided its `file_path` takes precedence.
pub fn extract_imports_with_registry(
    tree: &Tree,
    source: &str,
    language: &Language,
    context: Option<ImportExtractionContext>,
    registry: Option<&PluginRegistry>,
    file_path: &str,
) -> Result<ImportTable, ParseError> {
    // Extract standardized imports
    let file_path = context
        .as_ref()
        .map(|c| c.file_path.to_string_lossy().to_string())
        .unwrap_or_else(|| file_path.to_string());
    let language_str = language.to_string();
    let Some(extractor) =
        create_extractor_with_registry(*language, registry, &file_path, &language_str)
    else {
        if matches!(language, Language::Custom(_)) {
            return Err(ParseError::ast_parsing(format!(
                "No extractor available for language: {}",
                language
            )));
        }
        return Ok(ImportTable::from_standardized(
            &StandardizedImportTable::new(""),
        ));
    };

    let standardized_imports = if let Some(ctx) = context {
        // Use context-aware extraction for relative import resolution
        extractor.extract_imports_with_context(tree, source, &ctx)
    } else {
        extractor.extract_imports(tree, source)
    };

    // Build import table
    let mut std_table = StandardizedImportTable::new("");
    for import in standardized_imports {
        std_table.add_import(import);
    }

    Ok(ImportTable::from_standardized(&std_table))
}

/// Extract named re-exports from a parsed file's AST.
///
/// Re-export detection reuses the symbol extractor's `extract_exports`
/// output (`is_reexport` mark). Only named re-exports become records;
/// wildcard re-exports (`export * from`, `pub use module::*`) carry no
/// concrete local name and are intentionally not carried as records.
///
/// Source-module conversion rules:
/// - Rust (`pub use module::Item as Alias`): `source_module` is the full
///   `module::Item` path, so the last `::` segment is the original name.
/// - JS/TS (`export { x as y } from './mod'`): `source_module` is the
///   module specifier and `original_name` is the symbol name.
///
/// Returns an empty vector when the language has no extractor or no
/// named re-exports.
pub fn extract_reexports(tree: &Tree, source: &str, language: &Language) -> Vec<ReexportRecord> {
    let language_str = language.to_string();
    let Some(extractor) = create_extractor_with_registry(*language, None, "", &language_str) else {
        return Vec::new();
    };

    extractor
        .extract_exports(tree, source)
        .into_iter()
        .filter(|export| export.is_reexport && export.target.name != "*")
        .filter_map(|export| {
            let source_module = export.target.source_module.as_deref()?;
            let (original_module, original_name) = match export.target.original_name.as_deref() {
                // Rust alias form: original_name carries the full
                // `module::Item` path.
                Some(original) if original.contains("::") => {
                    let (module, name) = original.rsplit_once("::")?;
                    (module.to_string(), name.to_string())
                }
                // JS/TS alias form: original_name is the symbol name.
                Some(original) => (source_module.to_string(), original.to_string()),
                // No alias: Rust sources the full `module::Item` path;
                // other languages treat source_module as the module.
                None => match source_module.rsplit_once("::") {
                    Some((module, name)) if name == export.target.name => {
                        (module.to_string(), name.to_string())
                    }
                    _ => (source_module.to_string(), export.target.name.clone()),
                },
            };
            if export.target.name.is_empty()
                || original_module.is_empty()
                || original_name.is_empty()
            {
                return None;
            }
            Some(ReexportRecord::new(
                export.target.name.clone(),
                original_module,
                original_name,
            ))
        })
        .collect()
}
///
/// Unlike [`extract_imports_with_registry`], this path does not need a parsed
/// tree (a custom language may have no registered tree-sitter grammar). The
/// plugins operate on raw source text; each matching plugin's `PluginImport`
/// results are merged into the returned table. Returns an error when `language`
/// is not custom or no registry is available, so callers can fall back to the
/// AST-based path for built-in languages.
pub fn extract_imports_from_plugin(
    source: &str,
    language: &Language,
    registry: Option<&PluginRegistry>,
    file_path: &str,
) -> Result<ImportTable, ParseError> {
    if !matches!(language, Language::Custom(_)) {
        return Err(ParseError::ast_parsing(
            "plugin import extraction requires a custom language".to_string(),
        ));
    }
    let registry = registry.ok_or_else(|| {
        ParseError::ast_parsing(
            "no plugin registry available for custom language import extraction".to_string(),
        )
    })?;
    let language_str = language.to_string();
    let plugins = registry.get_plugins(
        cce_plugin::PluginCapability::SymbolExtract,
        Some(file_path),
        Some(&language_str),
    );

    let mut std_table = StandardizedImportTable::new("");
    for plugin in plugins {
        match plugin.extract_imports(source, file_path, &language_str) {
            Ok(Some(imports)) => {
                for import in imports {
                    std_table.add_import(import.into());
                }
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(
                    plugin = %plugin.metadata().id,
                    file_path = file_path,
                    error = %e,
                    "extract_imports failed, continuing with remaining plugins"
                );
            }
        }
    }
    Ok(ImportTable::from_standardized(&std_table))
}

/// Detect entity visibility based on language-specific rules.
///
/// Delegates to the per-language policy in [`crate::policy`]. This remains the
/// single public determination function for cross-file addressability, shared
/// by [`extract_exports_from_entities`] and the symbol table builder.
pub fn detect_entity_visibility(entity: &Entity, language: &Language) -> Visibility {
    crate::policy::detect_entity_visibility(entity, language)
}

/// Extract exports from entities based on language-specific visibility rules
///
/// # Arguments
///
/// * `entities` - Extracted entities from the file
/// * `language` - Programming language
///
/// # Returns
///
/// Vector of `ExportInfo` containing exported symbols
pub fn extract_exports_from_entities(entities: &[Entity], language: &Language) -> Vec<ExportInfo> {
    // Explicit-export languages (JS/TS family): when the extractor recorded
    // real `export` statements (`is_exported` metadata), only those symbols
    // are exports. A file without any `export` statement exports nothing,
    // even though its top-level symbols remain addressable by visibility.
    let explicit = matches!(
        language,
        Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx
    ) && entities
        .iter()
        .any(|e| e.metadata.get("is_exported").is_some_and(|v| v == "true"));
    let mut exports = Vec::new();

    for entity in entities {
        // Skip non-top-level entities
        if entity.depth > 0 || entity.parent.is_some() {
            continue;
        }

        // Converged on the shared visibility determination: an entity is
        // exported when its visibility is addressable outside its defining
        // scope. First phase exports Public, Package, Module, Restricted,
        // Protected and Internal levels; Private/Super/PrivateProtected/Friend
        // remain non-exported.
        let is_export = if explicit {
            entity
                .metadata
                .get("is_exported")
                .is_some_and(|v| v == "true")
        } else {
            matches!(
                detect_entity_visibility(entity, language),
                Visibility::Public
                    | Visibility::Package
                    | Visibility::Module
                    | Visibility::Restricted { .. }
                    | Visibility::Protected
                    | Visibility::Internal
                    | Visibility::ProtectedInternal
            )
        };

        if is_export {
            let export_type = if entity
                .metadata
                .get("is_default")
                .is_some_and(|v| v == "true")
            {
                ExportType::Default
            } else {
                ExportType::Named
            };

            exports.push(ExportInfo {
                function_id: entity.id,
                function_name: entity.name.clone(),
                export_type,
            });
        }
    }

    exports
}

/// Extract dependencies from import table
///
/// # Arguments
///
/// * `imports` - Import table for the file
///
/// # Returns
///
/// Vector of dependency paths (unique)
pub fn extract_dependencies_from_imports(imports: &ImportTable) -> Vec<String> {
    let mut dependencies = Vec::new();
    for import in &imports.standardized_imports {
        if !import.source.is_empty() && !dependencies.contains(&import.source) {
            dependencies.push(import.source.clone());
        }
    }
    dependencies
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_plugin::{CodePlugin, PluginBundle, PluginError, PluginMetadata, PluginRegistry};
    use cce_types::PluginImport;
    use std::sync::{Arc, Once};

    static INIT: Once = Once::new();

    /// Ensure the tree-sitter language resolver is registered for tests.
    fn ensure_resolver() {
        INIT.call_once(|| {
            cce_parser_core::set_language_resolver(
                cce_parser::tree_sitter_init::get_tree_sitter_language,
            );
        });
    }

    /// Hand-written `CodePlugin` test double (no Lua runtime): extracts
    /// `@import("...")` paths from zig source lines.
    struct ZigImportPlugin {
        metadata: PluginMetadata,
    }

    impl CodePlugin for ZigImportPlugin {
        fn metadata(&self) -> &PluginMetadata {
            &self.metadata
        }

        fn supports_symbol_extract(&self) -> bool {
            true
        }

        fn extract_imports(
            &self,
            content: &str,
            _file_path: &str,
            _language: &str,
        ) -> Result<Option<Vec<PluginImport>>, PluginError> {
            let mut imports = Vec::new();
            for line in content.lines() {
                if let Some(start) = line.find("@import(\"") {
                    let rest = &line[start + "@import(\"".len()..];
                    if let Some(end) = rest.find('"') {
                        imports.push(PluginImport::new(rest[..end].to_string()));
                    }
                }
            }
            if imports.is_empty() {
                Ok(None)
            } else {
                Ok(Some(imports))
            }
        }
    }

    /// Test double that always declines (`Ok(None)`), used to verify the
    /// override-tier fall-through chain.
    struct DecliningImportPlugin {
        metadata: PluginMetadata,
    }

    impl CodePlugin for DecliningImportPlugin {
        fn metadata(&self) -> &PluginMetadata {
            &self.metadata
        }

        fn supports_symbol_extract(&self) -> bool {
            true
        }

        fn extract_imports(
            &self,
            _content: &str,
            _file_path: &str,
            _language: &str,
        ) -> Result<Option<Vec<PluginImport>>, PluginError> {
            Ok(None)
        }
    }

    fn importing_plugin(id: &str) -> Arc<ZigImportPlugin> {
        Arc::new(ZigImportPlugin {
            metadata: PluginMetadata {
                id: id.to_string(),
                name: id.to_string(),
                version: "test".to_string(),
                capabilities: vec!["symbol_extract".to_string()],
                ..Default::default()
            },
        })
    }

    fn declining_plugin(id: &str) -> Arc<DecliningImportPlugin> {
        Arc::new(DecliningImportPlugin {
            metadata: PluginMetadata {
                id: id.to_string(),
                name: id.to_string(),
                version: "test".to_string(),
                capabilities: vec!["symbol_extract".to_string()],
                ..Default::default()
            },
        })
    }

    fn zig_registry() -> PluginRegistry {
        let mut registry = PluginRegistry::new();
        // File-pattern routing is the primary filter; the language constraint
        // is omitted so the tests do not need a registered plugin-language
        // name (which is a process-global and races across parallel tests).
        registry.register_bundle(
            PluginBundle::new(importing_plugin("zig_symbol_extract"))
                .with_file_patterns(vec!["*.zig".to_string()]),
        );
        registry
    }

    fn dummy_tree() -> tree_sitter::Tree {
        ensure_resolver();
        let mut parser = cce_parser_core::AstParser::new();
        parser
            .parse_with_tree("fn main() {}", &Language::Rust)
            .expect("rust parses")
            .0
    }

    #[test]
    fn test_extract_imports_with_registry_uses_plugin() {
        let registry = zig_registry();
        let content = "const std = @import(\"std\");\nconst mem = @import(\"mem.zig\");";
        let imports = extract_imports_with_registry(
            &dummy_tree(),
            content,
            &Language::Custom(0),
            None,
            Some(&registry),
            "src/lib.zig",
        )
        .expect("plugin extraction succeeds");
        assert_eq!(imports.import_count(), 2);
        assert!(
            imports
                .standardized_imports
                .iter()
                .any(|i| i.source == "std")
        );
    }

    #[test]
    fn test_extract_imports_with_registry_no_plugin_is_error() {
        let registry = PluginRegistry::new();
        let result = extract_imports_with_registry(
            &dummy_tree(),
            "const std = @import(\"std\");",
            &Language::Custom(0),
            None,
            Some(&registry),
            "src/lib.zig",
        );
        assert!(
            result.is_err(),
            "custom language without a SymbolExtract plugin must report an error"
        );
    }

    #[test]
    fn test_extract_imports_chain_falls_through_to_lower_priority() {
        // The higher-priority plugin declines (`Ok(None)`); the lower-priority
        // one produces the imports. The chain must fall through instead of
        // returning empty (override-tier semantics).
        let mut registry = PluginRegistry::new();
        let mut higher_bundle = PluginBundle::new(declining_plugin("declining_extractor"))
            .with_file_patterns(vec!["*.zig".to_string()]);
        higher_bundle = higher_bundle.with_priority(100);
        registry.register_bundle(higher_bundle);
        registry.register_bundle(
            PluginBundle::new(importing_plugin("real_extractor"))
                .with_file_patterns(vec!["*.zig".to_string()])
                .with_priority(10),
        );

        let content = "const std = @import(\"std\");\nconst mem = @import(\"mem.zig\");";
        let imports = extract_imports_with_registry(
            &dummy_tree(),
            content,
            &Language::Custom(0),
            None,
            Some(&registry),
            "src/lib.zig",
        )
        .expect("lower-priority plugin extraction succeeds");
        assert_eq!(imports.import_count(), 2);
    }

    #[test]
    fn test_extract_imports_with_registry_ignores_registry_for_builtins() {
        ensure_resolver();
        let registry = PluginRegistry::new();
        let mut parser = cce_parser_core::AstParser::new();
        let (tree, _) = parser
            .parse_with_tree("use std::collections::HashMap;", &Language::Rust)
            .expect("rust parses");
        let imports = extract_imports_with_registry(
            &tree,
            "use std::collections::HashMap;",
            &Language::Rust,
            None,
            Some(&registry),
            "src/main.rs",
        )
        .expect("built-in extraction succeeds");
        assert_eq!(imports.import_count(), 1);
    }

    #[test]
    fn extract_reexports_ts_named_and_wildcard_forms() {
        ensure_resolver();
        let code = "export { foo as bar } from './mod';\nexport * from './wild';\n";
        let mut parser = cce_parser_core::AstParser::new();
        let (tree, _) = parser
            .parse_with_tree(code, &Language::TypeScript)
            .expect("typescript parses");

        let records = extract_reexports(&tree, code, &Language::TypeScript);
        // Wildcard re-exports carry no local name and must be skipped.
        assert_eq!(records.len(), 1, "only named re-exports become records");
        let record = &records[0];
        assert_eq!(record.local_name, "bar");
        assert_eq!(record.original_module, "./mod");
        assert_eq!(record.original_name, "foo");
        assert_eq!(record.chain_depth, 0);
    }

    #[test]
    fn extract_reexports_rust_alias_brace_and_wildcard_forms() {
        ensure_resolver();
        let code = "pub use crate::a::Item as Alias;\npub use crate::b::{X, Y};\npub use crate::c::Z;\npub use crate::d::*;\n";
        let mut parser = cce_parser_core::AstParser::new();
        let (tree, _) = parser
            .parse_with_tree(code, &Language::Rust)
            .expect("rust parses");

        let records = extract_reexports(&tree, code, &Language::Rust);
        // Wildcard re-export (`pub use crate::d::*`) is skipped.
        assert_eq!(records.len(), 4);
        let alias = records
            .iter()
            .find(|r| r.local_name == "Alias")
            .expect("alias re-export present");
        assert_eq!(alias.original_module, "crate::a");
        assert_eq!(alias.original_name, "Item");
        for local in ["X", "Y", "Z"] {
            let record = records
                .iter()
                .find(|r| r.local_name == local)
                .unwrap_or_else(|| panic!("{local} re-export present"));
            assert_eq!(record.original_name, local);
        }
        assert_eq!(
            records
                .iter()
                .find(|r| r.local_name == "Z")
                .expect("plain re-export present")
                .original_module,
            "crate::c"
        );
        assert!(
            records
                .iter()
                .find(|r| r.local_name == "X")
                .expect("brace re-export present")
                .original_module
                == "crate::b"
        );
    }
}
