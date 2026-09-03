//! Local call resolver for resolving calls within a single file
//!
//! This module provides functionality for resolving local calls within a file,
//! converting Relation dst_name to EntityId and calculating call order.
//!
//! # Position in Architecture
//!
//! This module serves as a **pre-processing stage** before `RelationResolver`:
//! 1. `LocalCallResolver`: Resolves calls within a single file (file-level)
//! 2. `RelationResolver`: Resolves cross-file calls and builds the full index
//!
//! The separation allows for:
//! - Efficient single-file processing during incremental updates
//! - Clear separation between local and cross-file resolution
//!
//! Note: This module was moved from parser::extractor to relation module
//! to maintain a clear separation of concerns:
//! - parser: Extracts raw semantic data
//! - relation: Resolves and indexes relationships

use cce_metrics::RelationMetrics;
use cce_types::relation::RelationLevel;
use cce_types::{Entity, EntityId, Relation};
use std::collections::HashMap;
use std::sync::Arc;

use crate::ast_accessor::count_arguments;

/// Local call within the same file (resolved to EntityId)
///
/// This is an internal type used by LocalCallResolver and IndexBuilder.
/// It is not stored in ParsedFile - local calls are resolved on-demand.
#[derive(Debug, Clone)]
pub struct LocalCall {
    /// Caller entity ID
    pub caller: EntityId,
    /// Callee entity ID (resolved within the file)
    pub callee: EntityId,
    /// Callee name (kept for debugging)
    pub callee_name: String,
    /// Call location
    pub span: cce_types::Span,
    /// Call order (sequence number within the function)
    pub call_order: usize,
    /// Relation type
    pub relation_type: cce_types::relation::RelationType,
    /// Standard library category (if this is a stdlib call)
    ///
    /// Set during Parser phase when the Relation is created.
    /// This eliminates duplicate stdlib detection in CallMerger.
    pub stdlib_category: Option<cce_types::StdlibCategory>,
}

/// Configuration for local call resolution
#[derive(Debug, Clone, Default)]
pub struct LocalCallResolverConfig {
    /// Whether to enable function signature matching
    pub enable_signature_matching: bool,
    /// Whether to skip cross-file calls silently
    pub skip_cross_file_calls: bool,
    /// Whether to log unresolved calls for debugging
    pub log_unresolved_calls: bool,
}

/// Local call resolver
///
/// Resolves function calls within a single file.
pub struct LocalCallResolver {
    config: LocalCallResolverConfig,
    /// Relation metrics; ambiguity events are only recorded when present
    metrics: Option<Arc<RelationMetrics>>,
}

impl LocalCallResolver {
    /// Create a new local call resolver with default configuration
    pub fn new() -> Self {
        Self {
            config: LocalCallResolverConfig::default(),
            metrics: None,
        }
    }

    /// Create a new local call resolver with custom configuration
    pub fn with_config(config: LocalCallResolverConfig) -> Self {
        Self {
            config,
            metrics: None,
        }
    }

    /// Attach relation metrics for ambiguity monitoring
    pub fn with_metrics(metrics: Arc<RelationMetrics>) -> Self {
        Self {
            config: LocalCallResolverConfig::default(),
            metrics: Some(metrics),
        }
    }

    /// Get the current configuration
    pub fn config(&self) -> &LocalCallResolverConfig {
        &self.config
    }

    /// Update the configuration
    pub fn set_config(&mut self, config: LocalCallResolverConfig) {
        self.config = config;
    }

    /// Resolve local calls within the file
    ///
    /// Converts Relation dst_name to EntityId for local calls,
    /// and calculates call order within each function.
    ///
    /// # Arguments
    ///
    /// * `relations` - Raw relations extracted from the file
    /// * `entities` - Entities extracted from the file
    ///
    /// # Returns
    ///
    /// Vector of resolved local calls
    pub fn resolve(&self, relations: &[Relation], entities: &[Entity]) -> Vec<LocalCall> {
        self.resolve_with_source(relations, entities, None)
    }

    /// Resolve local calls with access to the file source text.
    ///
    /// When `source` is supplied, the call site's actual argument count is
    /// derived from the call expression and used for overload disambiguation.
    pub fn resolve_with_source(
        &self,
        relations: &[Relation],
        entities: &[Entity],
        source: Option<&str>,
    ) -> Vec<LocalCall> {
        let mut local_calls = Vec::new();

        // Only process call relations
        let call_relations: Vec<_> = relations
            .iter()
            .filter(|r| r.relation_type.is_call())
            .collect();

        // Build entity lookup map for faster access
        let entity_map: HashMap<&str, Vec<&Entity>> = entities
            .iter()
            .filter(|e| e.kind.is_function_like())
            .fold(HashMap::new(), |mut map, entity| {
                map.entry(&entity.name)
                    .or_insert_with(Vec::new)
                    .push(entity);
                map
            });

        // Id lookup over all entities (not only function-like ones) for the
        // caller scope chain used in tier 1.
        let by_id: HashMap<EntityId, &Entity> = entities.iter().map(|e| (e.id, e)).collect();

        // Try to resolve callee for each call relation
        for relation in &call_relations {
            let callee_name = relation.dst_name();

            // Find matching callee within the file. The entity map is keyed
            // by simple names; extraction now preserves full paths
            // (`obj.method`, `Foo::new`), so fall back to the last segment
            // when the full name misses.
            let candidates: Option<&Vec<&Entity>> = entity_map.get(callee_name).or_else(|| {
                let last = callee_name.rsplit(['.', ':']).next().unwrap_or(callee_name);
                (last != callee_name)
                    .then(|| entity_map.get(last))
                    .flatten()
            });

            if let Some(candidates) = candidates {
                let argument_count = relation
                    .argument_count
                    .or_else(|| source.and_then(|src| count_call_arguments(src, relation.span)));
                let callee = self.select_callee(candidates, relation, &by_id, argument_count);

                // Self-loop suppression: a call that resolves to its own caller
                // is almost always a mis-resolved qualified call (e.g. value.clone
                // falling back to local `clone`) rather than intentional recursion.
                // Filter when the callee is the caller itself, or when the names
                // match and the call site lies strictly inside the caller's span.
                let caller_id = EntityId(relation.caller_id as u64);
                let is_self_loop = if callee.id == caller_id {
                    true
                } else if let Some(caller_entity) = by_id.get(&caller_id) {
                    callee.name == caller_entity.name
                        && relation.span.start_byte > caller_entity.span.start_byte
                        && relation.span.end_byte < caller_entity.span.end_byte
                        && relation.relation_type.is_call()
                } else {
                    false
                };
                if is_self_loop {
                    // For explicit `Self::method` or `self.method` recursion
                    // the raw name is qualified; keep it, otherwise drop the
                    // spurious edge.
                    let is_explicit_self = callee_name.starts_with("Self::")
                        || callee_name.starts_with("Self.")
                        || callee_name.starts_with("self.")
                        || callee_name.starts_with("self::")
                        || callee_name.starts_with("this.")
                        || callee_name.starts_with("this::")
                        || callee_name.starts_with("super.")
                        || callee_name.starts_with("super::")
                        || callee_name.starts_with("cls.")
                        || callee_name.starts_with("cls::")
                        || callee_name == "self"
                        || callee_name == "Self"
                        || callee_name == "this"
                        || callee_name == "super"
                        || callee_name == "cls";
                    if !is_explicit_self {
                        tracing::trace!(
                            "Suppressed self-loop: {} calling {} at {}:{}",
                            caller_id.0,
                            callee_name,
                            relation.span.start_byte,
                            relation.span.end_byte,
                        );
                        continue;
                    }
                }

                // Calculate call order
                let call_order =
                    Self::calculate_call_order(&relation.span, relation.caller_id, &call_relations);

                local_calls.push(LocalCall {
                    caller: EntityId(relation.caller_id as u64),
                    callee: callee.id,
                    callee_name: callee_name.to_string(),
                    span: relation.span,
                    call_order: call_order.unwrap_or(0),
                    relation_type: relation.relation_type,
                    stdlib_category: relation.stdlib_category,
                });
            } else if self.config.log_unresolved_calls {
                tracing::trace!(
                    "Could not resolve call to '{}' from caller {}",
                    callee_name,
                    relation.caller_id
                );
            }
        }

        // Sort by caller and call order for easier processing
        local_calls.sort_by_key(|c| (c.caller, c.call_order));

        local_calls
    }

    /// Select the callee from same-named candidates using tiered
    /// disambiguation.
    ///
    /// Tiers, in order:
    /// 1. **Scope chain**: candidates declared inside the caller's scope
    ///    chain (the caller or one of its ancestors) win — a nested or
    ///    sibling-scope definition shadows file-level overloads.
    /// 2. **Parameter signature**: when enabled and the call site's
    ///    argument count is known, prefer the candidate whose parameter
    ///    count matches the actual call arguments.
    /// 3. **Span-nearest**: the candidate whose declaration span is closest
    ///    to the call site (closest declaration wins for plain overloads).
    ///
    /// When several candidates remain at the chosen tier (true overload
    /// ambiguity), the deterministic first candidate is kept and the event
    /// is recorded through `relation_ambiguous_targets_total` so language
    /// teams can spot names that need a dedicated disambiguation rule.
    fn select_callee<'a>(
        &self,
        candidates: &[&'a Entity],
        relation: &Relation,
        by_id: &HashMap<EntityId, &'a Entity>,
        argument_count: Option<usize>,
    ) -> &'a Entity {
        if candidates.len() == 1 {
            return candidates[0];
        }

        // Tier 1: scope-chain candidates. The caller's scope chain is the
        // caller plus all its ancestors (walking `parent` pointers); a
        // candidate whose parent is part of that chain is declared within
        // the caller's scope and shadows everything else.
        if let Some(caller) = by_id.get(&EntityId(relation.caller_id as u64)) {
            let mut chain: Vec<&Entity> = Vec::new();
            let mut current: Option<&Entity> = Some(*caller);
            while let Some(entity) = current {
                chain.push(entity);
                current = entity
                    .parent
                    .and_then(|parent_id| by_id.get(&parent_id))
                    .copied();
            }
            let scope_candidates: Vec<&Entity> = candidates
                .iter()
                .copied()
                .filter(|c| {
                    c.parent
                        .map(|parent_id| chain.iter().any(|e| e.id == parent_id))
                        .unwrap_or(false)
                })
                .collect();
            if scope_candidates.len() == 1 {
                return scope_candidates[0];
            }
            if scope_candidates.len() > 1 {
                return self.record_ambiguity(scope_candidates);
            }
        }

        // Tier 2: parameter-count match against the call site's actual
        // argument count. A unique arity match beats positional proximity;
        // a non-unique or missing match defers to the next tier.
        if self.config.enable_signature_matching && argument_count.is_some() {
            let expected = argument_count.unwrap_or(0);
            let arity_matches: Vec<&Entity> = candidates
                .iter()
                .copied()
                .filter(|c| c.parameters.len() == expected)
                .collect();
            if arity_matches.len() == 1 {
                return arity_matches[0];
            }
        }

        // Tier 3: span-nearest — the declaration closest to the call site.
        let call_start = relation.span.start_byte;
        let mut nearest = candidates.to_vec();
        nearest.sort_by_key(|c| {
            let distance = c.span.start_byte.abs_diff(call_start);
            (distance, c.id.0)
        });
        if nearest[0].span.start_byte.abs_diff(call_start)
            == nearest[1].span.start_byte.abs_diff(call_start)
        {
            // equidistant declarations: genuinely ambiguous
            return self.record_ambiguity(nearest);
        }
        nearest[0]
    }

    /// Record an ambiguous-targets event and return the deterministic first
    /// candidate.
    fn record_ambiguity<'a>(&self, candidates: Vec<&'a Entity>) -> &'a Entity {
        if let Some(metrics) = &self.metrics {
            metrics.relation_ambiguous_targets_total.increment();
        }
        candidates[0]
    }

    /// Calculate call order within a function
    ///
    /// Returns the sequence number of the call within the caller function.
    fn calculate_call_order(
        call_span: &cce_types::Span,
        caller_id: i64,
        relations: &[&Relation],
    ) -> Option<usize> {
        // Find all calls from this caller
        let caller_calls: Vec<_> = relations
            .iter()
            .filter(|r| r.caller_id == caller_id && r.relation_type.is_call())
            .copied()
            .collect();

        // Sort by call position
        let mut sorted_calls = caller_calls.clone();
        sorted_calls.sort_by_key(|r| r.span.start_byte);

        // Find the position of current call
        sorted_calls
            .iter()
            .position(|r| r.span.start_byte == call_span.start_byte)
            .map(|pos| pos + 1) // 1-based indexing
    }

    /// Resolve local calls from a ParsedFile
    ///
    /// Convenience method that extracts relations and entities from a ParsedFile
    /// and resolves local calls. The file source is supplied so overload
    /// disambiguation can compare the call site's actual argument count.
    ///
    /// # Arguments
    ///
    /// * `parsed_file` - The parsed file containing relations and entities
    ///
    /// # Returns
    ///
    /// Vector of resolved local calls
    pub fn resolve_from_parsed_file(&self, parsed_file: &cce_types::ParsedFile) -> Vec<LocalCall> {
        // Convert raw relations to Relations
        let relations: Vec<Relation> = parsed_file
            .raw_relations
            .iter()
            .map(|r| Relation {
                caller_level: RelationLevel::Entity,
                caller_id: r.src.0 as i64,
                dst: cce_types::RelationTarget::unresolved(r.dst_name.clone()),
                relation_type: r.relation_type,
                span: r.span,
                stdlib_category: r.stdlib_category,
                argument_count: None,
            })
            .collect();

        self.resolve_with_source(&relations, &parsed_file.entities, Some(&parsed_file.source))
    }
}

/// Deterministic AST-based argument counting.
///
/// When a `tree_sitter::Node` for the call expression is available this path
/// is preferred – it counts `arguments` named children without scanning source
/// text, so nested parentheses and string literals cannot corrupt the count.
pub fn count_call_arguments_from_node(node: tree_sitter::Node) -> Option<usize> {
    count_arguments(node)
}

/// Legacy source-text argument counting for call sites without an AST node.
///
/// The scan is deterministic and handles string literals and comments, but
/// callers with a `Node` should use `count_call_arguments_from_node`.
/// This is a private fallback used only by `resolve_with_source`.
fn count_call_arguments(source: &str, span: cce_types::Span) -> Option<usize> {
    let bytes = source.as_bytes();
    let start = span.start_byte.min(bytes.len());
    let end = span.end_byte.min(bytes.len()).max(start);

    let mut i = start;
    while i < end && bytes[i] != b'(' {
        i += 1;
    }
    if i >= end {
        return None;
    }

    let mut depth = 1usize;
    let mut commas = 0usize;
    let mut saw_content = false;
    let mut in_string: Option<u8> = None;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    i += 1;
    while i < end {
        let b = bytes[i];
        if in_line_comment {
            if b == b'\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }
        if in_block_comment {
            if b == b'*' && i + 1 < end && bytes[i + 1] == b'/' {
                in_block_comment = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if let Some(quote) = in_string {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == quote {
                in_string = None;
            } else if quote == b'`' && b == b'$' && i + 1 < end && bytes[i + 1] == b'{' {
                depth += 1;
            }
            i += 1;
            continue;
        }
        if b == b'/' && i + 1 < end {
            if bytes[i + 1] == b'/' {
                in_line_comment = true;
                i += 2;
                continue;
            }
            if bytes[i + 1] == b'*' {
                in_block_comment = true;
                i += 2;
                continue;
            }
        }
        match b {
            b'"' | b'\'' | b'`' => in_string = Some(b),
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(if saw_content { commas + 1 } else { 0 });
                }
            }
            b',' if depth == 1 => commas += 1,
            _ => {}
        }
        if !b.is_ascii_whitespace() && depth >= 1 {
            saw_content = true;
        }
        i += 1;
    }
    None
}

impl Default for LocalCallResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::{EntityKind, RelationType, Span};
    use std::collections::HashMap;

    fn create_test_function_entity(id: u32, name: &str) -> Entity {
        Entity {
            id: EntityId(id.into()),
            kind: EntityKind::Function,
            name: name.to_string(),
            signature: format!("fn {}()", name),
            parameters: Vec::new(),
            return_type: None,
            span: Span::default(),
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

    fn create_test_relation(caller_id: u32, callee_name: &str) -> Relation {
        Relation::entity_relation(
            caller_id as i64,
            cce_types::RelationTarget::unresolved(callee_name.to_string()),
            RelationType::DirectCall,
            Span::default(),
        )
    }

    #[test]
    fn test_resolve_local_calls() {
        let resolver = LocalCallResolver::new();

        // Create test entities
        let entities = vec![
            create_test_function_entity(0, "foo"),
            create_test_function_entity(1, "bar"),
        ];

        // Create test relations (bar calls foo)
        let relations = vec![create_test_relation(1, "foo")];

        let local_calls = resolver.resolve(&relations, &entities);

        assert_eq!(local_calls.len(), 1);
        assert_eq!(local_calls[0].caller, EntityId(1));
        assert_eq!(local_calls[0].callee, EntityId(0));
        assert_eq!(local_calls[0].callee_name, "foo");
    }

    #[test]
    fn test_resolve_skips_cross_file_calls() {
        let resolver = LocalCallResolver::new();

        // Create test entities (only foo, no baz)
        let entities = vec![create_test_function_entity(0, "foo")];

        // Create test relation (foo calls baz which is in another file)
        let relations = vec![create_test_relation(0, "baz")];

        let local_calls = resolver.resolve(&relations, &entities);

        // Should not resolve cross-file calls
        assert_eq!(local_calls.len(), 0);
    }

    #[test]
    fn test_resolve_with_config() {
        let config = LocalCallResolverConfig {
            enable_signature_matching: true,
            skip_cross_file_calls: true,
            log_unresolved_calls: true,
        };
        let resolver = LocalCallResolver::with_config(config);

        // Create test entities
        let entities = vec![
            create_test_function_entity(0, "foo"),
            create_test_function_entity(1, "bar"),
        ];

        // Create test relations
        let relations = vec![create_test_relation(1, "foo")];

        let local_calls = resolver.resolve(&relations, &entities);

        assert_eq!(local_calls.len(), 1);
        assert_eq!(local_calls[0].caller, EntityId(1));
        assert_eq!(local_calls[0].callee, EntityId(0));
    }

    #[test]
    fn test_resolve_multiple_candidates() {
        let resolver = LocalCallResolver::new();

        // Create test entities with same name (function overloading)
        let mut entity1 = create_test_function_entity(0, "foo");
        entity1.parameters = vec![("x".to_string(), Some("i32".to_string()))];

        let mut entity2 = create_test_function_entity(1, "foo");
        entity2.parameters = vec![("x".to_string(), Some("f64".to_string()))];

        let entities = vec![entity1, entity2];

        // Create test relation
        let relations = vec![create_test_relation(2, "foo")];

        let local_calls = resolver.resolve(&relations, &entities);

        // Should resolve to first candidate when signature matching is disabled
        assert_eq!(local_calls.len(), 1);
        assert_eq!(local_calls[0].callee, EntityId(0));
    }

    /// Tier 2 disambiguation: the candidate whose parameter count matches
    /// the call site's actual argument count wins over positional proximity.
    #[test]
    fn arity_match_beats_span_proximity_for_overloads() {
        let config = LocalCallResolverConfig {
            enable_signature_matching: true,
            ..Default::default()
        };
        let resolver = LocalCallResolver::with_config(config);

        // Two same-named candidates with distinct arities. The two-parameter
        // declaration is placed CLOSER to both call sites so span-nearest
        // alone would always pick it; the arity match must override that.
        let mut one_param = create_test_function_entity(0, "foo");
        one_param.parameters = vec![("a".to_string(), None)];
        let mut two_param = create_test_function_entity(1, "foo");
        two_param.parameters = vec![("a".to_string(), None), ("b".to_string(), None)];
        one_param.span.start_byte = 0;
        one_param.span.end_byte = 5;
        two_param.span.start_byte = 10;
        two_param.span.end_byte = 20;
        let entities = vec![one_param, two_param];

        // Call sites inside a synthetic source snippet.
        let source = "let x = foo(1); let y = foo(2, 3);";
        let call_one_arg = create_test_relation(7, "foo");
        let call_two_args = create_test_relation(8, "foo");
        // Spans point at each call expression so argument counting works.
        let mut rel_one = call_one_arg;
        rel_one.span.start_byte = 8;
        rel_one.span.end_byte = 15;
        let mut rel_two = call_two_args;
        rel_two.span.start_byte = 23;
        rel_two.span.end_byte = 34;
        let relations = vec![rel_one, rel_two];

        let local_calls = resolver.resolve_with_source(&relations, &entities, Some(source));

        assert_eq!(local_calls.len(), 2);
        // foo(1) resolves to the single-parameter overload...
        let one = local_calls
            .iter()
            .find(|c| c.span.start_byte == 8)
            .expect("one-arg call resolved");
        assert_eq!(one.callee, EntityId(0));
        // ...and foo(2, 3) to the two-parameter overload.
        let two = local_calls
            .iter()
            .find(|c| c.span.start_byte == 23)
            .expect("two-arg call resolved");
        assert_eq!(two.callee, EntityId(1));
    }

    /// Without source access the arity tier cannot run; resolution falls
    /// back to the span-nearest candidate.
    #[test]
    fn missing_source_falls_back_to_span_nearest() {
        let config = LocalCallResolverConfig {
            enable_signature_matching: true,
            ..Default::default()
        };
        let resolver = LocalCallResolver::with_config(config);

        let mut one_param = create_test_function_entity(0, "foo");
        one_param.parameters = vec![("a".to_string(), None)];
        one_param.span.start_byte = 30;
        let mut two_param = create_test_function_entity(1, "foo");
        two_param.parameters = vec![("a".to_string(), None), ("b".to_string(), None)];
        two_param.span.start_byte = 5;
        two_param.span.end_byte = 50;
        let entities = vec![one_param, two_param];

        // The relation's default span points at byte 0, so the closer
        // declaration is id 1.
        let relations = vec![create_test_relation(7, "foo")];
        let local_calls = resolver.resolve(&relations, &entities);

        assert_eq!(local_calls[0].callee, EntityId(1));
    }
}
