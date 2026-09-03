//! Relation post-processing filter
//!
//! Centralizes all filtering logic that was previously scattered across the
//! resolver. The design follows `docs/plan/symbol-resolution-deterministic.md`.

use cce_types::ParsedFile;
use cce_types::entity::EntityKind;
use cce_types::language::Language;
use regex::Regex;
use std::sync::OnceLock;

use crate::symbol_table::ProjectSymbolTable;

/// Configuration for relation filtering.
#[derive(Debug, Clone)]
pub struct RelationFilterConfig {
    /// Regex patterns for callee names to filter (e.g. `^debug$`, `^println$`).
    pub filter_patterns: Vec<Regex>,
    /// Macro names to filter (exact match).
    pub filter_macros: Vec<String>,
    /// Package names to filter (exact match).
    pub filter_packages: Vec<String>,
}

impl Default for RelationFilterConfig {
    fn default() -> Self {
        Self {
            filter_patterns: vec![
                Regex::new(r"^debug$").unwrap(),
                Regex::new(r"^println$").unwrap(),
                Regex::new(r"^dbg$").unwrap(),
                Regex::new(r"^log::").unwrap(),
                Regex::new(r"^tracing::").unwrap(),
            ],
            filter_macros: vec![
                "debug".to_string(),
                "println".to_string(),
                "dbg".to_string(),
                "info".to_string(),
                "warn".to_string(),
                "error".to_string(),
            ],
            filter_packages: Vec::new(),
        }
    }
}

/// Post-processor that decides whether a resolved or unresolved relation
/// should be dropped.
#[derive(Debug, Clone, Default)]
pub struct RelationPostProcessor {
    config: RelationFilterConfig,
}

impl RelationPostProcessor {
    /// Create with custom config.
    pub fn new(config: RelationFilterConfig) -> Self {
        Self { config }
    }

    /// Whether the given callee name matches a filtered pattern.
    pub fn matches_filter_pattern(&self, callee_name: &str) -> bool {
        for pat in &self.config.filter_patterns {
            if pat.is_match(callee_name) {
                return true;
            }
        }
        false
    }

    /// Whether the given macro name is filtered.
    pub fn is_filtered_macro(&self, macro_name: &str) -> bool {
        self.config.filter_macros.contains(&macro_name.to_string())
    }

    /// Rust-specific `clone`/`clone_from` heuristic moved from `resolver.rs`.
    ///
    /// Returns `true` when the relation should be filtered (i.e. the
    /// `clone` call is a spurious fallback on a generic variable and no
    /// concrete member exists in the type index).
    #[allow(clippy::too_many_arguments)]
    pub fn should_filter_rust_clone(
        &self,
        dst_name: &str,
        language: Language,
        resolved: bool,
        receiver_type: Option<&str>,
        is_variable_receiver: bool,
        has_member: bool,
        receiver_raw: &str,
    ) -> bool {
        if language != Language::Rust {
            return false;
        }
        if !resolved {
            return false;
        }
        let method = dst_name.rsplit(['.', ':']).next().unwrap_or(dst_name);
        if !matches!(method, "clone" | "clone_from") {
            return false;
        }
        if receiver_raw.is_empty()
            || receiver_raw
                .chars()
                .next()
                .is_none_or(|c| !c.is_ascii_lowercase())
            || matches!(receiver_raw, "self" | "Self")
        {
            return false;
        }
        let generic_candidate = if let Some(recv_ty) = receiver_type {
            let stripped = recv_ty
                .split('<')
                .next()
                .unwrap_or(recv_ty)
                .trim()
                .trim_matches(|c| c == '&' || c == '*' || c == ' ')
                .trim();
            stripped.len() == 1
                && stripped
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_uppercase())
                || ["T", "E", "F", "K", "V", "U", "R"].contains(&stripped)
                || stripped == "unknown"
        } else {
            is_variable_receiver
        };
        if generic_candidate && !has_member {
            return true;
        }
        false
    }

    /// Generic post-filter for unresolved call relations that look like
    /// standard debug/log macros. Currently checks `matches_filter_pattern`
    /// and macro list.
    pub fn should_filter_unresolved_call(&self, callee_name: &str, _language: Language) -> bool {
        if self.matches_filter_pattern(callee_name) {
            return true;
        }
        if self.is_filtered_macro(callee_name) {
            return true;
        }
        false
    }

    /// Centralized auto-filter for Rust `clone`/`clone_from` on generic receivers.
    ///
    /// Computes `receiver_raw`, `is_variable_receiver` and `has_member`
    /// deterministically inside the post-processor so callers do not scatter
    /// string heuristics. Returns `true` when the relation should be dropped.
    pub fn should_filter_rust_clone_auto(
        &self,
        dst_name: &str,
        language: Language,
        parsed: &ParsedFile,
        symbol_table: &ProjectSymbolTable,
        receiver_type: Option<&str>,
        resolved: bool,
    ) -> bool {
        if language != Language::Rust || !resolved {
            return false;
        }
        let method = dst_name.rsplit(['.', ':']).next().unwrap_or(dst_name);
        if !matches!(method, "clone" | "clone_from") {
            return false;
        }
        // Deterministic receiver extraction via helper
        let receiver_raw = Self::extract_receiver_raw_simple(dst_name);
        if receiver_raw.is_empty()
            || receiver_raw
                .chars()
                .next()
                .is_none_or(|c| !c.is_ascii_lowercase())
            || matches!(receiver_raw, "self" | "Self")
        {
            return false;
        }
        let is_variable_receiver = parsed
            .entities
            .iter()
            .any(|e| e.name == receiver_raw && e.kind == EntityKind::Variable);
        let has_member = if let Some(recv_ty) = receiver_type {
            let stripped = recv_ty
                .split('<')
                .next()
                .unwrap_or(recv_ty)
                .trim()
                .trim_matches(|c| c == '&' || c == '*' || c == ' ')
                .trim();
            let global = symbol_table.global_type_index();
            global
                .all_types()
                .iter()
                .any(|t| t.key.qualified == stripped && t.members.contains_key(method))
        } else {
            false
        };
        self.should_filter_rust_clone(
            dst_name,
            language,
            true,
            receiver_type,
            is_variable_receiver,
            has_member,
            receiver_raw,
        )
    }

    /// Simple deterministic receiver extraction without allocation.
    fn extract_receiver_raw_simple(dst_name: &str) -> &str {
        if let Some(pos) = dst_name.rfind("::") {
            let prefix = &dst_name[..pos];
            return prefix.rsplit(['.', ':']).next().unwrap_or(prefix).trim();
        }
        if let Some(pos) = dst_name.rfind('.') {
            let prefix = &dst_name[..pos];
            // Handle `:` inside prefix (e.g. `a::b.c`)
            return prefix.rsplit(['.', ':']).next().unwrap_or(prefix).trim();
        }
        if let Some(pos) = dst_name.rfind(':') {
            let prefix = dst_name[..pos].trim_end_matches(':');
            return prefix.rsplit(['.', ':']).next().unwrap_or(prefix).trim();
        }
        ""
    }

    /// Decide whether `name`'s last-segment fallback should be blocked for
    /// Rust generic receivers. Centralizes the heuristic previously scattered
    /// in `name_candidates.rs::should_block_last_segment_fallback`.
    pub fn should_block_last_segment_fallback(
        &self,
        name: &str,
        parsed: &ParsedFile,
        symbol_table: &ProjectSymbolTable,
        is_stdlib: bool,
    ) -> bool {
        if is_stdlib {
            return false;
        }
        if parsed.language != Language::Rust {
            return false;
        }
        if !name.contains('.') && !name.contains(':') {
            return false;
        }
        let last = match Self::last_segment_for_name(name, is_stdlib) {
            Some(l) => l,
            None => return false,
        };
        if !parsed.local_symbols.contains_key(last) {
            return false;
        }
        let receiver = Self::extract_receiver_raw_simple(name);
        if receiver.is_empty() {
            return false;
        }
        if receiver
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase())
        {
            return false;
        }
        if receiver == "self" || receiver == "Self" {
            return true;
        }
        if receiver
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase())
        {
            if let Some(type_ctx) = symbol_table.get_type_inference_context(&parsed.path) {
                if let Some(binding) = type_ctx.get_variable_type(receiver) {
                    let ty = binding.type_name.trim();
                    if ty.len() == 1 && ty.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                        return true;
                    }
                    if ["T", "E", "F", "K", "V", "U", "R", "A", "B", "C"].contains(&ty) {
                        return true;
                    }
                    if ty == "unknown" && matches!(last, "clone" | "clone_from") {
                        return true;
                    }
                    if let Some(shape) = &binding.shape {
                        let shape_str = shape.to_type_string();
                        if shape_str.len() == 1
                            && shape_str
                                .chars()
                                .next()
                                .is_some_and(|c| c.is_ascii_uppercase())
                        {
                            return true;
                        }
                    }
                    if ty.contains("T: Clone") || ty.contains("T : Clone") {
                        return true;
                    }
                } else if matches!(last, "clone" | "clone_from") {
                    return true;
                }
            } else if matches!(last, "clone" | "clone_from") {
                return true;
            }
            return true;
        }
        false
    }

    fn last_segment_for_name(name: &str, is_stdlib: bool) -> Option<&str> {
        if is_stdlib {
            return None;
        }
        let last = name.rsplit(['.', ':']).next().unwrap_or(name);
        (last != name).then_some(last)
    }

    /// Unified filter for any relation (resolved or unresolved).
    ///
    /// Returns `true` when the relation should be dropped. For resolved
    /// relations this checks the Rust `clone` heuristic; for unresolved call
    /// relations it checks debug/log macro patterns. `is_stdlib` gates the
    /// debug filter so stdlib `println` is counted via `stdlib_filtered`
    /// instead.
    pub fn should_filter_relation(
        &self,
        dst_name: &str,
        language: Language,
        is_external: bool,
        is_stdlib: bool,
        relation_is_call: bool,
    ) -> bool {
        if is_external
            && !is_stdlib
            && relation_is_call
            && self.should_filter_unresolved_call(dst_name, language)
        {
            return true;
        }
        false
    }
}

static GLOBAL_PROCESSOR: OnceLock<RelationPostProcessor> = OnceLock::new();

/// Global default post-processor (used when no custom config is supplied).
pub fn global_post_processor() -> &'static RelationPostProcessor {
    GLOBAL_PROCESSOR.get_or_init(RelationPostProcessor::default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::language::Language;

    #[test]
    fn default_config_filters_debug() {
        let p = RelationPostProcessor::default();
        assert!(p.matches_filter_pattern("debug"));
        assert!(p.matches_filter_pattern("println"));
        assert!(p.matches_filter_pattern("log::info"));
        assert!(!p.matches_filter_pattern("my_debug"));
    }

    #[test]
    fn rust_clone_filter_generic_without_member() {
        let p = RelationPostProcessor::default();
        assert!(p.should_filter_rust_clone(
            "x.clone",
            Language::Rust,
            true,
            Some("T"),
            false,
            false,
            "x"
        ));
        assert!(!p.should_filter_rust_clone(
            "x.clone",
            Language::Rust,
            true,
            Some("MyStruct"),
            false,
            true,
            "x"
        ));
        assert!(!p.should_filter_rust_clone(
            "x.clone",
            Language::Rust,
            false,
            Some("T"),
            false,
            false,
            "x"
        ));
    }

    #[test]
    fn unresolved_debug_filtered() {
        let p = RelationPostProcessor::default();
        assert!(p.should_filter_unresolved_call("println", Language::Rust));
        assert!(!p.should_filter_unresolved_call("my_func", Language::Rust));
    }

    #[test]
    fn unified_filter_relation_external_debug() {
        let p = RelationPostProcessor::default();
        // external, non-stdlib, call → should filter debug
        assert!(p.should_filter_relation("println", Language::Rust, true, false, true));
        assert!(p.should_filter_relation("debug", Language::Rust, true, false, true));
        // stdlib gated: stdlib println should not be filtered as debug
        assert!(!p.should_filter_relation("println", Language::Rust, true, true, true));
        // resolved (is_external=false) should not filter
        assert!(!p.should_filter_relation("println", Language::Rust, false, false, true));
        // non-call relation should not filter
        assert!(!p.should_filter_relation("println", Language::Rust, true, false, false));
    }

    #[test]
    fn rust_clone_auto_generic_filtered() {
        use crate::index::builder::SymbolTableBuilder;
        use cce_types::language::Language;
        use cce_types::{Entity, EntityId, EntityKind, Span};
        use std::collections::HashMap;
        use std::path::PathBuf;

        let mut parsed = cce_types::ParsedFile::new(Language::Rust, "test.rs".to_string(), "");
        let var = Entity {
            id: EntityId(1),
            kind: EntityKind::Variable,
            name: "x".to_string(),
            signature: "".to_string(),
            parameters: vec![],
            return_type: None,
            span: Span::default(),
            depth: 0,
            parent: None,
            children: vec![],
            doc_comment: None,
            modifiers: vec![],
            attributes: HashMap::new(),
            metadata: HashMap::new(),
            is_stdlib: false,
            subtype: None,
            stdlib_category: None,
        };
        parsed.add_entity(var);
        let files = [&parsed];
        let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);
        let p = RelationPostProcessor::default();
        // generic T, no member → should filter
        assert!(p.should_filter_rust_clone_auto(
            "x.clone",
            Language::Rust,
            &parsed,
            &symbols,
            Some("T"),
            true
        ));
        // concrete type with no member still filters because has_member false, but generic check fails?
        // MyStruct is not generic, so not filtered
        assert!(!p.should_filter_rust_clone_auto(
            "x.clone",
            Language::Rust,
            &parsed,
            &symbols,
            Some("MyStruct"),
            true
        ));
        // non-Rust should not filter
        assert!(!p.should_filter_rust_clone_auto(
            "x.clone",
            Language::Python,
            &parsed,
            &symbols,
            Some("T"),
            true
        ));
        // self should not filter
        assert!(!p.should_filter_rust_clone_auto(
            "self.clone",
            Language::Rust,
            &parsed,
            &symbols,
            Some("T"),
            true
        ));
    }

    #[test]
    fn block_last_segment_fallback_generic() {
        use crate::index::builder::SymbolTableBuilder;
        use cce_types::language::Language;
        use cce_types::{Entity, EntityId, EntityKind, Span};
        use std::collections::HashMap;
        use std::path::PathBuf;

        let mut parsed = cce_types::ParsedFile::new(Language::Rust, "test.rs".to_string(), "");
        // Add local symbol entry for `clone` to trigger fallback check
        let mut local = Entity {
            id: EntityId(2),
            kind: EntityKind::Function,
            name: "clone".to_string(),
            signature: "".to_string(),
            parameters: vec![],
            return_type: None,
            span: Span::default(),
            depth: 0,
            parent: None,
            children: vec![],
            doc_comment: None,
            modifiers: vec![],
            attributes: HashMap::new(),
            metadata: HashMap::new(),
            is_stdlib: false,
            subtype: None,
            stdlib_category: None,
        };
        local.modifiers.push("pub".to_string());
        parsed.add_entity(local);
        // Manually insert local_symbols entry for `clone`
        parsed
            .local_symbols
            .insert("clone".to_string(), vec![EntityId(2)]);

        let files = [&parsed];
        let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);
        let p = RelationPostProcessor::default();
        // `x.clone` where x is lowercase variable and local symbol `clone` exists → should block
        assert!(p.should_block_last_segment_fallback("x.clone", &parsed, &symbols, false));
        // stdlib should not block
        assert!(!p.should_block_last_segment_fallback("x.clone", &parsed, &symbols, true));
        // non-Rust should not block
        let mut py_parsed = cce_types::ParsedFile::new(Language::Python, "test.py".to_string(), "");
        py_parsed
            .local_symbols
            .insert("clone".to_string(), vec![EntityId(2)]);
        assert!(!p.should_block_last_segment_fallback("x.clone", &py_parsed, &symbols, false));
    }
}
