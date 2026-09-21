use std::collections::HashSet;

use smallvec::SmallVec;

use crate::grouper::types::{EntityGroup, GroupType};
use cce_types::Span;
use cce_types::entity::{Entity, EntityId, GroupedEntity};

/// Function member processor
///
/// Groups function-level entities (macros, closures, statements, callbacks)
/// as members of their parent function, reducing standalone fragment count.
///
/// This processor runs after ClassMethodProcessor and operates on existing groups,
/// modifying them in place. For each function-like group it absorbs inner groups
/// whose span is fully contained, plus any still-unprocessed entities in the
/// function body. Inner functions are processed first so nested callbacks become
/// members of the enclosing function instead of leaking into nearby fragments.
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
        let mut processed_ids: HashSet<EntityId> =
            groups.iter().flat_map(|g| g.all_entity_ids()).collect();
        let mut absorbed_group_indices: HashSet<usize> = HashSet::new();
        let mut replacements: Vec<(usize, EntityGroup)> = Vec::new();

        let mut function_indices: Vec<usize> = groups
            .iter()
            .enumerate()
            .filter(|(_, group)| is_absorbing_function_group(group))
            .map(|(idx, _)| idx)
            .collect();
        function_indices.sort_by_key(|&idx| {
            let span = &groups[idx].span;
            span.end_byte.saturating_sub(span.start_byte)
        });

        for func_idx in function_indices {
            if absorbed_group_indices.contains(&func_idx) {
                continue;
            }
            let Some(header) = groups[func_idx].header.clone() else {
                continue;
            };
            let func_span = groups[func_idx].span;
            let header_id = header.id;

            let mut child_entities: Vec<Entity> = Vec::new();
            let mut inner_ids: HashSet<EntityId> = HashSet::new();

            for (idx, group) in groups.iter().enumerate() {
                if idx == func_idx || absorbed_group_indices.contains(&idx) {
                    continue;
                }
                if group.group_type == GroupType::FileDocumentation {
                    continue;
                }
                if group
                    .header
                    .as_ref()
                    .is_some_and(|h| h.kind.is_import_like())
                {
                    continue;
                }
                if !span_fully_inside(&group.span, &func_span) {
                    continue;
                }
                if group.span.start_byte == func_span.start_byte
                    && group.span.end_byte == func_span.end_byte
                {
                    continue;
                }

                absorbed_group_indices.insert(idx);
                if let Some(ref inner_header) = group.header {
                    if inner_ids.insert(inner_header.id) {
                        if let Some(entity) = entities.iter().find(|e| e.id == inner_header.id) {
                            child_entities.push(entity.clone());
                        }
                    }
                }
                for member in &group.members {
                    if inner_ids.insert(member.id) {
                        if let Some(entity) = entities.iter().find(|e| e.id == member.id) {
                            child_entities.push(entity.clone());
                        }
                    }
                }
            }

            for entity in entities {
                if entity.id == header_id || processed_ids.contains(&entity.id) {
                    continue;
                }
                if entity.kind.is_import_like() {
                    continue;
                }
                if is_entity_inside_span(entity, &func_span) && inner_ids.insert(entity.id) {
                    child_entities.push(entity.clone());
                }
            }

            if child_entities.is_empty() {
                continue;
            }

            let Some(func_entity) = entities.iter().find(|e| e.id == header_id).cloned() else {
                continue;
            };

            for child in &child_entities {
                processed_ids.insert(child.id);
            }
            processed_ids.insert(header_id);

            replacements.push((
                func_idx,
                create_function_with_members_group(func_entity, child_entities, language),
            ));
        }

        let association_count = replacements.len();
        for (idx, merged) in replacements {
            groups[idx] = merged;
        }

        if !absorbed_group_indices.is_empty() {
            let mut sorted: Vec<usize> = absorbed_group_indices.into_iter().collect();
            sorted.sort_unstable_by(|a, b| b.cmp(a));
            for idx in sorted {
                groups.remove(idx);
            }
        }

        association_count
    }
}

fn is_absorbing_function_group(group: &EntityGroup) -> bool {
    if group.header.as_ref().is_none_or(|h| !h.kind.is_function_like()) {
        return false;
    }
    matches!(
        group.group_type,
        GroupType::Standalone | GroupType::FunctionWithMembers
    )
}

fn span_fully_inside(inner: &Span, container: &Span) -> bool {
    inner.start_byte >= container.start_byte && inner.end_byte <= container.end_byte
}

fn is_entity_inside_span(entity: &Entity, container: &Span) -> bool {
    span_fully_inside(&entity.span, container)
}

fn create_function_with_members_group(
    function: Entity,
    child_entities: Vec<Entity>,
    language: cce_types::language::Language,
) -> EntityGroup {
    let func_id = function.id;
    let name = compact_str::CompactString::from(function.name.as_str());
    let kind = function.kind;
    let func_span = function.span;

    let mut sorted_children = child_entities;
    sorted_children.sort_by_key(|e| e.span.start_byte);

    let member_ids: SmallVec<[EntityId; 8]> = sorted_children.iter().map(|m| m.id).collect();

    let mut entity_spans = std::collections::HashMap::new();
    entity_spans.insert(func_id, func_span);

    let semantic_members: SmallVec<[GroupedEntity; 4]> = sorted_children
        .iter()
        .map(|m| {
            entity_spans.insert(m.id, m.span);
            GroupedEntity::from_entity(m)
        })
        .collect();

    let combined_span = EntityGroup::calculate_combined_span_from_map(&entity_spans);

    EntityGroup {
        group_id: compact_str::CompactString::from(format!("group_{}", func_id.0)),
        group_type: GroupType::FunctionWithMembers,
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

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::entity::EntityKind;
    use cce_types::language::Language;

    fn entity(id: u64, kind: EntityKind, name: &str, start: usize, end: usize) -> Entity {
        Entity::new(
            EntityId(id),
            kind,
            name.to_string(),
            Span::new(start, end, 0, 0, 0, 0),
        )
    }

    #[test]
    fn absorbs_inner_property_group_into_enclosing_function() {
        let func = entity(1, EntityKind::Function, "authenticate", 0, 100);
        let prop = entity(2, EntityKind::Property, "password", 40, 55);
        let mut groups = vec![
            EntityGroup::from_entity(func.clone(), Language::JavaScript),
            EntityGroup::from_entity(prop.clone(), Language::JavaScript),
        ];
        let entities = vec![func, prop];
        let count = FunctionMemberProcessor::new().process(
            &mut groups,
            &entities,
            Language::JavaScript,
        );
        assert_eq!(count, 1);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].group_type, GroupType::FunctionWithMembers);
        assert_eq!(groups[0].members.len(), 1);
        assert_eq!(groups[0].members[0].name, "password");
    }

    #[test]
    fn absorbs_nested_callback_into_outer_function() {
        let outer = entity(1, EntityKind::Function, "authenticate", 0, 200);
        let inner = entity(2, EntityKind::Function, "hash", 80, 150);
        let mut groups = vec![
            EntityGroup::from_entity(outer.clone(), Language::JavaScript),
            EntityGroup::from_entity(inner.clone(), Language::JavaScript),
        ];
        let entities = vec![outer, inner];
        FunctionMemberProcessor::new().process(&mut groups, &entities, Language::JavaScript);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name.as_str(), "authenticate");
        assert!(
            groups[0]
                .members
                .iter()
                .any(|m| m.name == "hash" && m.kind == EntityKind::Function)
        );
    }
}
