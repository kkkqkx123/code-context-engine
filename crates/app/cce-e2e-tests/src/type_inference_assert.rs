//! Deterministic type-inference assertions for E2E tests.
//!
//! The per-file [`TypeInferenceEngine`](cce_codegraph::type_inference::TypeInferenceEngine)
//! output is converted into a sorted, `EntityId`-free snapshot
//! ([`CanonicalTypeBinding`]) so tests can assert on inferred types without
//! depending on process-local ID assignment or scope insertion order.
//!
//! Human-readable visualization reuses
//! [`render_type_inference`](crate::structured_output::render_type_inference),
//! which runs the same engine and renders the
//! `Variables / Function Returns / Control-Flow Narrowing / Type Shapes`
//! markdown tables. Use the `export_type_inference` example to regenerate the
//! `outputs/scenarios/<lang>/structured/<case>/` reports and eyeball them;
//! use the helpers in this module for machine-checked assertions.

use std::collections::BTreeMap;

use cce_codegraph::type_inference::CrossFilePropagator;
use cce_codegraph::type_inference::TypeInferenceEngine;
use cce_codegraph::type_inference::propagate_variable_types;
use cce_codegraph::type_inference::traits::InferenceContext;
use cce_codegraph::type_inference::types::ScopedTypeContext;
use cce_codegraph::type_inference::types::origin_priority;
use cce_types::ParsedFile;

/// Binding kind inside a canonical snapshot.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum TypeBindingKind {
    Variable,
    Narrowed,
    Return,
}

impl std::fmt::Display for TypeBindingKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeBindingKind::Variable => write!(f, "variable"),
            TypeBindingKind::Narrowed => write!(f, "narrowed"),
            TypeBindingKind::Return => write!(f, "return"),
        }
    }
}

/// A single deterministic type binding entry.
///
/// `name` is the variable name for variable/narrowed bindings and the
/// function name for return bindings.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct CanonicalTypeBinding {
    pub file: String,
    pub name: String,
    pub kind: TypeBindingKind,
    pub inferred_type: String,
    pub origin: String,
    pub shape: String,
}

/// Run project-wide type inference for the given parsed files.
///
/// Phase 1 runs per-file single-pass + two-pass inference and merges them.
/// Phase 2 feeds every file context into a shared propagator and propagates
/// cross-file call-target return types (`x = f()` with `f` in a sibling
/// file) back into variable bindings, mirroring the production
/// `SymbolTableBuilder` cross-file path.
///
/// Returns one final context per file, aligned with `files` order.
/// Shared with [`render_type_inference`](crate::structured_output::render_type_inference)
/// so snapshot assertions and visualized markdown never diverge.
pub fn infer_project_contexts(files: &[ParsedFile]) -> Vec<ScopedTypeContext> {
    // Build a per-file member index so discriminated-union narrowing sees
    // the same field information as the production `SymbolTableBuilder`
    // path, which passes its module type index via `InferenceContext`.
    // Without this the export used `InferenceContext::default()` (no
    // index) and every field-discriminated narrowing rendered empty.
    let indexes: Vec<cce_codegraph::symbol_table::TypeMemberIndex> = files
        .iter()
        .map(|file| {
            let mut index = cce_codegraph::symbol_table::TypeMemberIndex::new();
            cce_codegraph::policy::type_member::build_type_index_for_file(
                &file.entities,
                "",
                &file.path,
                "",
                file.language,
                &mut index,
            );
            index
        })
        .collect();
    let mut merged_by_file = Vec::with_capacity(files.len());
    for (file, index) in files.iter().zip(indexes.iter()) {
        let inference_ctx = InferenceContext::new().with_type_index(index);
        let ctx = TypeInferenceEngine::infer_types(file, &inference_ctx);
        let ctx_two = TypeInferenceEngine::infer_types_two_pass(file, &inference_ctx);
        let mut merged = ctx.clone();
        merged.merge_from(&ctx_two);
        merged_by_file.push(merged);
    }

    let propagator = CrossFilePropagator::new();
    let contexts: dashmap::DashMap<String, ScopedTypeContext> = dashmap::DashMap::new();
    for (file, merged) in files.iter().zip(merged_by_file.iter()) {
        propagator.insert_file(&file.path, merged, &file.entities);
        contexts.insert(
            cce_types::normalize_project_path(&file.path),
            merged.clone(),
        );
    }
    {
        let file_refs: Vec<&ParsedFile> = files.iter().collect();
        propagate_variable_types(&file_refs, &propagator, &contexts);
    }

    files
        .iter()
        .zip(merged_by_file.iter())
        .map(|(file, merged)| {
            contexts
                .get(&cce_types::normalize_project_path(&file.path))
                .map(|r| r.value().clone())
                .unwrap_or_else(|| merged.clone())
        })
        .collect()
}

/// Collect a sorted snapshot of inferred types for the given parsed files.
///
/// Uses [`infer_project_contexts`], mirroring
/// [`render_type_inference`](crate::structured_output::render_type_inference)
/// so snapshot assertions and visualized markdown never diverge.
pub fn collect_type_bindings(files: &[ParsedFile]) -> Vec<CanonicalTypeBinding> {
    let mut out = Vec::new();
    for (file, merged) in files.iter().zip(infer_project_contexts(files).iter()) {
        let returns: BTreeMap<u64, String> = merged
            .return_types_iter()
            .map(|(eid, _)| {
                let func_name = crate::structured_output::types::resolve_inferred_return_name(
                    eid,
                    Some(file),
                    &[],
                    &BTreeMap::new(),
                );
                (eid.0, func_name)
            })
            .collect();
        let mut return_bindings: BTreeMap<u64, (String, String, String)> = BTreeMap::new();
        for (eid, binding) in merged.return_types_iter() {
            let shape = binding
                .shape
                .as_ref()
                .map(|s| s.to_type_string())
                .unwrap_or_default();
            let origin = binding.origin.map(|o| format!("{o:?}")).unwrap_or_default();
            return_bindings.insert(eid.0, (binding.type_name.clone(), origin, shape));
        }

        for frame in merged.frames_iter() {
            for (name, binding) in &frame.bindings {
                out.push(CanonicalTypeBinding {
                    file: file.path.clone(),
                    name: name.clone(),
                    kind: TypeBindingKind::Variable,
                    inferred_type: binding.type_name.clone(),
                    origin: binding.origin.map(|o| format!("{o:?}")).unwrap_or_default(),
                    shape: binding
                        .shape
                        .as_ref()
                        .map(|s| s.to_type_string())
                        .unwrap_or_default(),
                });
            }
            for (name, list) in &frame.narrowed {
                for binding in list {
                    out.push(CanonicalTypeBinding {
                        file: file.path.clone(),
                        name: name.clone(),
                        kind: TypeBindingKind::Narrowed,
                        inferred_type: binding.type_name.clone(),
                        origin: binding.origin.map(|o| format!("{o:?}")).unwrap_or_default(),
                        shape: binding
                            .shape
                            .as_ref()
                            .map(|s| s.to_type_string())
                            .unwrap_or_default(),
                    });
                }
            }
        }
        for (id, func_name) in &returns {
            if let Some((ty, origin, shape)) = return_bindings.get(id) {
                out.push(CanonicalTypeBinding {
                    file: file.path.clone(),
                    name: func_name.clone(),
                    kind: TypeBindingKind::Return,
                    inferred_type: ty.clone(),
                    origin: origin.clone(),
                    shape: shape.clone(),
                });
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Find bindings matching a name (substring) and kind.
pub fn find_bindings<'a>(
    bindings: &'a [CanonicalTypeBinding],
    name_substr: &str,
    kind: TypeBindingKind,
) -> Vec<&'a CanonicalTypeBinding> {
    bindings
        .iter()
        .filter(|b| b.kind == kind && b.name.contains(name_substr))
        .collect()
}

/// Assert that a variable has an inferred type containing `expected_substr`.
pub fn assert_variable_has_type(
    bindings: &[CanonicalTypeBinding],
    variable: &str,
    expected_substr: &str,
) {
    let hits = find_bindings(bindings, variable, TypeBindingKind::Variable);
    assert!(
        !hits.is_empty(),
        "Expected variable binding for '{variable}', got none. All bindings: {bindings:#?}"
    );
    assert!(
        hits.iter()
            .any(|b| b.inferred_type.contains(expected_substr)),
        "Variable '{variable}' should contain type '{expected_substr}', got: {:?}",
        hits.iter().map(|b| &b.inferred_type).collect::<Vec<_>>()
    );
}

/// Assert that a control-flow narrowed binding exists for a variable.
pub fn assert_narrowed_has_type(
    bindings: &[CanonicalTypeBinding],
    variable: &str,
    expected_substr: &str,
) {
    let hits = find_bindings(bindings, variable, TypeBindingKind::Narrowed);
    assert!(
        !hits.is_empty(),
        "Expected narrowed binding for '{variable}', got none. All bindings: {bindings:#?}"
    );
    assert!(
        hits.iter()
            .any(|b| b.inferred_type.contains(expected_substr)),
        "Narrowed '{variable}' should contain type '{expected_substr}', got: {:?}",
        hits.iter().map(|b| &b.inferred_type).collect::<Vec<_>>()
    );
}

/// Assert that a function has an inferred return type containing `expected_substr`.
pub fn assert_return_has_type(
    bindings: &[CanonicalTypeBinding],
    function: &str,
    expected_substr: &str,
) {
    let hits = find_bindings(bindings, function, TypeBindingKind::Return);
    assert!(
        !hits.is_empty(),
        "Expected return binding for '{function}', got none. All bindings: {bindings:#?}"
    );
    assert!(
        hits.iter()
            .any(|b| b.inferred_type.contains(expected_substr)),
        "Return of '{function}' should contain type '{expected_substr}', got: {:?}",
        hits.iter().map(|b| &b.inferred_type).collect::<Vec<_>>()
    );
}

/// Assert that no binding (of any kind) was produced for `name`.
///
/// Useful for documenting conservative no-guess behavior.
pub fn assert_no_binding(bindings: &[CanonicalTypeBinding], name: &str) {
    let hits: Vec<_> = bindings.iter().filter(|b| b.name == name).collect();
    assert!(
        hits.is_empty(),
        "Expected no binding for '{name}', got: {hits:#?}"
    );
}

/// Assert that `origin_priority` prefers `higher` over `lower`.
pub fn assert_origin_priority_higher(higher: &str, lower: &str) {
    let parse = |s: &str| match s {
        "TypeAnnotation" => {
            Some(cce_codegraph::type_inference::types::InferenceOrigin::TypeAnnotation)
        }
        "LiteralType" => Some(cce_codegraph::type_inference::types::InferenceOrigin::LiteralType),
        "ControlFlowNarrowing" => {
            Some(cce_codegraph::type_inference::types::InferenceOrigin::ControlFlowNarrowing)
        }
        "ConstructorCall" => {
            Some(cce_codegraph::type_inference::types::InferenceOrigin::ConstructorCall)
        }
        _ => None,
    };
    assert!(
        origin_priority(parse(higher)) > origin_priority(parse(lower)),
        "Expected origin '{higher}' to outrank '{lower}'"
    );
}

/// Assert that two snapshots are equivalent, printing a diff on failure.
#[macro_export]
macro_rules! assert_type_snapshot_eq {
    ($left:expr, $right:expr) => {{
        let left: &Vec<$crate::type_inference_assert::CanonicalTypeBinding> = &$left;
        let right: &Vec<$crate::type_inference_assert::CanonicalTypeBinding> = &$right;
        if left != right {
            let left_only: Vec<_> = left.iter().filter(|b| !right.contains(b)).collect();
            let right_only: Vec<_> = right.iter().filter(|b| !left.contains(b)).collect();
            panic!(
                "Type snapshot mismatch:\nleft-only:  {left_only:#?}\nright-only: {right_only:#?}"
            );
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(file: &str, name: &str, kind: TypeBindingKind, ty: &str) -> CanonicalTypeBinding {
        CanonicalTypeBinding {
            file: file.to_string(),
            name: name.to_string(),
            kind,
            inferred_type: ty.to_string(),
            origin: "TypeAnnotation".to_string(),
            shape: String::new(),
        }
    }

    #[test]
    fn test_assert_variable_has_type() {
        let snapshot = vec![binding("a.py", "count", TypeBindingKind::Variable, "int")];
        assert_variable_has_type(&snapshot, "count", "int");
    }

    #[test]
    fn test_snapshot_eq_macro() {
        let left = vec![binding("a.py", "x", TypeBindingKind::Variable, "str")];
        let right = left.clone();
        assert_type_snapshot_eq!(left, right);
    }

    #[test]
    fn test_origin_priority_helper() {
        assert_origin_priority_higher("TypeAnnotation", "LiteralType");
        assert_origin_priority_higher("ControlFlowNarrowing", "ConstructorCall");
    }

    /// Tuple unpacking resolves element types from the annotated
    /// parameter; exception bindings resolve to the caught type.
    #[test]
    fn test_destructuring_element_types() {
        use cce_parser::parser::ast_parser::AstParser;
        use cce_parser::parser::extractor::EntityExtractor;
        use cce_types::Language;

        let src = "def split_pair(pair: tuple[int, str]) -> str:\n    first, second = pair\n    return first\n\ntry:\n    pass\nexcept ValueError as e:\n    print(e)\n";
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let tree = ast_parser
            .parse_with_tree(src, &Language::Python)
            .expect("parse")
            .0;
        let entities = entity_extractor
            .extract(&tree, src, &Language::Python)
            .expect("extract");
        let mut file = ParsedFile::new(Language::Python, "m.py".to_string(), src.to_string());
        file.entities = entities;

        let bindings = collect_type_bindings(std::slice::from_ref(&file));
        assert_variable_has_type(&bindings, "first", "int");
        assert_variable_has_type(&bindings, "second", "str");
        assert_variable_has_type(&bindings, "e", "ValueError");
    }

    /// Call assignments resolve through sibling file return types
    /// via cross-file propagation.
    #[test]
    fn test_cross_file_return_propagation() {
        use cce_parser::parser::ast_parser::AstParser;
        use cce_parser::parser::extractor::EntityExtractor;
        use cce_types::Language;

        let models_src =
            "class User:\n    pass\n\n\ndef load_user(name: str) -> User:\n    return User()\n";
        let service_src = "from models import load_user\n\n\ndef main() -> None:\n    user = load_user(\"Alice\")\n";

        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let mut files = Vec::new();
        for (path, src) in [("models.py", models_src), ("service.py", service_src)] {
            let tree = ast_parser
                .parse_with_tree(src, &Language::Python)
                .expect("parse")
                .0;
            let entities = entity_extractor
                .extract(&tree, src, &Language::Python)
                .expect("extract");
            let mut file = ParsedFile::new(Language::Python, path.to_string(), src.to_string());
            file.entities = entities;
            files.push(file);
        }

        let bindings = collect_type_bindings(&files);
        assert_variable_has_type(&bindings, "user", "User");
        let hits = find_bindings(&bindings, "user", TypeBindingKind::Variable);
        assert!(
            hits.iter()
                .any(|b| b.origin.contains("CrossFilePropagation")),
            "expected CrossFilePropagation origin, got: {hits:#?}"
        );
    }
}
