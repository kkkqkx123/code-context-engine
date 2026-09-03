//! Post-processing: parent-child relationship resolution
//!
//! Populates children HashSet for each entity by scanning all entities
//! and adding child IDs to their parents.

use cce_types::Entity;

/// Fill children relationships based on parent field
///
/// O(n) complexity where n is the number of entities.
pub fn fill_children(entities: &mut [Entity]) {
    let mut id_to_index: std::collections::HashMap<cce_types::EntityId, usize> =
        std::collections::HashMap::new();
    for (i, entity) in entities.iter().enumerate() {
        id_to_index.insert(entity.id, i);
    }

    for i in 0..entities.len() {
        if let Some(parent_id) = entities[i].parent {
            if let Some(&parent_idx) = id_to_index.get(&parent_id) {
                entities[parent_idx].add_child(entities[i].id);
            }
        }
    }
}
