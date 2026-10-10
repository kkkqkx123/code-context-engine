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
use crate::tree_sitter_query::error::TreeSitterQueryError;
use crate::tree_sitter_query::executor::{Capture, QueryExecutor, QueryMatch};
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
    disambiguate_duplicate_siblings, establish_class_method_relationships,
    establish_function_scope_relationships, establish_go_method_relationships,
    establish_impl_method_relationships, establish_module_entity_relationships,
    establish_struct_field_relationships,
};
use super::post_processing;
use super::utils;

mod deduplication;
mod filtering;
mod metadata;
mod type_inference;

/// Folded tuple-unpacking matches keyed by assignment span; values carry
/// (name byte offset, name, template entity).
type PendingMultiple = std::collections::HashMap<(usize, usize), Vec<(usize, String, Entity)>>;

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

    /// Set the license header filtering configuration on the embedded
    /// comment processor.
    pub fn with_license_config(mut self, config: cce_config::LicenseHeaderConfig) -> Self {
        self.comment_processor.set_license_config(config);
        self
    }

    /// Replace the license header filtering configuration in place.
    pub fn set_license_config(&mut self, config: cce_config::LicenseHeaderConfig) {
        self.comment_processor.set_license_config(config);
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
    ) -> Result<Vec<Entity>, TreeSitterQueryError> {
        let matches = self
            .query_executor
            .execute_entity_query(tree, source, language)?;

        let mut context = ExtractionContext::new(self.id_counter.clone());
        let mut entities = Vec::new();
        let mut pending_annotations: Vec<String> = Vec::new();

        // First pass: collect all matches and identify impl/module blocks
        let mut impl_spans: Vec<std::ops::Range<usize>> = Vec::new();
        let mut module_spans: Vec<(cce_types::EntityId, std::ops::Range<usize>)> = Vec::new();

        // Tuple-unpacking matches (`first, second = pair`) yield one match
        // per bound name sharing the assignment span. They are collected
        // here and folded into a single comma-separated entity after the
        // loop so inference maps elements by position. Keyed by assignment
        // span; values carry (name byte offset, name, template entity).
        let mut pending_multiple: PendingMultiple = std::collections::HashMap::new();

        for mat in &matches {
            if let Some(mut entity) = self.process_match(mat, &mut context, source, language, tree)
            {
                let main_suffix_is_multiple = capture_module::parser::find_main_capture(mat)
                    .is_some_and(|main| main.name.ends_with(".multiple"));
                if main_suffix_is_multiple {
                    if let Some(name_cap) = capture_module::parser::find_name_capture(mat) {
                        let key = (entity.span.start_byte, entity.span.end_byte);
                        pending_multiple.entry(key).or_default().push((
                            name_cap.start_byte,
                            name_cap.text.trim().to_string(),
                            entity,
                        ));
                    }
                    continue;
                }
                // Fan-out for pattern matches binding independent names
                // (`case (x, y)`, `for k, v in ...`): one match carries a
                // name capture per variable, but `process_match` keeps only
                // the first. Clone one entity per extra name sharing the
                // same provenance metadata. (`.multiple` instead folds into
                // a single comma-separated entity for positional mapping.)
                // Blank bindings (`_`) are wildcards, never references: they
                // carry no symbol value and are dropped. When the first name
                // is blank but real siblings exist, the entity is re-homed
                // onto the first real name instead of the wildcard.
                let mut pattern_siblings = Vec::new();
                if let Some(main) = capture_module::parser::find_main_capture(mat) {
                    if main.name.ends_with(".loop") || main.name.ends_with(".case") {
                        let mut real_names: Vec<&Capture> = mat
                            .captures
                            .iter()
                            .filter(|c| crate::tree_sitter_query::capture::is_name_capture(&c.name))
                            .filter(|c| {
                                let name = c.text.trim();
                                !name.is_empty() && name != "_"
                            })
                            .collect();
                        if real_names.is_empty() {
                            continue;
                        }
                        let first = real_names.remove(0);
                        entity.name = utils::truncate_entity_name(first.text.trim().to_string());
                        entity.span = utils::create_span_from_capture(first);
                        for cap in real_names {
                            let sibling_name = cap.text.trim();
                            let mut sibling = entity.clone();
                            sibling.id = context.next_entity_id();
                            sibling.name = utils::truncate_entity_name(sibling_name.to_string());
                            sibling.span = utils::create_span_from_capture(cap);
                            pattern_siblings.push(sibling);
                        }
                    }
                }

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
                        continue;
                    }
                    // Other languages (e.g. Python decorators) keep
                    // annotations as standalone entities.
                    entities.push(entity);
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
                    entities.extend(pattern_siblings);
                }
            }
        }

        // Fold per-name tuple-unpacking matches into positional entities.
        for (_, mut group) in pending_multiple {
            if group.is_empty() {
                continue;
            }
            group.sort_by_key(|(byte, _, _)| *byte);
            let mut names = Vec::with_capacity(group.len());
            let mut template = None::<Entity>;
            for (_, name, entity) in group {
                if !name.is_empty() {
                    names.push(name);
                }
                if template.is_none() {
                    template = Some(entity);
                }
            }
            if let Some(mut entity) = template {
                if !names.is_empty() {
                    entity.name = names.join(", ");
                }
                entities.push(entity);
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

        // Fourth-C pass: recover generic arguments for bare constructor
        // calls (`new Container()`) from same-file class declarations
        // (`class Container<T>`). Runs after all match-level metadata so
        // `constructor_type` / `type_annotation` are already recorded.
        metadata::resolve_constructor_type_params(&mut entities);

        // Fifth pass: establish impl block -> method relationships based on span
        establish_impl_method_relationships(&mut entities);

        // Fifth-B: nest local variables and nested functions under the
        // innermost enclosing function (or type definition for methods) so
        // scoped names encode the enclosing scope and same-named locals in
        // different functions do not collide on stable symbol keys.
        establish_function_scope_relationships(&mut entities);

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

        // 7.5b: establish Go method -> struct parent relationships using receiver metadata.
        // Must run after receiver extraction so `receiver_type` metadata is available.
        establish_go_method_relationships(&mut entities);

        // 7.6: re-attach explicit `export` statement context (JS/TS family)
        // so export lists reflect real export statements.
        post_processing::mark_exported_entities(&mut entities, tree, source, language);

        // 7.7: assign C++ member visibility from access sections.
        post_processing::mark_cpp_access_sections(&mut entities, tree, source, language);

        // 7.8: CommonJS `var x = require('p')` — drop the Variable so the
        // whole statement stays out of retrieval conversion. The Require
        // entity remains for the relation index.
        post_processing::drop_js_require_bound_variables(&mut entities, language);

        // 7.85: tag duplicate siblings (same parent, name, kind, and signature)
        // with a source-order ordinal so each occurrence keeps its own stable
        // symbol key. Runs after every parenting pass so parents are final.
        disambiguate_duplicate_siblings(&mut entities);

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

        // Python: `self.x = ...` registers a class field only when it is a
        // real attribute declaration (in `__init__` or directly in a class
        // body). The same statement inside any other method is local
        // behavior, and registering it pollutes the field table and the
        // type-member index.
        if kind == cce_types::EntityKind::Field && language == &Language::Python {
            let field_node = tree
                .root_node()
                .descendant_for_byte_range(main_capture.start_byte, main_capture.end_byte);
            let allow =
                field_node.is_some_and(|node| python_field_scope_allows(node, source.as_bytes()));
            if !allow {
                return None;
            }
        }
        // `entity.macro.attribute.inner` shares the `attribute` subtype with
        // its outer counterpart; distinguish file-level inner attributes
        // (`#![...]`) so they are never buffered as entity modifiers.
        if main_capture.name.ends_with(".inner") {
            subtype = Some("attribute.inner".to_string());
        }
        // Pattern-bound variables (loop/except/with/case) span only their
        // bound name: statement-level spans would collapse distinct bindings
        // (`case (x, y)`) into one entity during span deduplication.
        let is_pattern_binding = main_capture.name.ends_with(".loop")
            || main_capture.name.ends_with(".except")
            || main_capture.name.ends_with(".with")
            || main_capture.name.ends_with(".case");
        let span = if is_pattern_binding {
            utils::create_span_from_capture(name_capture)
        } else {
            utils::create_span_from_capture(main_capture)
        };

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

        // Pattern matches binding several names at once (`first, second =
        // pair`) carry one name capture per variable. `.multiple` folds them
        // into a single comma-separated entity so the inference engine can
        // map tuple elements by position; the right-hand side is recorded as
        // the destructuring source.
        let mut entity = if main_capture.name.ends_with(".multiple") {
            let names: Vec<String> = mat
                .captures
                .iter()
                .filter(|c| crate::tree_sitter_query::capture::is_name_capture(&c.name))
                .map(|c| c.text.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect();
            let joined = if names.is_empty() {
                name_capture.text.clone()
            } else {
                names.join(", ")
            };
            Entity::new(id, kind, joined, span)
        } else {
            Entity::new(id, kind, name_capture.text.clone(), span)
        };
        entity.subtype = subtype;

        // Strip Ruby symbol prefix (`:name` → `name`) for attr_reader/
        // attr_writer/attr_accessor captures and other symbol-named entities.
        if language == &Language::Ruby && entity.name.starts_with(':') {
            entity.name = entity.name[1..].to_string();
        }

        // Structural captures name entities after whole AST subtrees (an IIFE
        // is named by its entire parenthesized body). Such names carry no
        // identifier value and pollute group names, NL headers, and symbol
        // tables, so they get a short synthesized name instead.
        if entity.subtype.as_deref() == Some("iife") {
            entity.name = "iife".to_string();
        }
        entity.name = utils::truncate_entity_name(entity.name);

        // Generic destructuring-source hookup: any `@....source` capture
        // records the provenance expression for pattern-bound variables
        // (`except E as e`, `case x:` against a subject, tuple unpacking
        // via `@....multiple.value`). The inference engine resolves
        // identifiers against parameters and known bindings; bare type
        // names bind directly.
        if !entity.metadata.contains_key("source_type") {
            if let Some(source) = mat
                .captures
                .iter()
                .find(|c| c.name.ends_with(".source") || c.name.ends_with(".multiple.value"))
            {
                let text = source.text.trim();
                if !text.is_empty() {
                    entity.set_metadata("source_type", text.to_string());
                }
            }
        }

        // Capture-level extraction
        entity.signature = capture_module::parser::extract_signature(mat, source);
        entity.parameters = capture_module::parser::extract_parameters(mat, language);
        let param_defaults = capture_module::parser::extract_parameter_defaults(mat, language);
        if !param_defaults.is_empty() {
            if let Ok(encoded) = serde_json::to_string(&param_defaults) {
                entity.set_metadata(cce_types::entity::meta_keys::PARAM_DEFAULTS, encoded);
            }
        }
        entity.return_type = capture_module::parser::extract_return_type(mat);

        // Plain JavaScript captures the return *expression*, not a type
        // annotation. Keep it only for literals (`return 1` → `number`);
        // drop anything else so callers fall back to `unknown` instead of
        // using `a() + b()` as a type name. TypeScript annotations pass
        // through untouched.
        if language == &Language::JavaScript {
            entity.return_type = entity
                .return_type
                .as_deref()
                .and_then(metadata::normalize_js_return_expression);
        }
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

/// Whether a Python `self.x = ...` statement is a field declaration:
/// directly inside a class body, or inside `__init__`. Walking stops at the
/// nearest enclosing function/class definition, so a nested helper inside
/// `__init__` does not count as declaring a field.
fn python_field_scope_allows(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        match ancestor.kind() {
            "class_definition" => return true,
            "function_definition" | "lambda" => {
                if ancestor.kind() != "function_definition" {
                    return false;
                }
                let Some(name_node) = ancestor.child_by_field_name("name") else {
                    return false;
                };
                return name_node
                    .utf8_text(source)
                    .is_ok_and(|name| name == "__init__");
            }
            _ => current = ancestor.parent(),
        }
    }
    false
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
    fn test_extract_csharp_class_base_metadata() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = r#"
public abstract class Shape
{
    public abstract string Kind { get; }
}

public class Circle : Shape
{
    public override string Kind => "Circle";
}
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::CSharp)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::CSharp)
            .expect("Failed to extract");

        let circle = entities
            .iter()
            .find(|e| e.kind == EntityKind::Class && e.name == "Circle")
            .expect("Circle class must be extracted");
        assert_eq!(
            circle.metadata.get("base_classes").map(String::as_str),
            Some("Shape"),
            "Circle must record its Shape base; all classes: {:?}",
            entities
                .iter()
                .filter(|e| e.kind == EntityKind::Class)
                .map(|e| (&e.name, e.metadata.get("base_classes")))
                .collect::<Vec<_>>()
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

        let point = classes
            .iter()
            .find(|e| e.name == "Point")
            .expect("Should find class Point");
        assert_eq!(point.signature, "Point");

        let init = entities
            .iter()
            .find(|e| e.name == "__init__")
            .expect("Should find __init__");
        assert_eq!(init.signature, "__init__ (self, x, y)");

        let distance = entities
            .iter()
            .find(|e| e.name == "distance")
            .expect("Should find distance");
        assert_eq!(distance.signature, "distance (self, other)");

        for entity in entities.iter().filter(|e| {
            matches!(
                e.kind,
                EntityKind::Function | EntityKind::Method | EntityKind::Class
            )
        }) {
            assert!(
                !entity.signature.contains("self.x = x")
                    && !entity.signature.contains("return 0.0"),
                "signature of {} must exclude the body, got {:?}",
                entity.name,
                entity.signature
            );
        }
    }

    #[test]
    fn test_extract_python_decorated_and_default_spacing() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = "@app.route(\"/read\")\ndef read() -> str:\n    return \"ok\"\n\ndef greet(name: str = \"a  b\"):\n    return name\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::Python)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::Python)
            .expect("Failed to extract");

        let read = entities
            .iter()
            .find(|e| e.name == "read")
            .expect("Should find read");
        assert_eq!(read.signature, "read () str");
        assert!(
            !read.signature.contains("@app.route"),
            "decorator must not enter signature, got {:?}",
            read.signature
        );

        let greet = entities
            .iter()
            .find(|e| e.name == "greet")
            .expect("Should find greet");
        assert_eq!(greet.signature, "greet (name: str = \"a  b\")");
    }

    #[test]
    fn test_extract_python_method_variants() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = "class Service:\n    @classmethod\n    def create(cls, name: str) -> str:\n        return name\n\n    def get(self, key: str) -> str:\n        return key\n\n    @staticmethod\n    def helper(x: int) -> int:\n        return x\n\n    @property\n    def label(self) -> str:\n        return \"x\"\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::Python)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::Python)
            .expect("Failed to extract");

        for expected in [
            ("create", "create (cls, name: str) str"),
            ("get", "get (self, key: str) str"),
            ("helper", "helper (x: int) int"),
            ("label", "label (self) str"),
        ] {
            let entity = entities
                .iter()
                .find(|e| e.name == expected.0)
                .unwrap_or_else(|| panic!("Should find {}", expected.0));
            assert_eq!(entity.signature, expected.1, "signature of {}", expected.0);
            assert!(
                !entity.signature.contains("return"),
                "body must not leak for {}, got {:?}",
                expected.0,
                entity.signature
            );
        }
    }

    #[test]
    fn test_extract_python_async_and_generator() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = "async def fetch(url: str) -> str:\n    return url\n\ndef gen(n: int):\n    yield n\n\ndouble = lambda x: x * 2\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::Python)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::Python)
            .expect("Failed to extract");

        let fetch = entities
            .iter()
            .find(|e| e.name == "fetch")
            .expect("Should find fetch");
        assert_eq!(fetch.signature, "fetch (url: str) str");

        let generator = entities
            .iter()
            .find(|e| e.name == "gen")
            .expect("Should find gen");
        assert_eq!(generator.signature, "gen (n: int)");

        let double = entities
            .iter()
            .find(|e| e.name == "double")
            .expect("Should find double");
        assert_eq!(double.signature, "double = lambda x: x * 2");
    }

    #[test]
    fn test_extract_rust_generic_struct_and_fn() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = "pub struct OnceCell<T> {\n    value: T,\n}\n\npub fn get<T>(key: &str) -> Option<T> {\n    None\n}\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::Rust)
            .expect("Failed to extract");

        let strukt = entities
            .iter()
            .find(|e| e.kind == EntityKind::Struct && e.name == "OnceCell")
            .expect("Should find struct OnceCell");
        assert_eq!(strukt.signature, "OnceCell <T>");
        assert!(
            !strukt.signature.contains("value: T,"),
            "struct body must not leak, got {:?}",
            strukt.signature
        );

        let func = entities
            .iter()
            .find(|e| e.name == "get")
            .expect("Should find fn get");
        assert_eq!(func.signature, "get <T> (key: &str) Option<T>");
        assert!(
            !func.signature.contains("None"),
            "function body must not leak, got {:?}",
            func.signature
        );
    }

    #[test]
    fn test_extract_rust_enum_trait_impl_const() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = "pub enum Color {\n    Red,\n    Green,\n}\n\npub trait Shape {\n    fn area(&self) -> f64;\n}\n\nimpl Point {\n    pub fn new() -> Self {\n        Point\n    }\n}\n\npub const MAX: u32 = 100;\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::Rust)
            .expect("Failed to extract");

        let color = entities
            .iter()
            .find(|e| e.kind == EntityKind::Enum && e.name == "Color")
            .expect("Should find enum Color");
        assert_eq!(color.signature, "Color");

        let shape = entities
            .iter()
            .find(|e| e.name == "Shape")
            .expect("Should find trait Shape");
        assert_eq!(shape.signature, "Shape");
        assert!(
            !shape.signature.contains("area"),
            "trait body must not leak, got {:?}",
            shape.signature
        );

        let imp = entities
            .iter()
            .find(|e| e.kind == EntityKind::InherentImpl)
            .expect("Should find inherent impl");
        assert_eq!(imp.signature, "Point");
        assert!(
            !imp.signature.contains("Self"),
            "impl body must not leak, got {:?}",
            imp.signature
        );

        let max = entities
            .iter()
            .find(|e| e.name == "MAX")
            .expect("Should find const MAX");
        assert_eq!(max.signature, "MAX u32");
    }

    #[test]
    fn test_extract_go_signatures() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = "package main\n\ntype Point struct {\n    X int\n}\n\nfunc (p Point) Distance(other Point) int {\n    return 0\n}\n\nfunc New(name string) Point {\n    return Point{}\n}\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::Go)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::Go)
            .expect("Failed to extract");

        for expected in [
            ("Point", "Point"),
            ("Distance", "Distance (other Point) int"),
            ("New", "New (name string) Point"),
        ] {
            let entity = entities
                .iter()
                .find(|e| e.name == expected.0)
                .unwrap_or_else(|| panic!("Should find {}", expected.0));
            assert_eq!(entity.signature, expected.1, "signature of {}", expected.0);
            assert!(
                !entity.signature.contains("return 0") && !entity.signature.contains("X int"),
                "body must not leak for {}, got {:?}",
                expected.0,
                entity.signature
            );
        }
    }

    #[test]
    fn test_extract_go_range_loop_skips_blank_and_summarizes_source() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = "package main\n\nfunc f() {\n\tfor _, tt := range []struct {\n\t\tname string\n\t\tvalue any\n\t}{\n\t\t{\"base type\", 1},\n\t\t{\"zero value\", 0},\n\t\t{\"base type\", 1},\n\t\t{\"zero value\", 0},\n\t\t{\"base type\", 1},\n\t\t{\"zero value\", 0},\n\t\t{\"base type\", 1},\n\t\t{\"zero value\", 0},\n\t\t{\"base type\", 1},\n\t\t{\"zero value\", 0},\n\t} {\n\t\tprintln(tt.name)\n\t}\n}\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::Go)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::Go)
            .expect("Failed to extract");

        assert!(
            entities.iter().all(|e| e.name != "_"),
            "blank range bindings must not become entities"
        );
        let tt = entities
            .iter()
            .find(|e| e.name == "tt")
            .expect("Should find range variable tt");
        assert!(
            tt.signature.starts_with("tt []struct {"),
            "summary must keep the collection head, got {:?}",
            tt.signature
        );
        assert!(
            tt.signature.contains("name string"),
            "summary must keep field shapes, got {:?}",
            tt.signature
        );
        assert!(
            !tt.signature.contains("base type"),
            "summary must drop literal rows, got {:?}",
            tt.signature
        );
        assert!(
            tt.signature.ends_with("..."),
            "summary must be marked, got {:?}",
            tt.signature
        );
        assert!(tt.signature.chars().count() <= 500);
    }

    #[test]
    fn test_extract_java_signatures() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = "public class Point extends Shape {\n    public Point(int x) {\n        this.x = x;\n    }\n    public int getX() {\n        return x;\n    }\n}\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::Java)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::Java)
            .expect("Failed to extract");

        for expected in [("Point", "Point Shape"), ("getX", "int getX ()")] {
            let entity = entities
                .iter()
                .find(|e| e.name == expected.0)
                .unwrap_or_else(|| panic!("Should find {}", expected.0));
            assert_eq!(entity.signature, expected.1, "signature of {}", expected.0);
        }
        let ctor = entities
            .iter()
            .find(|e| e.kind == EntityKind::Constructor)
            .expect("Should find constructor");
        assert_eq!(ctor.signature, "Point (int x)");
        for entity in &entities {
            assert!(
                !entity.signature.contains("this.x") && !entity.signature.contains("return x"),
                "body must not leak for {}, got {:?}",
                entity.name,
                entity.signature
            );
        }
    }

    #[test]
    fn test_extract_typescript_signatures() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = "class Store extends Base {\n    get(key: string): string {\n        return \"\";\n    }\n}\nfunction add(a: number, b: number): number {\n    return a + b;\n}\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::TypeScript)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::TypeScript)
            .expect("Failed to extract");

        for expected in [
            ("Store", "Store Base"),
            ("get", "get (key: string) string"),
            ("add", "add (a: number, b: number) number"),
        ] {
            let entity = entities
                .iter()
                .find(|e| e.name == expected.0)
                .unwrap_or_else(|| panic!("Should find {}", expected.0));
            assert_eq!(entity.signature, expected.1, "signature of {}", expected.0);
            assert!(
                !entity.signature.contains("return"),
                "body must not leak for {}, got {:?}",
                expected.0,
                entity.signature
            );
        }
    }

    #[test]
    fn test_extract_javascript_signatures() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = "class Dog extends Animal {\n    bark(volume) {\n        return volume;\n    }\n}\nfunction add(a, b) {\n    return a + b;\n}\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::JavaScript)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::JavaScript)
            .expect("Failed to extract");

        for expected in [
            ("Dog", "Dog Animal"),
            ("bark", "bark (volume)"),
            ("add", "add (a, b)"),
        ] {
            let entity = entities
                .iter()
                .find(|e| e.name == expected.0)
                .unwrap_or_else(|| panic!("Should find {}", expected.0));
            assert_eq!(entity.signature, expected.1, "signature of {}", expected.0);
            assert!(
                !entity.signature.contains("return"),
                "body must not leak for {}, got {:?}",
                expected.0,
                entity.signature
            );
        }
    }

    #[test]
    fn test_extract_javascript_chained_assignment_signatures() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = "req.get =\nreq.header = function header(name) {\n    return name;\n};\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::JavaScript)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::JavaScript)
            .expect("Failed to extract");

        for expected in [("get", "get (name)"), ("header", "header (name)")] {
            let entity = entities
                .iter()
                .find(|e| e.kind == EntityKind::Method && e.name == expected.0)
                .unwrap_or_else(|| panic!("Should find method {}", expected.0));
            assert_eq!(entity.signature, expected.1, "signature of {}", expected.0);
            assert!(
                !entity.signature.contains("return"),
                "body must not leak for {}, got {:?}",
                expected.0,
                entity.signature
            );
        }
    }

    #[test]
    fn test_extract_javascript_chained_non_function_stays_variable() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = "req.originalUrl = req.url = req.originalUrl.replace(/x/, 'y');\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::JavaScript)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::JavaScript)
            .expect("Failed to extract");

        assert!(
            entities.iter().all(|e| !e.signature.trim().is_empty()),
            "chained non-function assignment must not produce empty signatures"
        );
        assert!(
            entities
                .iter()
                .all(|e| e.kind != EntityKind::Method || !e.name.contains("originalUrl")),
            "chained non-function assignment must not be classified as a method"
        );
    }

    #[test]
    fn test_extract_javascript_callback_signatures() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = "function suite() {\n    describe('Auth', function () {\n        it('logs in', function (done) {\n            return done;\n        });\n    });\n    app.post('/login', function (req, res) {\n        return res;\n    });\n}\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::JavaScript)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::JavaScript)
            .expect("Failed to extract");

        for expected in [
            ("describe", "describe 'Auth'"),
            ("it", "it 'logs in'"),
            ("app.post", "app.post '/login'"),
        ] {
            let entity = entities
                .iter()
                .find(|e| e.kind == EntityKind::Function && e.name == expected.0)
                .unwrap_or_else(|| panic!("Should find callback {}", expected.0));
            assert_eq!(entity.signature, expected.1, "signature of {}", expected.0);
            assert!(
                !entity.signature.contains("return"),
                "callback body must not leak for {}, got {:?}",
                expected.0,
                entity.signature
            );
        }
    }

    #[test]
    fn test_extract_javascript_arrow_callback_without_label() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = "function setup() {\n    app.use(function (req, res, next) {\n        return next;\n    });\n}\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::JavaScript)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::JavaScript)
            .expect("Failed to extract");

        let entity = entities
            .iter()
            .find(|e| e.kind == EntityKind::Function && e.name == "app.use")
            .expect("Should find callback app.use");
        assert_eq!(entity.signature, "app.use");
        assert!(
            !entity.signature.contains("return"),
            "callback body must not leak, got {:?}",
            entity.signature
        );
    }

    #[test]
    fn test_extract_c_signatures() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code =
            "struct Point {\n    int x;\n};\n\nint add(int a, int b) {\n    return a + b;\n}\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::C)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::C)
            .expect("Failed to extract");

        for expected in [("Point", "Point"), ("add", "int add (int a, int b)")] {
            let entity = entities
                .iter()
                .find(|e| e.name == expected.0)
                .unwrap_or_else(|| panic!("Should find {}", expected.0));
            assert_eq!(entity.signature, expected.1, "signature of {}", expected.0);
            assert!(
                !entity.signature.contains("return a") && !entity.signature.contains("int x;"),
                "body must not leak for {}, got {:?}",
                expected.0,
                entity.signature
            );
        }
    }

    #[test]
    fn test_extract_csharp_method_signature() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = "public class Store {\n    public string Get(string key) {\n        return key;\n    }\n}\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::CSharp)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::CSharp)
            .expect("Failed to extract");

        let class = entities
            .iter()
            .find(|e| e.kind == EntityKind::Class && e.name == "Store")
            .expect("Should find class Store");
        assert_eq!(class.signature, "Store");

        let method = entities
            .iter()
            .find(|e| e.name == "Get")
            .expect("Should find method Get");
        assert_eq!(method.signature, "string Get (string key)");
        assert!(
            !method.signature.contains("return key"),
            "body must not leak, got {:?}",
            method.signature
        );
    }

    #[test]
    fn test_extract_csharp_namespace_signatures() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = "namespace Shop {\n    public class Store {\n    }\n}\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::CSharp)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::CSharp)
            .expect("Failed to extract");

        let namespace = entities
            .iter()
            .find(|e| e.kind == EntityKind::Namespace && e.name == "Shop")
            .expect("Should find namespace Shop");
        assert_eq!(namespace.signature, "Shop");
        assert!(
            !namespace.signature.contains("class Store"),
            "namespace body must not leak, got {:?}",
            namespace.signature
        );

        let file_scoped = "namespace Shop;\npublic class Store {\n}\n";
        let tree = ast_parser
            .parse_with_tree(file_scoped, &Language::CSharp)
            .expect("Failed to parse")
            .0;
        let entities = extractor
            .extract(&tree, file_scoped, &Language::CSharp)
            .expect("Failed to extract");
        let namespace = entities
            .iter()
            .find(|e| e.kind == EntityKind::Namespace && e.name == "Shop")
            .expect("Should find file-scoped namespace Shop");
        assert_eq!(namespace.signature, "Shop");
    }

    #[test]
    fn test_extract_csharp_property_signature() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = "public class Fixture {\n    public IServiceProvider Provider {\n        get {\n            return container;\n        }\n    }\n}\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::CSharp)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::CSharp)
            .expect("Failed to extract");

        let property = entities
            .iter()
            .find(|e| e.kind == EntityKind::Property && e.name == "Provider")
            .expect("Should find property Provider");
        assert_eq!(property.signature, "IServiceProvider Provider");
        assert!(
            !property.signature.contains("return"),
            "accessor body must not leak, got {:?}",
            property.signature
        );
    }

    #[test]
    fn test_extract_typescript_namespace_signature() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = "namespace Shop {\n    export const rate = 1;\n}\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::TypeScript)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::TypeScript)
            .expect("Failed to extract");

        let namespace = entities
            .iter()
            .find(|e| e.kind == EntityKind::Namespace && e.name == "Shop")
            .expect("Should find namespace Shop");
        assert_eq!(namespace.signature, "Shop");
        assert!(
            !namespace.signature.contains("rate"),
            "namespace body must not leak, got {:?}",
            namespace.signature
        );
    }

    #[test]
    fn test_extract_java_enum_constant_signature() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = "enum Status {\n    ASYNC(\"a\") {\n        void run() {\n        }\n    },\n    SYNC;\n}\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::Java)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::Java)
            .expect("Failed to extract");

        let constant = entities
            .iter()
            .find(|e| e.kind == EntityKind::EnumVariant && e.name == "ASYNC")
            .expect("Should find enum constant ASYNC");
        assert_eq!(constant.signature, "ASYNC (\"a\")");
        assert!(
            !constant.signature.contains("run"),
            "constant class body must not leak, got {:?}",
            constant.signature
        );
    }

    #[test]
    fn test_extract_python_except_signature() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = "try:\n    risky()\nexcept ValueError as e:\n    raise e\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::Python)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::Python)
            .expect("Failed to extract");

        let binding = entities
            .iter()
            .find(|e| e.kind == EntityKind::Variable && e.name == "e")
            .expect("Should find except binding e");
        assert_eq!(binding.signature, "ValueError e");
        assert!(
            !binding.signature.contains("raise"),
            "except body must not leak, got {:?}",
            binding.signature
        );
    }

    #[test]
    fn test_extract_rust_module_excludes_body() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = "pub mod sync {\n    pub struct Guard;\n    pub fn lock() -> Guard {\n        Guard\n    }\n}\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::Rust)
            .expect("Failed to extract");

        let module = entities
            .iter()
            .find(|e| e.kind == EntityKind::Module && e.name == "sync")
            .expect("Should find mod sync");
        assert_eq!(module.signature, "sync");
        assert!(
            !module.signature.contains("Guard"),
            "module body must not leak into signature, got {:?}",
            module.signature
        );
    }

    #[test]
    fn test_extract_rust_macro_rules_excludes_rules() {
        let mut ast_parser = AstParser::new();
        let extractor = EntityExtractor::new();

        let code = "macro_rules! setup {\n    () => { 1 };\n}\n";

        let tree = ast_parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse")
            .0;

        let entities = extractor
            .extract(&tree, code, &Language::Rust)
            .expect("Failed to extract");

        let found = entities.iter().find(|e| e.name == "setup");
        if let Some(mac) = found {
            assert!(
                !mac.signature.contains("=>"),
                "macro rules must not leak into signature, got {:?}",
                mac.signature
            );
        }
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
