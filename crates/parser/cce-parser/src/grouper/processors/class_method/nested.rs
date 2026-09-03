use compact_str::CompactString;

use super::processor::ClassMethodProcessor;
use super::types::NestedExtractionContext;
use crate::grouper::types::EntityGroup;
use crate::grouper::types::GroupType;
use cce_types::entity::Entity;

impl ClassMethodProcessor {
    /// Extract nested entity groups recursively
    ///
    /// This method extracts nested classes/structs from a parent entity,
    /// supporting up to the specified maximum nesting depth.
    pub(super) fn extract_nested_groups(
        &self,
        parent_entity: &Entity,
        ctx: NestedExtractionContext<'_>,
    ) -> Vec<EntityGroup> {
        if ctx.max_depth == 0 {
            // Mark all child type definitions as processed to avoid creating standalone groups
            for &child_id in &parent_entity.children {
                if let Some(child_entity) = ctx.all_entities.iter().find(|e| e.id == child_id) {
                    if child_entity.kind.is_type_definition() {
                        // SAFETY: We only insert into the HashSet, no concurrent access
                        ctx.processed_ids.insert(child_id);
                    }
                }
            }
            return Vec::new();
        }

        let mut nested_groups = Vec::new();

        // Find child entities that are type definitions
        for &child_id in &parent_entity.children {
            if ctx.processed_ids.contains(&child_id) {
                continue;
            }

            if let Some(child_entity) = ctx.all_entities.iter().find(|e| e.id == child_id) {
                // Only process type definitions (Class, Struct, Interface, etc.)
                if !child_entity.kind.is_type_definition() {
                    continue;
                }

                // Estimate line count, filter tiny nested entities
                let line_count = estimate_line_count(child_entity);
                if line_count < ctx.min_nested_size {
                    ctx.processed_ids.insert(child_id);
                    continue;
                }

                // Recursively extract deeper nested groups
                let deeper_nested = {
                    let deeper_ctx = NestedExtractionContext {
                        all_entities: ctx.all_entities,
                        max_depth: ctx.max_depth - 1,
                        min_nested_size: ctx.min_nested_size,
                        processed_ids: ctx.processed_ids,
                        language: ctx.language,
                        parsed_file: ctx.parsed_file,
                        config: ctx.config,
                    };
                    self.extract_nested_groups(child_entity, deeper_ctx)
                };

                // Get child fields and methods
                let children: Vec<Entity> = child_entity
                    .children
                    .iter()
                    .filter_map(|&cid| ctx.all_entities.iter().find(|e| e.id == cid))
                    .cloned()
                    .collect();

                let fields: Vec<&Entity> = children
                    .iter()
                    .filter(|e| e.kind.is_variable_like())
                    .collect();

                let methods: Vec<Entity> = children
                    .iter()
                    .filter(|e| e.kind.is_function_like())
                    .cloned()
                    .collect();

                // Mark stdlib/constructor/complex methods roles and merge
                // simple getters/setters when enabled.
                let pattern_result = self.apply_pattern_processing(
                    child_entity,
                    &fields,
                    &methods,
                    ctx.language,
                    ctx.config,
                );

                // Create nested entity group
                let mut group = EntityGroup::from_entity(child_entity.clone(), *ctx.language);
                group.nested_groups = deeper_nested.into_boxed_slice();
                group.nesting_level = ctx.max_depth;
                group.parent_group_id = Some(CompactString::from(parent_entity.name.as_str()));
                group.has_significant_nested = !group.nested_groups.is_empty();
                group.pattern_info = pattern_result.pattern_info;
                group.member_roles = pattern_result.member_roles;

                if group.has_significant_nested {
                    group.group_type = match child_entity.kind {
                        cce_types::entity::EntityKind::Class => GroupType::ClassWithNestedClasses,
                        cce_types::entity::EntityKind::Struct => GroupType::StructWithNestedStructs,
                        _ => group.group_type,
                    };
                }

                nested_groups.push(group);
                ctx.processed_ids.insert(child_id);
            }
        }

        nested_groups
    }
}

/// Estimate the line count of an entity from its span
fn estimate_line_count(entity: &Entity) -> usize {
    let span = &entity.span;
    if span.end_position.row >= span.start_position.row {
        span.end_position.row - span.start_position.row + 1
    } else {
        let source_len = span.len();
        source_len / 30 + 1
    }
}
