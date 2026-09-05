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
use crate::tree_sitter_query::loader::{QueryLoader, QueryType};
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
use std::collections::{HashMap, HashSet};

/// Remove generic import relations shadowed by specific ones.
///
/// Dependency queries pair a whole-statement pattern (e.g.
/// `dependency.import`) with specific sub-patterns (named, default,
/// namespace, dynamic). Both fire for a single statement such as
/// `import { helper } from "..."`, yielding duplicate edges. When one
/// statement span produces both a generic `ImportStandard` and a more
/// specific import edge for the same target, the generic edge is noise
/// and is dropped. Side-effect imports only match the generic pattern,
/// so they are preserved.
fn deduplicate_generic_import_relations(relations: &mut Vec<Relation>) {
    use cce_types::RelationType;
    if relations.len() <= 1 {
        return;
    }
    let mut groups: HashMap<(usize, usize, String), Vec<usize>> = HashMap::new();
    for (idx, rel) in relations.iter().enumerate() {
        if !rel.relation_type.is_import() {
            continue;
        }
        groups
            .entry((
                rel.span.start_byte,
                rel.span.end_byte,
                rel.dst_name().to_string(),
            ))
            .or_default()
            .push(idx);
    }
    let mut remove: HashSet<usize> = HashSet::new();
    for (_, idxs) in groups {
        if idxs.len() <= 1 {
            continue;
        }
        let has_specific = idxs
            .iter()
            .any(|&i| !matches!(relations[i].relation_type, RelationType::ImportStandard));
        if has_specific {
            for &i in &idxs {
                if matches!(relations[i].relation_type, RelationType::ImportStandard) {
                    remove.insert(i);
                }
            }
        }
    }
    if !remove.is_empty() {
        let mut kept = Vec::with_capacity(relations.len() - remove.len());
        for (idx, rel) in relations.drain(..).enumerate() {
            if !remove.contains(&idx) {
                kept.push(rel);
            }
        }
        *relations = kept;
    }
}

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
        // Template and style languages declare no call query: they have no
        // call semantics, so absence yields no relations instead of an error.
        if !matches!(language, Language::Custom(_))
            && !QueryLoader::supports_builtin_query(*language, QueryType::Call)
        {
            return Ok(Vec::new());
        }
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
        // Languages without a declared dependency query have no dependency
        // semantics; absence yields no relations instead of an error.
        if !matches!(language, Language::Custom(_))
            && !QueryLoader::supports_builtin_query(*language, QueryType::Dependency)
        {
            return Ok(Vec::new());
        }
        let matches = self
            .query_executor
            .execute_dependency_query(tree, source, language)?;

        let entity_index = EntityIndex::new(entities);
        let mut relations = Vec::new();

        for mat in &matches {
            if let Some(dep_capture) = find_dependency_capture(mat) {
                if Self::is_shadowed_require(mat, dep_capture, entities, language) {
                    continue;
                }
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

        deduplicate_generic_import_relations(&mut relations);

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

    /// Whether a `require()` dependency match is shadowed by a local binding.
    ///
    /// The tree-sitter query matches on identifier text only, so
    /// `function f(require) { require("./x") }` still fires. Drop the edge
    /// when the call sits inside a function whose parameters bind `require`,
    /// or when a `require` variable in the same scope precedes the call.
    /// TS `import x = require()` has no function capture and never filters.
    fn is_shadowed_require(
        mat: &QueryMatch,
        dep_capture: &Capture,
        entities: &[Entity],
        language: &Language,
    ) -> bool {
        if !matches!(language, Language::JavaScript | Language::TypeScript) {
            return false;
        }
        if !dep_capture.name.contains("dependency.require") {
            return false;
        }
        let Some(func) = mat
            .captures
            .iter()
            .find(|c| c.name.ends_with("dependency.require.function"))
        else {
            return false;
        };
        if func.text != "require" {
            return false;
        }
        let call_byte = func.start_byte;
        let enclosing = entities
            .iter()
            .filter(|e| {
                e.kind.is_function_like()
                    && e.span.start_byte <= call_byte
                    && call_byte <= e.span.end_byte
            })
            .min_by_key(|e| e.span.end_byte - e.span.start_byte);
        if let Some(func_entity) = enclosing {
            if func_entity
                .parameters
                .iter()
                .any(|(n, _)| n == "require" || n.ends_with(" require"))
            {
                return true;
            }
            for e in entities {
                if e.name == "require"
                    && e.span.start_byte < call_byte
                    && e.span.start_byte >= func_entity.span.start_byte
                    && e.span.end_byte <= func_entity.span.end_byte
                {
                    return true;
                }
            }
            return false;
        }
        entities
            .iter()
            .any(|e| e.name == "require" && e.span.start_byte < call_byte)
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
            "rust self.clone must be preserved"
        );
    }

    #[test]
    fn test_python_tuple_unpacking_entities() {
        // Tuple unpacking yields one comma-separated variable entity
        // carrying the right-hand side as its source.
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let code = "def f(pair):\n    first, second = pair\n    return first\n";
        let tree = ast_parser
            .parse_with_tree(code, &Language::Python)
            .expect("parse")
            .0;
        let entities = entity_extractor
            .extract(&tree, code, &Language::Python)
            .expect("extract");
        let unpacked = entities
            .iter()
            .find(|e| e.kind == cce_types::entity::EntityKind::Variable && e.name.contains("first"))
            .expect("tuple unpacking entity should exist");
        assert_eq!(unpacked.name, "first, second");
        assert_eq!(
            unpacked.metadata.get("source_type").map(String::as_str),
            Some("pair")
        );
    }

    #[test]
    fn test_python_pattern_binding_entities() {
        // Loop and exception bindings exist as entities.
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let code = "for k, v in items:\n    pass\ntry:\n    pass\nexcept ValueError as e:\n    pass\nwith open('f') as fh:\n    pass\n";
        let tree = ast_parser
            .parse_with_tree(code, &Language::Python)
            .expect("parse")
            .0;
        let entities = entity_extractor
            .extract(&tree, code, &Language::Python)
            .expect("extract");
        let names: Vec<&str> = entities
            .iter()
            .filter(|e| e.kind == cce_types::entity::EntityKind::Variable)
            .map(|e| e.name.as_str())
            .collect();
        for expected in ["k", "v", "e", "fh"] {
            assert!(
                names.contains(&expected),
                "pattern binding '{expected}' should exist, got {names:?}"
            );
        }
        let except_entity = entities
            .iter()
            .find(|e| e.name == "e")
            .expect("except binding should exist");
        assert_eq!(
            except_entity
                .metadata
                .get("source_type")
                .map(String::as_str),
            Some("ValueError")
        );

        let case_code = "def f(value):\n    match value:\n        case (x, 0):\n            return x\n        case (x, y):\n            return y\n";
        let case_tree = ast_parser
            .parse_with_tree(case_code, &Language::Python)
            .expect("parse")
            .0;
        let case_entities = entity_extractor
            .extract(&case_tree, case_code, &Language::Python)
            .expect("extract");
        let case_names: Vec<&str> = case_entities
            .iter()
            .filter(|e| e.kind == cce_types::entity::EntityKind::Variable)
            .map(|e| e.name.as_str())
            .collect();
        for expected in ["x", "y"] {
            assert!(
                case_names.contains(&expected),
                "case binding '{expected}' should exist, got {case_names:?}"
            );
        }
    }

    #[test]
    fn test_string_argument_call_is_not_require_import() {
        // A call with a string literal argument must not produce
        // an ImportStandard edge to that literal.
        for language in [Language::JavaScript, Language::TypeScript] {
            let mut ast_parser = AstParser::new();
            let entity_extractor = EntityExtractor::new();
            let relation_extractor = RelationExtractor::new();

            let code = r#"
function createUser(name, id) {
  return { kind: 'user', name, id };
}
const user = createUser('Alice', 1);
"#;

            let tree = ast_parser
                .parse_with_tree(code, &language)
                .expect("Failed to parse")
                .0;
            let entities = entity_extractor
                .extract(&tree, code, &language)
                .expect("Failed to extract entities");
            let relations = relation_extractor
                .extract(&tree, code, &language, &entities, Some(1))
                .expect("Failed to extract relations");

            assert!(
                relations
                    .iter()
                    .filter(|r| r.relation_type == RelationType::ImportStandard)
                    .all(|r| !r.dst_name().contains("Alice")),
                "string literal must not become an import edge in {language:?}: {relations:?}"
            );
            assert!(
                relations.iter().any(|r| r.dst_name() == "createUser"),
                "real call edge must be preserved in {language:?}: {relations:?}"
            );
        }
    }

    #[test]
    fn test_real_require_import_still_detected() {
        // The predicate fix must not break genuine require() detection.
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let relation_extractor = RelationExtractor::new();

        let code = "const { loadUser } = require(\"./models\");\n";

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

        assert!(
            relations
                .iter()
                .any(|r| r.relation_type == RelationType::ImportStandard
                    && r.dst_name().contains("models")),
            "genuine require() import must still be detected: {relations:?}"
        );
    }

    #[test]
    fn test_assigned_require_yields_single_import_edge() {
        // The bare-call require pattern fires on the inner call_expression
        // regardless of its parent, so no declarator-level pattern may exist:
        // such a pattern would emit a second edge with a different span that
        // survives span-grouped dedup. `const x = require()` must yield
        // exactly one ImportStandard edge.
        for (language, code) in [
            (Language::JavaScript, "const x = require(\"./m\");\n"),
            (Language::JavaScript, "var y = require(\"./m\");\n"),
            (Language::TypeScript, "const x: any = require(\"./m\");\n"),
        ] {
            let mut ast_parser = AstParser::new();
            let entity_extractor = EntityExtractor::new();
            let relation_extractor = RelationExtractor::new();

            let tree = ast_parser
                .parse_with_tree(code, &language)
                .expect("Failed to parse")
                .0;
            let entities = entity_extractor
                .extract(&tree, code, &language)
                .expect("Failed to extract entities");
            let relations = relation_extractor
                .extract(&tree, code, &language, &entities, Some(1))
                .expect("Failed to extract relations");

            let imports: Vec<_> = relations
                .iter()
                .filter(|r| {
                    r.relation_type == RelationType::ImportStandard && r.dst_name().contains("./m")
                })
                .collect();
            assert_eq!(
                imports.len(),
                1,
                "assigned require() must yield exactly one import edge in {language:?}: {relations:?}"
            );
        }
    }

    #[test]
    fn test_shadowed_require_param_yields_no_import() {
        // A shadowed builtin produces no import edge, while a top-level
        // call in the same file is still kept.
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let relation_extractor = RelationExtractor::new();
        let code = "function f(require) { return require(\"./x\"); }\nrequire(\"./y\");\n";
        let tree = ast_parser
            .parse_with_tree(code, &Language::JavaScript)
            .expect("parse")
            .0;
        let entities = entity_extractor
            .extract(&tree, code, &Language::JavaScript)
            .expect("extract");
        let relations = relation_extractor
            .extract(&tree, code, &Language::JavaScript, &entities, Some(1))
            .expect("relations");
        let imports: Vec<_> = relations
            .iter()
            .filter(|r| r.relation_type == RelationType::ImportStandard)
            .collect();
        assert!(
            imports.iter().all(|r| !r.dst_name().contains("./x")),
            "shadowed require must not import: {relations:?}"
        );
        assert!(
            imports.iter().any(|r| r.dst_name().contains("./y")),
            "top-level require must still import: {relations:?}"
        );
    }

    #[test]
    fn test_js_destructuring_entities() {
        // Object and array destructuring yield comma-folded entities.
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let code = "const {name, age} = user;\nconst [first, second] = pair;\n";
        let tree = ast_parser
            .parse_with_tree(code, &Language::JavaScript)
            .expect("parse")
            .0;
        let entities = entity_extractor
            .extract(&tree, code, &Language::JavaScript)
            .expect("extract");
        let vars: Vec<&str> = entities
            .iter()
            .filter(|e| e.kind == cce_types::entity::EntityKind::Variable)
            .map(|e| e.name.as_str())
            .collect();
        assert!(
            vars.iter().any(|n| n.contains("name") && n.contains("age")),
            "object destructuring entity missing, got {vars:?}"
        );
        assert!(
            vars.iter()
                .any(|n| n.contains("first") && n.contains("second")),
            "array destructuring entity missing, got {vars:?}"
        );
    }

    #[test]
    fn test_rust_pattern_entities() {
        // Tuple and struct patterns with branch bindings exist as entities.
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let code = "fn f(pair: (i32, i32), p: Point, opt: Option<i32>, x: Option<i32>) { let (a, b) = pair; if let Some(v) = opt {} match x { Some(y) => {}, _ => {} } }\n";
        let tree = ast_parser
            .parse_with_tree(code, &Language::Rust)
            .expect("parse")
            .0;
        let entities = entity_extractor
            .extract(&tree, code, &Language::Rust)
            .expect("extract");
        let vars: Vec<&str> = entities
            .iter()
            .filter(|e| e.kind == cce_types::entity::EntityKind::Variable)
            .map(|e| e.name.as_str())
            .collect();
        assert!(
            vars.iter().any(|n| n.contains('a') && n.contains('b')),
            "tuple pattern entity missing, got {vars:?}"
        );
        assert!(vars.contains(&"v"), "if-let binding missing, got {vars:?}");
    }

    #[test]
    fn test_c_enum_member_entities() {
        // C enumerators are distinct enum variant entities.
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let code = "enum Color { RED, GREEN = 2, BLUE };\n";
        let tree = ast_parser
            .parse_with_tree(code, &Language::C)
            .expect("parse")
            .0;
        let entities = entity_extractor
            .extract(&tree, code, &Language::C)
            .expect("extract");
        for expected in ["RED", "GREEN", "BLUE"] {
            assert!(
                entities
                    .iter()
                    .any(|e| e.kind == cce_types::entity::EntityKind::EnumVariant
                        && e.name == expected),
                "enum member '{expected}' missing, got {:?}",
                entities
                    .iter()
                    .map(|e| (&e.kind, &e.name))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn test_dart_function_params_extracted() {
        // Function and method signatures carry parameters.
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let code = "String greet(String name, int age) { return name; }\n";
        let tree = ast_parser
            .parse_with_tree(code, &Language::Dart)
            .expect("parse")
            .0;
        let entities = entity_extractor
            .extract(&tree, code, &Language::Dart)
            .expect("extract");
        let func = entities
            .iter()
            .find(|e| e.name == "greet")
            .expect("greet entity should exist");
        assert_eq!(func.parameters.len(), 2, "params: {:?}", func.parameters);
        assert!(func.parameters.iter().any(|(n, _)| n == "name"));
        assert!(func.parameters.iter().any(|(n, _)| n == "age"));
    }

    #[test]
    fn test_java_record_and_pattern_entities() {
        // Record components, pattern variables and loop variables
        // exist as entities.
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let code = "public record Point(String name, int age) {}\nclass A {\n    String f(Object obj, String[] args) {\n        if (obj instanceof String s) { return s; }\n        for (String current : args) { }\n        return \"x\";\n    }\n}\n";
        let tree = ast_parser
            .parse_with_tree(code, &Language::Java)
            .expect("parse")
            .0;
        let entities = entity_extractor
            .extract(&tree, code, &Language::Java)
            .expect("extract");
        let names: Vec<(&cce_types::entity::EntityKind, &str)> = entities
            .iter()
            .map(|e| (&e.kind, e.name.as_str()))
            .collect();
        for expected in ["name", "age"] {
            assert!(
                entities
                    .iter()
                    .any(|e| e.kind == cce_types::entity::EntityKind::Field && e.name == expected),
                "record component '{expected}' missing, got {names:?}"
            );
        }
        assert!(
            entities
                .iter()
                .any(|e| e.kind == cce_types::entity::EntityKind::Variable && e.name == "s"),
            "instanceof pattern var 's' missing, got {names:?}"
        );
        assert!(
            entities
                .iter()
                .any(|e| e.kind == cce_types::entity::EntityKind::Variable && e.name == "current"),
            "enhanced-for var 'current' missing, got {names:?}"
        );
    }

    #[test]
    fn test_cpp_range_for_and_structured_binding_entities() {
        // Range loop variables and structured bindings exist as entities.
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let code = "int f(std::vector<int>& v) {\n    for (auto& elem : v) { }\n    auto [a, b] = p;\n    return 0;\n}\n";
        let tree = ast_parser
            .parse_with_tree(code, &Language::Cpp)
            .expect("parse")
            .0;
        let entities = entity_extractor
            .extract(&tree, code, &Language::Cpp)
            .expect("extract");
        let names: Vec<(&cce_types::entity::EntityKind, &str)> = entities
            .iter()
            .map(|e| (&e.kind, e.name.as_str()))
            .collect();
        assert!(
            entities
                .iter()
                .any(|e| e.kind == cce_types::entity::EntityKind::Variable && e.name == "elem"),
            "range-for var 'elem' missing, got {names:?}"
        );
        assert!(
            entities
                .iter()
                .any(|e| e.kind == cce_types::entity::EntityKind::Variable
                    && e.name.contains('a')
                    && e.name.contains('b')),
            "structured binding entity missing, got {names:?}"
        );
    }

    #[test]
    fn test_kotlin_destructuring_entities() {
        // Destructuring declarations fold into one entity carrying
        // the right-hand side as its source.
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let code = "fun f(pair: Pair<Int, String>) {\n    val (first, second) = pair\n}\n";
        let tree = ast_parser
            .parse_with_tree(code, &Language::Kotlin)
            .expect("parse")
            .0;
        let entities = entity_extractor
            .extract(&tree, code, &Language::Kotlin)
            .expect("extract");
        let folded = entities
            .iter()
            .find(|e| {
                e.kind == cce_types::entity::EntityKind::Variable
                    && e.name.contains("first")
                    && e.name.contains("second")
            })
            .expect("folded destructuring entity should exist");
        assert_eq!(
            folded.metadata.get("source_type").map(String::as_str),
            Some("pair")
        );
    }

    #[test]
    fn test_csharp_pattern_entities() {
        // Loop variables, pattern designations, output variables and
        // tuple deconstruction exist as entities.
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let code = "class D {\n    void M(object obj, string[] items) {\n        foreach (var current in items) { }\n        if (obj is string s) { }\n        var (a, b) = (1, 2);\n        if (int.TryParse(\"1\", out var result)) { }\n    }\n}\n";
        let tree = ast_parser
            .parse_with_tree(code, &Language::CSharp)
            .expect("parse")
            .0;
        let entities = entity_extractor
            .extract(&tree, code, &Language::CSharp)
            .expect("extract");
        let names: Vec<(&cce_types::entity::EntityKind, &str)> = entities
            .iter()
            .map(|e| (&e.kind, e.name.as_str()))
            .collect();
        for expected in ["current", "s", "result"] {
            assert!(
                entities.iter().any(
                    |e| e.kind == cce_types::entity::EntityKind::Variable && e.name == expected
                ),
                "pattern var '{expected}' missing, got {names:?}"
            );
        }
        assert!(
            entities
                .iter()
                .any(|e| e.kind == cce_types::entity::EntityKind::Variable
                    && e.name.contains('a')
                    && e.name.contains('b')),
            "tuple deconstruction entity missing, got {names:?}"
        );
    }

    #[test]
    fn test_typescript_import_require_clause_detected() {
        // `import x = require("m")` parses as `import_require_clause`, not a
        // `call_expression`, so it needs its own dependency pattern.
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let relation_extractor = RelationExtractor::new();

        let code = "import x = require(\"./m\");\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::TypeScript)
            .expect("Failed to parse")
            .0;
        let entities = entity_extractor
            .extract(&tree, code, &Language::TypeScript)
            .expect("Failed to extract entities");
        let relations = relation_extractor
            .extract(&tree, code, &Language::TypeScript, &entities, Some(1))
            .expect("Failed to extract relations");

        assert!(
            relations
                .iter()
                .any(|r| r.relation_type == RelationType::ImportStandard
                    && r.dst_name().contains("./m")),
            "TS import-require must be detected as an import edge: {relations:?}"
        );
    }
}
