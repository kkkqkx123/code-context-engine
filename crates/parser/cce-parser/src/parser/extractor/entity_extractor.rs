//! Entity extractor: orchestrates capture parsing and post-processing
//!
//! This module coordinates the entity extraction pipeline:
//!
//! 1. **capture** - Pure tree-sitter capture → entity data extraction
//! 2. **post_processing** - Entity enrichment (attributes, modifiers, stdlib, etc.)
//!
//! # Design
//!
//! `EntityExtractor::extract()` is the single entry point. It:
//! 1. Executes tree-sitter queries via `QueryExecutor`
//! 2. For each match, calls `process_match()` which delegates to `capture::` functions
//! 3. Applies post-processing stages
//! 4. Resolves parent-child relationships
//!
//! This separation ensures capture parsing is testable independently from
//! post-processing logic.

use crate::parser::comment_processor::CommentProcessor;
use crate::tree_sitter_query::error::QueryError;
use crate::tree_sitter_query::executor::{QueryExecutor, QueryMatch};
use cce_types::language::Language;
use cce_types::{Entity, EntityKind};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use tree_sitter::Tree;

use super::annotation_handler::{
    cfg_attribute_targets_test, is_test_attribute, language_has_annotation_semantics,
    should_skip_rust_attr,
};
use super::capture as capture_module;
use super::context::ExtractionContext;
use super::parent_child_resolver::{
    establish_class_method_relationships, establish_impl_method_relationships,
    establish_module_entity_relationships, establish_struct_field_relationships,
};
use super::post_processing;
use super::utils;

mod deduplication;
mod filtering;
mod metadata;
mod type_inference;

/// Entity extractor
///
/// Orchestrates the extraction pipeline: query execution → capture parsing → post-processing.
pub struct EntityExtractor {
    /// Query executor
    query_executor: Arc<QueryExecutor>,
    /// Shared global EntityId counter (unique across all extracted files)
    id_counter: Arc<AtomicU64>,
    /// Comment processor for associating doc comments with entities
    comment_processor: CommentProcessor,
    /// Optional plugin registry for the `LangHeuristics` entity-kind hook
    /// (capture names unknown to the built-in mapping).
    heuristics_registry: Option<Arc<cce_plugin::PluginRegistry>>,
}

impl EntityExtractor {
    /// Create a new entity extractor
    pub fn new() -> Self {
        Self {
            query_executor: Arc::new(QueryExecutor::new()),
            id_counter: Arc::new(AtomicU64::new(0)),
            comment_processor: CommentProcessor::new(),
            heuristics_registry: None,
        }
    }

    /// Create with custom query executor
    pub fn with_executor(executor: Arc<QueryExecutor>) -> Self {
        Self {
            query_executor: executor,
            id_counter: Arc::new(AtomicU64::new(0)),
            comment_processor: CommentProcessor::new(),
            heuristics_registry: None,
        }
    }

    /// Create with custom query executor and comment processor
    pub fn with_executor_and_comment_processor(
        executor: Arc<QueryExecutor>,
        comment_processor: CommentProcessor,
    ) -> Self {
        Self {
            query_executor: executor,
            id_counter: Arc::new(AtomicU64::new(0)),
            comment_processor,
            heuristics_registry: None,
        }
    }

    /// Attach a plugin registry for the `LangHeuristics` entity-kind hook.
    ///
    /// When the built-in capture→kind mapping cannot classify a capture
    /// name, plugins are consulted in priority order (first non-`None` wins).
    pub fn with_heuristics_registry(mut self, registry: Arc<cce_plugin::PluginRegistry>) -> Self {
        self.heuristics_registry = Some(registry);
        self
    }

    /// Configure the shared entity ID counter to start at `seed`.
    ///
    /// Hot-update parses reuse the raw `EntityId` space of the previously
    /// indexed epoch. Seeding the counter above the existing maximum prevents
    /// freshly parsed entities from colliding with unchanged entities that were
    /// cloned into the candidate epoch.
    pub fn with_id_seed(mut self, seed: u64) -> Self {
        self.id_counter = Arc::new(AtomicU64::new(seed));
        self
    }

    /// Extract entities from source code
    ///
    /// Pipeline:
    /// 1. Execute tree-sitter entity query
    /// 2. Process each match: capture parsing → post-processing
    /// 3. Attach buffered annotations to entities
    /// 4. Resolve parent-child relationships
    pub fn extract(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
    ) -> Result<Vec<Entity>, QueryError> {
        let matches = self
            .query_executor
            .execute_entity_query(tree, source, language)?;

        let mut context = ExtractionContext::new(self.id_counter.clone());
        let mut entities = Vec::new();
        let mut pending_annotations: Vec<String> = Vec::new();

        // First pass: collect all matches and identify impl/module blocks
        let mut impl_spans: Vec<std::ops::Range<usize>> = Vec::new();
        let mut module_spans: Vec<(cce_types::EntityId, std::ops::Range<usize>)> = Vec::new();

        for mat in &matches {
            if let Some(mut entity) = self.process_match(mat, &mut context, source, language, tree)
            {
                let is_attribute_usage = entity.kind.is_annotation_like()
                    || (entity.kind.is_macro_like()
                        && entity.subtype.as_deref() == Some("attribute"));

                if is_attribute_usage {
                    if language == &Language::Rust && should_skip_rust_attr(&entity) {
                        continue;
                    }
                    // Inner attributes (`#![...]`) are file-level directives,
                    // never entity modifiers — they must not leak onto the next
                    // entity as a pending annotation.
                    if entity.subtype.as_deref() == Some("attribute.inner") {
                        continue;
                    }
                    // Languages whose annotation/attribute nodes modify the
                    // next entity in source. Annotation entities themselves are
                    // not retained as entities: they are buffered here and
                    // consumed by the following entity (kind promotion +
                    // `test_annotations` metadata for the grouper detector).
                    if language_has_annotation_semantics(language) {
                        pending_annotations.push(entity.name.clone());
                    }
                    continue;
                } else {
                    if !pending_annotations.is_empty() {
                        let annotations_str = pending_annotations.join(", ");
                        if entity.kind.is_function_like()
                            && pending_annotations.iter().any(|a| is_test_attribute(a))
                        {
                            entity.kind = EntityKind::TestCase;
                        } else if entity.kind.is_module_like()
                            && pending_annotations
                                .iter()
                                .any(|a| cfg_attribute_targets_test(a))
                        {
                            // `#[cfg(test)] mod tests` becomes a test suite so
                            // the TestSuiteProcessor can group it with cases.
                            entity.kind = EntityKind::TestSuite;
                        }
                        // Preserve the AST attribute names (e.g. Rust
                        // `#[cfg(test)]` before `mod tests`) for the grouper
                        // test detector, which owns the test-marker semantics.
                        entity.set_metadata("test_annotations", annotations_str);
                        pending_annotations.clear();
                    }

                    // Track impl blocks for filtering nested methods
                    if matches!(
                        entity.kind,
                        EntityKind::InherentImpl | EntityKind::TraitImpl
                    ) {
                        let span_range = entity.span.start_byte..entity.span.end_byte;
                        impl_spans.push(span_range);
                    }

                    // Track module blocks for establishing parent-child relationships
                    if entity.kind.is_module_like() || entity.kind == EntityKind::TestSuite {
                        let span_range = entity.span.start_byte..entity.span.end_byte;
                        module_spans.push((entity.id, span_range));
                    }

                    entities.push(entity);
                }
            }
        }

        // Fix namespace spans for file-scoped languages (PHP, C#)
        adjust_namespace_spans(&mut entities, source, language);
        // Rebuild module_spans after span adjustment so parent-child resolution uses corrected spans
        module_spans.clear();
        for entity in &entities {
            if entity.kind.is_module_like() || entity.kind == EntityKind::TestSuite {
                let span_range = entity.span.start_byte..entity.span.end_byte;
                module_spans.push((entity.id, span_range));
            }
        }

        // Note: impl block methods are NOT filtered out.
        // They remain as independent entities. We need to establish parent-child
        // relationships based on span containment, since tree-sitter returns
        // flat matches without nesting information.

        // Second pass: deduplicate entities with same span
        // Tree-sitter may return multiple captures for the same code entity.
        // Keep only the most specific entity for each span.
        deduplication::deduplicate_entities_by_span(&mut entities);

        // 2.5: Remove entities whose span is fully contained within a parent
        // entity and are pure implementation detail noise (local variables).
        // This prevents fragments from appearing as independent retrieval units.
        deduplication::deduplicate_contained_entities(&mut entities);

        // Third pass: remove low-value entities
        // Filter out short/generic placeholders that don't represent meaningful
        // business entities (e.g., single-char type parameters like T, F).
        // This must happen before parent-child resolution to avoid noise.
        filtering::filter_low_value_entities(&mut entities);

        // Fourth pass: associate doc comments with entities
        // This must happen before parent-child resolution so that doc comments
        // are available when generating NL descriptions.
        match self
            .comment_processor
            .process(tree, source, language, &mut entities)
        {
            Ok(_file_doc) => {}
            Err(e) => {
                tracing::warn!("Comment processing failed: {e}");
            }
        }

        // Fourth-B pass: derive doc-comment types (Ruby YARD, PHPDoc) now
        // that doc comments are attached. Match-level extraction runs
        // before comment association, so this cannot live in metadata.rs.
        metadata::extract_doc_type_metadata(&mut entities, language);

        // Fifth pass: establish impl block -> method relationships based on span
        establish_impl_method_relationships(&mut entities);

        // Sixth pass: establish struct/class -> field relationships based on span
        // Must run before module relationships so fields are claimed by their
        // struct/class/enum/trait/interface before modules can claim them.
        establish_struct_field_relationships(&mut entities);

        // Sixth-B pass: establish class/struct -> method relationships based on span.
        // Catches Python/Ruby/JS class methods that aren't inside impl blocks.
        // Runs after field relationships so methods don't conflict with fields,
        // and before module relationships so classes claim methods before modules.
        establish_class_method_relationships(&mut entities);

        // Seventh pass: establish module -> child entity relationships based on span
        establish_module_entity_relationships(&mut entities, &module_spans);

        // 7.5: extract Go receiver types for method entities
        post_processing::extract_receiver_for_entities(&mut entities, language);

        // Eighth pass: fill children based on parent field
        post_processing::fill_children(&mut entities);

        Ok(entities)
    }

    /// Process a single query match
    ///
    /// Stages:
    /// 1. Capture parsing (kind_mapper, capture_parser)
    /// 2. Post-processing (attributes, type params, modifiers, stdlib, test analysis)
    fn process_match(
        &self,
        mat: &QueryMatch,
        context: &mut ExtractionContext,
        source: &str,
        language: &Language,
        tree: &Tree,
    ) -> Option<Entity> {
        let main_capture = capture_module::parser::find_main_capture(mat)?;
        let name_capture = capture_module::parser::find_name_capture(mat)?;

        let kind = match capture_module::determine_entity_kind(&main_capture.name) {
            Some(kind) => kind,
            None => {
                // `LangHeuristics` plugin hook: custom query capture names that
                // the built-in mapping does not recognize can be classified by
                // plugins (first non-`None` wins).
                let Some(registry) = &self.heuristics_registry else {
                    return None;
                };
                crate::plugin::heuristics::entity_kind(registry, &main_capture.name)?
            }
        };
        let mut subtype = capture_module::parser::extract_subtype_from_capture(&main_capture.name);
        // `entity.macro.attribute.inner` shares the `attribute` subtype with
        // its outer counterpart; distinguish file-level inner attributes
        // (`#![...]`) so they are never buffered as entity modifiers.
        if main_capture.name.ends_with(".inner") {
            subtype = Some("attribute.inner".to_string());
        }
        let span = utils::create_span_from_capture(main_capture);

        // Validate span to filter out tree-sitter phantom/error-recovery nodes
        // with inconsistent positions (end_byte < start_byte, end_row < start_row,
        // or zero-width spans from error recovery).
        if span.end_byte < span.start_byte
            || span.end_position.row < span.start_position.row
            || span.start_byte == span.end_byte
        {
            return None;
        }

        let id = context.next_entity_id();

        let mut entity = Entity::new(id, kind, name_capture.text.clone(), span);
        entity.subtype = subtype;

        // Capture-level extraction
        entity.signature = capture_module::parser::extract_signature(mat, source);
        entity.parameters = capture_module::parser::extract_parameters(mat);
        entity.return_type = capture_module::parser::extract_return_type(mat);
        entity.doc_comment = capture_module::parser::extract_doc_comment(mat);
        entity.attributes = capture_module::parser::extract_attributes(mat);

        // Extract language-specific metadata from captures
        metadata::extract_metadata(mat, &mut entity, language, source, tree);

        // Post-processing stages
        post_processing::extract_modifiers(mat, &mut entity, language);
        if language == &Language::Rust {
            post_processing::extract_rust_attributes(mat, source, &mut entity);
        }
        post_processing::mark_stdlib(&mut entity, language);

        let _guard = super::context::ScopedEntity::new(context, &mut entity);

        Some(entity)
    }
}

impl Default for EntityExtractor {
    fn default() -> Self {
        Self::new()
    }
}

fn adjust_namespace_spans(entities: &mut [cce_types::Entity], source: &str, language: &Language) {
    let Some(policy) = crate::parser::extractor::namespace_policy::namespace_policy_for(*language)
    else {
        return;
    };
    if !policy.covers_file_scope() {
        return;
    }
    let file_end_line = source.lines().count();
    let file_end_byte = source.len();

    let ns_info: Vec<(cce_types::EntityId, usize, usize)> = entities
        .iter()
        .filter(|e| e.kind.is_namespace())
        .map(|e| (e.id, e.span.start_position.row, e.span.start_byte))
        .collect();
    if ns_info.is_empty() {
        return;
    }
    let mut sorted = ns_info.clone();
    sorted.sort_by_key(|(_, row, _)| *row);

    for entity in entities.iter_mut().filter(|e| e.kind.is_namespace()) {
        let cur_row = entity.span.start_position.row;
        let mut next_row = file_end_line;
        let mut next_byte = file_end_byte;
        for (_, row, byte) in &sorted {
            if *row > cur_row && *row < next_row {
                next_row = *row;
                next_byte = *byte;
            } else if *row > cur_row && *row == next_row && *byte < next_byte {
                next_byte = *byte;
            }
        }
        if next_row > cur_row {
            entity.span.end_position.row = next_row;
            entity.span.end_position.column = 0;
            entity.span.end_byte = next_byte;
        } else {
            entity.span.end_position.row = file_end_line;
            entity.span.end_position.column = 0;
            entity.span.end_byte = file_end_byte;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast_parser::AstParser;

    #[test]
    fn test_extract_rust_entities() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = r#"
struct Point {
    x: f64,
    y: f64,
}

fn distance(p1: &Point, p2: &Point) -> f64 {
    0.0
}
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::Rust)
            .expect("Failed to extract");

        assert!(!entities.is_empty(), "Should find at least one entity");

        let structs: Vec<_> = entities
            .iter()
            .filter(|e| e.kind == EntityKind::Struct)
            .collect();
        assert!(!structs.is_empty(), "Should find at least one struct");

        let functions: Vec<_> = entities
            .iter()
            .filter(|e| e.kind == EntityKind::Function)
            .collect();
        assert!(!functions.is_empty(), "Should find at least one function");
    }

    #[test]
    fn test_extract_rust_multiple_functions() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = r#"
pub fn normalize_name(input: &str) -> String {
    input.trim().to_lowercase()
}

pub fn format_user(user: &str) -> String {
    user.to_string()
}
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::Rust)
            .expect("Failed to extract");

        let names: Vec<_> = entities
            .iter()
            .filter(|e| e.kind == EntityKind::Function)
            .map(|e| e.name.as_str())
            .collect();

        assert!(
            names.contains(&"normalize_name"),
            "normalize_name should be extracted as a function"
        );
        assert!(
            names.contains(&"format_user"),
            "format_user should be extracted as a function"
        );
    }

    #[test]
    fn test_extract_fnv_typealias() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = r#"
/// A convenience alias for creating a hash map with an FNV hasher.
pub(crate) type HashMap<K, V> =
    std::collections::HashMap<K, V, std::hash::BuildHasherDefault<Hasher>>;

/// A hasher that implements the Fowler–Noll–Vo (FNV) hash.
pub(crate) struct Hasher(u64);

impl Hasher {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
}

impl Default for Hasher {
    fn default() -> Hasher {
        Hasher(Hasher::OFFSET_BASIS)
    }
}

impl std::hash::Hasher for Hasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes.iter() {
            self.0 = self.0 ^ u64::from(byte);
            self.0 = self.0.wrapping_mul(Hasher::PRIME);
        }
    }
}
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::Rust)
            .expect("Failed to extract");

        // Also test raw query matches
        use crate::tree_sitter_query::executor::QueryExecutor;
        let executor = QueryExecutor::new();
        let matches = executor
            .execute_entity_query(&tree, code, &Language::Rust)
            .expect("query");
        assert!(
            !matches.is_empty(),
            "Query should extract at least one entity"
        );

        let type_aliases: Vec<_> = entities
            .iter()
            .filter(|e| e.kind == EntityKind::TypeAlias)
            .collect();
        assert!(
            !type_aliases.is_empty(),
            "Should find the HashMap type alias"
        );
        assert_eq!(type_aliases[0].name, "HashMap");
    }

    #[test]
    fn test_extract_python_entities() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = r#"
class Point:
    def __init__(self, x, y):
        self.x = x
        self.y = y

    def distance(self, other):
        return 0.0
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::Python)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::Python)
            .expect("Failed to extract");

        assert!(!entities.is_empty());

        let classes: Vec<_> = entities
            .iter()
            .filter(|e| e.kind == EntityKind::Class)
            .collect();
        assert!(!classes.is_empty());
    }

    #[test]
    fn test_extract_rust_inherent_impl_no_generics() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = r#"
pub struct OnceBool {
    inner: u32,
}

impl OnceBool {
    pub const fn new() -> Self {
        Self { inner: 0 }
    }

    pub fn get(&self) -> Option<bool> {
        None
    }

    pub fn set(&self, value: bool) -> Result<(), ()> {
        Ok(())
    }
}
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::Rust)
            .expect("Failed to extract");

        let structs: Vec<_> = entities
            .iter()
            .filter(|e| e.kind == EntityKind::Struct)
            .collect();
        assert_eq!(structs.len(), 1, "Should find exactly one struct");
        assert_eq!(structs[0].name, "OnceBool");

        let impls: Vec<_> = entities
            .iter()
            .filter(|e| e.kind == EntityKind::InherentImpl)
            .collect();
        assert_eq!(impls.len(), 1, "Should find exactly one inherent impl");
        assert_eq!(impls[0].name, "OnceBool");

        let methods: Vec<_> = entities
            .iter()
            .filter(|e| e.kind == EntityKind::Method)
            .collect();
        assert_eq!(
            methods.len(),
            0,
            "Methods inside impl block should be filtered out (they become children of impl)"
        );
    }

    #[test]
    fn test_extract_rust_inherent_impl_with_generics() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = r#"
pub struct OnceCell<T> {
    value: T,
}

impl<T> OnceCell<T> {
    pub const fn new() -> OnceCell<T> {
        OnceCell { value: unsafe { std::mem::zeroed() } }
    }

    pub fn get(&self) -> Option<&T> {
        None
    }

    pub fn set(&self, value: T) -> Result<(), T> {
        Ok(())
    }
}
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::Rust)
            .expect("Failed to extract");

        let structs: Vec<_> = entities
            .iter()
            .filter(|e| e.kind == EntityKind::Struct)
            .collect();
        assert_eq!(structs.len(), 1, "Should find exactly one struct");
        assert_eq!(structs[0].name, "OnceCell");

        let impls: Vec<_> = entities
            .iter()
            .filter(|e| e.kind == EntityKind::InherentImpl)
            .collect();
        assert_eq!(impls.len(), 1, "Should find exactly one inherent impl");
        assert_eq!(impls[0].name, "OnceCell");

        let methods: Vec<_> = entities
            .iter()
            .filter(|e| e.kind == EntityKind::Method)
            .collect();
        assert_eq!(
            methods.len(),
            0,
            "Methods inside generic impl block should be filtered out"
        );
    }

    #[test]
    fn test_extract_rust_trait_impl() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = r#"
pub trait Display {
    fn fmt(&self) -> String;
}

pub struct Point {
    x: f64,
    y: f64,
}

impl Display for Point {
    fn fmt(&self) -> String {
        format!("({}, {})", self.x, self.y)
    }
}
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::Rust)
            .expect("Failed to extract");

        let traits: Vec<_> = entities
            .iter()
            .filter(|e| e.kind == EntityKind::Trait)
            .collect();
        assert_eq!(traits.len(), 1, "Should find exactly one trait");
        assert_eq!(traits[0].name, "Display");

        let structs: Vec<_> = entities
            .iter()
            .filter(|e| e.kind == EntityKind::Struct)
            .collect();
        assert_eq!(structs.len(), 1, "Should find exactly one struct");
        assert_eq!(structs[0].name, "Point");

        let impls: Vec<_> = entities
            .iter()
            .filter(|e| e.kind == EntityKind::TraitImpl)
            .collect();
        assert_eq!(impls.len(), 1, "Should find exactly one trait impl");
        assert_eq!(impls[0].name, "Display");

        let methods: Vec<_> = entities
            .iter()
            .filter(|e| e.kind == EntityKind::Method)
            .collect();
        assert_eq!(
            methods.len(),
            0,
            "Methods inside trait impl should be filtered out"
        );
    }

    #[test]
    fn test_extract_rust_impl_with_unsafe_send_sync() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = r#"
pub struct OnceCell<T> {
    value: T,
}

unsafe impl<T: Sync + Send> Sync for OnceCell<T> {}
unsafe impl<T: Send> Send for OnceCell<T> {}

impl<T> OnceCell<T> {
    pub fn new() -> Self {
        OnceCell { value: unsafe { std::mem::zeroed() } }
    }
}
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::Rust)
            .expect("Failed to extract");

        let impls: Vec<_> = entities
            .iter()
            .filter(|e| e.kind == EntityKind::InherentImpl || e.kind == EntityKind::TraitImpl)
            .collect();

        let inherent_impls: Vec<_> = impls
            .iter()
            .filter(|e| e.kind == EntityKind::InherentImpl)
            .collect();
        let trait_impls: Vec<_> = impls
            .iter()
            .filter(|e| e.kind == EntityKind::TraitImpl)
            .collect();

        assert_eq!(
            inherent_impls.len(),
            1,
            "Should find exactly one inherent impl (impl<T> OnceCell<T>)"
        );
        assert_eq!(
            trait_impls.len(),
            2,
            "Should find two trait impls (Sync and Send)"
        );
        assert!(trait_impls.iter().any(|entity| entity.name == "Sync"));
        assert!(trait_impls.iter().any(|entity| entity.name == "Send"));
        assert!(
            trait_impls
                .iter()
                .all(|entity| entity.name.chars().all(|c| c.is_alphanumeric() || c == '_'))
        );
    }
}
