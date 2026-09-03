use crate::parser::components::Components;
use crate::parser::context::ParseContext;
use crate::parser::extractor::StructuralExtractor;
use cce_types::EntityId;
use cce_types::ParseError;

/// Extract structural relationships from AST.
///
/// Processes structural tree-sitter queries to extract:
/// - Element containment (parent/child in HTML/JSX/Svelte/Vue)
/// - Component usage patterns (constructor calls in templates)
/// - Event callback bindings
/// - Template references
/// - CSS containment (media rules, keyframes, etc.)
///
/// Complements EntityExtraction and RelationExtraction. Processes structural
/// queries that were previously defined but never consumed.
pub(crate) fn run(
    context: &mut ParseContext,
    components: &mut Components,
) -> Result<(), ParseError> {
    let language = context
        .language()
        .ok_or_else(|| ParseError::ast_parsing("Language not detected".to_string()))?;

    if !StructuralExtractor::supports_language(language) {
        return Ok(());
    }

    let tree = context
        .tree
        .as_ref()
        .ok_or_else(|| ParseError::ast_parsing("AST tree not available".to_string()))?;

    let (structural_entities, structural_relations) = components
        .structural_extractor
        .extract(tree, &context.source, language, &context.entities)
        .map_err(|e| {
            ParseError::ast_parsing(format!(
                "Structural extraction failed for file '{}': {}",
                context.file_path, e
            ))
        })?;

    // Merge structural entities with existing entities, adjusting IDs to avoid conflicts
    let base_id = context.entities.len() as u64;
    let adjusted_entities: Vec<_> = structural_entities
        .into_iter()
        .enumerate()
        .map(|(idx, mut entity)| {
            entity.id = EntityId(base_id + idx as u64);
            entity
        })
        .collect();

    context.entities.extend(adjusted_entities);

    // Merge structural relations with existing relations
    context.relations.extend(structural_relations);

    Ok(())
}
