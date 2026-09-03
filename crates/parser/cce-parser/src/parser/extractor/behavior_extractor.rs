//! Behavior extractor for function-body behavior facts
//!
//! This extractor consumes the behavior query schema and stores raw behavior
//! facts in a dedicated sidecar. It does not infer semantics from names or
//! signatures; it only uses tree-sitter captures.

use std::sync::Arc;

use crate::parser::extractor::utils::extract_text_without_comments;
use crate::tree_sitter_query::capture::behavior::{
    extract_behavior_kind, is_main_behavior_capture,
};
use crate::tree_sitter_query::error::QueryError;
use crate::tree_sitter_query::executor::{QueryExecutor, QueryMatch};
use cce_types::language::Language;
use cce_types::{BehaviorFact, BehaviorFactKind, BehaviorStore, Entity, EntityId};
use tree_sitter::Tree;

/// Behavior extractor
pub struct BehaviorExtractor {
    query_executor: Arc<QueryExecutor>,
}

impl BehaviorExtractor {
    /// Create a new behavior extractor
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

    /// Check if this language currently supports behavior queries.
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

    /// Extract behavior facts and attach them to the behavior sidecar.
    pub fn extract(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
        entities: &[Entity],
        behavior: &mut BehaviorStore,
    ) -> Result<(), QueryError> {
        if !Self::supports_language(language) {
            return Ok(());
        }

        let matches = self
            .query_executor
            .execute_behavior_query(tree, source, language)?;

        let entity_index = EntityIndex::new(entities);

        for mat in &matches {
            self.process_match(mat, tree, source, &entity_index, behavior);
        }

        for entity_behavior in behavior.values_mut() {
            entity_behavior.facts.sort_by(|a, b| {
                a.start_byte
                    .cmp(&b.start_byte)
                    .then_with(|| a.kind.cmp(&b.kind))
            });
        }

        behavior.retain_non_empty();

        Ok(())
    }

    fn process_match(
        &self,
        mat: &QueryMatch,
        tree: &Tree,
        source: &str,
        entity_index: &EntityIndex,
        behavior: &mut BehaviorStore,
    ) {
        let Some(main_capture) = mat
            .captures
            .iter()
            .filter(|capture| is_main_behavior_capture(&capture.name))
            .min_by_key(|capture| (capture.start_byte, capture.end_byte))
        else {
            return;
        };

        let Some(entity_id) = entity_index.find_by_position(main_capture.start_byte) else {
            return;
        };

        let Some(kind) = extract_behavior_kind(&main_capture.name) else {
            return;
        };

        let (expanded_start, expanded_end) = super::utils::expand_to_statement_boundary(
            tree,
            main_capture.start_byte,
            main_capture.end_byte,
        );

        let fact_text = extract_text_without_comments(tree, source, expanded_start, expanded_end);

        let Some(fact_kind) = BehaviorFactKind::from_capture_label(kind.capture_label()) else {
            return;
        };

        let fact = BehaviorFact::new(fact_kind, fact_text, expanded_start, expanded_end);
        if fact.text.is_empty() {
            return;
        }

        behavior.push_fact(entity_id, fact);
    }
}

fn is_behavior_entity(entity: &Entity) -> bool {
    entity.kind.is_function_like()
}

/// Sorted entity index for locating the function-like entity that owns a behavior node.
struct EntityIndex {
    functions: Vec<(usize, usize, EntityId)>,
}

impl EntityIndex {
    fn new(entities: &[Entity]) -> Self {
        let mut functions: Vec<_> = entities
            .iter()
            .filter(|entity| is_behavior_entity(entity))
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

impl Default for BehaviorExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast_parser::AstParser;

    #[test]
    fn test_behavior_extractor_creation() {
        let extractor = BehaviorExtractor::new();
        let _ = extractor.query_executor.loader().cache_stats();
        assert!(BehaviorExtractor::supports_language(&Language::Rust));
        assert!(BehaviorExtractor::supports_language(&Language::Python));
        assert!(BehaviorExtractor::supports_language(&Language::JavaScript));
        assert!(!BehaviorExtractor::supports_language(&Language::Unknown));
    }

    #[test]
    fn test_behavior_extractor_attaches_sidecar() {
        let mut parser = AstParser::new();
        let extractor = BehaviorExtractor::new();
        let code = r#"
fn demo(input: Option<i32>) -> Result<i32, ()> {
    let mut value = 0;
    let _ = &value;
    let _ = maybe_result()?;
    let _ = 1 << 2;
    let _ = 8 >> 1;
    value <<= 1;
    value >>= 1;
    process(config.max_retries);
    value = replace_value();
    println!("starting server");

    Ok(value)
}
"#;

        let tree = parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse Rust code")
            .0;

        let entities = crate::parser::extractor::EntityExtractor::new()
            .extract(&tree, code, &Language::Rust)
            .expect("Failed to extract entities");

        let mut behavior = BehaviorStore::default();
        extractor
            .extract(&tree, code, &Language::Rust, &entities, &mut behavior)
            .expect("Failed to extract behavior");

        let demo = entities
            .iter()
            .find(|entity| entity.name == "demo" && entity.kind.is_function_like())
            .expect("demo function entity should exist");

        let demo_behavior = behavior
            .get(demo.id)
            .expect("demo behavior entry should exist");
        assert!(!demo_behavior.facts.is_empty());
        assert!(
            demo_behavior
                .facts
                .iter()
                .any(|fact| fact.kind == BehaviorFactKind::DataBind)
        );
        assert!(
            demo_behavior
                .facts
                .iter()
                .any(|fact| fact.kind == BehaviorFactKind::OpShiftLeft)
        );
        assert!(
            demo_behavior
                .facts
                .iter()
                .any(|fact| fact.kind == BehaviorFactKind::OpShiftRight)
        );
        let data_statements: Vec<_> = demo_behavior
            .facts
            .iter()
            .filter(|fact| fact.kind == BehaviorFactKind::DataStatement)
            .collect();
        assert!(
            !data_statements.is_empty(),
            "standalone statements should produce DataStatement facts"
        );
        assert!(
            data_statements
                .iter()
                .any(|fact| fact.text.contains("process(config.max_retries)"))
        );
        assert!(
            data_statements
                .iter()
                .any(|fact| fact.text == "value = replace_value();")
        );
        assert!(
            data_statements
                .iter()
                .any(|fact| fact.text.contains("println!(\"starting server\")"))
        );
        assert!(
            data_statements
                .iter()
                .all(|fact| !fact.text.contains("comment")),
            "behavior facts should not keep inline comments"
        );
        assert!(
            demo_behavior
                .facts
                .iter()
                .all(|fact| !fact.text.contains("comment")),
            "behavior facts should not keep inline comments"
        );
    }
}
