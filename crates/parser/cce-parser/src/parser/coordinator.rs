//! Parse coordinator for orchestrating the parsing pipeline
//!
//! This module provides a high-level coordinator that manages the entire parsing
//! process through a pipeline of stages.
//!
//! # Architecture
//!
//! ```text
//! ParseCoordinator
//!   ├── Components (shared components)
//!   └── ParseContext (execution context)
//! ```
//!
//! # Design Principles
//!
//! - **Separation of Concerns**: Coordination logic is separate from business logic
//! - **Static Pipeline**: No dynamic dispatch, fixed pipeline execution

use crate::parser::components::Components;
use crate::parser::context::ParseContext;
use crate::parser::embedded_types::EmbeddedParseConfig;
use crate::parser::helpers;
use crate::parser::pipeline;
use cce_metrics::ParserMetrics;
use cce_types::language::{Language, LanguageInfo};
use cce_types::{ParseError, ParsedFile, RawRelationData};
use std::sync::Arc;

/// Main coordinator for the parsing process
///
/// This is the top-level entry point for parsing. It manages the component
/// registry and executes the parse pipeline.
pub struct ParseCoordinator {
    components: Components,
    /// Monitoring metrics (optional)
    metrics: Option<Arc<ParserMetrics>>,
}

impl Clone for ParseCoordinator {
    fn clone(&self) -> Self {
        // Create a new coordinator with the same plugin registry and metrics
        // but a fresh Components (components are stateless or have internal sharing)
        Self {
            components: Components::new(),
            metrics: self.metrics.clone(),
        }
    }
}

impl ParseCoordinator {
    /// Create a new parse coordinator
    pub fn new() -> Self {
        Self {
            components: Components::new(),
            metrics: None,
        }
    }

    /// Create a new parse coordinator wired with the plugin registry for the
    /// `LangHeuristics` entity-kind hook.
    pub fn with_plugin_registry(registry: Arc<cce_plugin::PluginRegistry>) -> Self {
        Self {
            components: Components::with_plugin_registry(registry),
            metrics: None,
        }
    }

    /// Create a new parse coordinator with custom embedded parse config
    pub fn with_embedded_config(config: EmbeddedParseConfig) -> Self {
        Self {
            components: Components::with_embedded_config(config),
            metrics: None,
        }
    }

    /// Create a new parse coordinator with a seeded entity ID counter.
    ///
    /// Hot-update paths reuse the raw `EntityId` space of the previously
    /// indexed epoch, so the counter is seeded one above the existing maximum
    /// to avoid collisions between freshly parsed and unchanged entities.
    pub fn with_entity_id_seed(seed: u64) -> Self {
        Self {
            components: Components::with_entity_id_seed(seed),
            metrics: None,
        }
    }

    /// Set monitoring metrics
    pub fn with_metrics(mut self, metrics: Arc<ParserMetrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Set monitoring metrics after construction
    pub fn set_metrics(&mut self, metrics: Arc<ParserMetrics>) {
        self.metrics = Some(metrics);
    }

    /// Parse a file and return structured result
    ///
    /// This is the main entry point for parsing. It creates a parse context,
    /// executes the pipeline, and builds the final result.
    pub fn parse(&mut self, file_path: &str, content: &str) -> Result<ParsedFile, ParseError> {
        let start = std::time::Instant::now();

        // Create context
        let mut context = ParseContext::new(file_path.to_string(), content.to_string());

        // Execute pipeline
        let result = pipeline::execute_full(&mut context, &mut self.components);

        // Record metrics if enabled
        if let Some(metrics) = &self.metrics {
            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
            let success = result.is_ok();

            metrics.record_parse(elapsed_ms, success);
        }

        match result {
            Ok(()) => {
                // Build result
                let parsed = self.build_result(context, start)?;
                Ok(parsed)
            }
            Err(e) => Err(e),
        }
    }

    /// Parse with pre-detected language info
    pub fn parse_with_language_info(
        &mut self,
        file_path: &str,
        content: &str,
        language_info: &LanguageInfo,
    ) -> Result<ParsedFile, ParseError> {
        let start = std::time::Instant::now();

        // Create context with pre-detected language
        let mut context = ParseContext::new(file_path.to_string(), content.to_string());
        context.language_info = Some(language_info.clone());

        // Execute pipeline (skip language detection stage)
        let result = pipeline::execute_skip_language_detection(&mut context, &mut self.components);

        // Record metrics if enabled
        if let Some(metrics) = &self.metrics {
            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
            let success = result.is_ok();

            metrics.record_parse(elapsed_ms, success);
        }

        match result {
            Ok(()) => {
                // Build result
                let parsed = self.build_result(context, start)?;
                Ok(parsed)
            }
            Err(e) => Err(e),
        }
    }

    /// Build the final ParsedFile from context
    fn build_result(
        &self,
        context: ParseContext,
        _start: std::time::Instant,
    ) -> Result<ParsedFile, ParseError> {
        let language = context
            .language()
            .cloned()
            .ok_or_else(|| ParseError::ast_parsing("Language not detected".to_string()))?;

        // Merge entities first
        let mut all_entities = context.entities;
        all_entities.extend(context.block_entities);

        // Convert relations to RawRelationData
        let raw_relations: Vec<RawRelationData> = context
            .relations
            .into_iter()
            .map(|r| RawRelationData {
                src: cce_types::EntityId(r.caller_id as u64),
                level: r.caller_level,
                dst_name: r.dst_name().to_string(),
                relation_type: r.relation_type,
                span: r.span,
                stdlib_category: r.stdlib_category,
            })
            .collect();

        // Merge block relations
        let mut all_raw_relations = raw_relations;
        all_raw_relations.extend(context.block_relations);

        // Extract cross-block relations for SFC files and TSX/JSX
        let block_relations = if !context.embedded_blocks.is_empty()
            || matches!(
                language,
                Language::Vue | Language::Svelte | Language::Tsx | Language::Jsx
            ) {
            helpers::resolve_cross_block_relations(&context.embedded_blocks, &all_entities)
        } else {
            Vec::new()
        };

        // Extract imports from AST if available (before source is moved into ParsedFile)
        let import_table = context.tree.as_ref().and_then(|tree| {
            crate::relation_helpers::extract_imports(tree, &context.source, &language, None).ok()
        });

        // Extract named re-exports (Rust `pub use`, JS/TS `export { x } from`)
        // from the same AST. Resolution happens in the symbol table, so the
        // raw records are enough here.
        let reexports = context
            .tree
            .as_ref()
            .map(|tree| {
                crate::relation_helpers::extract_reexports(tree, &context.source, &language)
            })
            .unwrap_or_default();

        // The full-content hash is computed once here (source is in hand) and
        // reused by the relation build instead of re-hashing.
        let file_hash = Some(cce_utils::hash::calculate_hash(context.source.as_bytes()));

        let parsed = ParsedFile {
            language,
            path: context.file_path.clone(),
            source: context.source.into(),
            entities: all_entities,
            local_symbols: context.local_symbols,
            raw_relations: all_raw_relations,
            behavior: context.behavior,
            control_flow: context.control_flow,
            embedded_blocks: context.embedded_blocks,
            block_relations,
            file_doc_comment: context.file_doc_comment,
            file_doc_span: context.file_doc_span,
            import_table,
            reexports,
            file_hash,
        };

        Ok(parsed)
    }
}

impl Default for ParseCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::EntityKind;

    #[test]
    fn test_entity_id_seed_prevents_hot_update_collision() {
        let code_a = "fn alpha() -> i32 { 1 }\nfn beta() -> i32 { 2 }";
        let code_b = "fn gamma() -> i32 { 3 }\nfn delta() -> i32 { 4 }";

        // Full-index parse: counter starts at 0 and stays monotonic across files.
        let mut full = ParseCoordinator::new();
        let parsed_a = full.parse("a.rs", code_a).expect("parse a");
        let full_ids: std::collections::HashSet<u64> =
            parsed_a.entities.iter().map(|e| e.id.0).collect();
        let max_full = full_ids.iter().copied().max().unwrap_or(0);

        // Hot-update parse: a fresh coordinator is seeded one above the
        // previously indexed maximum (mirrors `max_entity_id_for_epoch`).
        let mut hot = ParseCoordinator::with_entity_id_seed(max_full + 1);
        let parsed_b = hot.parse("b.rs", code_b).expect("parse b");
        let hot_ids: std::collections::HashSet<u64> =
            parsed_b.entities.iter().map(|e| e.id.0).collect();

        assert!(
            full_ids.is_disjoint(&hot_ids),
            "hot-update entity ids must not overlap unchanged full-index ids"
        );
        assert!(
            hot_ids.iter().all(|&id| id > max_full),
            "hot-update entity ids must be seeded above the full-index maximum"
        );
    }

    #[test]
    fn test_coordinator_parse_rust() {
        let mut coordinator = ParseCoordinator::new();
        let code = r#"
fn main() {
    println!("Hello, World!");
}

fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#;
        let result = coordinator.parse("main.rs", code);
        assert!(result.is_ok());

        let parsed = result.expect("Failed to parse");
        assert_eq!(parsed.language, Language::Rust);
        assert!(!parsed.entities.is_empty());
    }

    #[test]
    fn test_coordinator_computes_file_hash_once_for_relation_build() {
        let mut coordinator = ParseCoordinator::new();
        let code = "fn main() {\n    println!(\"Hello, World!\");\n}\n";
        let parsed = coordinator
            .parse("main.rs", code)
            .expect("rust should parse");

        let expected = cce_utils::hash::calculate_hash(code.as_bytes());
        assert_eq!(
            parsed.file_hash.as_deref(),
            Some(expected.as_str()),
            "parse must attach the full-content hash for the relation build"
        );
    }

    #[test]
    fn test_coordinator_parse_python() {
        let mut coordinator = ParseCoordinator::new();
        let code = r#"
def main():
    print("Hello, World!")

def add(a, b):
    return a + b
"#;
        let result = coordinator.parse("main.py", code);
        assert!(result.is_ok());

        let parsed = result.expect("Failed to parse");
        assert_eq!(parsed.language, Language::Python);
        assert!(!parsed.entities.is_empty());
    }

    #[test]
    fn test_coordinator_parse_rust_utils_fixture() {
        let mut coordinator = ParseCoordinator::new();
        let code = r#"
use crate::model::User;

pub fn normalize_name(input: &str) -> String {
    input.trim().to_lowercase()
}

pub fn format_user(user: &User) -> String {
    user.display_name()
}
"#;

        let parsed = coordinator
            .parse("src/utils.rs", code)
            .expect("Failed to parse utils fixture");

        let function_names: Vec<_> = parsed
            .entities
            .iter()
            .filter(|entity| entity.kind == EntityKind::Function)
            .map(|entity| entity.name.as_str())
            .collect();

        assert!(
            function_names.contains(&"normalize_name"),
            "normalize_name should survive coordinator parsing"
        );
        assert!(
            function_names.contains(&"format_user"),
            "format_user should survive coordinator parsing"
        );
    }

    #[test]
    fn test_pipeline_stages() {
        use crate::parser::components::Components;
        use crate::parser::context::ParseContext;
        use crate::parser::pipeline;

        let mut components = Components::new();
        let mut context = ParseContext::new("test.rs".to_string(), "fn main() {}".to_string());

        let result = pipeline::execute_full(&mut context, &mut components);

        assert!(result.is_ok());
        assert!(context.language_info.is_some());
        assert!(context.tree.is_some());
        assert!(!context.entities.is_empty());
    }

    #[test]
    fn test_scoped_name_basic() {
        let code = r#"
fn outer_func() {
    fn inner_func() {}
}

struct MyStruct {
    field: i32,
}

impl MyStruct {
    fn method(&self) {}
}
"#;

        let mut coordinator = ParseCoordinator::new();
        let parsed = coordinator.parse("test.rs", code).expect("Failed to parse");

        let scoped_names = parsed.resolve_all_scoped_names();

        assert!(!scoped_names.is_empty(), "Should have some scoped names");

        let has_outer = scoped_names.values().any(|n| n == "outer_func");
        assert!(has_outer, "Should have outer_func");

        let has_struct = scoped_names.values().any(|n| n == "MyStruct");
        assert!(has_struct, "Should have MyStruct");

        let has_method = scoped_names.values().any(|n| n == "MyStruct::method");
        assert!(has_method, "Should have MyStruct::method");
    }

    #[test]
    fn test_scoped_name_consistency_in_single_parse() {
        let code = r#"
fn get_value() -> i32 {
    42
}

fn main() {
    let x = get_value();
    
}
"#;

        let mut coordinator = ParseCoordinator::new();
        let parsed = coordinator.parse("test.rs", code).expect("Failed to parse");

        let scoped_names = parsed.resolve_all_scoped_names();

        assert!(!scoped_names.is_empty());

        let main_fn = parsed
            .entities
            .iter()
            .find(|e| e.name == "main")
            .expect("Should have main function");

        let main_scoped_name = scoped_names
            .get(&main_fn.id)
            .expect("main should have scoped name");

        assert_eq!(main_scoped_name, "main");

        let get_value_fn = parsed
            .entities
            .iter()
            .find(|e| e.name == "get_value")
            .expect("Should have get_value function");

        let get_value_scoped_name = scoped_names
            .get(&get_value_fn.id)
            .expect("get_value should have scoped name");

        assert_eq!(get_value_scoped_name, "get_value");
    }

    #[test]
    fn test_relation_capture_generation() {
        use cce_types::RelationCapture;

        let code = r#"
fn helper() -> i32 {
    42
}

fn main() {
    let x = helper();
}
"#;

        let mut coordinator = ParseCoordinator::new();
        let parsed = coordinator.parse("test.rs", code).expect("Failed to parse");

        // Verify we have relations
        assert!(!parsed.raw_relations.is_empty(), "Should have relations");

        // Get scoped names
        let scoped_names = parsed.resolve_all_scoped_names();

        // Generate RelationCapture records
        let mut captures = Vec::new();
        for relation in &parsed.raw_relations {
            if let Some(caller_scoped_name) = scoped_names.get(&relation.src) {
                if let Some(entity) = parsed.get_entity(relation.src) {
                    let capture = RelationCapture::new(
                        "test.rs".to_string(),
                        caller_scoped_name.clone(),
                        entity.kind,
                        relation.dst_name.clone(),
                        relation.relation_type,
                        relation.span,
                        1,
                    );
                    captures.push(capture);
                }
            }
        }

        // Verify captures were created
        assert!(!captures.is_empty(), "Should have relation captures");

        // Verify capture fields
        let capture = &captures[0];
        assert_eq!(capture.file_path, "test.rs");
        assert!(
            !capture.relation_id.is_empty(),
            "relation_id should be generated"
        );
        assert!(
            !capture.caller_scoped_name.is_empty(),
            "caller_scoped_name should be set"
        );
        assert_eq!(capture.symbol_version, 1);
    }
}
