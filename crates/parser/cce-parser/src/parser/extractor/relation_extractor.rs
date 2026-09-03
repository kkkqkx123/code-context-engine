//! Relation extractor for semantic relation extraction
//!
//! Extracts relations (calls, dependencies) between entities using tree-sitter queries.
//!
//! # Design Principles
//!
//! - **Deferred Resolution**: Calee names are stored as strings, resolved later by IndexBuilder
//! - **Caller Identification**: Uses entity stack to find current caller
//! - **Stateless Output**: Output structures are self-contained

mod entity_index;
mod relation_handlers;

use crate::parser::stdlib::StdlibDetector;
use crate::tree_sitter_query::error::QueryError;
use crate::tree_sitter_query::executor::{Capture, QueryExecutor, QueryMatch};
use cce_types::language::Language;
use cce_types::{Entity, Relation, RelationTarget, StdlibCategory};
use std::sync::Arc;
use tree_sitter::Tree;

use super::utils;
use entity_index::EntityIndex;
use relation_handlers::{
    build_full_callee_name, determine_call_relation_type, determine_dependency_relation_type,
    extract_impl_block_relations, find_callee_capture, find_dependency_capture,
    normalize_callee_name,
};

/// Relation extractor
///
/// Extracts relations between entities using tree-sitter queries.
pub struct RelationExtractor {
    /// Query executor
    query_executor: Arc<QueryExecutor>,
}

impl RelationExtractor {
    /// Create a new relation extractor
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

    /// Extract relations from source code
    ///
    /// # Arguments
    ///
    /// * `tree` - Parsed tree-sitter tree
    /// * `source` - Source code string
    /// * `language` - Programming language
    /// * `entities` - Previously extracted entities (for source identification)
    /// * `file_id` - File ID for file-level relations (imports, exports, etc.)
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<Relation>)` - List of relations (unresolved)
    /// * `Err(QueryError)` - If query execution fails
    pub fn extract(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
        entities: &[Entity],
        file_id: Option<i64>,
    ) -> Result<Vec<Relation>, QueryError> {
        let mut relations = Vec::new();

        // Extract call relations
        let call_relations = self.extract_calls(tree, source, language, entities, file_id)?;
        relations.extend(call_relations);

        // Extract dependency relations (file-level)
        let dep_relations = self.extract_dependencies(tree, source, language, entities, file_id)?;
        relations.extend(dep_relations);

        Ok(relations)
    }

    /// Extract call relations
    fn extract_calls(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
        entities: &[Entity],
        file_id: Option<i64>,
    ) -> Result<Vec<Relation>, QueryError> {
        let matches = self
            .query_executor
            .execute_call_query(tree, source, language)?;

        // Build entity index for efficient caller lookup
        let entity_index = EntityIndex::new(entities);

        let mut relations = Vec::new();

        for mat in &matches {
            if let Some(relation) =
                self.process_call_match(mat, &entity_index, language, file_id, tree, source)
            {
                relations.push(relation);
            }
        }

        Ok(relations)
    }

    /// Extract dependency relations
    fn extract_dependencies(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
        entities: &[Entity],
        file_id: Option<i64>,
    ) -> Result<Vec<Relation>, QueryError> {
        let matches = self
            .query_executor
            .execute_dependency_query(tree, source, language)?;

        let entity_index = EntityIndex::new(entities);
        let mut relations = Vec::new();

        for mat in &matches {
            if let Some(dep_capture) = find_dependency_capture(mat) {
                if let Some(relation) =
                    self.process_dependency_match(dep_capture, file_id, &entity_index)
                {
                    relations.push(relation);
                }
            }
        }

        // Derive impl-block structural relations from parsed entities.
        // Impl blocks are parsed once during entity extraction; the
        // dependency query does not re-match impl_item nodes.
        relations.extend(extract_impl_block_relations(entities));

        Ok(relations)
    }

    /// Process a call match and extract relation
    ///
    /// Calls without a containing entity (module-level statements, top-level
    /// script code) are emitted as file-level relations instead of being
    /// dropped, so module-level calls survive into the relation graph.
    fn process_call_match(
        &self,
        mat: &QueryMatch,
        index: &EntityIndex,
        language: &Language,
        file_id: Option<i64>,
        tree: &Tree,
        source: &str,
    ) -> Option<Relation> {
        // Find the callee name
        let callee_capture = find_callee_capture(mat)?;

        // Determine relation type first (needed for caller lookup strategy)
        let relation_type = determine_call_relation_type(&callee_capture.name);

        // Find caller using entity index with relation-type-aware lookup
        let call_start = mat.captures.first().map(|c| c.start_byte).unwrap_or(0);
        let caller_id = index.find_caller_by_type(call_start, relation_type);

        // Create span from capture
        let span = utils::create_span_from_capture(callee_capture);
        let callee_name = build_full_callee_name(mat, language, tree, source)
            .unwrap_or_else(|| normalize_callee_name(&callee_capture.text));

        // Pre-compute argument count using AST when available.
        // This avoids re-scanning source text later in local_call_resolver.
        let argument_count = if relation_type.is_call() {
            // Find the call expression node using the callee capture's byte range.
            // The callee capture is typically a child of the call expression,
            // so we walk up to find the call_expression ancestor.
            let callee_node = tree
                .root_node()
                .descendant_for_byte_range(callee_capture.start_byte, callee_capture.end_byte);
            callee_node.and_then(|node| {
                // Walk up to find the call_expression ancestor
                let mut current = Some(node);
                while let Some(n) = current {
                    if n.kind() == "call_expression"
                        || n.kind() == "macro_invocation"
                        || n.kind() == "function_call"
                    {
                        return cce_parser_core::count_call_arguments_from_node(n);
                    }
                    current = n.parent();
                }
                None
            })
        } else {
            None
        };

        // Detect stdlib and set its category. This is the PRIMARY detection point for
        // relations (the single source of truth for RawRelationData.stdlib_category).
        //
        // # Design: Why Detect Here?
        //
        // We detect stdlib at relation extraction time (not just during entity parsing)
        // because:
        // 1. Relations reference targets that may be external (different files/packages)
        // 2. We use is_stdlib_by_type() with RelationType for higher accuracy than
        //    simple name matching
        // 3. This ensures RawRelationData.stdlib_category is always populated
        //
        // For entities that ARE stdlib (detected in mark_stdlib), this detection
        // confirms and provides additional categorization based on relation type.
        //
        // See STDLIB_SUMMARY.md for design rationale and architecture.
        let stdlib_category =
            if StdlibDetector::is_stdlib_by_type(&callee_name, &relation_type, language) {
                // Get the specific category from the language-specific detector
                match language {
                    cce_types::Language::JavaScript | cce_types::Language::TypeScript => {
                        crate::parser::stdlib::javascript::JavaScriptStdlibDetector::get_category(
                            &callee_name,
                        )
                    }
                    cce_types::Language::Rust => {
                        crate::parser::stdlib::rust::RustStdlibDetector::get_category(&callee_name)
                    }
                    cce_types::Language::Python => {
                        crate::parser::stdlib::python::PythonStdlibDetector::get_category(
                            &callee_name,
                        )
                    }
                    cce_types::Language::Go => {
                        crate::parser::stdlib::go::GoStdlibDetector::get_category(&callee_name)
                    }
                    cce_types::Language::Java => {
                        crate::parser::stdlib::java::JavaStdlibDetector::get_category(&callee_name)
                    }
                    cce_types::Language::CSharp => {
                        crate::parser::stdlib::csharp::CSharpStdlibDetector::get_category(
                            &callee_name,
                        )
                    }
                    _ => Some(StdlibCategory::Other),
                }
            } else {
                None
            };

        Some(match caller_id {
            Some(caller_id) => Relation::new(
                caller_id,
                RelationTarget::unresolved(callee_name),
                relation_type,
                span,
            )
            .with_stdlib_category(stdlib_category)
            .with_argument_count(argument_count),
            // Module-level call: no containing entity. Keep the edge as a
            // file-level relation so it survives into the graph instead
            // of being silently dropped.
            None => Relation::file_relation(
                file_id.unwrap_or(0),
                RelationTarget::unresolved(callee_name),
                relation_type,
                span,
            )
            .with_stdlib_category(stdlib_category)
            .with_argument_count(argument_count),
        })
    }

    /// Process a dependency capture and extract relation
    fn process_dependency_match(
        &self,
        dep_capture: &Capture,
        file_id: Option<i64>,
        index: &EntityIndex,
    ) -> Option<Relation> {
        // Create span from capture
        let span = utils::create_span_from_capture(dep_capture);

        // Determine dependency type from capture name
        let relation_type = determine_dependency_relation_type(&dep_capture.name);

        if relation_type.is_structural() {
            let caller_id = index.find_structural_owner(dep_capture.start_byte)?;
            Some(Relation::entity_relation(
                caller_id.0 as i64,
                RelationTarget::unresolved(normalize_callee_name(&dep_capture.text)),
                relation_type,
                span,
            ))
        } else {
            // For file-level dependencies, the caller is the file itself.
            let caller_id = file_id.unwrap_or(0);
            let dst_name = normalize_callee_name(&dep_capture.text);
            if dst_name.is_empty() {
                return None;
            }
            Some(Relation::file_relation(
                caller_id,
                RelationTarget::unresolved(dst_name),
                relation_type,
                span,
            ))
        }
    }

    // Note: resolve_local_calls and calculate_call_order have been moved to
    // relation::LocalCallResolver to maintain separation of concerns:
    // - parser module: Extracts raw semantic data
    // - relation module: Resolves and indexes relationships
}

impl Default for RelationExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast_parser::AstParser;
    use crate::parser::extractor::entity_extractor::EntityExtractor;
    use cce_types::RelationType;

    #[test]
    fn test_extract_rust_calls() {
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let relation_extractor = RelationExtractor::new();

        let code = r#"
fn foo() -> i32 {
    1
}

fn bar() -> i32 {
    foo() + 1
}
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse")
            .0;

        let entities = entity_extractor
            .extract(&tree, code, &Language::Rust)
            .expect("Failed to extract entities");

        let relations = relation_extractor
            .extract(&tree, code, &Language::Rust, &entities, Some(1))
            .expect("Failed to extract relations");

        // Should find the call from bar to foo
        let _calls: Vec<_> = relations
            .iter()
            .filter(|r| r.relation_type.is_call())
            .collect();

        // Note: The actual number depends on query results
        // This test just verifies the extraction doesn't fail
        assert!(!entities.is_empty());
    }

    #[test]
    fn test_extract_rust_trait_impl_relations() {
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let relation_extractor = RelationExtractor::new();

        let code = r#"
trait MyTrait {
    fn f(&self);
}

struct MyStruct;

impl MyStruct {
    fn inherent(&self) {}
}

impl MyTrait for MyStruct {
    fn f(&self) {}
}
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse")
            .0;

        let entities = entity_extractor
            .extract(&tree, code, &Language::Rust)
            .expect("Failed to extract entities");

        let relations = relation_extractor
            .extract(&tree, code, &Language::Rust, &entities, Some(1))
            .expect("Failed to extract relations");

        let implementations: Vec<_> = relations
            .iter()
            .filter(|r| r.relation_type == RelationType::Implementation)
            .collect();
        let impl_associations: Vec<_> = relations
            .iter()
            .filter(|r| r.relation_type == RelationType::ImplAssociation)
            .collect();

        // Trait impl block yields exactly one Implementation (callee = trait)
        assert_eq!(implementations.len(), 1, "relations: {relations:?}");
        assert_eq!(
            implementations[0].dst_name(),
            "MyTrait",
            "Implementation callee should be the trait name, got {:?}",
            implementations[0].dst_name()
        );

        // Inherent + trait impl blocks each yield one ImplAssociation (callee = target type)
        assert_eq!(impl_associations.len(), 2, "relations: {relations:?}");
        let targets: Vec<_> = impl_associations
            .iter()
            .map(|r| r.dst_name().to_string())
            .collect();
        assert_eq!(
            targets,
            vec!["MyStruct".to_string(), "MyStruct".to_string()]
        );
    }

    #[test]
    fn test_extract_python_calls() {
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let relation_extractor = RelationExtractor::new();

        let code = r#"
def foo():
    return 1

def bar():
    return foo() + 1
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::Python)
            .expect("Failed to parse")
            .0;

        let entities = entity_extractor
            .extract(&tree, code, &Language::Python)
            .expect("Failed to extract entities");

        let _relations = relation_extractor
            .extract(&tree, code, &Language::Python, &entities, Some(1))
            .expect("Failed to extract relations");

        // This test just verifies the extraction doesn't fail
        assert!(!entities.is_empty());
    }

    #[test]
    fn test_extract_full_scoped_type_reference() {
        // Regression: a type-position reference like
        // `std::collections::HashMap` must keep the full scoped path instead
        // of truncating to the path segment `std::collections`.
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let relation_extractor = RelationExtractor::new();

        let code = r#"
fn main() {
    let map: std::collections::HashMap<String, i32> = std::collections::HashMap::new();
}
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse")
            .0;
        let entities = entity_extractor
            .extract(&tree, code, &Language::Rust)
            .expect("Failed to extract entities");
        let relations = relation_extractor
            .extract(&tree, code, &Language::Rust, &entities, Some(1))
            .expect("Failed to extract relations");

        let refs: Vec<_> = relations
            .iter()
            .filter(|r| r.relation_type == RelationType::TypeReference)
            .collect();
        assert!(
            refs.iter()
                .any(|r| r.dst_name() == "std::collections::HashMap"),
            "expected full scoped type reference, got {refs:?}"
        );
    }

    #[test]
    fn test_extract_full_callee_name_rust_associated_and_method() {
        // Regression: `Vec::new()` and `v.push(...)` must preserve the
        // type path / receiver instead of truncating to the bare function
        // name.
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let relation_extractor = RelationExtractor::new();

        let code = r#"
fn main() {
    let mut v = Vec::new();
    v.push(1);
}
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse")
            .0;
        let entities = entity_extractor
            .extract(&tree, code, &Language::Rust)
            .expect("Failed to extract entities");
        let relations = relation_extractor
            .extract(&tree, code, &Language::Rust, &entities, Some(1))
            .expect("Failed to extract relations");

        let calls: Vec<_> = relations
            .iter()
            .filter(|r| r.relation_type.is_call())
            .collect();
        let names: Vec<&str> = calls.iter().map(|r| r.dst_name()).collect();
        assert!(
            names.contains(&"Vec::new"),
            "expected Vec::new in {names:?}"
        );
        assert!(names.contains(&"v.push"), "expected v.push in {names:?}");
        // The stdlib category must be derived from the type path prefix.
        let vec_new = calls
            .iter()
            .find(|r| r.dst_name() == "Vec::new")
            .expect("Vec::new call present");
        assert_eq!(
            vec_new.stdlib_category,
            Some(StdlibCategory::Collection),
            "Vec::new category should come from the `Vec` prefix"
        );
    }

    #[test]
    fn test_extract_full_callee_name_javascript_method() {
        // Regression: `console.log(...)` and `Math.max(...)` must keep
        // their receivers so stdlib detection sees the qualified name.
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let relation_extractor = RelationExtractor::new();

        let code = r#"
function main() {
    console.log("hello");
    Math.max(1, 2);
}
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::JavaScript)
            .expect("Failed to parse")
            .0;
        let entities = entity_extractor
            .extract(&tree, code, &Language::JavaScript)
            .expect("Failed to extract entities");
        let relations = relation_extractor
            .extract(&tree, code, &Language::JavaScript, &entities, Some(1))
            .expect("Failed to extract relations");

        let calls: Vec<_> = relations
            .iter()
            .filter(|r| r.relation_type.is_call())
            .collect();
        let names: Vec<&str> = calls.iter().map(|r| r.dst_name()).collect();
        assert!(
            names.contains(&"console.log"),
            "expected console.log in {names:?}"
        );
        assert!(
            names.contains(&"Math.max"),
            "expected Math.max in {names:?}"
        );
        let console_log = calls
            .iter()
            .find(|r| r.dst_name() == "console.log")
            .expect("console.log call present");
        assert_eq!(
            console_log.stdlib_category,
            Some(StdlibCategory::Io),
            "console.log category should come from the `console` prefix"
        );
    }

    #[test]
    fn test_build_full_callee_name_trivial_receiver_falls_back() {
        // Regression: `this.method()` inside a JS method must still
        // produce `method` (the receiver is trivial) so local resolution
        // keeps working.
        let mat = QueryMatch {
            index: 0,
            pattern_index: 0,
            captures: vec![
                Capture {
                    name: "call.method.object".to_string(),
                    text: "this".to_string(),
                    start_byte: 0,
                    end_byte: 4,
                    start_point: (0, 0),
                    end_point: (0, 4),
                },
                Capture {
                    name: "call.method.function".to_string(),
                    text: "method".to_string(),
                    start_byte: 5,
                    end_byte: 11,
                    start_point: (0, 5),
                    end_point: (0, 11),
                },
            ],
        };
        // Create a minimal tree for AST-based name extraction.
        let mut ast_parser = AstParser::new();
        let source = "this.method()";
        let (tree, _) = ast_parser
            .parse_with_tree(source, &Language::JavaScript)
            .expect("Failed to parse");
        assert_eq!(
            relation_handlers::build_full_callee_name(&mat, &Language::JavaScript, &tree, source)
                .as_deref(),
            Some("method"),
            "trivial receiver must not be preserved"
        );
        // Rust `self.method()` must be preserved for type-member dispatch.
        assert_eq!(
            relation_handlers::build_full_callee_name(&mat, &Language::Rust, &tree, source)
                .as_deref(),
            Some("this.method"),
            "rust trivial this should be preserved as qualified"
        );
        let mut rust_self_mat = QueryMatch {
            index: 0,
            pattern_index: 0,
            captures: vec![
                Capture {
                    name: "call.method.object".to_string(),
                    text: "self".to_string(),
                    start_byte: 0,
                    end_byte: 4,
                    start_point: (0, 0),
                    end_point: (0, 4),
                },
                Capture {
                    name: "call.method.function".to_string(),
                    text: "clone".to_string(),
                    start_byte: 5,
                    end_byte: 10,
                    start_point: (0, 5),
                    end_point: (0, 10),
                },
            ],
        };
        let rust_source = "self.clone()";
        let (rust_tree, _) = ast_parser
            .parse_with_tree(rust_source, &Language::Rust)
            .expect("Failed to parse");
        assert_eq!(
            relation_handlers::build_full_callee_name(
                &rust_self_mat,
                &Language::Rust,
                &rust_tree,
                rust_source
            )
            .as_deref(),
            Some("self.clone"),
            "rust self.clone must be preserved"
        );
        rust_self_mat.captures[0].text = "Self".to_string();
        assert_eq!(
            relation_handlers::build_full_callee_name(
                &rust_self_mat,
                &Language::Rust,
                &rust_tree,
                rust_source
            )
            .as_deref(),
            Some("Self.clone"),
            "rust Self.clone must be preserved"
        );
    }
}
