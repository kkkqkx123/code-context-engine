//! Change computation logic for hot update
//!
//! This module provides functions for computing entity changes
//! by comparing old and new entity lists.

use std::collections::HashMap;

use cce_types::entity::{Entity, EntityId};

use super::types::{EntityChange, EntityChangeType};

/// Compare two entity lists and generate changes
#[allow(dead_code)]
pub fn compute_entity_changes(
    old_entities: &[Entity],
    new_entities: &[Entity],
) -> Vec<EntityChange> {
    let mut changes = Vec::new();

    // Build lookup maps by entity ID
    let old_map: HashMap<EntityId, &Entity> = old_entities.iter().map(|e| (e.id, e)).collect();
    let new_map: HashMap<EntityId, &Entity> = new_entities.iter().map(|e| (e.id, e)).collect();

    // Find added and modified entities
    for new_entity in new_entities {
        match old_map.get(&new_entity.id) {
            None => {
                // New entity
                changes.push(
                    EntityChange::new(
                        new_entity.id,
                        new_entity.name.clone(),
                        EntityChangeType::Added,
                    )
                    .with_entity(new_entity.clone()),
                );
            }
            Some(old_entity) => {
                // Check if modified
                if entities_differ(old_entity, new_entity) {
                    changes.push(
                        EntityChange::new(
                            new_entity.id,
                            new_entity.name.clone(),
                            EntityChangeType::Modified,
                        )
                        .with_entity(new_entity.clone())
                        .with_previous_entity((*old_entity).clone()),
                    );
                } else {
                    // Unchanged
                    changes.push(EntityChange::new(
                        new_entity.id,
                        new_entity.name.clone(),
                        EntityChangeType::Unchanged,
                    ));
                }
            }
        }
    }

    // Find deleted entities
    for old_entity in old_entities {
        if !new_map.contains_key(&old_entity.id) {
            changes.push(
                EntityChange::new(
                    old_entity.id,
                    old_entity.name.clone(),
                    EntityChangeType::Deleted,
                )
                .with_previous_entity(old_entity.clone()),
            );
        }
    }

    changes
}

/// Check if two entities differ
#[allow(dead_code)]
fn entities_differ(old: &Entity, new: &Entity) -> bool {
    // Compare key fields that indicate semantic changes
    if old.name != new.name {
        return true;
    }
    if old.kind != new.kind {
        return true;
    }
    if old.signature != new.signature {
        return true;
    }
    if old.return_type != new.return_type {
        return true;
    }
    if old.parameters.len() != new.parameters.len() {
        return true;
    }
    // Compare parameters
    for (old_param, new_param) in old.parameters.iter().zip(new.parameters.iter()) {
        if old_param != new_param {
            return true;
        }
    }
    // Compare span (location changes)
    if old.span != new.span {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;
    use cce_types::entity::EntityKind;

    fn create_test_entity(id: u32, name: &str) -> Entity {
        Entity::new(
            EntityId(id.into()),
            EntityKind::Function,
            name.to_string(),
            Span::default(),
        )
    }

    #[test]
    fn test_compute_entity_changes() {
        let old_entities = vec![
            create_test_entity(1, "func1"),
            create_test_entity(2, "func2"),
        ];

        let mut modified_entity = create_test_entity(1, "func1");
        modified_entity.signature = "fn func1() -> i32".to_string();

        let new_entities = vec![modified_entity, create_test_entity(3, "func3")];

        let changes = compute_entity_changes(&old_entities, &new_entities);

        assert_eq!(changes.len(), 3); // 1 modified, 1 added, 1 deleted
    }

    #[test]
    fn test_compute_entity_changes_no_changes() {
        let entities = vec![
            create_test_entity(1, "func1"),
            create_test_entity(2, "func2"),
        ];

        let changes = compute_entity_changes(&entities, &entities);

        // All should be unchanged
        assert_eq!(changes.len(), 2);
        assert!(
            changes
                .iter()
                .all(|c| c.change_type == EntityChangeType::Unchanged)
        );
    }

    #[test]
    fn test_compute_entity_changes_all_added() {
        let new_entities = vec![
            create_test_entity(1, "func1"),
            create_test_entity(2, "func2"),
        ];

        let changes = compute_entity_changes(&[], &new_entities);

        assert_eq!(changes.len(), 2);
        assert!(changes.iter().all(|c| c.is_added()));
    }

    #[test]
    fn test_compute_entity_changes_all_deleted() {
        let old_entities = vec![
            create_test_entity(1, "func1"),
            create_test_entity(2, "func2"),
        ];

        let changes = compute_entity_changes(&old_entities, &[]);

        assert_eq!(changes.len(), 2);
        assert!(changes.iter().all(|c| c.is_deleted()));
    }
}
