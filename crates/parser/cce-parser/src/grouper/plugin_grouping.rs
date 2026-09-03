//! Plugin grouping logic for entity preprocessing pipeline
//!
//! This module handles:
//! - Plugin override tier (GroupOverride capability)
//! - Plugin entity injection (EntityExtract capability)
//! - Post-group hook chain (Group capability)

use std::collections::HashMap;

use tracing::warn;

use crate::plugin::PluginRegistry;
use crate::plugin::convert::{
    allocate_plugin_ids, entity_to_plugin_entity, plugin_entity_to_group,
    raw_relation_to_plugin_relation,
};
use cce_plugin::PluginCapability;
use cce_types::entity::{Entity, EntityId, ParsedFile};
use cce_types::language::Language;
use cce_types::plugin::{GroupPluginContext, PluginEntity, PluginRelation};

use super::types::EntityGroup;

/// Inject supplementary entities extracted by `EntityExtract` plugins.
///
/// Each plugin entity becomes a standalone group (children become nested
/// groups), so it participates in the remaining grouper stages and the
/// downstream NL/chunking pipeline.
pub fn inject_plugin_entities(
    plugin_registry: &Option<std::sync::Arc<PluginRegistry>>,
    groups: &mut Vec<EntityGroup>,
    parsed_file: &ParsedFile,
) {
    let Some(registry) = plugin_registry else {
        return;
    };
    let language = parsed_file.language.to_string();
    let extractors = registry.get_plugins(
        PluginCapability::EntityExtract,
        Some(&parsed_file.path),
        Some(&language),
    );
    if extractors.is_empty() {
        return;
    }

    let max_existing = parsed_file
        .entities
        .iter()
        .map(|e| e.id.0)
        .max()
        .unwrap_or(0);

    let mut next_id = max_existing.saturating_add(1);
    let mut injected = 0usize;
    // Cross-plugin dedup: the same (kind, name) extracted by several
    // plugins (e.g. two plugins both recognizing Flask routes) is injected
    // only once to avoid duplicate groups in recall.
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();

    for plugin in extractors {
        match plugin.extract_entities(&parsed_file.source, &parsed_file.path, &language) {
            Ok(Some(entities)) if !entities.is_empty() => {
                let ids =
                    allocate_plugin_ids(parsed_file.entities.iter().map(|e| e.id), entities.len());
                for (entity, id) in entities.into_iter().zip(ids) {
                    let key = (entity.kind.clone(), entity.name.clone());
                    if !seen.insert(key) {
                        tracing::trace!(
                            plugin = %plugin.metadata().id,
                            file_path = %parsed_file.path,
                            kind = %entity.kind,
                            name = %entity.name,
                            "duplicate plugin entity skipped"
                        );
                        continue;
                    }
                    next_id = next_id.max(id.0 + 1);
                    let group_id = format!("plugin_{}_{}", plugin.metadata().id, id.0);
                    let group = plugin_entity_group_with_children(
                        entity,
                        parsed_file.language,
                        id,
                        group_id,
                        &mut next_id,
                    );
                    groups.push(group);
                    injected += 1;
                }
            }
            Ok(_) => {}
            Err(e) => {
                warn!(
                    plugin = %plugin.metadata().id,
                    file_path = %parsed_file.path,
                    error = %e,
                    "EntityExtract plugin failed"
                );
            }
        }
    }

    if injected > 0 {}
}

/// Convert a plugin entity (and its children) into an [`EntityGroup`].
fn plugin_entity_group_with_children(
    entity: cce_types::PluginEntity,
    language: Language,
    id: EntityId,
    group_id: String,
    next_id: &mut u64,
) -> EntityGroup {
    let mut group = plugin_entity_to_group(&entity, language, id, group_id);
    if !entity.children.is_empty() {
        let mut nested = Vec::with_capacity(entity.children.len());
        for child in entity.children {
            let child_id = EntityId(*next_id);
            *next_id += 1;
            let child_group_id = format!("{}_child_{}", group.group_id, child_id.0);
            nested.push(plugin_entity_group_with_children(
                child,
                language,
                child_id,
                child_group_id,
                next_id,
            ));
        }
        group.nested_groups = nested.into_boxed_slice();
        group.has_significant_nested = true;
    }
    group
}

/// Run the `Group` post-processing hook after built-in grouping.
pub fn apply_plugin_post_group(
    plugin_registry: &Option<std::sync::Arc<PluginRegistry>>,
    groups: Vec<EntityGroup>,
    parsed_file: &ParsedFile,
) -> Vec<EntityGroup> {
    let Some(registry) = plugin_registry else {
        return groups;
    };
    let language = parsed_file.language.to_string();
    let hooks = registry.get_plugins(
        PluginCapability::Group,
        Some(&parsed_file.path),
        Some(&language),
    );
    if hooks.is_empty() {
        return groups;
    }

    let context = GroupPluginContext {
        file_path: parsed_file.path.clone(),
        language,
        source: parsed_file.source.to_string(),
        entities: serialize_parsed_entities(parsed_file),
        relations: serialize_parsed_relations(parsed_file),
    };

    let mut current = groups;
    for plugin in hooks {
        // Clone so `Ok(None)` (decline) keeps the current group list.
        let before = current.len();
        let started = std::time::Instant::now();
        match plugin.post_group(current.clone(), context.clone()) {
            Ok(Some(new_groups)) => {
                tracing::debug!(
                    plugin = %plugin.metadata().id,
                    file_path = %parsed_file.path,
                    groups_before = before,
                    groups_after = new_groups.len(),
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "post_group hook applied"
                );
                current = new_groups;
            }
            Ok(None) => {}
            Err(e) => {
                warn!(
                    plugin = %plugin.metadata().id,
                    file_path = %parsed_file.path,
                    error = %e,
                    "post_group hook failed, keeping built-in groups"
                );
            }
        }
    }
    current
}

/// Try the `Group` full-override tier (`GroupOverride` capability).
///
/// Plugins (pre-split by the registry at the built-in boundary) are
/// queried in priority order; the first plugin returning a non-empty
/// group list fully replaces built-in grouping. Returns `None` when no
/// plugin overrides (built-in grouping runs as usual).
pub fn try_plugin_group_override(
    _plugin_registry: &Option<std::sync::Arc<PluginRegistry>>,
    parsed_file: &ParsedFile,
    overriders: &[&std::sync::Arc<dyn cce_plugin::CodePlugin>],
) -> Option<Vec<EntityGroup>> {
    if overriders.is_empty() {
        return None;
    }
    let language = parsed_file.language.to_string();
    let context = GroupPluginContext {
        file_path: parsed_file.path.clone(),
        language,
        source: parsed_file.source.to_string(),
        entities: serialize_parsed_entities(parsed_file),
        relations: serialize_parsed_relations(parsed_file),
    };
    for plugin in overriders {
        match plugin.group(context.clone()) {
            Ok(Some(groups)) if !groups.is_empty() => {
                return Some(groups);
            }
            Ok(_) => {}
            Err(e) => {
                warn!(
                    plugin = %plugin.metadata().id,
                    file_path = %parsed_file.path,
                    error = %e,
                    "group override failed, falling back to built-in grouping"
                );
            }
        }
    }
    None
}

/// Serialize the parsed entities into plugin-entity form for the `Group`
/// override tier.
fn serialize_parsed_entities(parsed_file: &ParsedFile) -> Vec<PluginEntity> {
    let entities_by_id: HashMap<EntityId, &Entity> =
        parsed_file.entities.iter().map(|e| (e.id, e)).collect();
    parsed_file
        .entities
        .iter()
        .filter(|e| e.parent.is_none())
        .map(|e| entity_to_plugin_entity(e, &entities_by_id))
        .collect()
}

/// Serialize the raw relations into plugin-relation form for the `Group`
/// override tier.
fn serialize_parsed_relations(parsed_file: &ParsedFile) -> Vec<PluginRelation> {
    parsed_file
        .raw_relations
        .iter()
        .map(raw_relation_to_plugin_relation)
        .collect()
}

/// `LangHeuristics` stdlib classification for custom-language files.
///
/// Entities whose stdlib status is unknown are classified via the
/// `classify_stdlib(module_path)` hook (module path = entity name),
/// first non-`None` plugin answer wins. Built-in languages are
/// untouched (their detectors already ran during extraction).
pub fn apply_stdlib_heuristics(
    plugin_registry: &Option<std::sync::Arc<PluginRegistry>>,
    parsed_file: &ParsedFile,
) -> Vec<Entity> {
    if !matches!(parsed_file.language, Language::Custom(_)) {
        return parsed_file.entities.clone();
    }
    let Some(registry) = plugin_registry else {
        return parsed_file.entities.clone();
    };
    let mut entities = parsed_file.entities.clone();
    for entity in entities.iter_mut() {
        if entity.is_stdlib || entity.stdlib_category.is_some() {
            continue;
        }
        if let Some(category) = crate::plugin::heuristics::classify_stdlib(registry, &entity.name) {
            entity.is_stdlib = true;
            entity.stdlib_category = Some(category);
        }
    }
    entities
}
