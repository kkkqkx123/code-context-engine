//! Structural extractor for processing structural tree-sitter queries
//!
//! Structural queries capture relationships that go beyond simple entity definitions:
//! - Element containment (parent/child relationships in HTML/JSX/Svelte/Vue)
//! - Template references (ref bindings, id references)
//! - Component usage patterns
//! - Event callback bindings
//! - Scoped style content
//! - CSS containment (media queries containing rules, keyframes containing blocks)
//!
//! These patterns are defined in `structural_query()` functions in scheme files but
//! were previously never consumed by any extractor, causing data loss.

use crate::tree_sitter_query::capture;
use crate::tree_sitter_query::error::QueryError;
use crate::tree_sitter_query::executor::{QueryExecutor, QueryMatch};
use cce_types::language::Language;
use cce_types::{Entity, EntityId, EntityKind, Relation, RelationTarget, RelationType};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use tree_sitter::Tree;

use super::ExtractionContext;
use super::utils;

/// Structural extractor
///
/// Processes structural query results to extract containment relationships,
/// template references, component usage, and other structural patterns.
///
/// This extractor is a complement to EntityExtractor and RelationExtractor.
/// It processes the `structural_query()` results that are defined in scheme
/// files but were previously never consumed.
pub struct StructuralExtractor {
    /// Query executor
    query_executor: Arc<QueryExecutor>,
}

impl StructuralExtractor {
    /// Create a new structural extractor
    pub fn new() -> Self {
        Self {
            query_executor: Arc::new(QueryExecutor::new()),
        }
    }

    /// Create with custom query executor
    pub fn with_executor(executor: Arc<QueryExecutor>) -> Self {
        Self {
            query_executor: executor,
        }
    }

    /// Check if structural queries are supported for this language
    pub fn supports_language(language: &Language) -> bool {
        matches!(
            language,
            Language::Vue | Language::Svelte | Language::Tsx | Language::Css
        )
    }

    /// Extract structural entities and relations from source code
    ///
    /// Returns (new_entities, new_relations) that should be merged into
    /// the main parsing pipeline output.
    pub fn extract(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
        entities: &[Entity],
    ) -> Result<(Vec<Entity>, Vec<Relation>), QueryError> {
        if !Self::supports_language(language) {
            return Ok((Vec::new(), Vec::new()));
        }

        let matches = self
            .query_executor
            .execute_structural_query(tree, source, language)?;

        // Owner lookup: the smallest entity span containing a match position
        // attributes the structural relation to that entity; a match outside
        // every entity belongs to the file itself.
        let mut owner_spans: Vec<(usize, usize, EntityId)> = entities
            .iter()
            .map(|e| (e.span.start_byte, e.span.end_byte, e.id))
            .collect();
        utils::sort_spans_by_start(&mut owner_spans);

        let mut context = ExtractionContext::new(Arc::new(AtomicU64::new(0)));
        let mut entities_out = Vec::new();
        let mut relations = Vec::new();

        for mat in &matches {
            let start = mat.captures.first().map(|c| c.start_byte).unwrap_or(0);
            let owner = utils::find_smallest_containing(&owner_spans, start);
            self.process_contains(mat, &mut context, &mut entities_out, &mut relations, owner);
            self.process_template_refs(mat, &mut context, &mut entities_out, &mut relations, owner);
            self.process_component_usage(mat, &mut relations, owner);
            self.process_event_callbacks(mat, &mut relations, owner);
        }

        Ok((entities_out, relations))
    }

    /// Process containment relationships (@entity.contains.*)
    ///
    /// Creates `RelationType::ElementContains` relations for element parent/child
    /// relationships and CSS containment (media/rules, keyframes/blocks).
    fn process_contains(
        &self,
        mat: &QueryMatch,
        _context: &mut ExtractionContext,
        _entities: &mut Vec<Entity>,
        relations: &mut Vec<Relation>,
        owner: Option<EntityId>,
    ) {
        let parent_capture = mat.captures.iter().find(|c| {
            c.name.contains(capture::CATEGORY_CONTAINS)
                && (c.name.contains(".parent.") || c.name.contains(".rule"))
                && (c.name.ends_with(".name") || c.name.ends_with(".selector"))
        });
        let child_capture = mat.captures.iter().find(|c| {
            c.name.contains(capture::CATEGORY_CONTAINS)
                && (c.name.contains(".child.") || c.name.contains(".nested."))
                && (c.name.ends_with(".name") || c.name.ends_with(".rule"))
        });
        let container_capture = mat.captures.iter().find(|c| {
            (c.name.ends_with(".contains") || c.name.ends_with(".children"))
                && c.name.contains(capture::CATEGORY_CONTAINS)
        });

        let make_relation = |target: String, rtype: RelationType, span: cce_types::Span| match owner
        {
            Some(id) => Relation::new(id, RelationTarget::unresolved(target), rtype, span),
            None => Relation::file_relation(0, RelationTarget::unresolved(target), rtype, span),
        };

        if let (Some(parent), Some(child)) = (parent_capture, child_capture) {
            let container = container_capture.unwrap_or(parent);
            let span = utils::create_span_from_capture(container);
            relations.push(make_relation(
                parent.text.clone(),
                RelationType::ElementContains,
                span,
            ));
            relations.push(make_relation(
                child.text.clone(),
                RelationType::ElementContains,
                span,
            ));
        } else if let Some(container) = container_capture {
            let span = utils::create_span_from_capture(container);
            relations.push(make_relation(
                container.text.clone(),
                RelationType::ElementContains,
                span,
            ));
        }

        let at_rule = mat.captures.iter().find(|c| {
            c.name.contains("media") || c.name.contains("keyframes") || c.name.contains("supports")
        });

        if let Some(at) = at_rule {
            let span = utils::create_span_from_capture(at);
            relations.push(make_relation(at.text.clone(), RelationType::Contains, span));
        }
    }

    /// Process template references (@entity.template.*)
    fn process_template_refs(
        &self,
        mat: &QueryMatch,
        context: &mut ExtractionContext,
        entities: &mut Vec<Entity>,
        relations: &mut Vec<Relation>,
        owner: Option<EntityId>,
    ) {
        let ref_capture = mat
            .captures
            .iter()
            .find(|c| c.name.contains("entity.template_reference"));
        let value_capture = mat
            .captures
            .iter()
            .find(|c| c.name.contains(".value") && c.name.contains("entity.template"));

        if let (Some(_ref), Some(value)) = (ref_capture.or(value_capture), value_capture) {
            let entity = Entity::new(
                context.next_entity_id(),
                EntityKind::Variable,
                value.text.clone(),
                utils::create_span_from_capture(value),
            );
            entities.push(entity);
        }

        let html_ref = mat.captures.iter().find(|c| {
            c.name.contains("entity.template_reference")
                || c.name.contains("entity.template.ref")
                || c.name.contains("entity.template.id")
        });
        if let Some(r) = html_ref {
            let span = utils::create_span_from_capture(r);
            let relation = match owner {
                Some(id) => Relation::new(
                    id,
                    RelationTarget::unresolved(r.text.clone()),
                    RelationType::TemplateReference,
                    span,
                ),
                None => Relation::file_relation(
                    0,
                    RelationTarget::unresolved(r.text.clone()),
                    RelationType::TemplateReference,
                    span,
                ),
            };
            relations.push(relation);
        }
    }

    /// Process component usage (@call.constructor.component.*)
    fn process_component_usage(
        &self,
        mat: &QueryMatch,
        relations: &mut Vec<Relation>,
        owner: Option<EntityId>,
    ) {
        let name_capture = mat.captures.iter().find(|c| {
            c.name.starts_with("call.constructor.component") && c.name.ends_with(".name")
        });

        if let Some(name) = name_capture {
            let span = utils::create_span_from_capture(name);
            let relation = match owner {
                Some(id) => Relation::new(
                    id,
                    RelationTarget::unresolved(name.text.clone()),
                    RelationType::ConstructorCall,
                    span,
                ),
                None => Relation::file_relation(
                    0,
                    RelationTarget::unresolved(name.text.clone()),
                    RelationType::ConstructorCall,
                    span,
                ),
            };
            relations.push(relation);
        }
    }

    /// Process event callbacks (@call.callback.event.*)
    fn process_event_callbacks(
        &self,
        mat: &QueryMatch,
        relations: &mut Vec<Relation>,
        owner: Option<EntityId>,
    ) {
        let event_capture = mat.captures.iter().find(|c| {
            c.name.starts_with("call.callback.event")
                && (c.name.ends_with(".name") || c.name.ends_with(".handler"))
        });

        if let Some(event) = event_capture {
            let span = utils::create_span_from_capture(event);
            let relation = match owner {
                Some(id) => Relation::new(
                    id,
                    RelationTarget::unresolved(event.text.clone()),
                    RelationType::EventCallback,
                    span,
                ),
                None => Relation::file_relation(
                    0,
                    RelationTarget::unresolved(event.text.clone()),
                    RelationType::EventCallback,
                    span,
                ),
            };
            relations.push(relation);
        }
    }
}

impl Default for StructuralExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast_parser::AstParser;

    #[test]
    fn test_structural_extractor_creation() {
        let _extractor = StructuralExtractor::new();
        assert!(StructuralExtractor::supports_language(&Language::Vue));
        assert!(StructuralExtractor::supports_language(&Language::Svelte));
        assert!(StructuralExtractor::supports_language(&Language::Tsx));
        assert!(StructuralExtractor::supports_language(&Language::Css));
        assert!(!StructuralExtractor::supports_language(&Language::Rust));
        assert!(!StructuralExtractor::supports_language(&Language::Python));
    }

    #[test]
    fn test_structural_extract_svelte_returns_ok() {
        let mut ast_parser = AstParser::new();
        let extractor = StructuralExtractor::new();

        let code = r#"
<script>
  let count = 0;
</script>

<button on:click={() => count++}>
  Clicks: {count}
</button>

<style>
  button { color: red; }
</style>
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::Svelte)
            .expect("Failed to parse")
            .0;

        let result = extractor.extract(&tree, code, &Language::Svelte, &[]);
        assert!(result.is_ok());

        let (_entities, _relations) = result.expect("Extraction failed");
    }

    #[test]
    fn test_structural_extract_vue_returns_ok() {
        let mut ast_parser = AstParser::new();
        let extractor = StructuralExtractor::new();

        let code = r#"
<template>
  <div class="container">
    <ChildComponent :prop="value" @click="handler" />
  </div>
</template>

<script setup>
import ChildComponent from './Child.vue'
const value = 'test'
function handler() {}
</script>
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::Vue)
            .expect("Failed to parse")
            .0;

        let result = extractor.extract(&tree, code, &Language::Vue, &[]);
        assert!(result.is_ok());

        let (_entities, _relations) = result.expect("Extraction failed");
    }

    #[test]
    fn test_structural_extract_css_returns_ok() {
        let mut ast_parser = AstParser::new();
        let extractor = StructuralExtractor::new();

        let code = r#"
@media (max-width: 768px) {
  .container { width: 100%; }
}

@keyframes slide {
  from { transform: translateX(0); }
  to { transform: translateX(100px); }
}
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::Css)
            .expect("Failed to parse")
            .0;

        let result = extractor.extract(&tree, code, &Language::Css, &[]);
        assert!(result.is_ok());

        let (_entities, _relations) = result.expect("Extraction failed");
    }

    #[test]
    fn test_structural_extract_svelte_template_reference() {
        // Regression: `bind:this` must produce a TemplateReference
        // relation. Previously the misspelled `entity.templateerence`
        // predicate and the duplicate `entity.template.bind` pattern meant
        // Svelte never produced one.
        let mut ast_parser = AstParser::new();
        let extractor = StructuralExtractor::new();

        let code = r#"
<script>
  let el;
</script>

<div bind:this={el}></div>
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::Svelte)
            .expect("Failed to parse")
            .0;

        let (entities, relations) = extractor
            .extract(&tree, code, &Language::Svelte, &[])
            .expect("Extraction failed");

        let refs: Vec<_> = relations
            .iter()
            .filter(|r| r.relation_type == RelationType::TemplateReference)
            .collect();
        assert!(
            !refs.is_empty(),
            "expected TemplateReference relations in {relations:?}"
        );

        // The variable entity for the bound reference must exist exactly
        // once (the duplicate bind pattern used to create two).
        let vars: Vec<_> = entities
            .iter()
            .filter(|e| e.kind == EntityKind::Variable)
            .collect();
        assert_eq!(
            vars.len(),
            1,
            "expected exactly one variable entity, got {vars:?}"
        );
        assert_eq!(vars[0].name, "el");
    }
}
