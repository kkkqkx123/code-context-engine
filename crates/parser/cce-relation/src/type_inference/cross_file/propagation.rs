use cce_types::language::Language;
use cce_types::normalize_project_path;
use dashmap::DashMap;

use super::super::call_utils::split_call_target;
use super::super::propagator::CrossFilePropagator;
use super::super::types::{
    InferenceOrigin, ScopedTypeContext, TypeBinding, binding_supersedes, parse_type_shape,
    type_shape_to_string,
};
use super::arg_inference::infer_arg_shape;
use super::call_parsing::parse_call_chain;
use super::resolution::{candidate_downgrades_existing, resolve_single_call_binding};

const MAX_ITERATIONS: usize = 10;
const MAX_CHAIN_DEPTH: usize = 5;

/// Whether a call root (callee or chain receiver) is declared in the
/// caller's own file.
///
/// A locally declared root means the callee resolves same-file, so the
/// propagated binding carries `FunctionReturn` rather than
/// `CrossFilePropagation`, matching what same-file call resolution would
/// stamp. Genuinely cross-file callees keep the propagation origin.
fn call_root_is_local(file: &cce_types::ParsedFile, root: &str) -> bool {
    let root = root.trim();
    if root.is_empty() {
        return false;
    }
    file.entities.iter().any(|e| {
        e.name == root
            && matches!(
                e.kind,
                cce_types::entity::EntityKind::Function
                    | cce_types::entity::EntityKind::Method
                    | cce_types::entity::EntityKind::Constructor
                    | cce_types::entity::EntityKind::Variable
                    | cce_types::entity::EntityKind::Field
                    | cce_types::entity::EntityKind::Property
            )
    })
}

/// Origin for a propagated binding given the call root's locality.
fn propagated_origin(file: &cce_types::ParsedFile, root: &str) -> InferenceOrigin {
    if call_root_is_local(file, root) {
        InferenceOrigin::FunctionReturn
    } else {
        InferenceOrigin::CrossFilePropagation
    }
}

/// Origin for a propagated call-CHAIN binding.
///
/// Both the chain root and the resolved trailing member must be declared
/// in the caller's file; a locally rooted chain into a cross-file method
/// still carries the propagation origin.
fn propagated_chain_origin(
    file: &cce_types::ParsedFile,
    chain: &[super::call_parsing::CallStep],
) -> InferenceOrigin {
    let root_local = chain
        .first()
        .is_some_and(|s| call_root_is_local(file, &s.method_name));
    let member_local = chain.last().is_some_and(|s| {
        file.entities.iter().any(|e| {
            e.name == s.method_name
                && matches!(
                    e.kind,
                    cce_types::entity::EntityKind::Function
                        | cce_types::entity::EntityKind::Method
                        | cce_types::entity::EntityKind::Constructor
                        | cce_types::entity::EntityKind::Field
                        | cce_types::entity::EntityKind::Property
                )
        })
    });
    if root_local && member_local {
        InferenceOrigin::FunctionReturn
    } else {
        InferenceOrigin::CrossFilePropagation
    }
}

/// Propagate cross-file return types into variable bindings.
///
/// For each variable entity that lacks a high-confidence type but is assigned
/// via a function call (metadata `call_target` or `constructor_type`), look up
/// the callee's return type in the propagator and add a variable binding with
/// `Medium` confidence. This enables `x = foo()` where `foo` returns `MyType`
/// to infer `x: MyType` even when `foo` is defined in another file.
///
/// Supports iterative propagation and chain calls like `x = foo().bar()`.
pub fn propagate_variable_types(
    files: &[&cce_types::ParsedFile],
    propagator: &CrossFilePropagator,
    contexts: &DashMap<String, ScopedTypeContext>,
) {
    let mut changed = true;
    let mut iterations = 0;
    while changed && iterations < MAX_ITERATIONS {
        changed = false;
        iterations += 1;
        for file in files {
            let normalized = normalize_project_path(&file.path);
            let Some(mut ctx_ref) = contexts.get_mut(&normalized) else {
                continue;
            };
            let ctx = ctx_ref.value_mut();

            for entity in &file.entities {
                // Local call results can surface as Variable, Field, or
                // Property entities depending on the language extractor
                // (for example Kotlin local `val user = loadUser(...)`).
                if !matches!(
                    entity.kind,
                    cce_types::entity::EntityKind::Variable
                        | cce_types::entity::EntityKind::Field
                        | cce_types::entity::EntityKind::Property
                ) {
                    continue;
                }
                if let Some(existing) = ctx.get_variable_type(&entity.name) {
                    if super::super::types::origin_is_authoritative(existing.origin) {
                        continue;
                    }
                }

                // Try to find a call target that could provide a return type.
                let call_target = entity
                    .metadata
                    .get("call_target")
                    .or_else(|| entity.metadata.get("constructor_type"))
                    .cloned();

                if let Some(target) = call_target {
                    // Handle chain calls: try iterative resolution
                    let language = file.language;
                    let chain = parse_call_chain(&target);
                    // Stored targets may carry an argument list (`foo(a)`);
                    // name lookups always use the stripped callee name.
                    let stripped_name = || split_call_target(&target).0;
                    let simple_target = if chain.len() > MAX_CHAIN_DEPTH {
                        continue;
                    } else if chain.len() > 1 {
                        let mut visited = std::collections::HashSet::new();
                        let mut current_binding: Option<TypeBinding> = None;
                        let mut cycle_detected = false;
                        // A chain only propagates when every step resolves.
                        // Propagating the intermediate receiver on a later
                        // miss (`names.get(0)` yielding `ArrayList<String>`
                        // instead of `String`) leaks the wrong type.
                        let mut fully_resolved = true;
                        for (idx, step) in chain.iter().enumerate() {
                            if !visited.insert(step.method_name.clone()) {
                                cycle_detected = true;
                                break;
                            }
                            if visited.len() > MAX_CHAIN_DEPTH {
                                cycle_detected = true;
                                break;
                            }
                            if idx == 0 {
                                if let Some(var_binding) = ctx.get_variable_type(&step.method_name)
                                {
                                    current_binding = Some(var_binding.clone());
                                } else if let Some(binding) =
                                    propagator.get_return_type_by_name(&step.method_name)
                                {
                                    current_binding = Some(binding.clone());
                                } else if let Some(binding) =
                                    propagator.get_field_type_by_name(&step.method_name)
                                {
                                    current_binding = Some(binding.clone());
                                } else {
                                    fully_resolved = false;
                                    break;
                                }
                            } else {
                                let cur = current_binding.as_ref().map(|b| b.type_name.clone());
                                if let Some(cur_type) = cur {
                                    // Well-known collection element accessors
                                    // (`List.get(i)` yields `E`) resolve
                                    // before the generic member lookup. The
                                    // intermediate origin below is internal
                                    // only: the final binding is re-stamped
                                    // through `propagated_chain_origin`, and
                                    // same-file results already bound by the
                                    // extractor (`FunctionReturn` outranks
                                    // propagation) are never downgraded.
                                    if let Some(element) = collection_element_access(
                                        language,
                                        &cur_type,
                                        &step.method_name,
                                    ) {
                                        current_binding = Some(TypeBinding {
                                            type_name: type_shape_to_string(&element),
                                            type_entity_id: None,
                                            span: entity.span,
                                            origin: Some(InferenceOrigin::CrossFilePropagation),
                                            shape: Some(element),
                                        });
                                    } else if let Some(member_binding) =
                                        propagator.lookup_member_type(&cur_type, &step.method_name)
                                    {
                                        current_binding = Some(member_binding);
                                    } else if let Some(binding) =
                                        propagator.get_field_type_by_name(&step.method_name)
                                    {
                                        current_binding = Some(binding.clone());
                                    } else if let Some(binding) =
                                        propagator.get_return_type_by_name(&step.method_name)
                                    {
                                        current_binding = Some(binding.clone());
                                    } else {
                                        fully_resolved = false;
                                        break;
                                    }
                                } else {
                                    fully_resolved = false;
                                    break;
                                }
                            }
                        }
                        if cycle_detected {
                            continue;
                        }
                        if fully_resolved {
                            if let Some(binding) = current_binding {
                                let propagated = TypeBinding {
                                    type_name: binding.type_name.clone(),
                                    type_entity_id: binding.type_entity_id,
                                    span: entity.span,
                                    origin: Some(propagated_chain_origin(file, &chain)),
                                    shape: binding.shape.clone(),
                                };
                                let should_insert =
                                    ctx.get_variable_type(&entity.name).is_none_or(|existing| {
                                        !candidate_downgrades_existing(
                                            Some(existing),
                                            propagated.shape.as_ref(),
                                        ) && binding_supersedes(propagated.origin, existing.origin)
                                    });
                                if should_insert {
                                    ctx.add_variable_type(entity.name.clone(), propagated);
                                    changed = true;
                                }
                                continue;
                            }
                        }
                        // Fallback to simple target if chain resolution failed
                        super::super::call_utils::simple_callee_name(&stripped_name()).to_string()
                    } else {
                        // Strip qualification: `module.func` / `$this->m` -> `func` / `m`
                        super::super::call_utils::simple_callee_name(&stripped_name()).to_string()
                    };
                    if simple_target.is_empty() {
                        continue;
                    }
                    // Generic refinement first, unsubstituted return second.
                    let resolved =
                        resolve_single_call_binding(propagator, language, &target, &mut |arg| {
                            infer_arg_shape(ctx, language, arg)
                        });
                    if let Some((type_name, type_entity_id, shape)) = resolved {
                        let propagated = TypeBinding {
                            type_name,
                            type_entity_id,
                            span: entity.span,
                            origin: Some(propagated_origin(file, &simple_target)),
                            shape,
                        };
                        let should_insert =
                            ctx.get_variable_type(&entity.name).is_none_or(|existing| {
                                !candidate_downgrades_existing(
                                    Some(existing),
                                    propagated.shape.as_ref(),
                                ) && binding_supersedes(propagated.origin, existing.origin)
                            });
                        if should_insert {
                            ctx.add_variable_type(entity.name.clone(), propagated);
                            changed = true;
                        }
                    }
                }
            }
        }
    }
}

/// Well-known collection element accessors with deterministic stdlib
/// semantics (`names.get(0)` on `ArrayList<String>` yields `String`).
///
/// Only exact receiver-base/method pairs resolve: Java `List` family
/// `get(int)` yields the element type, `Map` family `get(key)` yields the
/// value type. Anything else returns `None` so callers stay conservative.
pub(crate) fn collection_element_access(
    language: Language,
    receiver: &str,
    method: &str,
) -> Option<super::super::types::TypeShape> {
    if language != Language::Java || method != "get" {
        return None;
    }
    let shape = parse_type_shape(receiver, language)?;
    let (base, args) = match &shape {
        super::super::types::TypeShape::Generic { base, args } => (base.as_str(), args),
        _ => return None,
    };
    let index = match base {
        "ArrayList" | "LinkedList" | "Vector" | "Stack" | "List" | "AbstractList" => 0,
        "HashMap" | "LinkedHashMap" | "TreeMap" | "WeakHashMap" | "Map" | "AbstractMap"
        | "SortedMap" => 1,
        _ => return None,
    };
    args.get(index).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;
    use cce_types::entity::{Entity, EntityId, EntityKind};
    use cce_types::language::Language;

    use crate::type_inference::types::parse_type_shape;

    fn dummy_span() -> Span {
        Span::default()
    }

    #[test]
    fn test_variable_propagation() {
        let propagator = CrossFilePropagator::new();
        let mut ctx_a = ScopedTypeContext::new(Language::Python);
        ctx_a.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "User".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities_a = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "create_user".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("a.py", &ctx_a, &entities_a);

        let mut file_b = cce_types::ParsedFile::new(Language::Python, "b.py".to_string(), "");
        let var = Entity::new(
            EntityId(2),
            EntityKind::Variable,
            "u".to_string(),
            dummy_span(),
        )
        .with_metadata("call_target", "create_user");
        file_b.add_entity(var);

        let contexts: DashMap<String, ScopedTypeContext> = DashMap::new();
        let ctx_b = ScopedTypeContext::new(Language::Python);
        contexts.insert("b.py".to_string(), ctx_b);

        propagate_variable_types(&[&file_b], &propagator, &contexts);

        let ctx = contexts
            .get("b.py")
            .expect("test context 'b.py' must exist");
        let binding = ctx
            .get_variable_type("u")
            .expect("variable 'u' must be bound");
        assert_eq!(binding.type_name, "User");
        assert!(binding.origin.is_some());
    }

    #[test]
    fn test_broken_chain_does_not_leak_receiver_type() {
        // `first = names.unknownMethod(0)` must not inherit the receiver
        // type when the trailing call does not resolve.
        let propagator = CrossFilePropagator::new();
        let mut file = cce_types::ParsedFile::new(Language::Java, "A.java".to_string(), "");
        let first = Entity::new(
            EntityId(2),
            EntityKind::Variable,
            "first".to_string(),
            dummy_span(),
        )
        .with_metadata("call_target", "names.unknownMethod(0)");
        file.add_entity(first);

        let contexts: DashMap<String, ScopedTypeContext> = DashMap::new();
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_variable_type(
            "names".to_string(),
            TypeBinding {
                type_name: "ArrayList<String>".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: parse_type_shape("ArrayList<String>", Language::Java),
            },
        );
        contexts.insert("A.java".to_string(), ctx);

        propagate_variable_types(&[&file], &propagator, &contexts);

        let ctx = contexts
            .get("A.java")
            .expect("test context 'A.java' must exist");
        assert!(ctx.get_variable_type("first").is_none());
    }

    #[test]
    fn test_collection_get_chain_resolves_element_type() {
        // End-to-end through the chain path: `first = names.get(0)` on
        // `ArrayList<String>` binds `String`, not the receiver.
        let propagator = CrossFilePropagator::new();
        let mut file = cce_types::ParsedFile::new(Language::Java, "A.java".to_string(), "");
        let first = Entity::new(
            EntityId(2),
            EntityKind::Variable,
            "first".to_string(),
            dummy_span(),
        )
        .with_metadata("call_target", "names.get(0)");
        file.add_entity(first);

        let contexts: DashMap<String, ScopedTypeContext> = DashMap::new();
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_variable_type(
            "names".to_string(),
            TypeBinding {
                type_name: "ArrayList<String>".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: parse_type_shape("ArrayList<String>", Language::Java),
            },
        );
        contexts.insert("A.java".to_string(), ctx);

        propagate_variable_types(&[&file], &propagator, &contexts);

        let ctx = contexts
            .get("A.java")
            .expect("test context 'A.java' must exist");
        let binding = ctx.get_variable_type("first").expect("element binding");
        assert_eq!(binding.type_name, "String");
    }

    #[test]
    fn test_collection_get_resolves_element_type() {
        assert_eq!(
            collection_element_access(Language::Java, "ArrayList<String>", "get")
                .map(|s| type_shape_to_string(&s)),
            Some("String".to_string())
        );
        assert_eq!(
            collection_element_access(Language::Java, "HashMap<String, Integer>", "get")
                .map(|s| type_shape_to_string(&s)),
            Some("Integer".to_string())
        );
        assert!(collection_element_access(Language::Java, "ArrayList<String>", "add").is_none());
        assert!(collection_element_access(Language::Java, "String", "get").is_none());
        assert!(collection_element_access(Language::Python, "list", "get").is_none());
    }

    #[test]
    fn test_property_propagation() {
        let propagator = CrossFilePropagator::new();
        let mut ctx_a = ScopedTypeContext::new(Language::Kotlin);
        ctx_a.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "User".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities_a = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "loadUser".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("User.kt", &ctx_a, &entities_a);

        let mut file_b = cce_types::ParsedFile::new(Language::Kotlin, "Service.kt".to_string(), "");
        let property = Entity::new(
            EntityId(2),
            EntityKind::Property,
            "user".to_string(),
            dummy_span(),
        )
        .with_metadata("call_target", "loadUser(\"Alice\")");
        file_b.add_entity(property);

        let contexts: DashMap<String, ScopedTypeContext> = DashMap::new();
        let ctx_b = ScopedTypeContext::new(Language::Kotlin);
        contexts.insert("Service.kt".to_string(), ctx_b);

        propagate_variable_types(&[&file_b], &propagator, &contexts);

        let ctx = contexts
            .get("Service.kt")
            .expect("test context 'Service.kt' must exist");
        let binding = ctx
            .get_variable_type("user")
            .expect("variable 'user' must be bound");
        assert_eq!(binding.type_name, "User");
        assert_eq!(binding.origin, Some(InferenceOrigin::CrossFilePropagation));
    }

    fn generic_return_binding(type_name: &str, language: Language) -> TypeBinding {
        TypeBinding {
            type_name: type_name.to_string(),
            type_entity_id: None,
            span: dummy_span(),
            origin: None,
            shape: parse_type_shape(type_name, language),
        }
    }

    fn generic_param_binding(type_name: &str, language: Language) -> TypeBinding {
        TypeBinding {
            type_name: type_name.to_string(),
            type_entity_id: None,
            span: dummy_span(),
            origin: None,
            shape: parse_type_shape(type_name, language),
        }
    }

    #[test]
    fn test_generic_call_refinement() {
        let propagator = CrossFilePropagator::new();
        let mut ctx_a = ScopedTypeContext::new(Language::TypeScript);
        ctx_a.add_return_type(
            EntityId(1),
            generic_return_binding("T", Language::TypeScript),
        );
        ctx_a.add_parameter_types(
            EntityId(1),
            vec![generic_param_binding("T", Language::TypeScript)],
        );
        let entities_a = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "identity".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("a.ts", &ctx_a, &entities_a);

        let mut file_b = cce_types::ParsedFile::new(Language::TypeScript, "b.ts".to_string(), "");
        let var = Entity::new(
            EntityId(2),
            EntityKind::Variable,
            "y".to_string(),
            dummy_span(),
        )
        .with_metadata("call_target", "identity(42)");
        file_b.add_entity(var);

        let contexts: DashMap<String, ScopedTypeContext> = DashMap::new();
        contexts.insert(
            "b.ts".to_string(),
            ScopedTypeContext::new(Language::TypeScript),
        );

        propagate_variable_types(&[&file_b], &propagator, &contexts);

        let ctx = contexts
            .get("b.ts")
            .expect("test context 'b.ts' must exist");
        let binding = ctx
            .get_variable_type("y")
            .expect("variable 'y' must be bound");
        assert_eq!(binding.type_name, "number");
    }

    #[test]
    fn test_any_return_refines_from_single_known_arg() {
        // `def identity(x: Any) -> Any` called as `identity(42)` binds the
        // argument shape instead of leaking the bare dynamic name.
        let propagator = CrossFilePropagator::new();
        let mut ctx_a = ScopedTypeContext::new(Language::Python);
        ctx_a.add_return_type(EntityId(1), generic_return_binding("Any", Language::Python));
        ctx_a.add_parameter_types(
            EntityId(1),
            vec![generic_param_binding("Any", Language::Python)],
        );
        let entities_a = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "identity".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("a.py", &ctx_a, &entities_a);

        let mut file_b = cce_types::ParsedFile::new(Language::Python, "b.py".to_string(), "");
        let var = Entity::new(
            EntityId(2),
            EntityKind::Variable,
            "y".to_string(),
            dummy_span(),
        )
        .with_metadata("call_target", "identity(42)");
        file_b.add_entity(var);

        let contexts: DashMap<String, ScopedTypeContext> = DashMap::new();
        contexts.insert("b.py".to_string(), ScopedTypeContext::new(Language::Python));

        propagate_variable_types(&[&file_b], &propagator, &contexts);

        let ctx = contexts
            .get("b.py")
            .expect("test context 'b.py' must exist");
        let binding = ctx
            .get_variable_type("y")
            .expect("variable 'y' must be bound");
        assert_eq!(binding.type_name, "int");
    }

    #[test]
    fn test_any_return_multi_arg_stays_conservative() {
        // Multi-argument dynamic returns keep the bare name: the
        // passthrough position is ambiguous.
        let propagator = CrossFilePropagator::new();
        let mut ctx_a = ScopedTypeContext::new(Language::Python);
        ctx_a.add_return_type(EntityId(1), generic_return_binding("Any", Language::Python));
        ctx_a.add_parameter_types(
            EntityId(1),
            vec![
                generic_param_binding("Any", Language::Python),
                generic_param_binding("Any", Language::Python),
            ],
        );
        let entities_a = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "combine".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("a.py", &ctx_a, &entities_a);

        let mut file_b = cce_types::ParsedFile::new(Language::Python, "b.py".to_string(), "");
        let var = Entity::new(
            EntityId(2),
            EntityKind::Variable,
            "y".to_string(),
            dummy_span(),
        )
        .with_metadata("call_target", "combine(1, 2)");
        file_b.add_entity(var);

        let contexts: DashMap<String, ScopedTypeContext> = DashMap::new();
        contexts.insert("b.py".to_string(), ScopedTypeContext::new(Language::Python));

        propagate_variable_types(&[&file_b], &propagator, &contexts);

        let ctx = contexts
            .get("b.py")
            .expect("test context 'b.py' must exist");
        let binding = ctx
            .get_variable_type("y")
            .expect("variable 'y' must be bound");
        assert_eq!(binding.type_name, "Any");
    }

    #[test]
    fn test_generic_refinement_falls_back_without_args() {
        // Bare `identity` (no argument list) keeps the unsubstituted return.
        let propagator = CrossFilePropagator::new();
        let mut ctx_a = ScopedTypeContext::new(Language::TypeScript);
        ctx_a.add_return_type(
            EntityId(1),
            generic_return_binding("T", Language::TypeScript),
        );
        ctx_a.add_parameter_types(
            EntityId(1),
            vec![generic_param_binding("T", Language::TypeScript)],
        );
        let entities_a = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "identity".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("a.ts", &ctx_a, &entities_a);

        let mut file_b = cce_types::ParsedFile::new(Language::TypeScript, "b.ts".to_string(), "");
        let var = Entity::new(
            EntityId(2),
            EntityKind::Variable,
            "y".to_string(),
            dummy_span(),
        )
        .with_metadata("call_target", "identity");
        file_b.add_entity(var);

        let contexts: DashMap<String, ScopedTypeContext> = DashMap::new();
        contexts.insert(
            "b.ts".to_string(),
            ScopedTypeContext::new(Language::TypeScript),
        );

        propagate_variable_types(&[&file_b], &propagator, &contexts);

        let ctx = contexts
            .get("b.ts")
            .expect("test context 'b.ts' must exist");
        let binding = ctx
            .get_variable_type("y")
            .expect("variable 'y' must be bound");
        assert_eq!(binding.type_name, "T");
    }

    #[test]
    fn test_generic_coarse_result_never_downgrades_concrete() {
        // `y` already holds a fully substituted type from an earlier pass;
        // an unresolvable call must not overwrite it with bare parameters.
        let propagator = CrossFilePropagator::new();
        let mut ctx_a = ScopedTypeContext::new(Language::TypeScript);
        ctx_a.add_return_type(
            EntityId(1),
            generic_return_binding("Pair<A, B>", Language::TypeScript),
        );
        ctx_a.add_parameter_types(
            EntityId(1),
            vec![
                generic_param_binding("A", Language::TypeScript),
                generic_param_binding("B", Language::TypeScript),
            ],
        );
        let entities_a = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "makePair".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("a.ts", &ctx_a, &entities_a);

        let mut file_b = cce_types::ParsedFile::new(Language::TypeScript, "b.ts".to_string(), "");
        let var = Entity::new(
            EntityId(2),
            EntityKind::Variable,
            "p".to_string(),
            dummy_span(),
        )
        .with_metadata("call_target", "makePair(42, z)");
        file_b.add_entity(var);

        let contexts: DashMap<String, ScopedTypeContext> = DashMap::new();
        let mut ctx_b = ScopedTypeContext::new(Language::TypeScript);
        ctx_b.add_variable_type(
            "p".to_string(),
            TypeBinding {
                type_name: "Pair<number, string>".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: Some(InferenceOrigin::CrossFilePropagation),
                shape: parse_type_shape("Pair<number, string>", Language::TypeScript),
            },
        );
        contexts.insert("b.ts".to_string(), ctx_b);

        propagate_variable_types(&[&file_b], &propagator, &contexts);

        let ctx = contexts
            .get("b.ts")
            .expect("test context 'b.ts' must exist");
        assert_eq!(
            ctx.get_variable_type("p")
                .expect("variable 'p' must be bound")
                .type_name,
            "Pair<number, string>"
        );
    }

    #[test]
    fn test_propagation_preserves_authoritative_binding() {
        let propagator = CrossFilePropagator::new();
        let mut ctx_a = ScopedTypeContext::new(Language::Python);
        ctx_a.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "User".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities_a = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "create_user".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("a.py", &ctx_a, &entities_a);

        let mut file_b = cce_types::ParsedFile::new(Language::Python, "b.py".to_string(), "");
        let var = Entity::new(
            EntityId(2),
            EntityKind::Variable,
            "u".to_string(),
            dummy_span(),
        )
        .with_metadata("call_target", "create_user");
        file_b.add_entity(var);

        let contexts: DashMap<String, ScopedTypeContext> = DashMap::new();
        let mut ctx_b = ScopedTypeContext::new(Language::Python);
        ctx_b.add_variable_type(
            "u".to_string(),
            TypeBinding {
                type_name: "Admin".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: Some(InferenceOrigin::TypeAnnotation),
                shape: None,
            },
        );
        contexts.insert("b.py".to_string(), ctx_b);

        propagate_variable_types(&[&file_b], &propagator, &contexts);

        let ctx = contexts
            .get("b.py")
            .expect("test context 'b.py' must exist");
        assert_eq!(
            ctx.get_variable_type("u")
                .expect("variable 'u' must be bound")
                .type_name,
            "Admin"
        );
    }
}

#[cfg(test)]
mod locality_tests {
    use super::*;
    use cce_types::entity::{Entity, EntityId, EntityKind};

    fn dummy_span() -> cce_types::Span {
        cce_types::Span::default()
    }

    fn file_with(path: &str, language: Language, entities: Vec<Entity>) -> cce_types::ParsedFile {
        let mut file = cce_types::ParsedFile::new(language, path.to_string(), "");
        for e in entities {
            file.add_entity(e);
        }
        file
    }

    #[test]
    fn test_same_file_callee_stamps_function_return() {
        // `dup = container.duplicate()` with both sides in one file must
        // not carry the cross-file origin.
        let propagator = CrossFilePropagator::new();
        let mut callee_ctx = ScopedTypeContext::new(Language::Python);
        callee_ctx.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "Container".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let callee = Entity::new(
            EntityId(1),
            EntityKind::Method,
            "duplicate".to_string(),
            dummy_span(),
        );
        propagator.insert_file("a.py", &callee_ctx, &[callee.clone()]);

        let dup = Entity::new(
            EntityId(2),
            EntityKind::Variable,
            "dup".to_string(),
            dummy_span(),
        )
        .with_metadata("call_target", "container.duplicate()");
        let container = Entity::new(
            EntityId(3),
            EntityKind::Variable,
            "container".to_string(),
            dummy_span(),
        );
        let file = file_with("a.py", Language::Python, vec![callee, container, dup]);

        let contexts: DashMap<String, ScopedTypeContext> = DashMap::new();
        let mut ctx = ScopedTypeContext::new(Language::Python);
        ctx.add_variable_type(
            "container".to_string(),
            TypeBinding {
                type_name: "Container".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        contexts.insert("a.py".to_string(), ctx);

        propagate_variable_types(&[&file], &propagator, &contexts);

        let ctx = contexts
            .get("a.py")
            .expect("test context 'a.py' must exist");
        let binding = ctx.get_variable_type("dup").expect("dup binds");
        assert_eq!(binding.type_name, "Container");
        assert_eq!(binding.origin, Some(InferenceOrigin::FunctionReturn));
    }

    #[test]
    fn test_cross_file_callee_keeps_propagation_origin() {
        // Same setup but the callee lives in another file: the
        // cross-file origin is preserved.
        let propagator = CrossFilePropagator::new();
        let mut callee_ctx = ScopedTypeContext::new(Language::Python);
        callee_ctx.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "Container".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let callee = Entity::new(
            EntityId(1),
            EntityKind::Method,
            "duplicate".to_string(),
            dummy_span(),
        );
        propagator.insert_file("other.py", &callee_ctx, &[callee]);

        let dup = Entity::new(
            EntityId(2),
            EntityKind::Variable,
            "dup".to_string(),
            dummy_span(),
        )
        .with_metadata("call_target", "container.duplicate()");
        let container = Entity::new(
            EntityId(3),
            EntityKind::Variable,
            "container".to_string(),
            dummy_span(),
        );
        let file = file_with("a.py", Language::Python, vec![container, dup]);

        let contexts: DashMap<String, ScopedTypeContext> = DashMap::new();
        let mut ctx = ScopedTypeContext::new(Language::Python);
        ctx.add_variable_type(
            "container".to_string(),
            TypeBinding {
                type_name: "Container".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        contexts.insert("a.py".to_string(), ctx);

        propagate_variable_types(&[&file], &propagator, &contexts);

        let ctx = contexts
            .get("a.py")
            .expect("test context 'a.py' must exist");
        let binding = ctx.get_variable_type("dup").expect("dup binds");
        assert_eq!(binding.origin, Some(InferenceOrigin::CrossFilePropagation));
    }
}
