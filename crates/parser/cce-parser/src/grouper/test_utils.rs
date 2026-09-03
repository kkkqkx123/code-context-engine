//! Test utilities for pattern detection tests
//!
//! This module provides helper functions for creating test entities
//! with various configurations.

use cce_types::Span;
use cce_types::entity::{Entity, EntityId, EntityKind};

use crate::grouper::metadata;

/// Create a simple interface entity
pub fn create_interface(id: EntityId, name: &str) -> Entity {
    Entity::new(id, EntityKind::Interface, name.to_string(), Span::default())
}

/// Create a class entity with metadata
///
/// # Arguments
/// * `id` - The entity ID
/// * `name` - The class name
/// * `fields` - Fields metadata (comma-separated)
/// * `methods` - Methods metadata (comma-separated)
/// * `base_types` - Base types metadata (comma-separated)
///
/// # Returns
/// A new class entity with metadata set
pub fn create_class_with_metadata(
    id: EntityId,
    name: &str,
    fields: &str,
    methods: &str,
    base_types: &str,
) -> Entity {
    let mut entity = Entity::new(id, EntityKind::Class, name.to_string(), Span::default());
    if !fields.is_empty() {
        entity
            .metadata
            .insert(metadata::FIELDS.to_string(), fields.to_string());
    }
    if !methods.is_empty() {
        entity
            .metadata
            .insert(metadata::METHODS.to_string(), methods.to_string());
    }
    if !base_types.is_empty() {
        entity
            .metadata
            .insert(metadata::BASE_TYPES.to_string(), base_types.to_string());
    }
    entity
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_interface() {
        let interface = create_interface(EntityId(0), "TestInterface");
        assert_eq!(interface.name, "TestInterface");
        assert_eq!(interface.kind, EntityKind::Interface);
    }

    #[test]
    fn test_create_class_with_metadata() {
        let class = create_class_with_metadata(
            EntityId(0),
            "UserRepository",
            "db: Database",
            "save, find, delete",
            "Repository",
        );

        assert_eq!(class.name, "UserRepository");
        assert_eq!(
            class.metadata.get(metadata::FIELDS),
            Some(&"db: Database".to_string())
        );
        assert_eq!(
            class.metadata.get(metadata::METHODS),
            Some(&"save, find, delete".to_string())
        );
        assert_eq!(
            class.metadata.get(metadata::BASE_TYPES),
            Some(&"Repository".to_string())
        );
    }
}
