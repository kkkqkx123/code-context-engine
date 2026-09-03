use super::*;
use cce_types::Span;
use cce_types::entity::{EntityId, EntityKind};

#[test]
fn test_skip_zero_width_entity() {
    let entity = Entity::new(
        EntityId(1),
        EntityKind::Enum,
        "Void".to_string(),
        Span::new(100, 100, 10, 0, 10, 0),
    );
    assert!(
        should_skip_low_value_entity(&entity),
        "zero-width entity (end_byte == start_byte) should be skipped"
    );
}

#[test]
fn test_skip_negative_width_entity() {
    let entity = Entity::new(
        EntityId(2),
        EntityKind::Function,
        "_dummy".to_string(),
        Span::new(200, 150, 5, 0, 3, 0),
    );
    assert!(
        should_skip_low_value_entity(&entity),
        "negative-width entity (end_byte < start_byte) should be skipped"
    );
}

#[test]
fn test_skip_reversed_row_entity() {
    // Valid bytes but reversed rows — tree-sitter phantom pattern
    let entity = Entity::new(
        EntityId(5),
        EntityKind::Function,
        "_dummy".to_string(),
        Span::new(100, 115, 497, 0, 496, 0),
    );
    assert!(
        should_skip_low_value_entity(&entity),
        "entity with valid bytes but end_row < start_row should be skipped"
    );
}

#[test]
fn test_keep_valid_row_entity() {
    let entity = Entity::new(
        EntityId(6),
        EntityKind::Function,
        "real_func".to_string(),
        Span::new(100, 150, 5, 0, 6, 0),
    );
    assert!(
        !should_skip_low_value_entity(&entity),
        "entity with consistent positions should not be skipped"
    );
}

#[test]
fn test_keep_valid_entity() {
    let entity = Entity::new(
        EntityId(3),
        EntityKind::Function,
        "real_func".to_string(),
        Span::new(0, 200, 0, 0, 5, 0),
    );
    assert!(
        !should_skip_low_value_entity(&entity),
        "valid entity with positive span should not be skipped"
    );
}

#[test]
fn test_pipeline_creation() {
    let pipeline = PreprocessingPipeline::new();
    assert!(pipeline.config.enable_class_method_association);
    assert!(pipeline.config.enable_call_merging);
}

#[test]
fn test_pipeline_with_custom_config() {
    let config = NestProcessorConfig {
        small_class_threshold: 50,
        ..Default::default()
    };
    let pipeline = PreprocessingPipeline::with_config(config);
    assert_eq!(pipeline.config.small_class_threshold, 50);
}

#[test]
fn test_process_empty_entities() {
    let pipeline = PreprocessingPipeline::new();
    let entities: Vec<Entity> = Vec::new();
    let result = pipeline.process_entities(&entities, Language::Rust);

    assert_eq!(result.groups.len(), 0);
    assert_eq!(result.stats.input_entities, 0);
    assert_eq!(result.stats.output_groups, 0);
}

#[test]
fn test_builder() {
    let pipeline = PipelineBuilder::new()
        .config(NestProcessorConfig::small_codebase())
        .build();

    assert_eq!(pipeline.config.small_class_threshold, 50);
}

// ── Plugin integration: post-group chain / override / entity inject ──

use cce_plugin::{CodePlugin, PluginBundle, PluginError, PluginMetadata};
use cce_types::plugin::{GroupPluginContext, PluginEntity};

type PostGroupFn =
    fn(Vec<EntityGroup>, GroupPluginContext) -> Result<Option<Vec<EntityGroup>>, PluginError>;
type GroupOverrideFn = fn(GroupPluginContext) -> Result<Option<Vec<EntityGroup>>, PluginError>;
type EntityExtractFn = fn(&str, &str, &str) -> Result<Option<Vec<PluginEntity>>, PluginError>;

/// Configurable `CodePlugin` test double for grouper pipeline tests.
struct MockPlugin {
    meta: PluginMetadata,
    post_group: Option<PostGroupFn>,
    group_override: Option<GroupOverrideFn>,
    extract: Option<EntityExtractFn>,
}

impl MockPlugin {
    fn with_id(id: &str, priority: i32) -> Self {
        Self {
            meta: PluginMetadata {
                id: id.to_string(),
                name: id.to_string(),
                version: "0.1.0".to_string(),
                priority,
                capabilities: Vec::new(),
                capability_priorities: std::collections::HashMap::new(),
                description: None,
            },
            post_group: None,
            group_override: None,
            extract: None,
        }
    }

    fn post(mut self, f: PostGroupFn) -> Self {
        self.post_group = Some(f);
        self
    }

    fn override_group(mut self, f: GroupOverrideFn) -> Self {
        self.group_override = Some(f);
        self
    }

    fn extractor(mut self, f: EntityExtractFn) -> Self {
        self.extract = Some(f);
        self
    }
}

impl CodePlugin for MockPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.meta
    }
    fn supports_group(&self) -> bool {
        self.post_group.is_some()
    }
    fn supports_group_override(&self) -> bool {
        self.group_override.is_some()
    }
    fn supports_extract(&self) -> bool {
        self.extract.is_some()
    }
    fn post_group(
        &self,
        groups: Vec<EntityGroup>,
        context: GroupPluginContext,
    ) -> Result<Option<Vec<EntityGroup>>, PluginError> {
        match self.post_group {
            Some(f) => f(groups, context),
            None => Ok(None),
        }
    }
    fn group(&self, context: GroupPluginContext) -> Result<Option<Vec<EntityGroup>>, PluginError> {
        match self.group_override {
            Some(f) => f(context),
            None => Ok(None),
        }
    }
    fn extract_entities(
        &self,
        content: &str,
        file_path: &str,
        language: &str,
    ) -> Result<Option<Vec<PluginEntity>>, PluginError> {
        match self.extract {
            Some(f) => f(content, file_path, language),
            None => Ok(None),
        }
    }
}

/// Build a parsed python file with one function entity.
fn parsed_python_file(source: &str, entity: Option<Entity>) -> ParsedFile {
    let mut parsed = ParsedFile::new(Language::Python, "app.py".to_string(), source);
    if let Some(entity) = entity {
        parsed.entities.push(entity);
    }
    parsed
}

fn function_entity(id: u64, name: &str) -> Entity {
    Entity::new(
        EntityId(id),
        EntityKind::Function,
        name.to_string(),
        Span::new(0, 100, 0, 0, 4, 0),
    )
}

fn register(registry: &mut PluginRegistry, plugin: MockPlugin, patterns: Option<Vec<&str>>) {
    let mut bundle = PluginBundle::new(std::sync::Arc::new(plugin));
    if let Some(patterns) = patterns {
        bundle = bundle.with_file_patterns(patterns.into_iter().map(|p| p.to_string()).collect());
    }
    registry.register_bundle(bundle);
}

#[test]
fn test_post_group_chain_runs_in_priority_order() {
    fn rename_a(
        mut groups: Vec<EntityGroup>,
        _ctx: GroupPluginContext,
    ) -> Result<Option<Vec<EntityGroup>>, PluginError> {
        for g in groups.iter_mut() {
            g.name = CompactString::from(format!("hook_a:{}", g.name));
        }
        Ok(Some(groups))
    }
    fn rename_b(
        mut groups: Vec<EntityGroup>,
        _ctx: GroupPluginContext,
    ) -> Result<Option<Vec<EntityGroup>>, PluginError> {
        for g in groups.iter_mut() {
            g.name = CompactString::from(format!("{}:hook_b", g.name));
        }
        Ok(Some(groups))
    }

    let mut registry = PluginRegistry::new();
    register(
        &mut registry,
        MockPlugin::with_id("a", 100).post(rename_a),
        None,
    );
    register(
        &mut registry,
        MockPlugin::with_id("b", 10).post(rename_b),
        None,
    );

    let pipeline = PreprocessingPipeline::new().with_plugin_registry(std::sync::Arc::new(registry));
    let parsed = parsed_python_file("def foo(): pass", Some(function_entity(0, "foo")));
    let result = pipeline.process(&parsed);

    // Chain semantics: plugin A runs first (higher priority), its output
    // feeds plugin B.
    assert_eq!(result.groups.len(), 1);
    assert_eq!(result.groups[0].name.as_str(), "hook_a:foo:hook_b");
}

#[test]
fn test_post_group_decline_keeps_builtin_groups() {
    fn decline(
        _groups: Vec<EntityGroup>,
        _ctx: GroupPluginContext,
    ) -> Result<Option<Vec<EntityGroup>>, PluginError> {
        Ok(None)
    }
    let mut registry = PluginRegistry::new();
    register(
        &mut registry,
        MockPlugin::with_id("decline", 10).post(decline),
        None,
    );

    let pipeline = PreprocessingPipeline::new().with_plugin_registry(std::sync::Arc::new(registry));
    let parsed = parsed_python_file("def foo(): pass", Some(function_entity(0, "foo")));
    let result = pipeline.process(&parsed);
    assert_eq!(result.groups.len(), 1);
    assert_eq!(result.groups[0].name.as_str(), "foo");
}

#[test]
fn test_post_group_error_keeps_builtin_groups() {
    fn fail(
        _groups: Vec<EntityGroup>,
        _ctx: GroupPluginContext,
    ) -> Result<Option<Vec<EntityGroup>>, PluginError> {
        Err(PluginError::LogicError("boom".to_string()))
    }
    let mut registry = PluginRegistry::new();
    register(
        &mut registry,
        MockPlugin::with_id("fail", 10).post(fail),
        None,
    );

    let pipeline = PreprocessingPipeline::new().with_plugin_registry(std::sync::Arc::new(registry));
    let parsed = parsed_python_file("def foo(): pass", Some(function_entity(0, "foo")));
    let result = pipeline.process(&parsed);
    assert_eq!(result.groups.len(), 1);
    assert_eq!(result.groups[0].name.as_str(), "foo");
}

#[test]
fn test_group_override_replaces_builtin_groups() {
    fn override_groups(ctx: GroupPluginContext) -> Result<Option<Vec<EntityGroup>>, PluginError> {
        let mut groups = Vec::new();
        for entity in &ctx.entities {
            let mut group =
                EntityGroup::new(format!("override_{}", entity.id), GroupType::Standalone);
            group.name = CompactString::from(entity.name.clone());
            groups.push(group);
        }
        if groups.is_empty() {
            return Ok(None);
        }
        Ok(Some(groups))
    }
    let mut registry = PluginRegistry::new();
    register(
        &mut registry,
        MockPlugin::with_id("ovr", 100).override_group(override_groups),
        Some(vec!["*.py"]),
    );

    let pipeline = PreprocessingPipeline::new().with_plugin_registry(std::sync::Arc::new(registry));
    let mut parsed = parsed_python_file(
        "def foo(): pass\ndef bar(): pass",
        Some(function_entity(0, "foo")),
    );
    parsed.entities.push(function_entity(1, "bar"));
    let result = pipeline.process(&parsed);

    assert!(!result.groups.is_empty(), "override must produce groups");
    assert!(
        result
            .groups
            .iter()
            .all(|g| g.group_id.starts_with("override_")),
        "all groups must originate from the override plugin"
    );
}

#[test]
fn test_group_override_fallback_tier_runs_only_when_builtin_empty() {
    fn override_groups(ctx: GroupPluginContext) -> Result<Option<Vec<EntityGroup>>, PluginError> {
        let mut group = EntityGroup::new("below_1".to_string(), GroupType::Standalone);
        group.name = CompactString::from("below-group");
        group
            .metadata
            .insert("below_tier".to_string(), "true".to_string());
        let _ = ctx;
        Ok(Some(vec![group]))
    }
    let mut registry = PluginRegistry::new();
    register(
        &mut registry,
        MockPlugin::with_id("below", -1).override_group(override_groups),
        None,
    );

    let pipeline = PreprocessingPipeline::new().with_plugin_registry(std::sync::Arc::new(registry));

    // Empty file: built-in grouping produces nothing → fallback tier runs.
    let empty = parsed_python_file("", None);
    let result = pipeline.process(&empty);
    assert_eq!(result.groups.len(), 1);
    assert_eq!(result.groups[0].group_id.as_str(), "below_1");
    assert_eq!(
        result.groups[0]
            .metadata
            .get("below_tier")
            .map(String::as_str),
        Some("true")
    );

    // File with content: built-in grouping produces groups → the below
    // tier plugin must not be consulted.
    let populated = parsed_python_file("def foo(): pass", Some(function_entity(0, "foo")));
    let result = pipeline.process(&populated);
    assert_eq!(result.groups.len(), 1);
    assert_eq!(result.groups[0].name.as_str(), "foo");
    assert!(
        !result.groups[0].metadata.contains_key("below_tier"),
        "below-tier plugin must stay silent when the built-in produced groups"
    );
}

#[test]
fn test_group_override_error_falls_back_to_builtin() {
    fn fail(_ctx: GroupPluginContext) -> Result<Option<Vec<EntityGroup>>, PluginError> {
        Err(PluginError::ExecutionFailed("broken".to_string()))
    }
    let mut registry = PluginRegistry::new();
    register(
        &mut registry,
        MockPlugin::with_id("ovr_err", 100).override_group(fail),
        None,
    );

    let pipeline = PreprocessingPipeline::new().with_plugin_registry(std::sync::Arc::new(registry));
    let parsed = parsed_python_file("def foo(): pass", Some(function_entity(0, "foo")));
    let result = pipeline.process(&parsed);
    assert_eq!(result.groups.len(), 1);
    assert_eq!(result.groups[0].name.as_str(), "foo");
}

#[test]
fn test_post_group_applies_to_override_groups() {
    fn override_groups(ctx: GroupPluginContext) -> Result<Option<Vec<EntityGroup>>, PluginError> {
        let mut group = EntityGroup::new("ovr_1".to_string(), GroupType::Standalone);
        group.name = CompactString::from("override-group");
        let _ = ctx;
        Ok(Some(vec![group]))
    }
    fn annotate(
        mut groups: Vec<EntityGroup>,
        _ctx: GroupPluginContext,
    ) -> Result<Option<Vec<EntityGroup>>, PluginError> {
        for g in groups.iter_mut() {
            g.metadata
                .insert("post_applied".to_string(), "yes".to_string());
        }
        Ok(Some(groups))
    }
    let mut registry = PluginRegistry::new();
    register(
        &mut registry,
        MockPlugin::with_id("ovr", 100).override_group(override_groups),
        None,
    );
    register(
        &mut registry,
        MockPlugin::with_id("post", 50).post(annotate),
        None,
    );

    let pipeline = PreprocessingPipeline::new().with_plugin_registry(std::sync::Arc::new(registry));
    let parsed = parsed_python_file("def foo(): pass", Some(function_entity(0, "foo")));
    let result = pipeline.process(&parsed);
    assert_eq!(result.groups.len(), 1);
    assert_eq!(
        result.groups[0]
            .metadata
            .get("post_applied")
            .map(String::as_str),
        Some("yes"),
        "post_group chain must also run on override-provided groups"
    );
}

#[test]
fn test_entity_extract_injects_standalone_groups() {
    fn extract(
        _content: &str,
        _path: &str,
        _language: &str,
    ) -> Result<Option<Vec<PluginEntity>>, PluginError> {
        Ok(Some(vec![
            PluginEntity::new("r1", "route", "/users"),
            PluginEntity::new("r2", "route", "/items"),
        ]))
    }
    let mut registry = PluginRegistry::new();
    register(
        &mut registry,
        MockPlugin::with_id("routes", 10).extractor(extract),
        Some(vec!["*.py"]),
    );

    let pipeline = PreprocessingPipeline::new().with_plugin_registry(std::sync::Arc::new(registry));
    let parsed = parsed_python_file("@app.route('/users')", Some(function_entity(0, "foo")));
    let result = pipeline.process(&parsed);

    let names: Vec<String> = result.groups.iter().map(|g| g.name.to_string()).collect();
    assert!(
        names.contains(&"/users".to_string()) && names.contains(&"/items".to_string()),
        "injected route entities must participate as standalone groups, got: {names:?}"
    );
    let injected = result
        .groups
        .iter()
        .find(|g| g.name.as_str() == "/users")
        .expect("route group present");
    assert_eq!(injected.kind, EntityKind::Function);
    assert!(injected.group_id.starts_with("plugin_routes_"));
}

#[test]
fn test_entity_extract_dedup_same_kind_name() {
    fn extract(
        _content: &str,
        _path: &str,
        _language: &str,
    ) -> Result<Option<Vec<PluginEntity>>, PluginError> {
        // Both plugins recognize the same Flask route.
        Ok(Some(vec![PluginEntity::new("r1", "route", "/users")]))
    }
    let mut registry = PluginRegistry::new();
    register(
        &mut registry,
        MockPlugin::with_id("routes_a", 10).extractor(extract),
        Some(vec!["*.py"]),
    );
    register(
        &mut registry,
        MockPlugin::with_id("routes_b", 5).extractor(extract),
        Some(vec!["*.py"]),
    );

    let pipeline = PreprocessingPipeline::new().with_plugin_registry(std::sync::Arc::new(registry));
    let parsed = parsed_python_file("@app.route('/users')", Some(function_entity(0, "foo")));
    let result = pipeline.process(&parsed);

    let injected: Vec<&EntityGroup> = result
        .groups
        .iter()
        .filter(|g| g.group_id.starts_with("plugin_"))
        .collect();
    assert_eq!(
        injected.len(),
        1,
        "the same (kind, name) extracted by two plugins must be injected once"
    );
}

#[test]
fn test_entity_extract_children_become_nested_groups() {
    fn extract(
        _content: &str,
        _path: &str,
        _language: &str,
    ) -> Result<Option<Vec<PluginEntity>>, PluginError> {
        let mut child = PluginEntity::new("c1", "field", "config");
        child.span = Some(Span::new(10, 20, 1, 0, 1, 10));
        let mut entity = PluginEntity::new("r1", "route", "/admin");
        entity.children = vec![child];
        Ok(Some(vec![entity]))
    }
    let mut registry = PluginRegistry::new();
    register(
        &mut registry,
        MockPlugin::with_id("routes", 10).extractor(extract),
        Some(vec!["*.py"]),
    );

    let pipeline = PreprocessingPipeline::new().with_plugin_registry(std::sync::Arc::new(registry));
    let parsed = parsed_python_file("@app.route('/admin')", Some(function_entity(0, "foo")));
    let result = pipeline.process(&parsed);

    let injected = result
        .groups
        .iter()
        .find(|g| g.name.as_str() == "/admin")
        .expect("parent group present");
    assert!(injected.has_significant_nested);
    assert_eq!(injected.nested_groups.len(), 1);
    assert_eq!(injected.nested_groups[0].name.as_str(), "config");
    assert!(
        injected.nested_groups[0]
            .group_id
            .starts_with("plugin_routes_")
    );
}

#[test]
fn test_entity_extract_error_is_tolerated() {
    fn fail(
        _content: &str,
        _path: &str,
        _language: &str,
    ) -> Result<Option<Vec<PluginEntity>>, PluginError> {
        Err(PluginError::ResourceError("unavailable".to_string()))
    }
    let mut registry = PluginRegistry::new();
    register(
        &mut registry,
        MockPlugin::with_id("broken", 10).extractor(fail),
        Some(vec!["*.py"]),
    );

    let pipeline = PreprocessingPipeline::new().with_plugin_registry(std::sync::Arc::new(registry));
    let parsed = parsed_python_file("def foo(): pass", Some(function_entity(0, "foo")));
    let result = pipeline.process(&parsed);
    assert_eq!(result.groups.len(), 1);
    assert_eq!(result.groups[0].name.as_str(), "foo");
}
