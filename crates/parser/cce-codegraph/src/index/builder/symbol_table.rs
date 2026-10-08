//! Symbol table builder for project-level symbol resolution
//!
//! Builds project symbol tables from parsed files for cross-file reference resolution.
//! Creates the full four-level hierarchy:
//! - ProjectSymbolTable (project-level)
//!   - PackageSymbolTable (package-level)
//!     - ModuleSymbolTable (module/file-level)
//!       - LocalSymbolTable (entity-level)

use crate::symbol::{SymbolLocation, SymbolMetadata, Visibility};
use crate::symbol_table::{
    ImportBinding, ImportSourceType, LocalSymbolTable, ModuleSymbolTable, PackageSymbolTable,
    ProjectSymbolTable, ReexportBinding, strip_crate_prefix,
};
use crate::type_inference::{InferenceContext, TypeInferenceEngine};
use cce_metrics::domain::pipeline::RelationMetrics;
use cce_parser_core::determine_module_path;
use cce_types::language::Language;
use cce_types::{Entity, ParsedFile, normalize_project_path};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Symbol table builder for project-level symbol resolution
pub struct SymbolTableBuilder {
    project_root: PathBuf,
    metrics: Option<Arc<RelationMetrics>>,
}

impl SymbolTableBuilder {
    /// Create a new symbol table builder
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            metrics: None,
        }
    }

    /// Attach relation metrics so module-path collisions can be accounted.
    pub fn with_metrics(&mut self, metrics: Option<Arc<RelationMetrics>>) -> &mut Self {
        self.metrics = metrics;
        self
    }

    fn record_module_path_conflict(&self) {
        if let Some(metrics) = &self.metrics {
            metrics.module_path_conflicts.increment();
        }
    }

    /// Wire the file's standardized imports into the module table
    ///
    /// Import bindings are populated lazily: resolution resolves a binding's
    /// source module only when the bound name is actually looked up, which
    /// keeps incremental symbol-table construction cheap.
    ///
    /// When `project` is provided (incremental path), external dependency
    /// names are checked to classify imports as `ExternalDependency`.
    fn wire_imports(
        &self,
        file: &ParsedFile,
        module_table: &mut ModuleSymbolTable,
        project: Option<&ProjectSymbolTable>,
    ) {
        let Some(import_table) = &file.import_table else {
            return;
        };
        let mut std_table = cce_types::import::StandardizedImportTable::new(&file.path);
        for import in &import_table.standardized_imports {
            std_table.add_import(import.clone());
        }
        module_table.set_import_table(std_table);

        for import in &import_table.standardized_imports {
            if import.source.is_empty() {
                continue;
            }
            // Wildcard imports bind no names themselves; they are expanded
            // against the source module on demand (Level 2).
            if import.is_wildcard {
                module_table.add_wildcard_import(strip_crate_prefix(&import.source).to_string());
                continue;
            }

            let local_name = import.effective_name().to_string();
            if local_name.is_empty() {
                continue;
            }
            // The symbol imported from the source module: the alias target
            // when present (`use std::io as stdio` -> `io`), otherwise the
            // last `::` segment of the source path.
            let symbol_name = import
                .target
                .original_name
                .as_deref()
                .or_else(|| Some(&import.source))
                .and_then(|path| path.rsplit("::").next())
                .map(|segment| segment.to_string());

            // Classify import source type based on import kind and properties
            let (source_type, condition) = if import.is_system_header {
                (
                    ImportSourceType::StandardLibrary {
                        lang: file.language,
                    },
                    None,
                )
            } else if import.kind == cce_types::import::ImportKind::DynamicImport {
                (
                    ImportSourceType::Dynamic {
                        expression: import.source.clone(),
                    },
                    None,
                )
            } else {
                // Heuristic: detect conditional imports by checking common patterns
                // in the import context (e.g., Python try/except, Rust cfg attributes)
                let condition =
                    Self::detect_import_condition(file, &import.source, import.span.as_ref());
                if let Some(ref cond) = condition {
                    (
                        ImportSourceType::Conditional {
                            condition: cond.clone(),
                        },
                        Some(cond.clone()),
                    )
                } else if Self::is_internal_import(&import.source, file.language) {
                    (
                        ImportSourceType::InternalModule {
                            path: import.source.clone(),
                        },
                        None,
                    )
                } else if Self::is_external_dependency(&import.source, project) {
                    // Extract the top-level package name from the source path
                    let package = import
                        .source
                        .split("::")
                        .next()
                        .or_else(|| import.source.split('.').next())
                        .unwrap_or(&import.source)
                        .to_string();
                    (ImportSourceType::ExternalDependency { package }, None)
                } else {
                    (ImportSourceType::Unknown, None)
                }
            };

            module_table.add_import(ImportBinding {
                local_name,
                source_path: import.source.clone(),
                symbol_name,
                resolved_symbol: None,
                is_wildcard: false,
                source_type,
                condition,
            });
        }
    }

    /// Determine if an import source refers to an internal module (same package/project).
    ///
    /// Uses language-specific patterns to identify internal imports:
    /// - Rust: `crate::`, `self::`, `super::` prefixes
    /// - Python: relative imports (starting with `.`)
    /// - JavaScript/TypeScript: relative imports (starting with `./` or `../`)
    /// - Go: relative imports (starting with `.`)
    /// - C/C++: relative includes (quoted paths, not angle-bracket system headers)
    fn is_internal_import(source: &str, language: Language) -> bool {
        match language {
            Language::Rust => {
                source.starts_with("crate::")
                    || source.starts_with("self::")
                    || source.starts_with("super::")
            }
            Language::Python => source.starts_with('.'),
            Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
                source.starts_with("./") || source.starts_with("../")
            }
            Language::Go => source.starts_with('.'),
            Language::C | Language::Cpp => {
                // C/C++ quoted includes are typically project-local
                // Note: is_system_header already handles angle-bracket includes
                !source.starts_with('<')
            }
            _ => false,
        }
    }

    /// Check if an import source matches a known external dependency.
    ///
    /// Extracts the top-level package name from the source path and checks
    /// if it exists in the project's external dependency table. This enables
    /// proper classification of third-party imports as `ExternalDependency`.
    fn is_external_dependency(source: &str, project: Option<&ProjectSymbolTable>) -> bool {
        let Some(project) = project else {
            return false;
        };
        // Extract top-level package name from source path
        // Rust: "serde::Deserialize" -> "serde"
        // Python: "requests.api" -> "requests"
        // JS: "lodash/fp" -> "lodash"
        let package = source
            .split("::")
            .next()
            .or_else(|| source.split('.').next())
            .or_else(|| source.split('/').next())
            .unwrap_or(source);
        project.get_external_dep(package).is_some()
    }

    /// Detect if an import is conditional based on file context.
    ///
    /// Uses span information and heuristics to detect conditional imports:
    /// - Python: imports inside `try/except` blocks or guarded by `if` statements
    /// - Rust: imports with `#[cfg(...)]` attributes
    /// - JavaScript/TypeScript: imports inside `if` blocks or `try/catch` blocks
    ///
    /// Note: This is a best-effort heuristic. The parser could provide more
    /// accurate information by tracking the AST context of each import.
    fn detect_import_condition(
        file: &ParsedFile,
        source: &str,
        span: Option<&cce_types::types::Span>,
    ) -> Option<String> {
        // Extract the source region around the import for pattern matching
        let context = span.and_then(|s| {
            let start = s.start_byte;
            let end = s.end_byte;
            let source_bytes = file.source.as_bytes();
            if end <= source_bytes.len() {
                // Look back up to 200 characters for context (e.g., try/except, #[cfg])
                let lookback = start.saturating_sub(200);
                std::str::from_utf8(&source_bytes[lookback..end]).ok()
            } else {
                None
            }
        });

        match file.language {
            cce_types::language::Language::Python => {
                // Python conditional imports are typically in try/except blocks
                // Check for try/except context or common optional dependency patterns
                if let Some(ctx) = &context {
                    if ctx.contains("try")
                        && (ctx.contains("except") || ctx.contains("ImportError"))
                    {
                        return Some("try/except".to_string());
                    }
                    // Check for if __name__ == "__main__" guards or other conditional patterns
                    if ctx.contains("if ") && ctx.contains(":") {
                        // Look for the specific import line to see if it's inside an if block
                        let lines: Vec<&str> = ctx.lines().collect();
                        if let Some(last_line) = lines.last() {
                            if last_line.trim_start().starts_with("if ") {
                                return Some("if_guard".to_string());
                            }
                        }
                    }
                }
                // Fallback: check for common optional dependency patterns
                if source.starts_with("_") || source.contains("optional") {
                    Some("try/except".to_string())
                } else {
                    None
                }
            }
            cce_types::language::Language::Rust => {
                // Rust conditional imports use #[cfg(...)] attributes
                // Check for #[cfg] context before the import
                if let Some(ctx) = &context {
                    // Look for #[cfg(...)] pattern
                    if let Some(cfg_start) = ctx.rfind("#[cfg(") {
                        let cfg_region = &ctx[cfg_start..];
                        if let Some(cfg_end) = cfg_region.find(']') {
                            let cfg_attr = &cfg_region[..=cfg_end];
                            return Some(cfg_attr.to_string());
                        }
                    }
                    // Also check for #[cfg_attr(...)]
                    if let Some(cfg_start) = ctx.rfind("#[cfg_attr(") {
                        let cfg_region = &ctx[cfg_start..];
                        if let Some(cfg_end) = cfg_region.find(']') {
                            let cfg_attr = &cfg_region[..=cfg_end];
                            return Some(cfg_attr.to_string());
                        }
                    }
                }
                // Fallback: check for common cfg patterns in source path
                if source.contains("target_os") || source.contains("feature") {
                    Some(format!("cfg({})", source))
                } else {
                    None
                }
            }
            cce_types::language::Language::JavaScript
            | cce_types::language::Language::TypeScript
            | cce_types::language::Language::Jsx
            | cce_types::language::Language::Tsx => {
                // JS/TS conditional imports may be in if blocks or try/catch
                if let Some(ctx) = &context {
                    // Check for try/catch context
                    if ctx.contains("try") && ctx.contains("catch") {
                        return Some("try/catch".to_string());
                    }
                    // Check for if blocks
                    let lines: Vec<&str> = ctx.lines().collect();
                    if let Some(last_line) = lines.last() {
                        let trimmed = last_line.trim_start();
                        if trimmed.starts_with("if ") || trimmed.starts_with("else if ") {
                            return Some("if_guard".to_string());
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Wire the file's named re-exports into the module table as reexport
    /// bindings. Resolution happens lazily in
    /// `ProjectSymbolTable::resolve_via_module_reexport`, which caches the
    /// result back into the binding.
    fn wire_reexports(&self, file: &ParsedFile, module_table: &mut ModuleSymbolTable) {
        for record in &file.reexports {
            module_table.add_reexport(ReexportBinding {
                local_name: record.local_name.clone(),
                original_module: record.original_module.clone(),
                original_name: record.original_name.clone(),
                chain_depth: record.chain_depth,
                resolved_symbol: None,
            });
        }
    }

    /// Build project symbol table from a list of parsed files
    ///
    /// Creates the full four-level hierarchy:
    /// 1. Groups files into packages (by project root)
    /// 2. Creates ModuleSymbolTable per file with exports
    /// 3. Creates PackageSymbolTable per package with module_path_index
    /// 4. Creates ProjectSymbolTable with global_index and simple_name_index
    ///
    /// This enables:
    /// - Module path resolution (super::, crate::, relative paths)
    /// - Import-based resolution via module tables
    /// - Re-export resolution via module tables
    pub fn build(&self, files: &[&ParsedFile]) -> ProjectSymbolTable {
        let project_symbols = ProjectSymbolTable::new(self.project_root.clone());

        // Group files into packages
        // For now, all files belong to the default package derived from project_root
        let package_name = self
            .project_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "default".to_string());

        let language = files.first().map(|f| f.language).unwrap_or_default();
        let package = Arc::new(PackageSymbolTable::new(
            package_name.clone(),
            package_name.clone(),
            self.project_root.to_string_lossy().to_string(),
            language,
        ));

        // Build module tables for each file
        for file in files {
            let module_path =
                determine_module_path(Path::new(&file.path), &self.project_root, file.language)
                    .unwrap_or_default();

            let mut module_table = ModuleSymbolTable::new(
                module_path.clone(),
                file.path.clone(),
                file.language,
                package_name.clone(),
            );

            // Add exported entities from this file
            for entity in &file.entities {
                if self.is_entity_exported(entity, file.language) {
                    let location =
                        SymbolLocation::new(file.path.clone(), entity.span, file.language);
                    let metadata = SymbolMetadata::new(entity.name.clone(), entity.kind, location);
                    let visibility = self.detect_visibility(entity, file.language);
                    module_table.add_export(entity.name.clone(), metadata, visibility);
                }
            }

            // Also index exports explicitly declared from raw relations
            let exports =
                crate::helpers::extract_exports_from_entities(&file.entities, &file.language);
            for export in &exports {
                if !module_table.has_export(&export.function_name) {
                    if let Some(entity) = file.entities.iter().find(|e| e.id == export.function_id)
                    {
                        let location =
                            SymbolLocation::new(file.path.clone(), entity.span, file.language);
                        let metadata =
                            SymbolMetadata::new(entity.name.clone(), entity.kind, location);
                        let visibility = self.detect_visibility(entity, file.language);
                        module_table.add_export(export.function_name.clone(), metadata, visibility);
                    }
                }
            }

            // wire the file's imports into the module table so Level 2
            // (import-based) resolution is functional.
            // Pass None for project during build; external deps not yet registered.
            self.wire_imports(file, &mut module_table, None);

            // Wire named re-exports so Level 3 (re-export) resolution can
            // resolve them lazily against the package's module tables.
            self.wire_reexports(file, &mut module_table);

            // Create LocalSymbolTable for file-level scope resolution
            let local_table = LocalSymbolTable::from_parsed_file(file);
            module_table.set_local_table(local_table);

            // Build type-member index for this file
            {
                let mut type_index = crate::symbol_table::TypeMemberIndex::new();
                crate::policy::type_member::build_type_index_for_file(
                    &file.entities,
                    &module_path,
                    &file.path,
                    &package_name,
                    file.language,
                    &mut type_index,
                );
                module_table.set_type_index(type_index);
            }

            // Build lightweight type inference context for this file using
            // two-pass inference: collect declarations first, then resolve
            // references (enables forward references and recursive types).
            {
                let inference_ctx =
                    InferenceContext::new().with_type_index(module_table.type_index());
                let type_ctx = TypeInferenceEngine::infer_types_two_pass(file, &inference_ctx);
                project_symbols.set_type_inference_context(&file.path, type_ctx);
            }

            if package.add_module(module_table) {
                self.record_module_path_conflict();
            }
        }

        // Rebuild package exports from module tables
        package.rebuild_exports();
        // Add package to project BEFORE rebuilding global type index,
        // so rebuild_global_type_index can see this package's modules.
        project_symbols.add_package(package);
        // Build global type-member aggregation
        project_symbols.rebuild_global_type_index();
        project_symbols.rebuild_overload_sets();
        if let Some(metrics) = &self.metrics {
            metrics.symbol_table_rebuild_total.increment();
        }

        // Keep backward-compatible flat index for RelationResolver
        for file in files {
            for entity in &file.entities {
                if self.is_entity_exported(entity, file.language) {
                    let qualified_name =
                        format!("{}::{}", normalize_project_path(&file.path), entity.name);
                    project_symbols.insert_symbol(
                        qualified_name,
                        entity.id,
                        file.path.clone(),
                        file.path.clone(),
                    );
                }
            }

            let exports =
                crate::helpers::extract_exports_from_entities(&file.entities, &file.language);
            for export in &exports {
                let qualified_name = format!(
                    "{}::{}",
                    normalize_project_path(&file.path),
                    export.function_name
                );
                project_symbols.insert_symbol(
                    qualified_name,
                    export.function_id,
                    file.path.clone(),
                    file.path.clone(),
                );
            }
        }

        if let Some(metrics) = &self.metrics {
            project_symbols.set_metrics(Arc::clone(metrics));
        }
        // Build cross-file return-type cache and propagate into variables.
        project_symbols.rebuild_cross_file_propagator(files);

        let stats = project_symbols.project_stats();
        tracing::debug!(
            "Built project symbol table with {} symbols from {} files",
            stats.total_symbols,
            files.len()
        );

        project_symbols
    }

    /// Detect entity visibility using the shared language-aware rules.
    ///
    /// Delegates to `crate::policy::detect_entity_visibility`, the single
    /// determination function shared with `extract_exports_from_entities`.
    pub(crate) fn detect_visibility(&self, entity: &Entity, language: Language) -> Visibility {
        crate::policy::detect_entity_visibility(entity, &language)
    }

    /// Check if an entity is exported (publicly visible)
    pub fn is_entity_exported(
        &self,
        entity: &Entity,
        language: cce_types::language::Language,
    ) -> bool {
        crate::policy::is_entity_exported(entity, language)
    }

    /// Add a single parsed file's symbols to an existing project symbol table.
    ///
    /// This enables incremental symbol table building across batches:
    /// each batch adds its files to the same project-level table, and
    /// subsequent batches resolve relations against the accumulated symbols.
    pub fn add_file_to_project(&self, file: &ParsedFile, project: &ProjectSymbolTable) {
        let package_name = self
            .project_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "default".to_string());

        let module_path =
            determine_module_path(Path::new(&file.path), &self.project_root, file.language)
                .unwrap_or_default();

        let mut module_table = ModuleSymbolTable::new(
            module_path,
            file.path.clone(),
            file.language,
            package_name.clone(),
        );

        for entity in &file.entities {
            if self.is_entity_exported(entity, file.language) {
                let location = SymbolLocation::new(file.path.clone(), entity.span, file.language);
                let metadata = SymbolMetadata::new(entity.name.clone(), entity.kind, location);
                let visibility = self.detect_visibility(entity, file.language);
                module_table.add_export(entity.name.clone(), metadata, visibility);
            }
        }

        let exports = crate::helpers::extract_exports_from_entities(&file.entities, &file.language);
        for export in &exports {
            if !module_table.has_export(&export.function_name) {
                if let Some(entity) = file.entities.iter().find(|e| e.id == export.function_id) {
                    let location =
                        SymbolLocation::new(file.path.clone(), entity.span, file.language);
                    let metadata = SymbolMetadata::new(entity.name.clone(), entity.kind, location);
                    let visibility = self.detect_visibility(entity, file.language);
                    module_table.add_export(export.function_name.clone(), metadata, visibility);
                }
            }
        }

        let local_table = LocalSymbolTable::from_parsed_file(file);
        module_table.set_local_table(local_table);

        // wire the file's imports into the module table so Level 2
        // (import-based) resolution is functional.
        // Pass project reference to enable external dependency classification.
        self.wire_imports(file, &mut module_table, Some(project));

        // Wire named re-exports so Level 3 (re-export) resolution can
        // resolve them lazily against the package's module tables.
        self.wire_reexports(file, &mut module_table);

        // Build type-member index for incremental update
        {
            let module_path_clone = module_table.module_path.clone();
            let mut type_index = crate::symbol_table::TypeMemberIndex::new();
            crate::policy::type_member::build_type_index_for_file(
                &file.entities,
                &module_path_clone,
                &file.path,
                &package_name,
                file.language,
                &mut type_index,
            );
            module_table.set_type_index(type_index);
        }

        // Build lightweight type inference context for incremental update
        // using two-pass inference.
        {
            let inference_ctx = InferenceContext::new().with_type_index(module_table.type_index());
            let type_ctx = TypeInferenceEngine::infer_types_two_pass(file, &inference_ctx);
            project.set_type_inference_context(&file.path, type_ctx);
            // Cross-file incremental update: refresh propagator for this file
            // and propagate its variables (e.g., `x = foo()` where `foo` is in
            // another file).
            project.update_cross_file_for_file(file);
        }

        let package = if let Some(pkg) = project.get_package(&package_name) {
            pkg
        } else {
            Arc::new(PackageSymbolTable::new(
                package_name.clone(),
                package_name.clone(),
                self.project_root.to_string_lossy().to_string(),
                file.language,
            ))
        };

        // Incremental path: patch package exports and project indexes in
        // O(affected) time instead of scanning the whole package.
        let type_index_clone = module_table.type_index().clone();
        let delta = package.add_module_incremental(module_table);
        if delta.path_collision {
            self.record_module_path_conflict();
        }
        if let Some(metrics) = &self.metrics {
            metrics.symbol_table_incremental_total.increment();
        }
        project.apply_package_delta(Arc::clone(&package), &delta);
        project.apply_type_delta_for_file(&file.path, &type_index_clone);
        project.rebuild_overload_sets();

        // File-symbol incremental pruning: remove stale `file::symbol`
        // entries for this file that are no longer exported.
        {
            let mut new_simple_names: HashSet<String> = HashSet::new();
            for entity in &file.entities {
                if self.is_entity_exported(entity, file.language) {
                    new_simple_names.insert(entity.name.clone());
                }
            }
            for export in &exports {
                new_simple_names.insert(export.function_name.clone());
            }
            project.prune_file_symbols(&file.path, &new_simple_names);
        }

        for entity in &file.entities {
            if self.is_entity_exported(entity, file.language) {
                let qualified_name =
                    format!("{}::{}", normalize_project_path(&file.path), entity.name);
                project.insert_symbol(
                    qualified_name,
                    entity.id,
                    file.path.clone(),
                    file.path.clone(),
                );
            }
        }

        for export in &exports {
            let qualified_name = format!(
                "{}::{}",
                normalize_project_path(&file.path),
                export.function_name
            );
            project.insert_symbol(
                qualified_name,
                export.function_id,
                file.path.clone(),
                file.path.clone(),
            );
        }
    }

    /// Get module path for a file (public helper)
    pub fn get_module_path(
        &self,
        file_path: &str,
        language: cce_types::language::Language,
    ) -> Option<String> {
        determine_module_path(Path::new(file_path), &self.project_root, language)
    }

    /// Get the project root
    pub fn project_root(&self) -> &PathBuf {
        &self.project_root
    }

    /// Register an external library into the project symbol table.
    ///
    /// Uses the language-aware [`ImportResolutionStrategy`] to dispatch to the
    /// correct handler (header / Python / JavaScript) without `dyn` indirection.
    pub fn register_external_library(
        &self,
        project: &ProjectSymbolTable,
        library_path: &Path,
        language: Language,
        version: Option<String>,
    ) -> Result<(), crate::error::RelationError> {
        let mut registry = crate::external::ExternalLibraryRegistry::new()
            .with_depth(crate::external::ImportResolutionDepth::ImportedModule);
        registry.register_into_project(library_path, language, project, version)
    }

    /// Register multiple external libraries in bulk.
    pub fn register_external_libraries(
        &self,
        project: &ProjectSymbolTable,
        libraries: &[(PathBuf, Language, Option<String>)],
    ) -> usize {
        let mut registry = crate::external::ExternalLibraryRegistry::new()
            .with_depth(crate::external::ImportResolutionDepth::ImportedModule);
        let mut registered = 0;
        for (path, lang, version) in libraries {
            if registry
                .register_into_project(path, *lang, project, version.clone())
                .is_ok()
            {
                registered += 1;
            }
        }
        registered
    }

    /// Return the [`ImportResolutionStrategy`] for a language.
    ///
    /// Exposed for callers that need to branch on import granularity without
    /// replicating the language-to-strategy mapping.
    pub fn import_strategy_for(language: Language) -> crate::external::ImportResolutionStrategy {
        crate::external::ImportResolutionStrategy::for_language(language)
    }
}

impl Default for SymbolTableBuilder {
    fn default() -> Self {
        Self::new(PathBuf::from("."))
    }
}
