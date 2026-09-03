use std::collections::HashSet;

use smallvec::SmallVec;

use crate::grouper::types::EntityGroup;
use cce_types::Span;
use cce_types::entity::{Entity, EntityId, GroupedEntity};

/// Function member processor
///
/// Groups function-level entities (macro_invocations, closures, statements, etc.)
/// as members of their parent function, reducing standalone fragment count.
///
/// This processor runs after ClassMethodProcessor and operates on existing groups,
/// modifying them in place. For each standalone function-like entity, it finds
/// child entities within its span and merges them into a FunctionWithMembers group.
pub struct FunctionMemberProcessor;

impl Default for FunctionMemberProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl FunctionMemberProcessor {
    pub fn new() -> Self {
        Self
    }

    /// Process groups to associate function-level members
    ///
    /// # Arguments
    /// * `groups` - Mutable reference to the list of entity groups
    /// * `entities` - All entities from the parsed file
    /// * `language` - Programming language
    ///
    /// # Returns
    /// Number of associations made
    pub fn process(
        &self,
        groups: &mut Vec<EntityGroup>,
        entities: &[Entity],
        language: cce_types::language::Language,
    ) -> usize {
        // Build a set of ALL processed entity IDs (across all groups)
        let processed_ids: HashSet<EntityId> =
            groups.iter().flat_map(|g| g.all_entity_ids()).collect();

        // Find standalone groups that are function-like entities
        // We need to identify the function entities first, then find their child entities
        // Strategy: find function-like entities in the entities list that have children
        // within their span, and aren't already processed

        let mut association_count = 0;
        let mut new_groups: Vec<EntityGroup> = Vec::new();
        let mut indices_to_remove: Vec<usize> = Vec::new();

        for (idx, group) in groups.iter().enumerate() {
            // Only process standalone groups that are function-like
            if !group.group_type.is_standalone() {
                continue;
            }

            let Some(header) = &group.header else {
                continue;
            };

            if !header.kind.is_function_like() {
                continue;
            }

            // Find child entities within this function's span that are NOT yet processed
            let child_entities: Vec<Entity> = entities
                .iter()
                .filter(|e| {
                    !processed_ids.contains(&e.id)
                        && e.id != header.id
                        && is_entity_inside_span(e, &group.span)
                })
                .cloned()
                .collect();

            if child_entities.is_empty() {
                continue;
            }

            // Find the actual function entity from entities list
            let func_entity = entities.iter().find(|e| e.id == header.id).cloned();
            let func_entity = match func_entity {
                Some(e) => e,
                None => continue,
            };

            let merged = create_function_with_members_group(func_entity, child_entities, language);
            new_groups.push(merged);
            indices_to_remove.push(idx);
            association_count += 1;
        }

        // Remove old standalone groups (in reverse order to preserve indices)
        for idx in indices_to_remove.into_iter().rev() {
            groups.remove(idx);
        }

        // Add new groups
        groups.extend(new_groups);

        association_count
    }
}

/// Check if an entity's span is fully inside the given container span
fn is_entity_inside_span(entity: &Entity, container: &Span) -> bool {
    entity.span.start_byte >= container.start_byte && entity.span.end_byte <= container.end_byte
}

/// Create a FunctionWithMembers group from a function entity and its child entities
fn create_function_with_members_group(
    function: Entity,
    child_entities: Vec<Entity>,
    language: cce_types::language::Language,
) -> EntityGroup {
    let func_id = function.id;
    let name = compact_str::CompactString::from(function.name.as_str());
    let kind = function.kind;
    let func_span = function.span;

    // Sort children by source position
    let mut sorted_children = child_entities;
    sorted_children.sort_by_key(|e| e.span.start_byte);

    let member_ids: SmallVec<[EntityId; 8]> = sorted_children.iter().map(|m| m.id).collect();

    // Convert children to GroupedEntity and collect spans
    let mut entity_spans = std::collections::HashMap::new();
    entity_spans.insert(func_id, func_span);

    let semantic_members: SmallVec<[GroupedEntity; 4]> = sorted_children
        .iter()
        .map(|m| {
            entity_spans.insert(m.id, m.span);
            GroupedEntity::from_entity(m)
        })
        .collect();

    // Calculate combined span covering function and all children
    let combined_span = EntityGroup::calculate_combined_span_from_map(&entity_spans);

    EntityGroup {
        group_id: compact_str::CompactString::from(format!("group_{}", func_id.0)),
        group_type: crate::grouper::types::GroupType::FunctionWithMembers,
        header: Some(GroupedEntity::from_entity(&function)),
        header_id: Some(func_id),
        members: semantic_members,
        member_ids,
        entity_spans,
        combined_source: None,
        combined_source_lazy: std::sync::OnceLock::new(),
        span: combined_span,
        kind,
        name,
        language,
        pattern_info: crate::grouper::types::PatternInfo::default(),
        member_roles: SmallVec::new(),
        nested_groups: Box::new([]),
        nesting_level: 0,
        parent_group_id: None,
        has_significant_nested: false,
        metadata: std::collections::HashMap::new(),
        test_info: cce_types::TestInfo::unknown(),
    }
}
