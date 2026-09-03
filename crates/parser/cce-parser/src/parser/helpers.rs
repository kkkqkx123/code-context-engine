//! Helper functions for parsing operations
//!
//! This module contains utility functions used by the parsing pipeline
//! for embedded block parsing and symbol table construction.

use crate::parser::embedded_types::{BlockRelation, BlockRelationType, EmbeddedBlock};
use cce_types::language::Language;
use cce_types::{Entity, EntityKind, ParseError, RawRelationData};
use std::collections::HashMap;
use tree_sitter::Tree;

/// Result of parsing embedded blocks from SFC files
///
/// Contains the extracted blocks, entities, and relations from embedded code sections.
#[derive(Debug, Clone)]
pub struct EmbeddedParseResult {
    /// Extracted embedded blocks (e.g., <script>, <template> in Vue)
    pub blocks: Vec<EmbeddedBlock>,
    /// Entities extracted from embedded blocks
    pub entities: Vec<Entity>,
    /// Relations extracted from embedded blocks
    pub relations: Vec<RawRelationData>,
}

/// Parse embedded blocks from SFC files
pub fn parse_embedded_blocks(
    tree: &Tree,
    source: &str,
    language: &Language,
    existing_entities: &[Entity],
    embedded_parser: &mut crate::parser::EmbeddedParser,
) -> Result<EmbeddedParseResult, ParseError> {
    // Extract blocks
    let blocks = embedded_parser.extract_blocks(tree, source, language)?;

    let mut all_block_entities = Vec::new();
    let mut all_block_relations = Vec::new();

    // Calculate starting entity ID
    let base_id = existing_entities.iter().map(|e| e.id.0).max().unwrap_or(0) + 1;

    // Parse each block
    let mut current_id = base_id;
    for block in &blocks {
        if !block.is_parseable() {
            continue;
        }

        match embedded_parser.parse_block(block, current_id) {
            Ok((mut entities, relations)) => {
                current_id += entities.len() as u64;
                all_block_entities.append(&mut entities);
                all_block_relations.extend(relations);
            }
            Err(e) => {
                tracing::warn!("Failed to parse embedded block: {}", e);
            }
        }
    }

    Ok(EmbeddedParseResult {
        blocks,
        entities: all_block_entities,
        relations: all_block_relations,
    })
}

/// Build local symbol table from entities
pub fn build_symbol_table(
    entities: &[Entity],
) -> std::collections::HashMap<String, Vec<cce_types::EntityId>> {
    let mut symbols: std::collections::HashMap<String, Vec<cce_types::EntityId>> =
        std::collections::HashMap::new();

    for entity in entities {
        symbols
            .entry(entity.name.clone())
            .or_default()
            .push(entity.id);
    }

    symbols
}

/// Resolve cross-block relations in SFC files (Vue/Svelte/TSX)
///
/// After embedded blocks are parsed, this function identifies relations
/// between entities in different blocks:
/// - Template component usage → Script imports/definitions
/// - Template event handlers → Script methods
/// - Template class/id references → Style selectors
///
/// # Arguments
/// * `embedded_blocks` - List of embedded blocks (template, script, style)
/// * `all_entities` - All entities from both main file and embedded blocks
///
/// # Returns
/// * `Vec<BlockRelation>` - Resolved cross-block relations
pub fn resolve_cross_block_relations(
    embedded_blocks: &[EmbeddedBlock],
    all_entities: &[Entity],
) -> Vec<BlockRelation> {
    if embedded_blocks.is_empty() {
        return Vec::new();
    }

    // Group entities by the block they belong to
    let (template_entities, script_entities, style_entities) =
        group_entities_by_block(embedded_blocks, all_entities);

    let mut relations = Vec::new();

    // 1. Template component → Script import/component definition
    relations.extend(resolve_component_usage(
        &template_entities,
        &script_entities,
    ));

    // 2. Template event handler → Script method
    relations.extend(resolve_event_handlers(&template_entities, &script_entities));

    // 3. Template class/id → Style selector
    relations.extend(resolve_template_to_style(
        &template_entities,
        &style_entities,
    ));

    relations
}

/// Group entities by which block they belong to
fn group_entities_by_block<'a>(
    blocks: &[EmbeddedBlock],
    entities: &'a [Entity],
) -> (Vec<&'a Entity>, Vec<&'a Entity>, Vec<&'a Entity>) {
    let mut template = Vec::new();
    let mut script = Vec::new();
    let mut style = Vec::new();

    for entity in entities {
        let block_type = blocks.iter().find(|b| b.contains_span(&entity.span));
        use crate::parser::embedded_types::BlockType;
        match block_type.map(|b| &b.block_type) {
            Some(BlockType::Template) => template.push(entity),
            Some(BlockType::Script) => script.push(entity),
            Some(BlockType::Style) => style.push(entity),
            _ => {} // Entity belongs to main file, skip
        }
    }

    (template, script, style)
}

/// Resolve component usage: template PascalCase elements → script imports/functions
fn resolve_component_usage(
    template_entities: &[&Entity],
    script_entities: &[&Entity],
) -> Vec<BlockRelation> {
    let mut relations = Vec::new();

    // Build a set of script entity names for fast lookup
    let script_names: HashMap<&str, &Entity> = script_entities
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                EntityKind::Function | EntityKind::Class | EntityKind::Variable
            )
        })
        .map(|e| (e.name.as_str(), *e))
        .collect();

    // Find template entities with PascalCase names (potential components)
    for template_entity in template_entities {
        let name = template_entity.name.as_str();

        // PascalCase check: starts with uppercase
        if name.chars().next().is_some_and(|c| c.is_uppercase()) {
            if let Some(script_entity) = script_names.get(name) {
                relations.push(BlockRelation {
                    source_id: template_entity.id,
                    target_id: script_entity.id,
                    relation_type: BlockRelationType::ComponentUsage,
                    description: format!(
                        "Template component `{}` references script definition",
                        name
                    ),
                });
            }
        }
    }

    relations
}

/// Resolve event handlers: template event bindings → script methods
fn resolve_event_handlers(
    template_entities: &[&Entity],
    script_entities: &[&Entity],
) -> Vec<BlockRelation> {
    let mut relations = Vec::new();

    // Build a set of script function names
    let function_names: HashMap<&str, &Entity> = script_entities
        .iter()
        .filter(|e| matches!(e.kind, EntityKind::Function | EntityKind::Method))
        .map(|e| (e.name.as_str(), *e))
        .collect();

    // Check template entities with directive/event metadata
    for entity in template_entities {
        // Check if entity has event handler attributes
        for (attr_key, attr_value) in &entity.attributes {
            let handler_name = attr_value.trim();
            if !handler_name.is_empty()
                && (attr_key.starts_with('@') || attr_key.starts_with("on:"))
            {
                if let Some(script_entity) = function_names.get(handler_name) {
                    relations.push(BlockRelation {
                        source_id: entity.id,
                        target_id: script_entity.id,
                        relation_type: BlockRelationType::EventHandler,
                        description: format!(
                            "Template event `{}` binds to script method `{}`",
                            attr_key, handler_name
                        ),
                    });
                }
            }
        }

        // Also check metadata for event handler references
        if let Some(directives) = entity.metadata.get("directives") {
            for handler_name in directives.split(',').map(|s| s.trim()) {
                if !handler_name.is_empty() && function_names.contains_key(handler_name) {
                    if let Some(script_entity) = function_names.get(handler_name) {
                        relations.push(BlockRelation {
                            source_id: entity.id,
                            target_id: script_entity.id,
                            relation_type: BlockRelationType::EventHandler,
                            description: format!(
                                "Template directive references script method `{}`",
                                handler_name
                            ),
                        });
                    }
                }
            }
        }
    }

    relations
}

/// Resolve template element class/id references to style selectors
fn resolve_template_to_style(
    template_entities: &[&Entity],
    style_entities: &[&Entity],
) -> Vec<BlockRelation> {
    let mut relations = Vec::new();

    // Build a set of style selector names
    let style_selector_map: HashMap<&str, &Entity> = style_entities
        .iter()
        .map(|e| (e.name.as_str(), *e))
        .collect();

    // Match template classes with style selectors (class selectors use
    // ".name" pattern). The relation flows FROM the style selector TO the
    // template element it styles: source_id is the style entity and
    // target_id is the template element.
    for template_entity in template_entities {
        if let Some(class_attr) = template_entity.attributes.get("class") {
            for class_name in class_attr.split_whitespace() {
                if class_name.is_empty() {
                    continue;
                }
                let selector_name = format!(".{}", class_name);
                if let Some(style_entity) = style_selector_map.get(selector_name.as_str()) {
                    relations.push(BlockRelation {
                        source_id: style_entity.id,
                        target_id: template_entity.id,
                        relation_type: BlockRelationType::StyleToTemplate,
                        description: format!(
                            "Style selector `.{}` matches template element class",
                            class_name
                        ),
                    });
                }
            }
        }

        if let Some(id_attr) = template_entity.attributes.get("id") {
            if id_attr.is_empty() {
                continue;
            }
            let selector_name = format!("#{}", id_attr);
            if let Some(style_entity) = style_selector_map.get(selector_name.as_str()) {
                relations.push(BlockRelation {
                    source_id: style_entity.id,
                    target_id: template_entity.id,
                    relation_type: BlockRelationType::StyleToTemplate,
                    description: format!(
                        "Style selector `#{}` matches template element id",
                        id_attr
                    ),
                });
            }
        }
    }

    relations
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::entity::EntityId;

    fn make_entity(id: u32, name: &str) -> Entity {
        Entity {
            id: EntityId(id.into()),
            kind: EntityKind::Element,
            name: name.to_string(),
            signature: String::new(),
            parameters: Vec::new(),
            return_type: None,
            span: cce_types::Span::default(),
            depth: 0,
            parent: None,
            children: Vec::new(),
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            metadata: HashMap::new(),
            is_stdlib: false,
            subtype: None,
            stdlib_category: None,
        }
    }

    #[test]
    fn style_to_template_points_at_template_not_style() {
        // Regression: the relation must flow FROM the style selector TO
        // the template element it styles. Previously both ends pointed at
        // the style entity (a self-loop).
        let mut template = make_entity(1, "div.container");
        let style = make_entity(2, ".container");
        let mut template_attrs = HashMap::new();
        template_attrs.insert("class".to_string(), "container".to_string());
        template.attributes = template_attrs;

        let relations = resolve_template_to_style(&[&template], &[&style]);
        assert_eq!(relations.len(), 1, "relations: {relations:?}");
        assert_eq!(
            relations[0].source_id, style.id,
            "source must be the style entity"
        );
        assert_eq!(
            relations[0].target_id, template.id,
            "target must be the template element"
        );
        assert_eq!(
            relations[0].relation_type,
            BlockRelationType::StyleToTemplate
        );
        assert_ne!(
            relations[0].source_id, relations[0].target_id,
            "the relation must not be a self-loop"
        );
    }

    #[test]
    fn style_to_template_matches_id_selector() {
        let mut template = make_entity(1, "div#app");
        let style = make_entity(2, "#app");
        let mut template_attrs = HashMap::new();
        template_attrs.insert("id".to_string(), "app".to_string());
        template.attributes = template_attrs;

        let relations = resolve_template_to_style(&[&template], &[&style]);
        assert_eq!(relations.len(), 1, "relations: {relations:?}");
        assert_eq!(relations[0].source_id, style.id);
        assert_eq!(relations[0].target_id, template.id);
    }
}
