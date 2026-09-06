//! Control-flow extractor for function-body control-flow facts
//!
//! This extractor consumes the control-flow query schema and stores raw
//! control-flow facts in a dedicated sidecar. It does not infer semantics from
//! names or signatures; it only uses tree-sitter captures.

use std::sync::Arc;

use crate::parser::extractor::utils::extract_text_without_comments;
use crate::tree_sitter_query::capture::control::{extract_control_kind, is_main_control_capture};
use crate::tree_sitter_query::error::QueryError;
use crate::tree_sitter_query::executor::{QueryExecutor, QueryMatch};
use cce_types::language::Language;
use cce_types::{ControlFlowFact, ControlFlowFactKind, ControlFlowStore, Entity, EntityId};
use tree_sitter::Tree;

/// Control-flow extractor
pub struct ControlFlowExtractor {
    query_executor: Arc<QueryExecutor>,
}

impl ControlFlowExtractor {
    /// Create a new control-flow extractor
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

    /// Check if this language currently supports control-flow queries.
    pub fn supports_language(language: &Language) -> bool {
        matches!(
            language,
            Language::Rust
                | Language::JavaScript
                | Language::Jsx
                | Language::TypeScript
                | Language::Tsx
                | Language::C
                | Language::Cpp
                | Language::CSharp
                | Language::Python
                | Language::Go
                | Language::Java
                | Language::Php
                | Language::Ruby
                | Language::Kotlin
                | Language::Scala
                | Language::Dart
                | Language::Bash
                | Language::Lua
        )
    }

    /// Extract control-flow facts and attach them to the control-flow sidecar.
    pub fn extract(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
        entities: &[Entity],
        control_flow: &mut ControlFlowStore,
    ) -> Result<(), QueryError> {
        if !Self::supports_language(language) {
            return Ok(());
        }

        let matches = self
            .query_executor
            .execute_control_flow_query(tree, source, language)?;

        let entity_index = EntityIndex::new(entities);

        for mat in &matches {
            self.process_match(mat, tree, source, &entity_index, control_flow);
        }

        for entity_control_flow in control_flow.values_mut() {
            entity_control_flow.facts.sort_by(|a, b| {
                a.start_byte
                    .cmp(&b.start_byte)
                    .then_with(|| a.kind.cmp(&b.kind))
            });
        }

        control_flow.retain_non_empty();

        Ok(())
    }

    fn process_match(
        &self,
        mat: &QueryMatch,
        tree: &Tree,
        source: &str,
        entity_index: &EntityIndex,
        control_flow: &mut ControlFlowStore,
    ) {
        let Some(main_capture) = mat
            .captures
            .iter()
            .filter(|capture| is_main_control_capture(&capture.name))
            .min_by_key(|capture| (capture.start_byte, capture.end_byte))
        else {
            return;
        };

        let Some(entity_id) = entity_index.find_by_position(main_capture.start_byte) else {
            return;
        };

        let Some(kind) = extract_control_kind(&main_capture.name) else {
            return;
        };

        let fact_text = extract_text_without_comments(
            tree,
            source,
            main_capture.start_byte,
            main_capture.end_byte,
        );

        let Some(fact_kind) = ControlFlowFactKind::from_capture_label(kind.capture_label()) else {
            return;
        };

        let fact = ControlFlowFact::new(
            fact_kind,
            fact_text,
            main_capture.start_byte,
            main_capture.end_byte,
        );
        if fact.text.is_empty() {
            return;
        }

        // Record the outer `else` continuation range when the fact text
        // carries one, so branch-aware consumers can test byte containment
        // without re-scanning source text.
        let fact = if fact_kind == ControlFlowFactKind::If {
            attach_else_range(fact)
        } else {
            fact
        };

        control_flow.push_fact(entity_id, fact);
    }
}

fn is_control_entity(entity: &Entity) -> bool {
    entity.kind.is_function_like()
}

/// Attach the outer `else` continuation byte range to an `if` fact.
///
/// The range covers the fact text from the `else` keyword to the fact end.
fn attach_else_range(fact: ControlFlowFact) -> ControlFlowFact {
    let Some(offset) = cce_types::find_outer_else_offset(&fact.text) else {
        return fact;
    };
    let start = fact.start_byte.saturating_add(offset);
    let end = fact.end_byte;
    fact.with_else_range(start, end)
}

/// Sorted entity index for locating the function-like entity that owns a control-flow node.
struct EntityIndex {
    functions: Vec<(usize, usize, EntityId)>,
}

impl EntityIndex {
    fn new(entities: &[Entity]) -> Self {
        let mut functions: Vec<_> = entities
            .iter()
            .filter(|entity| is_control_entity(entity))
            .map(|entity| (entity.span.start_byte, entity.span.end_byte, entity.id))
            .collect();

        functions.sort_by_key(|&(start, _, _)| start);

        Self { functions }
    }

    fn find_by_position(&self, pos: usize) -> Option<EntityId> {
        if self.functions.is_empty() {
            return None;
        }

        let idx = match self
            .functions
            .binary_search_by_key(&pos, |&(start, _, _)| start)
        {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };

        for i in (0..=idx).rev() {
            let (start, end, id) = self.functions[i];
            if start <= pos && pos < end {
                let mut best_id = id;
                let mut best_size = end - start;

                for j in (0..i).rev() {
                    let (candidate_start, candidate_end, candidate_id) = self.functions[j];
                    if candidate_start <= pos && pos < candidate_end {
                        let size = candidate_end - candidate_start;
                        if size < best_size {
                            best_size = size;
                            best_id = candidate_id;
                        }
                    } else if candidate_end <= pos {
                        break;
                    }
                }

                return Some(best_id);
            }
        }

        None
    }
}

impl Default for ControlFlowExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast_parser::AstParser;

    #[test]
    fn test_control_flow_extractor_creation() {
        let extractor = ControlFlowExtractor::new();
        let _ = extractor.query_executor.loader().cache_stats();
        assert!(ControlFlowExtractor::supports_language(&Language::Rust));
        assert!(ControlFlowExtractor::supports_language(&Language::Python));
        assert!(ControlFlowExtractor::supports_language(
            &Language::JavaScript
        ));
        assert!(!ControlFlowExtractor::supports_language(&Language::Unknown));
    }

    #[test]
    fn test_control_flow_extractor_attaches_sidecar() {
        let mut parser = AstParser::new();
        let extractor = ControlFlowExtractor::new();
        let code = r#"
fn demo(input: Option<i32>) -> Result<i32, ()> {
    if let Some(v) = input {
        return Ok(v); // inline comment
    }

    match input {
        Some(v) => return Ok(v),
        None => {}
    }

    for i in 0..3 {
        if i == 1 {
            continue;
        }
        break;
    }

    loop {
        return Err(());
    }
}
"#;

        let tree = parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse Rust code")
            .0;

        let entities = crate::parser::extractor::EntityExtractor::new()
            .extract(&tree, code, &Language::Rust)
            .expect("Failed to extract entities");

        let mut control_flow = ControlFlowStore::default();
        extractor
            .extract(&tree, code, &Language::Rust, &entities, &mut control_flow)
            .expect("Failed to extract control flow");

        let demo = entities
            .iter()
            .find(|entity| entity.name == "demo" && entity.kind.is_function_like())
            .expect("demo function entity should exist");

        let demo_control_flow = control_flow
            .get(demo.id)
            .expect("demo control flow entry should exist");
        assert!(!demo_control_flow.facts.is_empty());
        assert!(
            demo_control_flow
                .facts
                .iter()
                .any(|fact| fact.kind == ControlFlowFactKind::If)
        );
        assert!(
            demo_control_flow
                .facts
                .iter()
                .all(|fact| !fact.text.contains("inline comment")),
            "control-flow facts should not keep inline comments"
        );
    }

    #[test]
    fn test_attach_else_range_records_outer_else() {
        let fact = ControlFlowFact::new(
            ControlFlowFactKind::If,
            "if (a) { x(); } else { y(); }",
            10,
            40,
        );
        let fact = attach_else_range(fact);
        assert!(fact.has_else_range());
        let else_start = fact.else_start_byte.expect("else start recorded");
        assert!(fact.contains_byte_in_else(else_start));
        assert!(!fact.contains_byte_in_else(10));
    }

    #[test]
    fn test_attach_else_range_without_else_stays_empty() {
        let fact = ControlFlowFact::new(ControlFlowFactKind::If, "if (a) { x(); }", 10, 26);
        let fact = attach_else_range(fact);
        assert!(!fact.has_else_range());
    }
}
