//! Shared entity-name classifiers and variable metadata keys.
//!
//! Extraction (`deduplicate_contained_entities`) and NL conversion
//! (`variable_carries_retrieval_value`) both need to answer the same two
//! questions about a variable entity: does it carry type provenance, and
//! does its name denote a module export path. The answers live here so the
//! two stages cannot drift apart.

/// Metadata keys recording a variable's type provenance.
///
/// Any of these (annotation, constructor call, literal, call target,
/// destructuring source, or legacy inference keys) marks a variable as
/// worth preserving for inference and retrieval; bare locals without them
/// remain eligible for contained-entity removal.
pub const VARIABLE_TYPE_METADATA_KEYS: &[&str] = &[
    "type_annotation",
    "constructor_type",
    "literal_type",
    "call_target",
    "explicit_type",
    "var_type",
    "inferred_type",
    "source_type",
];

/// Whether an assignment-target name denotes a module export path.
///
/// Matches the CommonJS surface precisely: `exports`, `exports.*`, and
/// anything routed through `module.exports` / `module.*`. Deliberately
/// narrower than substring matching so locals that merely mention the
/// words (e.g. `exported_count`, `my_module_var`) are not misclassified
/// as exports.
pub fn is_export_assignment_name(name: &str) -> bool {
    let name = name.trim().to_lowercase();
    name == "exports"
        || name.starts_with("exports.")
        || name.contains("module.exports")
        || name == "module"
        || name.starts_with("module.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_assignment_names() {
        assert!(is_export_assignment_name("exports"));
        assert!(is_export_assignment_name("exports.etag"));
        assert!(is_export_assignment_name("module.exports"));
        assert!(is_export_assignment_name("module.exports.foo"));
        assert!(is_export_assignment_name("module"));
    }

    #[test]
    fn test_non_export_names_rejected() {
        assert!(!is_export_assignment_name("exported_count"));
        assert!(!is_export_assignment_name("my_module_var"));
        assert!(!is_export_assignment_name("res.send"));
        assert!(!is_export_assignment_name("app"));
    }
}
