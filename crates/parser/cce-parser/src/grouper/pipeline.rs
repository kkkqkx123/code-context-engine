//! PreprocessingPipeline - Core implementation
//!
//! This pipeline handles entity preprocessing between Parser and AstToNl.
//! It improves natural language conversion quality by:
//!
//! 1. Merging simple repeated function calls
//! 2. Associating classes with their methods based on size
//! 3. Grouping test suites with their test cases

use std::collections::HashMap;

use cce_plugin::PluginCapability;
use cce_types::FILE_DOC_SENTINEL_ID;
use cce_types::entity::{Entity, EntityId, EntityKind, GroupedEntity, ParsedFile};
use cce_types::language::Language;
use cce_types::test_info::TestInfo;
use compact_str::CompactString;

use crate::plugin::PluginRegistry;

use super::builtin_stages::{
    assert_no_same_name_nested_groups, drop_import_only_groups, resolve_group_hierarchy,
    should_skip_low_value_entity,
};
use super::plugin_grouping::{
    apply_plugin_post_group, apply_stdlib_heuristics, inject_plugin_entities,
    try_plugin_group_override,
};
use super::processors::{
    CallMerger, ClassMethodProcessor, FunctionMemberProcessor, SmallFragmentMerger,
    TestSuiteProcessor,
};
use super::recognizers::test_suite::TestSuiteDetector;
use super::types::{EntityGroup, GroupType, ProcessingResult, ProcessingStats};
use cce_config::NestProcessorConfig;

/// PreprocessingPipeline - Coordinates entity preprocessing for better NL conversion
///
/// This pipeline sits between the Parser and AstToNl stages, optimizing
/// the entity structure before natural language conversion.
///
/// # Pipeline Stages
///
/// 1. **Call Merging** - Merge repeated simple calls
/// 2. **Test Suite Grouping** - Group test suites with test cases
/// 3. **Class-Method Association** - Associate small classes with methods
/// 4. **Source Generation** - Generate combined source for groups
///
/// # Example
///
/// ```ignore
/// use cce_parser::grouper::PreprocessingPipeline;
/// use cce_parser::types::entity::ParsedFile;
///
/// let pipeline = PreprocessingPipeline::new();
/// // let result = pipeline.process(&parsed_file);
/// //
/// // for group in result.groups {
/// //     println!("Group: {} ({:?})", group.name, group.group_type);
/// // }
/// ```
pub struct PreprocessingPipeline {
    /// Configuration
    config: NestProcessorConfig,

    /// Class-method processor
    class_method_processor: ClassMethodProcessor,

    /// Call merger
    call_merger: CallMerger,

    /// Test suite processor
    test_suite_processor: TestSuiteProcessor,

    /// Function member processor (groups function internals)
    function_member_processor: FunctionMemberProcessor,

    /// Small fragment merger (merges adjacent small standalone groups)
    small_fragment_merger: SmallFragmentMerger,

    /// Plugin registry for pattern detection extensions
    plugin_registry: Option<std::sync::Arc<PluginRegistry>>,
}

impl Default for PreprocessingPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl PreprocessingPipeline {
    /// Create a new processor with default configuration
    pub fn new() -> Self {
        Self::with_config(NestProcessorConfig::default())
    }

    /// Create a new processor with custom configuration
    pub fn with_config(config: NestProcessorConfig) -> Self {
        Self {
            class_method_processor: ClassMethodProcessor::new(&config.getter_setter),
            call_merger: CallMerger::new(),
            test_suite_processor: TestSuiteProcessor::new(),
            function_member_processor: FunctionMemberProcessor::new(),
            small_fragment_merger: SmallFragmentMerger::with_config(&config),
            config,
            plugin_registry: None,
        }
    }

    /// Set plugin registry
    pub fn with_plugin_registry(mut self, plugin_registry: std::sync::Arc<PluginRegistry>) -> Self {
        self.plugin_registry = Some(plugin_registry);
        self
    }

    /// Process a parsed file and return entity groups
    ///
    /// This is the main entry point for the processor.
    ///
    /// Three-tier `GroupOverride` order: override-tier plugins (priority
    /// ≥ 0) → built-in grouping → below-builtin fallback plugins (negative
    /// priority, only when built-in grouping produced no groups).
    pub fn process(&self, parsed_file: &ParsedFile) -> ProcessingResult {
        let stats = ProcessingStats {
            input_entities: parsed_file.entities.len(),
            ..Default::default()
        };

        let group_override_segments = self.plugin_registry.as_ref().map(|registry| {
            let language = parsed_file.language.to_string();
            registry.get_override_plugins(
                PluginCapability::GroupOverride,
                Some(&parsed_file.path),
                Some(&language),
            )
        });

        // Step 0a: Group full-override tier (GroupOverride capability, priority ≥ 0).
        // A plugin providing `group()` fully replaces the built-in grouping
        // stages. EntityExtract injection, the post-group hook chain, and
        // combined-source generation still run on the plugin's groups so the
        // downstream converter/chunker remain agnostic.
        if let Some((above, _)) = &group_override_segments {
            if let Some(groups) =
                try_plugin_group_override(&self.plugin_registry, parsed_file, above)
            {
                return self.finish_plugin_groups(groups, parsed_file, stats);
            }
        }

        // Steps 1+ (built-in grouping pipeline).
        let result = self.builtin_process(parsed_file, stats);

        // Step 0b: below-builtin fallback tier (negative priority) — only
        // when the built-in grouping produced no groups at all.
        if result.groups.is_empty() {
            if let Some((_, below)) = &group_override_segments {
                if let Some(groups) =
                    try_plugin_group_override(&self.plugin_registry, parsed_file, below)
                {
                    return self.finish_plugin_groups(groups, parsed_file, result.stats);
                }
            }
        }
        let mut result = result;
        if drop_import_only_groups(&mut result.groups) > 0 {
            result.stats.output_groups = result.groups.len();
        }
        result
    }

    /// Finish plugin-provided groups: EntityExtract injection, import-only
    /// group removal, post-group hook chain and combined-source generation,
    /// then package the result.
    fn finish_plugin_groups(
        &self,
        mut groups: Vec<EntityGroup>,
        parsed_file: &ParsedFile,
        mut stats: ProcessingStats,
    ) -> ProcessingResult {
        if self.plugin_registry.is_some() {
            inject_plugin_entities(&self.plugin_registry, &mut groups, parsed_file);
            groups = apply_plugin_post_group(&self.plugin_registry, groups, parsed_file);
        }
        drop_import_only_groups(&mut groups);
        let file_source = parsed_file.source.as_ref();
        for group in &mut groups {
            if group.group_type == GroupType::FileDocumentation {
                continue;
            }
            group.calculate_combined_span();
            group.generate_combined_source(file_source);
        }
        stats.output_groups = groups.len();
        ProcessingResult {
            groups,
            entity_meta: HashMap::new(),
            behavior: parsed_file.behavior.clone(),
            control_flow: parsed_file.control_flow.clone(),
            stats,
        }
    }

    /// Built-in grouping pipeline (all steps after the plugin override tier).
    fn builtin_process(
        &self,
        parsed_file: &ParsedFile,
        mut stats: ProcessingStats,
    ) -> ProcessingResult {
        // Step 0: `LangHeuristics` stdlib classification for custom-language
        // files (entities whose stdlib status is unknown to the built-in
        // detectors; plugin answers are in priority order, first wins).
        let mut entities = apply_stdlib_heuristics(&self.plugin_registry, parsed_file);

        // Step 1: Merge simple repeated calls (if enabled)
        if self.config.enable_call_merging {
            let (merged, merge_count) = self.merge_simple_calls(&entities, parsed_file);
            stats.merged_calls = merge_count;

            entities = merged;
        }

        // Step 2: Process test suites and test cases (if enabled)
        // Test suite processing happens before class-method to ensure test entities
        // are properly grouped even if they contain class definitions
        let mut groups = if self.config.enable_test_entity_grouping {
            let (groups, test_assoc_count) = self.process_test_suites(&entities, parsed_file);

            // Note: test_assoc_count could be tracked in ProcessingStats if needed
            let _ = test_assoc_count;
            groups
        } else {
            Vec::new()
        };

        // Step 3: Process class-methods for remaining entities (if enabled)
        // Extract entities that haven't been grouped yet (non-test entities)
        let processed_ids: std::collections::HashSet<_> =
            groups.iter().flat_map(|g| g.all_entity_ids()).collect();

        let remaining_entities: Vec<_> = entities
            .iter()
            .filter(|e| !processed_ids.contains(&e.id))
            .cloned()
            .collect();

        // Filter out low-value entities before any downstream processing.
        // This handles phantom nodes (zero/negative width) from tree-sitter
        // error recovery, zero-variant enums, and stub functions.
        let filtered_entities: Vec<_> = remaining_entities
            .into_iter()
            .filter(|e| !should_skip_low_value_entity(e))
            .collect();

        if self.config.enable_class_method_association && !filtered_entities.is_empty() {
            // Identify impl block entities (inherent and trait impls) and their
            // method children. Methods that are children of an impl block should
            // NOT be processed by ClassMethodProcessor — they'll be added as
            // members of the impl block group instead.
            let impl_children_ids: std::collections::HashSet<_> = filtered_entities
                .iter()
                .filter(|e| e.kind.is_impl_block())
                .flat_map(|e| e.children.iter())
                .copied()
                .collect();

            // Methods of inherent impl blocks are excluded from
            // class-method association; they are grouped with the impl.
            let entities_for_class_processing: Vec<_> = filtered_entities
                .iter()
                .filter(|e| !impl_children_ids.contains(&e.id))
                .cloned()
                .collect();

            let (class_groups, assoc_count) =
                self.process_class_methods(&entities_for_class_processing, parsed_file);
            stats.class_method_associations = assoc_count;

            groups.extend(class_groups);

            // Handle remaining entities not processed by ClassMethodProcessor.
            // Impl block entities (inherent and trait impls) are excluded from
            // ClassMethodProcessor but need their methods as members for proper
            // export output.
            let class_processed_ids: std::collections::HashSet<_> =
                groups.iter().flat_map(|g| g.all_entity_ids()).collect();
            let still_remaining: Vec<_> = filtered_entities
                .iter()
                .filter(|e| !class_processed_ids.contains(&e.id))
                .filter(|e| !impl_children_ids.contains(&e.id))
                .cloned()
                .collect();

            if !still_remaining.is_empty() {
                // Build a name→EntityId map to detect if an impl targets a TypeAlias.
                // TypeAlias entities with inherent methods need each method as a separate
                // group to avoid merging distinct methods into one chunk.
                let typealias_names: std::collections::HashSet<&str> = entities
                    .iter()
                    .filter(|e| e.kind == EntityKind::TypeAlias)
                    .map(|e| e.name.as_str())
                    .collect();

                let language = parsed_file.language;
                for entity in still_remaining {
                    if entity.kind.is_impl_block() {
                        let methods: Vec<Entity> = entity
                            .children
                            .iter()
                            .filter_map(|&child_id| {
                                entities.iter().find(|e| e.id == child_id).cloned()
                            })
                            .filter(|e| e.kind.is_function_like())
                            .collect();

                        if !methods.is_empty() {
                            if typealias_names.contains(entity.name.as_str()) {
                                // TypeAlias target: each method gets its own group
                                // so the converter produces per-method chunks.
                                for method in methods {
                                    let mut group = EntityGroup::from_entity(method, language);
                                    group.parent_group_id =
                                        Some(CompactString::from(entity.name.as_str()));
                                    groups.push(group);
                                }
                            } else {
                                let group =
                                    EntityGroup::impl_block_with_methods(entity, methods, language);
                                groups.push(group);
                            }
                        } else {
                            groups.push(EntityGroup::from_entity(entity, language));
                        }
                    } else {
                        groups.push(EntityGroup::from_entity(entity, language));
                    }
                }
            }
        } else if !filtered_entities.is_empty() {
            let language = parsed_file.language;
            for entity in filtered_entities {
                groups.push(EntityGroup::from_entity(entity, language));
            }
        }

        // Inject supplementary plugin entities (EntityExtract).
        // Regex-based extractors (e.g. Flask route decorators) complement the
        // tree-sitter entity stream with framework-specific entities. Each
        // becomes a standalone group so it flows through the rest of the
        // pipeline (grouping → NL generation → chunking).
        if self.plugin_registry.is_some() {
            inject_plugin_entities(&self.plugin_registry, &mut groups, parsed_file);
        }

        // Process function member grouping (if enabled)
        // Groups function-level entities (macros, closures, statements) as members
        if self.config.enable_function_member_grouping {
            let func_member_count = self.function_member_processor.process(
                &mut groups,
                &parsed_file.entities,
                parsed_file.language,
            );
            if func_member_count > 0 {}
        }

        // Resolve group hierarchy (parent-child relationships)
        // Links module groups with their child groups based on entity parent/children relationships.
        if self.config.enable_group_hierarchy {
            let link_count = resolve_group_hierarchy(&mut groups, &parsed_file.entities);
            if link_count > 0 {}
        }

        // Step 5: Merge small adjacent standalone fragments (if enabled).
        // Test info is annotated BEFORE merging so the merger can respect
        // test boundaries (a test fragment must never be merged into a
        // production fragment group, and vice versa).
        if self.config.enable_small_fragment_merging {
            self.annotate_groups_test_info(&mut groups, parsed_file, &entities);
            let file_source = parsed_file.source.as_ref();
            let merge_count = self.small_fragment_merger.process(&mut groups, file_source);
            if merge_count > 0 {}
        }

        // Re-run test-info annotation after fragment merging so freshly
        // created MergedFragments groups are covered.
        //
        // AST-level detection (attribute adjacency + constrained conventions)
        // runs on the (call-merged) entity list and is merged per group with
        // the file-level path rule. The marker is orthogonal to grouping:
        // groups keep their `group_type` regardless of test status.
        self.annotate_groups_test_info(&mut groups, parsed_file, &entities);

        // Inject call paths from raw_relations into group metadata
        if !parsed_file.raw_relations.is_empty() {
            let mut call_paths_map: HashMap<EntityId, Vec<String>> = HashMap::new();
            for rel in &parsed_file.raw_relations {
                call_paths_map
                    .entry(rel.src)
                    .or_default()
                    .push(rel.dst_name.clone());
            }
            for paths in call_paths_map.values_mut() {
                paths.sort();
                paths.dedup();
            }

            for group in &mut groups {
                if let Some(header_id) = group.header_id {
                    if let Some(paths) = call_paths_map.get(&header_id) {
                        if !paths.is_empty() {
                            if let Some(ref mut header) = group.header {
                                header
                                    .metadata
                                    .insert("call_paths".to_string(), paths.join(", "));
                            }
                        }
                    }
                }
                for member in &mut group.members {
                    if let Some(paths) = call_paths_map.get(&member.id) {
                        if !paths.is_empty() {
                            member
                                .metadata
                                .insert("call_paths".to_string(), paths.join(", "));
                        }
                    }
                }
            }
        }

        // Create FileDocumentation group if file_doc_comment exists
        if let Some(ref doc_comment) = parsed_file.file_doc_comment {
            let trimmed = doc_comment.trim();
            if !trimmed.is_empty() {
                let file_name = cce_types::path::file_name_str(&parsed_file.path);
                // Use the full relative path as the group ID base so that
                // different files sharing a name (e.g. multiple `mod.rs`)
                // produce unique document IDs.
                let file_doc_id = cce_types::path::group_id_base(&parsed_file.path);
                let mut doc_group = EntityGroup::new(
                    format!("file_doc_{}", file_doc_id),
                    GroupType::FileDocumentation,
                );
                doc_group.kind = EntityKind::Module;
                doc_group.name = CompactString::from(file_name);
                doc_group.language = parsed_file.language;
                // Sentinel ID shared with the comment processor dispatch
                let sentinel_id = FILE_DOC_SENTINEL_ID;
                if let Some(span) = parsed_file.file_doc_span {
                    doc_group.span = span;
                    doc_group.entity_spans.insert(sentinel_id, span);
                } else {
                    tracing::warn!(
                        file_path = %parsed_file.path,
                        "File documentation has no source span"
                    );
                }
                doc_group.header = Some(GroupedEntity {
                    id: sentinel_id,
                    name: file_name.to_string(),
                    kind: EntityKind::Module,
                    doc_comment: Some(trimmed.to_string()),
                    ..Default::default()
                });
                groups.push(doc_group);
            }
        }

        // Plugin post-group hook (Group capability). Runs after built-in
        // grouping (including the file-documentation group) and before
        // combined-source generation. Plugins may merge/split/rename groups or
        // annotate metadata.
        if self.plugin_registry.is_some() {
            groups = apply_plugin_post_group(&self.plugin_registry, groups, parsed_file);
        }

        // Step 6: Generate combined source for each group
        let file_source = parsed_file.source.as_ref();
        for group in &mut groups {
            // FileDocumentation groups use the doc comment as their source,
            // not a source-code span. Their content is purely documentation.
            if group.group_type == GroupType::FileDocumentation {
                if let Some(ref header) = group.header {
                    if let Some(ref doc) = header.doc_comment {
                        group.combined_source = Some(std::sync::Arc::from(doc.clone()));
                    }
                }
                continue;
            }
            group.calculate_combined_span();
            if !group.generate_combined_source(file_source) {
                tracing::warn!(
                    "Failed to generate combined source for group {}",
                    group.group_id
                );
            }
        }

        assert_no_same_name_nested_groups(&groups, &parsed_file.path);

        // Calculate final statistics
        stats.output_groups = groups.len();
        stats.standalone_entities = groups
            .iter()
            .filter(|g| g.group_type == GroupType::Standalone)
            .count();

        ProcessingResult {
            groups,
            entity_meta: HashMap::new(),
            behavior: parsed_file.behavior.clone(),
            control_flow: parsed_file.control_flow.clone(),
            stats,
        }
    }

    /// Process a parsed file and return only the entity groups
    ///
    /// This is a convenience method that discards processing metadata
    /// (statistics, entity_meta) and returns only the groups.
    /// Use this when you only need the grouped entities for downstream processing.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let groups = pipeline.entity_groups(&parsed_file);
    /// // groups: Vec<EntityGroup>
    /// ```
    pub fn entity_groups(&self, parsed_file: &ParsedFile) -> Vec<EntityGroup> {
        self.process(parsed_file).groups
    }

    /// Process entities directly (without ParsedFile)
    ///
    /// Useful when you only have entities without file context.
    ///
    /// **Limitations**: This method lacks `local_calls` information from the original file,
    /// so call merging functionality will be disabled. For full functionality,
    /// use [`process`](Self::process) with a [`ParsedFile`] instead.
    ///
    /// # Arguments
    /// * `entities` - Slice of entities to process
    /// * `language` - Programming language of the entities
    ///
    /// # Returns
    /// * Processing result with entity groups
    pub fn process_entities(&self, entities: &[Entity], language: Language) -> ProcessingResult {
        let mut stats = ProcessingStats {
            input_entities: entities.len(),
            ..Default::default()
        };

        // Create a temporary ParsedFile for components (empty source, no local_calls)
        let temp_parsed_file = ParsedFile::new(language, String::new(), "");

        // Note: Call merging is skipped in this mode because we lack local_calls information.
        // The entities are used as-is.
        let entities = entities.to_vec();

        // Step 1: Process test suites (test entity grouping works)
        let mut groups = if self.config.enable_test_entity_grouping {
            let (groups, test_assoc_count) = self.process_test_suites(&entities, &temp_parsed_file);
            let _ = test_assoc_count;
            groups
        } else {
            Vec::new()
        };

        // Step 2: Process class-methods for remaining entities
        let processed_ids: std::collections::HashSet<_> =
            groups.iter().flat_map(|g| g.all_entity_ids()).collect();

        let remaining_entities: Vec<_> = entities
            .iter()
            .filter(|e| !processed_ids.contains(&e.id))
            .cloned()
            .collect();

        let filtered_entities: Vec<_> = remaining_entities
            .into_iter()
            .filter(|e| !should_skip_low_value_entity(e))
            .collect();

        if self.config.enable_class_method_association && !filtered_entities.is_empty() {
            let (class_groups, assoc_count) =
                self.process_class_methods(&filtered_entities, &temp_parsed_file);
            stats.class_method_associations = assoc_count;
            groups.extend(class_groups);
        } else if !filtered_entities.is_empty() {
            for entity in filtered_entities {
                groups.push(EntityGroup::from_entity(entity, language));
            }
        }

        // Resolve group hierarchy (parent/child links across groups).
        if self.config.enable_group_hierarchy {
            let link_count = resolve_group_hierarchy(&mut groups, &entities);
            if link_count > 0 {}
        }

        // Step 4: Annotate test info (AST detection + file-level path rule)
        self.annotate_groups_test_info(&mut groups, &temp_parsed_file, &entities);

        stats.output_groups = groups.len();
        stats.standalone_entities = groups
            .iter()
            .filter(|g| g.group_type == GroupType::Standalone)
            .count();

        let mut result = ProcessingResult {
            groups,
            entity_meta: HashMap::new(),
            behavior: temp_parsed_file.behavior.clone(),
            control_flow: temp_parsed_file.control_flow.clone(),
            stats,
        };
        if drop_import_only_groups(&mut result.groups) > 0 {
            result.stats.output_groups = result.groups.len();
        }
        result
    }

    /// Merge simple repeated calls
    fn merge_simple_calls(
        &self,
        entities: &[Entity],
        parsed_file: &ParsedFile,
    ) -> (Vec<Entity>, usize) {
        use crate::grouper::context::FileProcessingContext;
        let ctx = FileProcessingContext::new(entities, parsed_file, &self.config);
        self.call_merger.merge(ctx)
    }

    /// Process class methods
    fn process_class_methods(
        &self,
        entities: &[Entity],
        parsed_file: &ParsedFile,
    ) -> (Vec<EntityGroup>, usize) {
        use crate::grouper::context::FileProcessingContext;
        let ctx = FileProcessingContext::new(entities, parsed_file, &self.config);
        self.class_method_processor.process(ctx)
    }

    /// Process test suites
    fn process_test_suites(
        &self,
        entities: &[Entity],
        parsed_file: &ParsedFile,
    ) -> (Vec<EntityGroup>, usize) {
        use crate::grouper::context::FileProcessingContext;
        let ctx = FileProcessingContext::new(entities, parsed_file, &self.config);
        self.test_suite_processor.process(ctx)
    }

    /// `LangHeuristics` test-file detection.
    ///
    /// Consulted only when the built-in path rule produced no signal
    /// (`TestInfo::unknown`); a plugin `Some(true)` marks the file as a test
    /// file (path granularity), `Some(false)` keeps the built-in result.
    fn plugin_test_file_info(&self, parsed_file: &ParsedFile) -> TestInfo {
        if let Some(registry) = &self.plugin_registry {
            if crate::plugin::heuristics::is_test_file(
                registry,
                &parsed_file.path,
                &parsed_file.source,
            ) == Some(true)
            {
                return TestInfo::test_path();
            }
        }
        TestInfo::unknown()
    }

    /// Annotate `test_info` on every group (including nested groups).
    ///
    /// Merge order:
    /// 1. File-level path rule (Medium) applies to all groups of the file.
    /// 2. Entity-level AST detection (High) overrides path signals.
    /// 3. Any member marked `Test` makes the whole group `Test`.
    fn annotate_groups_test_info(
        &self,
        groups: &mut [EntityGroup],
        parsed_file: &ParsedFile,
        entities: &[Entity],
    ) {
        use crate::grouper::context::FileProcessingContext;

        let mut file_info = TestInfo::from_path(Some(&parsed_file.language), &parsed_file.path);
        // `LangHeuristics` hook: plugins may classify files the path rule
        // leaves unknown (e.g. custom-language test files).
        if file_info.is_unknown() {
            let plugin_info = self.plugin_test_file_info(parsed_file);
            if !plugin_info.is_unknown() {
                file_info = plugin_info;
            }
        }
        let ctx = FileProcessingContext::new(entities, parsed_file, &self.config);
        let entity_infos = TestSuiteDetector::new().detect_test_info(&ctx);
        let mut annotated = 0;

        for group in groups.iter_mut() {
            let info = Self::merged_group_test_info(group, &file_info, &entity_infos);
            if info != TestInfo::unknown() {
                annotated += 1;
            }
            group.test_info = info;
            for nested in group.nested_groups.iter_mut() {
                let nested_info = Self::merged_group_test_info(nested, &file_info, &entity_infos);
                nested.test_info = nested_info;
            }
        }

        // Propagate test_info from child groups to parent groups.
        // When a container entity (e.g. namespace) creates a standalone group
        // but its child entities are in test-marked groups, the container group
        // should inherit the test status.
        let entity_map: HashMap<EntityId, &Entity> = entities.iter().map(|e| (e.id, e)).collect();

        // Build map: entity_id -> group index (for groups that are test-marked)
        let mut test_group_by_entity: HashMap<EntityId, usize> = HashMap::new();
        for (idx, group) in groups.iter().enumerate() {
            if group.test_info.is_test() {
                for entity_id in group.all_entity_ids() {
                    test_group_by_entity.insert(entity_id, idx);
                }
            }
        }

        // Propagate via entity children
        for group in groups.iter_mut() {
            if !group.test_info.is_unknown() {
                continue;
            }
            let Some(header_id) = group.header_id else {
                continue;
            };
            let Some(entity) = entity_map.get(&header_id) else {
                continue;
            };

            // Check if any child entity belongs to a test-marked group
            for child_id in &entity.children {
                if test_group_by_entity.contains_key(child_id) {
                    group.test_info = TestInfo::test_ast();
                    annotated += 1;
                    break;
                }
            }
        }

        if annotated > 0 {}
    }

    /// Merge file-level info with every entity-level marker of a group.
    fn merged_group_test_info(
        group: &EntityGroup,
        file_info: &TestInfo,
        entity_infos: &std::collections::HashMap<EntityId, TestInfo>,
    ) -> TestInfo {
        let mut info = *file_info;
        for entity_id in group.all_entity_ids() {
            if let Some(entity_info) = entity_infos.get(&entity_id) {
                info = info.merge(entity_info);
            }
        }
        info
    }
}

/// Builder for PreprocessingPipeline
#[derive(Default)]
pub struct PipelineBuilder {
    config: NestProcessorConfig,
}

impl PipelineBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the configuration
    pub fn config(mut self, config: NestProcessorConfig) -> Self {
        self.config = config;
        self
    }

    /// Build the pipeline
    pub fn build(self) -> PreprocessingPipeline {
        PreprocessingPipeline::with_config(self.config)
    }
}

#[cfg(test)]
mod tests;
