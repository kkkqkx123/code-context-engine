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

/// Classify `module_path` as a standard-library item.
///
/// Returns the category decided by the first plugin that answers, or `None`
/// when every plugin declined or the answer is not a known category.
pub fn classify_stdlib(registry: &PluginRegistry, module_path: &str) -> Option<StdlibCategory> {
    for plugin in heuristics_plugins(registry) {
        if !plugin.supports_stdlib_heuristic() {
            continue;
        }
        match plugin.classify_stdlib(module_path) {
            Ok(Some(category)) => {
                if let Ok(cat) = serde_json::from_str::<StdlibCategory>(&json_quoted(&category)) {
                    return Some(cat);
                }
                // Accept lowercase display labels as a convenience.
                let lower = category.to_lowercase();
                let cat = match lower.as_str() {
                    "collections" | "collection" | "data_structure" => StdlibCategory::Collection,
                    "io" | "i/o" => StdlibCategory::Io,
                    "concurrency" | "threading" => StdlibCategory::Concurrency,
                    "utilities" | "utility" => StdlibCategory::Utility,
                    "strings" | "string" => StdlibCategory::String,
                    "numerics" | "numeric" => StdlibCategory::Numeric,
                    "errors" | "error" => StdlibCategory::Error,
                    "macros" | "macro" => StdlibCategory::Macro,
                    "traits" | "trait" => StdlibCategory::Trait,
                    _ => continue,
                };
                return Some(cat);
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

/// Decide whether `file_path`/`content` is a test file.
///
/// Returns the first plugin decision, or `None` to defer to the built-in
/// path/AST rules.
pub fn is_test_file(registry: &PluginRegistry, file_path: &str, content: &str) -> Option<bool> {
    for plugin in heuristics_plugins(registry) {
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
pub fn entity_kind(registry: &PluginRegistry, capture_name: &str) -> Option<EntityKind> {
    for plugin in heuristics_plugins(registry) {
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
fn heuristics_plugins(registry: &PluginRegistry) -> Vec<&Arc<dyn CodePlugin>> {
    registry.get_plugins(PluginCapability::LangHeuristics, None, None)
}

/// Wrap a plain enum-variant name (e.g. `"function"`) as a JSON string so
/// serde can deserialize it into the corresponding enum variant.
fn json_quoted(name: &str) -> String {
    format!("\"{name}\"")
}
