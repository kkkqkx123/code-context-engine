use cce_types::{Entity, EntityId, EntityKind, RelationType};

/// Entity index for efficient caller lookup
///
/// Maintains function and component entities sorted by start position for O(log n) lookup.
/// Supports both function-like entities (functions, methods) and component-like entities
/// (components, elements, templates) for frontend framework support.
pub(crate) struct EntityIndex {
    /// Vector of (start_byte, end_byte, entity_id) for function-like entities
    functions: Vec<(usize, usize, EntityId)>,
    /// Vector of (start_byte, end_byte, entity_id) for component-like entities
    /// Includes Component, Element, Template, ScriptContent
    components: Vec<(usize, usize, EntityId)>,
    /// Vector of (start_byte, end_byte, entity_id) for entities that can own
    /// structural relations such as inheritance or implementation.
    structural_owners: Vec<(usize, usize, EntityId)>,
}

impl EntityIndex {
    /// Create a new entity index from entities
    ///
    /// Indexes both function-like entities and component-like entities for frontend support.
    pub(crate) fn new(entities: &[Entity]) -> Self {
        let mut functions: Vec<_> = entities
            .iter()
            .filter(|e| e.kind.is_function_like())
            .map(|e| (e.span.start_byte, e.span.end_byte, e.id))
            .collect();

        let mut components: Vec<_> = entities
            .iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    EntityKind::Component
                        | EntityKind::Element
                        | EntityKind::Template
                        | EntityKind::ScriptContent
                )
            })
            .map(|e| (e.span.start_byte, e.span.end_byte, e.id))
            .collect();

        let mut structural_owners: Vec<_> = entities
            .iter()
            .filter(|e| {
                e.kind.is_type_definition()
                    || e.kind.is_module_like()
                    || e.kind.is_impl_block()
                    || e.kind.is_template_entity()
            })
            .map(|e| (e.span.start_byte, e.span.end_byte, e.id))
            .collect();

        // Sort by start position for binary search
        functions.sort_by_key(|&(start, _, _)| start);
        components.sort_by_key(|&(start, _, _)| start);
        structural_owners.sort_by_key(|&(start, _, _)| start);

        EntityIndex {
            functions,
            components,
            structural_owners,
        }
    }

    /// Find the caller entity for a call at the given position based on relation type
    ///
    /// Different relation types have different caller lookup strategies:
    /// - EventCallback: First tries to find function caller, falls back to component caller
    /// - ParameterBinding: Uses component caller directly
    /// - Other types: Uses function caller only
    pub(crate) fn find_caller_by_type(
        &self,
        call_start: usize,
        relation_type: RelationType,
    ) -> Option<EntityId> {
        match relation_type {
            // For event callbacks: try function first, then component/template
            RelationType::EventCallback => self
                .find_in_functions(call_start)
                .or_else(|| self.find_in_components(call_start)),
            // For parameter bindings: use component/template as caller
            RelationType::ParameterBinding | RelationType::TemplateReference => {
                self.find_in_components(call_start)
            }
            // For other relation types: only look for function callers
            _ => self.find_in_functions(call_start),
        }
    }

    /// Find in function-like entities
    pub(crate) fn find_in_functions(&self, pos: usize) -> Option<EntityId> {
        self.find_in_list(&self.functions, pos)
    }

    /// Find in component-like entities
    pub(crate) fn find_in_components(&self, pos: usize) -> Option<EntityId> {
        self.find_in_list(&self.components, pos)
    }

    /// Find the smallest entity that contains a structural relation position.
    pub(crate) fn find_structural_owner(&self, pos: usize) -> Option<EntityId> {
        self.find_in_list(&self.structural_owners, pos)
    }

    fn find_in_list(&self, entities: &[(usize, usize, EntityId)], pos: usize) -> Option<EntityId> {
        if entities.is_empty() {
            return None;
        }

        let idx = match entities.binary_search_by_key(&pos, |&(start, _, _)| start) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };

        let mut best_id: Option<EntityId> = None;
        let mut best_size = usize::MAX;

        for i in (0..=idx).rev() {
            let (start, end, id) = entities[i];
            if start <= pos && pos < end {
                let size = end - start;
                if size < best_size {
                    best_size = size;
                    best_id = Some(id);
                }
            }
        }

        best_id
    }
}
