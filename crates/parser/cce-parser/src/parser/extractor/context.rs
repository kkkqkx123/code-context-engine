//! Extraction context for managing entity stack and depth
//!
//! During AST traversal, we maintain a stack of entities to:
//! - Track current nesting depth
//! - Identify parent-child relationships
//! - Find the current caller for call relations

use cce_types::{Entity, EntityId};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Extraction context
///
/// Maintains entity stack during AST traversal for:
/// - Depth tracking
/// - Parent-child relationship identification
/// - Caller identification for call relations
#[derive(Debug)]
pub struct ExtractionContext {
    /// Current entity stack (from root to current)
    entity_stack: Vec<EntityId>,
    /// Shared global ID counter (unique across all extraction contexts)
    id_counter: Arc<AtomicU64>,
}

impl ExtractionContext {
    /// Create a new extraction context with a shared global counter
    pub fn new(counter: Arc<AtomicU64>) -> Self {
        Self {
            entity_stack: Vec::new(),
            id_counter: counter,
        }
    }

    /// Generate next globally unique entity ID
    pub fn next_entity_id(&self) -> EntityId {
        let id = self.id_counter.fetch_add(1, Ordering::Relaxed);
        EntityId(id)
    }

    /// Get current depth (stack size)
    pub fn current_depth(&self) -> usize {
        self.entity_stack.len()
    }

    /// Get current parent entity (top of stack)
    pub fn current_parent(&self) -> Option<EntityId> {
        self.entity_stack.last().copied()
    }

    /// Enter an entity (push to stack)
    ///
    /// Sets entity's depth and parent based on current context.
    pub fn enter_entity(&mut self, entity: &mut Entity) {
        // Set depth based on current stack size
        entity.depth = self.entity_stack.len();

        // Set parent to top of stack
        entity.parent = self.entity_stack.last().copied();

        // Push to stack
        self.entity_stack.push(entity.id);
    }

    /// Exit current entity (pop from stack)
    pub fn exit_entity(&mut self) -> Option<EntityId> {
        self.entity_stack.pop()
    }

    /// Get the current caller entity
    ///
    /// For call relations, the caller is the topmost function/method entity in the stack.
    /// Returns None if no function/method is in the stack.
    pub fn current_caller(&self, entities: &[Entity]) -> Option<EntityId> {
        // Search from top to bottom for a function-like entity
        for &entity_id in self.entity_stack.iter().rev() {
            if let Some(entity) = entities.iter().find(|e| e.id == entity_id) {
                if entity.kind.is_function_like() {
                    return Some(entity_id);
                }
            }
        }
        None
    }

    /// Get the current type definition entity
    ///
    /// For method extraction, we need to know which class/struct we're inside.
    /// Returns None if no type definition is in the stack.
    pub fn current_type_definition(&self, entities: &[Entity]) -> Option<EntityId> {
        // Search from top to bottom for a type definition entity
        for &entity_id in self.entity_stack.iter().rev() {
            if let Some(entity) = entities.iter().find(|e| e.id == entity_id) {
                if entity.kind.is_type_definition() {
                    return Some(entity_id);
                }
            }
        }
        None
    }

    /// Check if we're inside a function/method
    pub fn is_inside_function(&self, entities: &[Entity]) -> bool {
        self.current_caller(entities).is_some()
    }

    /// Check if we're inside a type definition
    pub fn is_inside_type_definition(&self, entities: &[Entity]) -> bool {
        self.current_type_definition(entities).is_some()
    }

    /// Get the entity stack (for debugging)
    pub fn stack(&self) -> &[EntityId] {
        &self.entity_stack
    }

    /// Reset the context (entity stack only, id_counter is shared globally)
    pub fn reset(&mut self) {
        self.entity_stack.clear();
    }
}

/// Scoped entity guard
///
/// Automatically exits entity when dropped.
/// Use this for RAII-style entity scope management.
pub struct ScopedEntity<'a> {
    context: &'a mut ExtractionContext,
}

impl<'a> ScopedEntity<'a> {
    /// Create a new scoped entity
    pub fn new(context: &'a mut ExtractionContext, entity: &mut Entity) -> Self {
        context.enter_entity(entity);
        Self { context }
    }
}

impl<'a> Drop for ScopedEntity<'a> {
    fn drop(&mut self) {
        self.context.exit_entity();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::EntityKind;

    #[test]
    fn test_context_basic() {
        let counter = Arc::new(AtomicU64::new(0));
        let ctx = ExtractionContext::new(counter);

        // Generate IDs
        let id0 = ctx.next_entity_id();
        let id1 = ctx.next_entity_id();
        assert_eq!(id0, EntityId(0));
        assert_eq!(id1, EntityId(1));

        // Initial state
        assert_eq!(ctx.current_depth(), 0);
        assert!(ctx.current_parent().is_none());
    }

    #[test]
    fn test_entity_stack() {
        let counter = Arc::new(AtomicU64::new(0));
        let mut ctx = ExtractionContext::new(counter);

        // Create and enter first entity
        let id0 = ctx.next_entity_id();
        let mut entity0 = Entity::new(
            id0,
            EntityKind::Class,
            "MyClass".to_string(),
            Default::default(),
        );
        ctx.enter_entity(&mut entity0);

        assert_eq!(entity0.depth, 0);
        assert!(entity0.parent.is_none());
        assert_eq!(ctx.current_depth(), 1);
        assert_eq!(ctx.current_parent(), Some(id0));

        // Create and enter second entity (nested)
        let id1 = ctx.next_entity_id();
        let mut entity1 = Entity::new(
            id1,
            EntityKind::Method,
            "my_method".to_string(),
            Default::default(),
        );
        ctx.enter_entity(&mut entity1);

        assert_eq!(entity1.depth, 1);
        assert_eq!(entity1.parent, Some(id0));
        assert_eq!(ctx.current_depth(), 2);
        assert_eq!(ctx.current_parent(), Some(id1));

        // Exit second entity
        let exited = ctx.exit_entity();
        assert_eq!(exited, Some(id1));
        assert_eq!(ctx.current_depth(), 1);
        assert_eq!(ctx.current_parent(), Some(id0));

        // Exit first entity
        let exited = ctx.exit_entity();
        assert_eq!(exited, Some(id0));
        assert_eq!(ctx.current_depth(), 0);
        assert!(ctx.current_parent().is_none());
    }

    #[test]
    fn test_current_caller() {
        let counter = Arc::new(AtomicU64::new(0));
        let mut ctx = ExtractionContext::new(counter);
        let entities = vec![
            Entity::new(
                EntityId(0),
                EntityKind::Class,
                "MyClass".to_string(),
                Default::default(),
            ),
            Entity::new(
                EntityId(1),
                EntityKind::Method,
                "my_method".to_string(),
                Default::default(),
            ),
        ];

        // Enter class
        let mut class_entity = entities[0].clone();
        ctx.enter_entity(&mut class_entity);

        // Class is not a function, so no caller
        assert!(ctx.current_caller(&entities).is_none());

        // Enter method
        let mut method_entity = entities[1].clone();
        method_entity.id = EntityId(1);
        ctx.enter_entity(&mut method_entity);

        // Method is a function-like entity, so it's the caller
        assert_eq!(ctx.current_caller(&entities), Some(EntityId(1)));
    }

    #[test]
    fn test_scoped_entity() {
        let counter = Arc::new(AtomicU64::new(0));
        let mut ctx = ExtractionContext::new(counter);
        let id = ctx.next_entity_id();

        {
            let mut entity = Entity::new(
                id,
                EntityKind::Function,
                "test".to_string(),
                Default::default(),
            );
            let _scoped = ScopedEntity::new(&mut ctx, &mut entity);

            // After scoped entity is created, we can't access ctx due to borrow
            // Just verify the entity was modified
            assert_eq!(entity.depth, 0);
        }

        // Scoped entity automatically exited
        assert_eq!(ctx.current_depth(), 0);
    }
}
