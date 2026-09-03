//! Relation index builder
//!
//! Provides a fluent interface for building relation indexes.
//! The builder delegates complex operations to specialized modules:
//! - BuilderConfig: Configuration management
//! - SymbolTableBuilder: Symbol table construction
//! - FileProcessor: File processing and relation resolution

mod batch_processor;
mod config;
mod file_processor;
mod plugin_symbol_replay;
mod symbol_table;

pub use config::BuilderConfig;
pub use symbol_table::SymbolTableBuilder;

use super::core::{ExportInfo, ExportType, RelationIndex};
use crate::dependency_graph::FileDependencyGraph;
use crate::index::{
    EntityIndexOps, ExportIndexOps, FileIndexOps, ImportIndexOps, RelationQueryOps,
};
use crate::symbol_table::ProjectSymbolTable;
use cce_metrics::RelationMetrics;
use cce_parser_core::AstParser;
use cce_plugin::PluginRegistry;
use cce_types::{Entity, EntityId, FileInfo, ImportTable, ParsedFile, ResolvedRelation};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// Relation index builder
///
/// Provides a fluent interface for building relation indexes.
/// Supports import source classification for better dependency analysis.
pub struct IndexBuilder {
    /// The underlying relation index
    index: RelationIndex,
    /// Builder configuration
    config: BuilderConfig,
    /// File dependency graph for tracking cross-file dependencies
    dependency_graph: Arc<FileDependencyGraph>,
    /// Monitoring metrics (optional)
    metrics: Option<Arc<RelationMetrics>>,
    /// Plugin registry (optional) for the `RelationExtract` capability.
    plugin_registry: Option<Arc<PluginRegistry>>,
    /// Whether `RelationExtract` plugin symbols/relations are replayed during
    /// partial (hot-update) builds. Mirrors the full-build
    /// `plugin_symbols_enabled` gate.
    plugin_symbols_enabled: bool,
    /// Build-wide lazy AST parser: the fallback import-extraction path
    /// reuses a single `AstParser` across all files instead of paying the
    /// tree-sitter grammar initialization cost once per file.
    ast_parser: OnceLock<Mutex<AstParser>>,
}

impl IndexBuilder {
    /// Create a new index builder
    pub fn new() -> Self {
        let index = RelationIndex::new();
        let dependency_graph = Arc::clone(&index.dependency_graph);
        Self {
            index,
            config: BuilderConfig::new(),
            dependency_graph,
            metrics: None,
            plugin_registry: None,
            plugin_symbols_enabled: false,
            ast_parser: OnceLock::new(),
        }
    }

    /// Create from an existing index
    pub fn from_index(index: RelationIndex) -> Self {
        let dependency_graph = Arc::clone(&index.dependency_graph);
        Self {
            index,
            config: BuilderConfig::new(),
            dependency_graph,
            metrics: None,
            plugin_registry: None,
            plugin_symbols_enabled: false,
            ast_parser: OnceLock::new(),
        }
    }

    /// Create an index builder with automatic configuration loading.
    ///
    /// `manifest_scan_depth` controls how many directory levels below the
    /// root are searched for additional build manifests (0 = root only).
    pub fn with_auto_config(
        project_root: impl AsRef<std::path::Path>,
        enable_stdlib_filtering: bool,
        manifest_scan_depth: usize,
    ) -> Result<Self, crate::config_parser::ConfigParseError> {
        let config =
            config::load_auto_config(project_root, enable_stdlib_filtering, manifest_scan_depth)?;
        let index = RelationIndex::new();
        let dependency_graph = Arc::clone(&index.dependency_graph);
        Ok(Self {
            index,
            config,
            dependency_graph,
            metrics: None,
            plugin_registry: None,
            plugin_symbols_enabled: false,
            ast_parser: OnceLock::new(),
        })
    }

    /// Set external packages for import classification
    pub fn set_external_packages(
        &mut self,
        language: cce_types::language::Language,
        packages: std::collections::HashSet<String>,
    ) -> &mut Self {
        self.config.set_external_packages(language, packages);
        self
    }

    /// Set full dependency information for enhanced classification
    pub fn set_external_dependencies(
        &mut self,
        language: cce_types::language::Language,
        dependencies: Vec<crate::config_parser::UntypedDependency>,
    ) -> &mut Self {
        self.config
            .set_external_dependencies(language, dependencies);
        self
    }

    /// Automatically load external packages from a BuildConfigParser
    pub fn auto_load_external_packages(
        &mut self,
        config_parser: &crate::config_parser::BuildConfigParser,
    ) -> &mut Self {
        self.config.auto_load_external_packages(config_parser);
        self
    }

    /// Clear all external packages and dependencies
    pub fn clear_external_packages(&mut self) -> &mut Self {
        self.config.clear();
        self
    }

    /// Get external packages for rollback
    pub fn get_external_packages(
        &self,
    ) -> Option<HashMap<cce_types::language::Language, std::collections::HashSet<String>>> {
        self.config.get_external_packages()
    }

    /// Set all external packages at once (for rollback)
    pub fn set_all_external_packages(
        &mut self,
        packages: HashMap<cce_types::language::Language, std::collections::HashSet<String>>,
    ) {
        self.config.set_all_external_packages(packages);
    }

    /// Automatically load full dependency information from a BuildConfigParser
    pub fn auto_load_dependencies(
        &mut self,
        config_parser: &crate::config_parser::BuildConfigParser,
    ) -> &mut Self {
        self.config.auto_load_dependencies(config_parser);
        self
    }

    /// Set whether to filter out standard library calls
    pub fn set_filter_stdlib_calls(&mut self, filter: bool) -> &Self {
        self.config.set_filter_stdlib_calls(filter);
        self
    }

    /// Configure deterministic relation graph construction policies.
    pub fn set_graph_options(
        &mut self,
        max_relations_per_file: usize,
        analyze_imports: bool,
        track_cross_file_deps: bool,
    ) -> &Self {
        self.config
            .set_max_relations_per_file(max_relations_per_file);
        self.config
            .set_graph_options(analyze_imports, track_cross_file_deps);
        self
    }

    /// Set whether `SymbolExtract` plugins supply import/export extraction
    /// for custom languages.
    pub fn set_symbol_extract_enabled(&mut self, enabled: bool) -> &Self {
        self.config.set_symbol_extract_enabled(enabled);
        self
    }

    /// Set whether to auto-load external dependency symbols from package
    /// manager caches during the build phase.
    pub fn set_load_external_symbols(&mut self, enabled: bool) -> &Self {
        self.config.set_load_external_symbols(enabled);
        self
    }

    /// Set whether to automatically detect external symbols.
    pub fn set_auto_detect_external_symbols(&mut self, enabled: bool) -> &Self {
        self.config.set_auto_detect_external_symbols(enabled);
        self
    }

    /// Set external symbols cache directory.
    pub fn set_external_symbols_cache_dir(&mut self, dir: Option<PathBuf>) -> &Self {
        self.config.set_external_symbols_cache_dir(dir);
        self
    }

    /// Set whether `import_table` is required.
    pub fn set_require_import_table(&mut self, required: bool) -> &Self {
        self.config.set_require_import_table(required);
        self
    }

    /// Pre-load external dependency symbols from package manager caches.
    ///
    /// This method discovers installed packages for all known external
    /// dependencies and extracts their public API surface into the project
    /// symbol table. It should be called after build manifests have been
    /// scanned and before relation resolution begins.
    ///
    /// When `auto_detect_external_symbols` is enabled, it will attempt to
    /// discover external packages automatically even if no explicit manifest
    /// was scanned. When a cache directory is configured, it will try to
    /// load from cache first and save after a successful load.
    ///
    /// Returns the number of packages successfully loaded.
    pub fn load_external_symbols(
        &mut self,
        project_root: &Path,
        symbol_table: &ProjectSymbolTable,
    ) -> usize {
        if !self.config.policy.load_external_symbols {
            return 0;
        }

        if self.config.policy.auto_detect_external_symbols {
            tracing::debug!("auto_detect_external_symbols enabled, checking for external packages");
        }

        if let Some(cache_dir) = &self.config.policy.external_symbols_cache_dir {
            if self.load_from_cache(cache_dir, symbol_table) {
                tracing::info!("loaded external symbols from cache: {:?}", cache_dir);
                return 0;
            }
        }

        let deps = match self.config.package_data.external_dependencies.as_ref() {
            Some(d) => d,
            None => return 0,
        };

        let load_config = crate::external::loader::ExternalLoadConfig {
            enabled: true,
            max_packages_per_language: 256,
            verbose: tracing::enabled!(tracing::Level::INFO),
        };

        let mut loader = crate::external::loader::ExternalSymbolLoader::new(load_config);
        let stats = loader.load_all_known(deps, project_root, symbol_table);

        if stats.loaded > 0 {
            tracing::info!(
                "External symbol loading: {} packages loaded, {} already present, {} failed",
                stats.loaded,
                stats.already_loaded,
                stats.failed
            );
            for (lang, count) in &stats.by_language {
                tracing::info!("  {:?}: {} packages", lang, count);
            }
        }

        if let Some(cache_dir) = &self.config.policy.external_symbols_cache_dir {
            self.save_to_cache(cache_dir, symbol_table);
        }

        stats.loaded
    }

    fn load_from_cache(&self, cache_dir: &Path, symbol_table: &ProjectSymbolTable) -> bool {
        let cache_file = cache_dir.join("external_symbols.cache");
        if !cache_file.exists() {
            return false;
        }
        // Expiry check: treat cache older than 7 days as stale
        if let Ok(metadata) = std::fs::metadata(&cache_file) {
            if let Ok(modified) = metadata.modified() {
                if let Ok(elapsed) = modified.elapsed() {
                    if elapsed.as_secs() > 7 * 24 * 3600 {
                        tracing::debug!("external symbols cache expired at {:?}", cache_file);
                        return false;
                    }
                }
            }
        }
        tracing::debug!("checking external symbols cache at {:?}", cache_file);
        let content = match std::fs::read_to_string(&cache_file) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    "failed to read external symbols cache {:?}: {}",
                    cache_file,
                    e
                );
                return false;
            }
        };
        let entries: Vec<crate::symbol_table::project::ExternalSymbolTable> =
            match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(
                        "failed to deserialize external symbols cache {:?}: {}",
                        cache_file,
                        e
                    );
                    return false;
                }
            };
        if entries.is_empty() {
            return false;
        }

        for table in entries {
            symbol_table.add_external_dep(table);
        }
        tracing::info!(
            "loaded external symbols from cache: {:?} ({} packages)",
            cache_dir,
            symbol_table.all_external_deps().len()
        );
        true
    }

    fn save_to_cache(&self, cache_dir: &Path, symbol_table: &ProjectSymbolTable) {
        let cache_file = cache_dir.join("external_symbols.cache");
        tracing::debug!("saving external symbols cache to {:?}", cache_file);
        if let Err(e) = std::fs::create_dir_all(cache_dir) {
            tracing::warn!("failed to create cache dir {:?}: {}", cache_dir, e);
            return;
        }
        let entries: Vec<crate::symbol_table::project::ExternalSymbolTable> = symbol_table
            .all_external_deps()
            .into_iter()
            .map(|arc| (*arc).clone())
            .collect();
        if entries.is_empty() {
            tracing::debug!("no external symbols to cache, skipping write");
            return;
        }
        let content = match serde_json::to_string(&entries) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("failed to serialize external symbols cache: {}", e);
                return;
            }
        };
        let tmp_file = cache_dir.join("external_symbols.cache.tmp");
        if let Err(e) = std::fs::write(&tmp_file, content) {
            tracing::warn!(
                "failed to write external symbols cache tmp {:?}: {}",
                tmp_file,
                e
            );
            return;
        }
        if let Err(e) = std::fs::rename(&tmp_file, &cache_file) {
            tracing::warn!(
                "failed to rename external symbols cache {:?} -> {:?}: {}",
                tmp_file,
                cache_file,
                e
            );
            let _ = std::fs::remove_file(&tmp_file);
        } else {
            tracing::debug!("saved external symbols cache to {:?}", cache_file);
        }
    }

    /// Get a reference to the underlying index
    pub fn index(&self) -> &RelationIndex {
        &self.index
    }

    pub fn config_fingerprint(&self) -> String {
        self.config.fingerprint()
    }

    pub fn has_external_packages(&self) -> bool {
        self.config
            .package_data
            .external_packages
            .as_ref()
            .is_some_and(|m| !m.is_empty())
    }

    /// Consume the builder and return the index
    pub fn build(self) -> RelationIndex {
        self.index
    }

    /// Get a reference to the dependency graph
    pub fn dependency_graph(&self) -> &FileDependencyGraph {
        &self.dependency_graph
    }

    /// Get a shared reference to the dependency graph (Arc clone)
    pub fn dependency_graph_arc(&self) -> Arc<FileDependencyGraph> {
        Arc::clone(&self.dependency_graph)
    }

    /// Set the dependency graph (for sharing with RelationIndex)
    pub fn with_dependency_graph(mut self, graph: Arc<FileDependencyGraph>) -> Self {
        self.index.dependency_graph = Arc::clone(&graph);
        self.dependency_graph = graph;
        self
    }

    /// Set monitoring metrics
    pub fn with_metrics(mut self, metrics: Arc<RelationMetrics>) -> Self {
        self.metrics = Some(Arc::clone(&metrics));
        self.index.set_metrics(Arc::clone(&metrics));
        self
    }

    /// Attach the plugin registry for the `RelationExtract` capability.
    ///
    /// Plugin symbols/relations only enter the index when
    /// `relation.plugin_symbols_enabled` is configured on (checked by the
    /// caller before invocation).
    pub fn with_plugin_registry(mut self, plugin_registry: Arc<PluginRegistry>) -> Self {
        self.plugin_registry = Some(plugin_registry);
        self
    }

    /// Enable `RelationExtract` plugin symbol/relation replay during partial
    /// (hot-update) builds, mirroring the full-build `plugin_symbols_enabled`
    /// gate.
    pub fn with_plugin_symbols_enabled(&mut self, enabled: bool) -> &mut Self {
        self.plugin_symbols_enabled = enabled;
        self
    }

    /// Get a reference to the metrics (if enabled)
    pub fn metrics(&self) -> Option<&Arc<RelationMetrics>> {
        self.metrics.as_ref()
    }

    /// Whether a plugin registry is attached (and therefore plugin symbols
    /// may be injected during this build).
    pub fn plugin_registry(&self) -> Option<&Arc<PluginRegistry>> {
        self.plugin_registry.as_ref()
    }

    // ========== Function Index Operations ==========

    /// Add a function entity to the index
    pub fn add_function(&self, entity_id: EntityId, entity: Entity) -> &Self {
        self.index.add_function(entity_id, entity);
        self
    }

    /// Add a function entity with file path to the index
    pub fn add_function_with_path(
        &self,
        entity_id: EntityId,
        entity: Entity,
        file_path: String,
    ) -> &Self {
        self.index
            .add_function_with_path(entity_id, entity, file_path);
        self
    }

    /// Add multiple function entities to the index
    pub fn add_functions(&self, functions: Vec<(EntityId, Entity)>) -> &Self {
        self.index.add_functions(functions);
        self
    }

    /// Add multiple function entities with file paths to the index
    pub fn add_functions_with_paths(&self, functions: Vec<(EntityId, Entity, String)>) -> &Self {
        self.index.add_functions_with_paths(functions);
        self
    }

    // ========== Import Index Operations ==========

    /// Add an import table to the index
    pub fn add_import_table(&self, file_id: String, import_table: ImportTable) -> &Self {
        self.index.add_import_table(file_id, import_table);
        self
    }

    /// Add import tables from a map
    pub fn add_import_tables(&self, imports: HashMap<String, ImportTable>) -> &Self {
        for (file_id, import_table) in imports {
            self.index.add_import_table(file_id, import_table);
        }
        self
    }

    /// Build import index from a map
    pub fn build_import_index(
        imports: HashMap<String, ImportTable>,
    ) -> HashMap<String, ImportTable> {
        imports
    }

    // ========== File Index Operations ==========

    /// Add a file to the index
    pub fn add_file(&self, file: FileInfo) -> &Self {
        self.index.add_file(file);
        self
    }

    /// Add multiple files to the index
    pub fn add_files(&self, files: Vec<FileInfo>) -> &Self {
        for file in files {
            self.index.add_file(file);
        }
        self
    }

    // ========== Export Index Operations ==========

    /// Add exports to the index
    pub fn add_exports(&self, file_id: String, exports: Vec<ExportInfo>) -> &Self {
        self.index.add_exports(file_id, exports);
        self
    }

    /// Add a single export to the index
    pub fn add_export(&self, file_id: &str, export: ExportInfo) -> &Self {
        self.index.add_export(file_id, export);
        self
    }

    /// Add a named export
    pub fn add_named_export(
        &self,
        file_id: &str,
        function_id: cce_types::EntityId,
        function_name: &str,
    ) -> &Self {
        self.index.add_export(
            file_id,
            ExportInfo {
                function_id,
                function_name: function_name.to_string(),
                export_type: ExportType::Named,
            },
        );
        self
    }

    /// Add a default export
    pub fn add_default_export(
        &self,
        file_id: &str,
        function_id: cce_types::EntityId,
        function_name: &str,
    ) -> &Self {
        self.index.add_export(
            file_id,
            ExportInfo {
                function_id,
                function_name: function_name.to_string(),
                export_type: ExportType::Default,
            },
        );
        self
    }

    // ========== Batch Operations ==========

    /// Process a file and add all its information to the index
    pub fn process_file(
        &self,
        file: FileInfo,
        functions: Vec<(EntityId, Entity)>,
        relations: Vec<ResolvedRelation>,
        import_table: Option<ImportTable>,
        exports: Vec<ExportInfo>,
    ) -> &Self {
        // Store file_id for later use
        let file_id = file.id.clone();

        // Add file
        self.index.add_file(file);

        // Add functions
        self.index.add_functions(functions);

        // Add resolved relations
        self.index.add_resolved_relations(relations);

        // Add imports if present
        if let Some(imports) = import_table {
            self.index.add_import_table(file_id.clone(), imports);
        }

        // Add exports
        if !exports.is_empty() {
            self.index.add_exports(file_id, exports);
        }

        self
    }

    // ========== Statistics ==========

    /// Check if the index is empty
    pub fn is_empty(&self) -> bool {
        self.index.function_count() == 0
            && self.index.call_count() == 0
            && self.index.file_count() == 0
    }

    /// Clear all indexes
    pub fn clear(&self) {
        self.index.clear();
    }

    // ========== ParsedFile Operations (New Architecture) ==========

    /// Create an initially empty project symbol table for a streamed build.
    ///
    /// Call [`Self::add_file_symbols`] once per parsed file before resolving
    /// any relations. The table preserves the resolver's existing scope and
    /// visibility semantics while allowing parsed files to be released.
    pub fn create_project_symbol_table(
        &self,
        project_root: impl AsRef<Path>,
    ) -> ProjectSymbolTable {
        let table = ProjectSymbolTable::new_with_config(
            project_root.as_ref().to_path_buf(),
            self.config.symbol_resolution.clone(),
        );
        if let Some(metrics) = &self.metrics {
            table.set_metrics(Arc::clone(metrics));
        }
        table
    }

    /// Add one parsed file's symbols to an existing project symbol table.
    pub fn add_file_symbols(&self, file: &ParsedFile, symbols: &ProjectSymbolTable) {
        let mut symbol_builder = symbol_table::SymbolTableBuilder::new(symbols.root_path.clone());
        symbol_builder.with_metrics(self.metrics.clone());
        symbol_builder.add_file_to_project(file, symbols);
    }

    /// Create a file processor wired to this builder's metrics and plugin
    /// registry. `config` is passed explicitly so callers can supply a
    /// temporary config (e.g. classification overrides).
    fn make_file_processor<'a>(
        &'a self,
        config: &'a BuilderConfig,
    ) -> file_processor::FileProcessor<'a> {
        let mut processor = file_processor::FileProcessor::with_registry(
            &self.index,
            config,
            &self.dependency_graph,
            self.plugin_registry.as_deref(),
        );
        processor.with_metrics(self.metrics.clone());
        processor.with_ast_parser(self.shared_ast_parser());
        processor
    }

    /// Lazily initialize and return the build-wide AST parser
    fn shared_ast_parser(&self) -> &Mutex<cce_parser_core::AstParser> {
        self.ast_parser
            .get_or_init(|| Mutex::new(cce_parser_core::AstParser::new()))
    }

    /// Register file metadata, dependencies, imports, exports, and entities.
    ///
    /// All files must pass through this stage before
    /// [`Self::resolve_file_relations`] runs so cross-file target entities are
    /// available regardless of batch order.
    pub fn register_file_entities(&self, file: &ParsedFile) {
        let processor = self.make_file_processor(&self.config);
        processor.index_file_core(file);
    }

    /// Resolve and register relations for a file using a complete symbol table.
    pub fn resolve_file_relations(&self, file: &ParsedFile, symbols: &ProjectSymbolTable) {
        let processor = self.make_file_processor(&self.config);
        let resolver = processor.create_resolver();
        processor.process_relations(file, symbols, &resolver);
    }

    /// Record metrics for a streamed build after all phases have completed.
    pub fn record_streamed_build(&self, elapsed: std::time::Duration, file_count: usize) {
        if let Some(metrics) = &self.metrics {
            let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
            let extracted_count = self.index.resolved_relation_count();
            metrics.record_build(elapsed_ms, extracted_count, file_count);
        }
    }

    /// Add a resolved relation to the index
    pub fn add_resolved_relation(&self, relation: ResolvedRelation) -> &Self {
        self.index.add_resolved_relation(relation);
        self
    }

    /// Add multiple resolved relations
    pub fn add_resolved_relations(&self, relations: Vec<ResolvedRelation>) -> &Self {
        for relation in relations {
            self.index.add_resolved_relation(relation);
        }
        self
    }
}

impl Default for IndexBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::FileLevelOps;
    use cce_metrics::MetricsRegistry;
    use cce_types::StableSymbolKey;
    use cce_types::relation::CallContext;
    use cce_types::{EntityKind, Language, RawRelationData, RelationType, Span};

    /// Helper function to create a test function entity
    fn create_test_function_entity(id: u32, name: &str) -> Entity {
        Entity {
            id: EntityId(id.into()),
            kind: EntityKind::Function,
            name: name.to_string(),
            signature: format!("fn {}()", name),
            parameters: Vec::new(),
            return_type: None,
            span: Span::default(),
            depth: 0,
            parent: None,
            children: Vec::new(),
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            metadata: HashMap::new(),
            is_stdlib: false,
            subtype: None,
            stdlib_category: None,
        }
    }

    #[test]
    fn test_add_function() {
        let builder = IndexBuilder::new();
        builder.add_function(EntityId(1), create_test_function_entity(1, "test"));
        let index = builder.build();
        assert_eq!(index.function_count(), 1);
    }

    #[test]
    fn test_add_functions() {
        let builder = IndexBuilder::new();
        builder.add_functions(vec![
            (EntityId(1), create_test_function_entity(1, "test1")),
            (EntityId(2), create_test_function_entity(2, "test2")),
        ]);
        let index = builder.build();
        assert_eq!(index.function_count(), 2);
    }

    #[test]
    fn test_add_resolved_relation() {
        let builder = IndexBuilder::new();
        builder.add_resolved_relation(ResolvedRelation {
            caller: EntityId(0),
            callee_id: Some(EntityId(1)),
            callee_name: "func_2".to_string(),
            relation_type: RelationType::DirectCall,
            span: cce_types::Span::default(),
            is_external: false,
            external_type: None,
            callee_symbol: None,
            stdlib_category: None,
            owner_type: None,
            call_context: CallContext::Direct,
        });
        let index = builder.build();
        assert_eq!(index.call_count(), 1);
    }

    /// coordinator products carry `import_table`, so registering a
    /// built-in-language file must never trigger the second-parse fallback.
    #[test]
    fn import_fallback_metric_not_incremented_when_import_table_present() {
        let registry = MetricsRegistry::new();
        let metrics = RelationMetrics::new(&registry, 1);
        let builder = IndexBuilder::new().with_metrics(metrics.clone());

        let mut file = ParsedFile::new(Language::Rust, "file.rs".to_string(), "fn main() {}");
        file.import_table = Some(ImportTable {
            file_id: "file.rs".to_string(),
            standardized_imports: Vec::new(),
            source_stats: Default::default(),
        });

        builder.register_file_entities(&file);

        assert_eq!(
            metrics.import_fallback_total.get(),
            0,
            "cached import_table must short-circuit the fallback parse"
        );
    }

    /// a built-in-language file without `import_table` (e.g. a directly
    /// constructed fixture) triggers the fallback and is observable via the
    /// fallback counter.
    #[test]
    fn import_fallback_metric_incremented_when_import_table_missing() {
        let registry = MetricsRegistry::new();
        let metrics = RelationMetrics::new(&registry, 1);
        let builder = IndexBuilder::new().with_metrics(metrics.clone());

        let file = ParsedFile::new(Language::Rust, "file.rs".to_string(), "fn main() {}");
        builder.register_file_entities(&file);

        assert_eq!(
            metrics.import_fallback_total.get(),
            1,
            "missing import_table must route through the fallback parse"
        );
    }

    /// custom-language files go through the plugin `SymbolExtract`
    /// path (no AST); they must not fall back to a tree-sitter parse.
    #[test]
    fn import_fallback_not_incremented_for_custom_language_with_symbol_extract() {
        let registry = MetricsRegistry::new();
        let metrics = RelationMetrics::new(&registry, 1);
        let mut builder = IndexBuilder::new().with_metrics(metrics.clone());
        builder.set_symbol_extract_enabled(true);

        let file = ParsedFile::new(Language::Custom(7), "file.plang".to_string(), "hello");
        builder.register_file_entities(&file);

        assert_eq!(
            metrics.import_fallback_total.get(),
            0,
            "custom languages must not fall back to a tree-sitter parse"
        );
    }

    #[test]
    fn streamed_build_matches_complete_input_build() {
        let mut caller = ParsedFile::new(Language::Rust, "caller.rs".to_string(), "fn caller() {}");
        caller.add_entity(create_test_function_entity(1, "caller"));
        caller.add_relation(RawRelationData {
            src: EntityId(1),
            level: cce_types::RelationLevel::Entity,
            dst_name: "callee".to_string(),
            relation_type: RelationType::DirectCall,
            span: Span::default(),
            stdlib_category: None,
        });

        let mut callee = ParsedFile::new(Language::Rust, "callee.rs".to_string(), "fn callee() {}");
        let mut callee_entity = create_test_function_entity(2, "callee");
        callee_entity.modifiers.push("pub".to_string());
        callee.add_entity(callee_entity);
        let files = [&caller, &callee];

        let complete_builder = IndexBuilder::new();
        complete_builder.add_parsed_files(&files);

        let streamed_builder = IndexBuilder::new();
        let symbols = streamed_builder.create_project_symbol_table(".");
        for file in files {
            streamed_builder.add_file_symbols(file, &symbols);
        }
        for file in files {
            streamed_builder.register_file_entities(file);
        }
        for file in files {
            streamed_builder.resolve_file_relations(file, &symbols);
        }

        assert_eq!(
            complete_builder.index().compute_fingerprint(),
            streamed_builder.index().compute_fingerprint()
        );
    }

    /// Hot-update symbol pre-population must resolve unchanged-file targets
    /// the same way a full build does. A nested target (`run` inside `service`)
    /// has a scoped name different from its simple name; the candidate builder
    /// must still resolve the caller's raw relation to the same internal entity
    /// the full build resolves it to.
    #[test]
    fn hot_update_symbol_prepopulation_matches_full_build_resolution() {
        let mut module_a = ParsedFile::new(Language::Rust, "module_a.rs".to_string(), "");
        let mut service = create_test_function_entity(0, "service");
        service.kind = EntityKind::Class;
        service.modifiers.push("pub".to_string());
        service.children = vec![EntityId(1)];
        let mut run = create_test_function_entity(1, "run");
        run.kind = EntityKind::Function;
        run.modifiers.push("pub".to_string());
        run.parent = Some(EntityId(0));
        module_a.add_entity(service);
        module_a.add_entity(run);

        let mut caller = ParsedFile::new(Language::Rust, "caller.rs".to_string(), "");
        caller.add_entity(create_test_function_entity(0, "caller"));
        caller.add_relation(RawRelationData {
            src: EntityId(0),
            level: cce_types::RelationLevel::Entity,
            dst_name: "run".to_string(),
            relation_type: RelationType::DirectCall,
            span: Span::default(),
            stdlib_category: None,
        });

        // Full build over both files.
        let full_builder = IndexBuilder::new();
        let symbols = full_builder.create_project_symbol_table(".");
        for file in [&caller, &module_a] {
            full_builder.add_file_symbols(file, &symbols);
        }
        for file in [&caller, &module_a] {
            full_builder.register_file_entities(file);
        }
        for file in [&caller, &module_a] {
            full_builder.resolve_file_relations(file, &symbols);
        }
        let full_index = full_builder.build();

        // Hot-update simulation: caller.rs changed, module_a.rs unchanged.
        let hot_index = full_index.detached_clone();
        hot_index.remove_file("caller.rs");
        let hot_builder = IndexBuilder::from_index(hot_index.clone());
        hot_builder.add_parsed_files_with_index_symbols(&[&caller], &hot_index, None);
        let hot_index = hot_builder.build();

        let run_key = StableSymbolKey::new(
            "module_a.rs",
            "service::run",
            EntityKind::Function,
            "fn run()",
        );
        let full_run = full_index
            .get_entity_id_by_symbol_key(&run_key)
            .expect("full build should register the run symbol");
        let hot_run = hot_index
            .get_entity_id_by_symbol_key(&run_key)
            .expect("hot update should keep the run symbol registered");

        // Both paths must resolve the caller's relation to the same internal
        // entity rather than dropping it as unresolved/external.
        let full_relations = full_index.get_resolved_relations_by_file("caller.rs");
        let hot_relations = hot_index.get_resolved_relations_by_file("caller.rs");
        assert_eq!(full_relations.len(), 1, "full build resolves one relation");
        assert_eq!(hot_relations.len(), 1, "hot update resolves one relation");
        assert_eq!(
            full_relations[0].1[0].callee_id,
            Some(full_run),
            "full build resolves run internally"
        );
        assert_eq!(
            hot_relations[0].1[0].callee_id,
            Some(hot_run),
            "hot update resolves run internally"
        );
        assert_eq!(
            full_relations[0].1[0].relation_type,
            hot_relations[0].1[0].relation_type
        );
    }

    #[test]
    fn test_add_file() {
        let builder = IndexBuilder::new();
        builder.add_file(FileInfo {
            id: "file_1".to_string(),
            path: "test.c".to_string(),
            language: "c".to_string(),
            file_hash: String::new(),
            file_size: 0,
            modified_time: 0,
            parse_status: cce_types::entity::ParseStatus::Pending,
            parse_errors: Vec::new(),
            parse_version: 0,
            entity_count: 0,
            relation_count: 0,
            export_count: 0,
            import_count: 0,
            depends_on: Vec::new(),
        });
        let index = builder.build();
        assert_eq!(index.file_count(), 1);
    }

    #[test]
    fn test_add_import_table() {
        let builder = IndexBuilder::new();
        use cce_types::{ImportKind, ImportTarget, TargetKind};

        let import_table = ImportTable {
            file_id: "file_1".to_string(),
            source_stats: cce_types::import::ImportSourceStats::default(),
            standardized_imports: vec![
                cce_types::import::StandardizedImport::new(ImportKind::SymbolImport, "./utils")
                    .with_target(ImportTarget::new("foo", TargetKind::Function)),
            ],
        };

        builder.add_import_table("file_1".to_string(), import_table);
        let index = builder.build();
        assert_eq!(index.import_count(), 1);
    }

    #[test]
    fn test_add_exports() {
        let builder = IndexBuilder::new();
        builder.add_exports(
            "file_1".to_string(),
            vec![
                ExportInfo {
                    function_id: cce_types::EntityId(1),
                    function_name: "foo".to_string(),
                    export_type: ExportType::Named,
                },
                ExportInfo {
                    function_id: cce_types::EntityId(2),
                    function_name: "default".to_string(),
                    export_type: ExportType::Default,
                },
            ],
        );

        let index = builder.build();
        let exports = index.get_exports("file_1").expect("Should have exports");
        assert_eq!(exports.len(), 2);
    }

    #[test]
    fn test_fluent_interface() {
        let builder = IndexBuilder::new();
        builder
            .add_function(EntityId(0), create_test_function_entity(0, "test"))
            .add_resolved_relation(ResolvedRelation {
                caller: EntityId(0),
                callee_id: Some(EntityId(1)),
                callee_name: "other".to_string(),
                relation_type: RelationType::DirectCall,
                span: cce_types::Span::default(),
                is_external: false,
                external_type: None,
                callee_symbol: None,
                stdlib_category: None,
                owner_type: None,
                call_context: CallContext::Direct,
            })
            .add_file(FileInfo {
                id: "file_1".to_string(),
                path: "test.c".to_string(),
                language: "c".to_string(),
                ..Default::default()
            });
        let index = builder.build();

        assert_eq!(index.function_count(), 1);
        assert_eq!(index.call_count(), 1);
        assert_eq!(index.file_count(), 1);
    }

    #[test]
    fn test_is_empty() {
        let builder = IndexBuilder::new();
        assert!(builder.is_empty());

        let builder = IndexBuilder::new();
        builder.add_function(EntityId(0), create_test_function_entity(0, "test"));
        assert!(!builder.is_empty());
    }

    #[test]
    fn test_index_builder_filter_stdlib_calls_default() {
        let _builder = IndexBuilder::new();
        // By default, filtering should be enabled
    }

    #[test]
    fn test_index_builder_set_filter_stdlib_calls() {
        let mut builder = IndexBuilder::new();
        builder.set_filter_stdlib_calls(false);
        builder.set_filter_stdlib_calls(true);
    }

    #[test]
    fn test_builder_and_index_share_dependency_graph() {
        let builder = IndexBuilder::new();
        builder
            .dependency_graph()
            .add_dependency("caller.rs", "target.rs");

        assert_eq!(
            builder.index().dependency_graph.get_dependents("target.rs"),
            vec!["caller.rs"]
        );
    }

    #[test]
    fn reexport_aliases_survive_extraction_wire_and_resolution() {
        use crate::symbol_table::ResolutionContext;
        use cce_parser::parser::ParseCoordinator;

        let mut coordinator = ParseCoordinator::new();
        let source_a = coordinator
            .parse("/project/src/a.rs", "pub fn item() -> i32 { 1 }")
            .expect("a.rs parses");
        let source_lib = coordinator
            .parse("/project/src/lib.rs", "pub use crate::a::item as Renamed;")
            .expect("lib.rs parses");

        // The coordinator's AST-based extraction produces the re-export record.
        assert_eq!(source_lib.reexports.len(), 1);
        let record = &source_lib.reexports[0];
        assert_eq!(record.local_name, "Renamed");
        assert_eq!(record.original_module, "crate::a");
        assert_eq!(record.original_name, "item");

        let builder = SymbolTableBuilder::new(PathBuf::from("/project"));
        let symbols = builder.build(&[&source_a, &source_lib]);

        let package = symbols
            .get_package("project")
            .expect("default package present");
        let lib_module = package
            .get_module("/project/src/lib.rs")
            .expect("lib module present");
        let binding = lib_module.get_reexport("Renamed").expect("wired reexport");
        assert_eq!(binding.original_module, "crate::a");
        assert_eq!(binding.original_name, "item");
        assert!(binding.resolved_symbol.is_none());

        // "Renamed" is not in any simple-name index, so Level 3 re-export
        // resolution must bridge it to the original symbol in a.rs.
        let symbol = symbols
            .resolve_enhanced(
                "Renamed",
                &ResolutionContext {
                    file_path: "/project/src/lib.rs".to_string(),
                    module_path: Vec::new(),
                    scope_chain: Vec::new(),
                },
            )
            .expect("re-export alias resolves to the original symbol");
        assert_eq!(symbol.name(), "item");
        assert_eq!(&*symbol.metadata.location.file_path, "/project/src/a.rs");
    }
}
