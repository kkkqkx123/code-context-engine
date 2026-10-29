//! Host-side resolution of the `LangHeuristics` capability.
//!
//! Consults plugins that implement the three language heuristics
//! (stdlib classification, test-file detection, entity-kind mapping) in
//! priority order; the first non-`None` result wins. Every method returns
//! `None` when no plugin provides an answer, so the built-in logic is
//! unchanged in the absence of heuristics plugins.

use cce_plugin::{CodePlugin, PluginCapability, PluginRegistry};
use cce_types::EntityKind;
use cce_types::StdlibCategory;
use std::sync::Arc;

/// Parse one plugin-returned stdlib category name.
///
/// Accepts the serialized variant name and, as a convenience, lowercase
/// display labels. Returns `None` for unknown names so callers decline
/// instead of guessing.
pub fn parse_stdlib_category(category: &str) -> Option<StdlibCategory> {
    if let Ok(cat) = serde_json::from_str::<StdlibCategory>(&json_quoted(category)) {
        return Some(cat);
    }
    let lower = category.to_lowercase();
    match lower.as_str() {
        "collections" | "collection" | "data_structure" => Some(StdlibCategory::Collection),
        "io" | "i/o" => Some(StdlibCategory::Io),
        "concurrency" | "threading" => Some(StdlibCategory::Concurrency),
        "utilities" | "utility" => Some(StdlibCategory::Utility),
        "strings" | "string" => Some(StdlibCategory::String),
        "numerics" | "numeric" => Some(StdlibCategory::Numeric),
        "errors" | "error" => Some(StdlibCategory::Error),
        "macros" | "macro" => Some(StdlibCategory::Macro),
        "traits" | "trait" => Some(StdlibCategory::Trait),
        _ => None,
    }
}

/// Classify `module_path` as a standard-library item.
///
/// Returns the category decided by the first plugin that answers, or `None`
/// when every plugin declined or the answer is not a known category.
pub fn classify_stdlib(
    registry: &PluginRegistry,
    module_path: &str,
    file_path: Option<&str>,
    language: Option<&str>,
) -> Option<StdlibCategory> {
    for plugin in heuristics_plugins(registry, file_path, language) {
        if !plugin.supports_stdlib_heuristic() {
            continue;
        }
        match plugin.classify_stdlib(module_path) {
            Ok(Some(category)) => {
                if let Some(cat) = parse_stdlib_category(&category) {
                    return Some(cat);
                }
                continue;
            }
            Ok(None) => continue,
            Err(err) => {
                tracing::warn!(
                    plugin = %plugin.metadata().id,
                    method = "classify_stdlib",
                    error = %err,
                    "LangHeuristics plugin call failed; falling back to built-in logic"
                );
                continue;
            }
        }
    }
    None
}

/// Classify a call site as a standard-library call.
///
/// Call-level companion to [`classify_stdlib`]: the first plugin whose
/// `classify_stdlib_call` answers wins. `relation_type` carries the stable
/// relation name when the caller has one. Callers consult built-in tables
/// first and reach here only for custom languages the built-ins miss, so
/// a decline keeps the conservative answer.
pub fn classify_stdlib_call(
    registry: &PluginRegistry,
    call_name: &str,
    relation_type: Option<&str>,
    file_path: Option<&str>,
    language: Option<&str>,
) -> Option<StdlibCategory> {
    for plugin in heuristics_plugins(registry, file_path, language) {
        if !plugin.supports_stdlib_call_heuristic() {
            continue;
        }
        match plugin.classify_stdlib_call(call_name, relation_type) {
            Ok(Some(category)) => {
                if let Some(cat) = parse_stdlib_category(&category) {
                    return Some(cat);
                }
                continue;
            }
            Ok(None) => continue,
            Err(err) => {
                tracing::warn!(
                    plugin = %plugin.metadata().id,
                    method = "classify_stdlib_call",
                    error = %err,
                    "LangHeuristics plugin call failed; falling back to built-in logic"
                );
                continue;
            }
        }
    }
    None
}

/// Collect plugin-contributed bare boilerplate method names.
///
/// The first non-empty answer wins and merges nothing: each language
/// owns one table, mirroring the built-in bare tables. An empty answer
/// (or no answering plugin) yields an empty list, keeping pruning
/// conservative for languages without a table.
pub fn plugin_bare_methods(
    registry: &PluginRegistry,
    file_path: Option<&str>,
    language: Option<&str>,
) -> Vec<String> {
    let language = language.unwrap_or_default();
    for plugin in heuristics_plugins(registry, file_path, Some(language)) {
        if !plugin.supports_bare_methods_heuristic() {
            continue;
        }
        match plugin.bare_boilerplate_methods(language) {
            Ok(Some(methods)) if !methods.is_empty() => return methods,
            Ok(_) => continue,
            Err(err) => {
                tracing::warn!(
                    plugin = %plugin.metadata().id,
                    method = "bare_boilerplate_methods",
                    error = %err,
                    "LangHeuristics plugin call failed; falling back to built-in logic"
                );
                continue;
            }
        }
    }
    Vec::new()
}

/// Decide whether `file_path`/`content` is a test file.
///
/// Returns the first plugin decision, or `None` to defer to the built-in
/// path/AST rules.
pub fn is_test_file(
    registry: &PluginRegistry,
    file_path: &str,
    content: &str,
    language: Option<&str>,
) -> Option<bool> {
    for plugin in heuristics_plugins(registry, Some(file_path), language) {
        if !plugin.supports_test_file_heuristic() {
            continue;
        }
        match plugin.is_test_file(file_path, content) {
            Ok(Some(decision)) => return Some(decision),
            Ok(None) => continue,
            Err(err) => {
                tracing::warn!(
                    plugin = %plugin.metadata().id,
                    method = "is_test_file",
                    error = %err,
                    "LangHeuristics plugin call failed; falling back to built-in logic"
                );
                continue;
            }
        }
    }
    None
}

/// Map a tree-sitter query capture name to an entity kind.
///
/// Returns the first plugin mapping that parses as an [`EntityKind`], or
/// `None` to defer to the built-in capture→kind mapping.
pub fn entity_kind(
    registry: &PluginRegistry,
    capture_name: &str,
    file_path: Option<&str>,
    language: Option<&str>,
) -> Option<EntityKind> {
    for plugin in heuristics_plugins(registry, file_path, language) {
        if !plugin.supports_entity_kind_heuristic() {
            continue;
        }
        match plugin.entity_kind(capture_name) {
            Ok(Some(kind)) => {
                if let Ok(k) = serde_json::from_str::<EntityKind>(&json_quoted(&kind)) {
                    return Some(k);
                }
                tracing::warn!(
                    plugin = %plugin.metadata().id,
                    method = "entity_kind",
                    kind = %kind,
                    capture = %capture_name,
                    "LangHeuristics plugin returned an unknown entity kind; ignoring it"
                );
                continue;
            }
            Ok(None) => continue,
            Err(err) => {
                tracing::warn!(
                    plugin = %plugin.metadata().id,
                    method = "entity_kind",
                    error = %err,
                    "LangHeuristics plugin call failed; falling back to built-in logic"
                );
                continue;
            }
        }
    }
    None
}

/// Heuristics plugins in priority order.
fn heuristics_plugins<'a>(
    registry: &'a PluginRegistry,
    file_path: Option<&str>,
    language: Option<&str>,
) -> Vec<&'a Arc<dyn CodePlugin>> {
    registry.get_plugins(PluginCapability::LangHeuristics, file_path, language)
}

/// Wrap a plain enum-variant name (e.g. `"function"`) as a JSON string so
/// serde can deserialize it into the corresponding enum variant.
fn json_quoted(name: &str) -> String {
    format!("\"{name}\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_plugin::{PluginError, PluginMetadata};
    use std::collections::HashMap;
    use std::sync::Arc;

    /// Fake call-level heuristics plugin for resolver tests.
    struct FakeCallPlugin {
        meta: PluginMetadata,
        call_answer: Option<Result<Option<String>, String>>,
        bare_answer: Option<Result<Option<Vec<String>>, String>>,
    }

    impl FakeCallPlugin {
        fn new(
            id: &str,
            call_answer: Option<Result<Option<String>, String>>,
            bare_answer: Option<Result<Option<Vec<String>>, String>>,
        ) -> Self {
            Self {
                meta: PluginMetadata {
                    id: id.to_string(),
                    name: id.to_string(),
                    version: "0.1.0".to_string(),
                    priority: 0,
                    capability_priorities: HashMap::new(),
                    capability_timeouts: HashMap::new(),
                    description: None,
                    capabilities: Vec::new(),
                },
                call_answer,
                bare_answer,
            }
        }
    }

    impl CodePlugin for FakeCallPlugin {
        fn metadata(&self) -> &PluginMetadata {
            &self.meta
        }
        fn supports_stdlib_call_heuristic(&self) -> bool {
            self.call_answer.is_some()
        }
        fn classify_stdlib_call(
            &self,
            _call_name: &str,
            _relation_type: Option<&str>,
        ) -> Result<Option<String>, PluginError> {
            match self.call_answer.clone().expect("call hook probed") {
                Ok(answer) => Ok(answer),
                Err(_) => Err(PluginError::ScriptError("boom".to_string())),
            }
        }
        fn supports_bare_methods_heuristic(&self) -> bool {
            self.bare_answer.is_some()
        }
        fn bare_boilerplate_methods(
            &self,
            _language: &str,
        ) -> Result<Option<Vec<String>>, PluginError> {
            match self.bare_answer.clone().expect("bare hook probed") {
                Ok(answer) => Ok(answer),
                Err(_) => Err(PluginError::ScriptError("boom".to_string())),
            }
        }
    }

    fn registry_with(plugin: FakeCallPlugin) -> PluginRegistry {
        let mut registry = PluginRegistry::new();
        registry.register(Arc::new(plugin));
        registry
    }

    #[test]
    fn test_classify_stdlib_call_first_answer_wins() {
        let mut registry = PluginRegistry::new();
        registry.register(Arc::new(FakeCallPlugin::new(
            "first",
            Some(Ok(Some("Collection".to_string()))),
            None,
        )));
        registry.register(Arc::new(FakeCallPlugin::new(
            "second",
            Some(Ok(Some("Io".to_string()))),
            None,
        )));
        assert_eq!(
            classify_stdlib_call(
                &registry,
                "put",
                Some("call.method"),
                None,
                Some("fakelang")
            ),
            Some(StdlibCategory::Collection)
        );
    }

    #[test]
    fn test_classify_stdlib_call_decline_and_unknown() {
        let registry = registry_with(FakeCallPlugin::new("decliner", Some(Ok(None)), None));
        assert_eq!(
            classify_stdlib_call(&registry, "put", None, None, Some("fakelang")),
            None
        );
        let registry = registry_with(FakeCallPlugin::new(
            "unknown",
            Some(Ok(Some("bogus".to_string()))),
            None,
        ));
        assert_eq!(
            classify_stdlib_call(&registry, "put", None, None, Some("fakelang")),
            None,
            "unknown categories must decline, not guess"
        );
    }

    #[test]
    fn test_classify_stdlib_call_error_falls_back() {
        let mut registry = PluginRegistry::new();
        registry.register(Arc::new(FakeCallPlugin::new(
            "broken",
            Some(Err(String::new())),
            None,
        )));
        registry.register(Arc::new(FakeCallPlugin::new(
            "fallback",
            Some(Ok(Some("Io".to_string()))),
            None,
        )));
        assert_eq!(
            classify_stdlib_call(&registry, "put", None, None, Some("fakelang")),
            Some(StdlibCategory::Io)
        );
    }

    #[test]
    fn test_plugin_bare_methods_first_non_empty_wins() {
        let mut registry = PluginRegistry::new();
        registry.register(Arc::new(FakeCallPlugin::new(
            "empty",
            None,
            Some(Ok(Some(Vec::new()))),
        )));
        registry.register(Arc::new(FakeCallPlugin::new(
            "full",
            None,
            Some(Ok(Some(vec!["emit".to_string()]))),
        )));
        assert_eq!(
            plugin_bare_methods(&registry, None, Some("fakelang")),
            vec!["emit".to_string()]
        );
        let empty = PluginRegistry::new();
        assert!(
            plugin_bare_methods(&empty, None, Some("fakelang")).is_empty(),
            "no plugin answer keeps pruning conservative"
        );
    }
}
