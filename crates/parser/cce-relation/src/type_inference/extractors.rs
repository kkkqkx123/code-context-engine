//! Shared extraction utilities for type inference.
//!
//! Functions here parse entity struct fields and metadata to produce
//! `TypeBinding` entries. They are called by per-language inferers.

use cce_types::entity::Entity;
use cce_types::language::Language;

use super::cross_file::{collection_element_access, infer_arg_shape};
use super::types::{
    InferenceOrigin, ScopedTypeContext, TypeBinding, TypeShape, parse_type_shape,
    type_shape_to_string,
};

use super::call_utils::{simple_callee_name, split_call_target, split_receiver_method};
use super::generics::{
    GenericTypeArg, parse_generic_type, resolve_collection_factory_shape, shape_contains_param,
    substitute_call_return_type,
};

/// Strip a leading colon from a captured type annotation.
///
/// Tree-sitter `type_annotation` nodes include the colon prefix
/// (`: string`); quoted forward references (`"Container"`) shed their
/// quotes since a type name never legitimately carries them. Bindings
/// must store the bare type name.
fn clean_annotation(ty: &str) -> &str {
    let normalized = ty.trim().trim_start_matches(':').trim();
    let bytes = normalized.as_bytes();
    if bytes.len() >= 2 {
        let (first, last) = (bytes[0], bytes[bytes.len() - 1]);
        if first == last && (first == b'"' || first == b'\'') {
            return normalized[1..normalized.len() - 1].trim();
        }
    }
    normalized
}

/// Normalize Go slice-prefix spellings to the structured shape spelling.
///
/// Go writes slices prefix (`[]T`) while [`TypeShape::Array`] renders
/// suffix (`T[]`). Without normalization the inferred name (`[]T`) and
/// its shape (`T[]`) disagree in reports. Only the bare `[]` prefix is
/// rewritten (`[][]T` -> `T[][]`); maps, arrays with lengths and other
/// spellings pass through untouched.
fn normalize_go_slice_spelling(cleaned: &str, language: Language) -> String {
    if language != Language::Go {
        return cleaned.to_string();
    }
    let mut rest = cleaned.trim();
    let mut depth = 0usize;
    while let Some(stripped) = rest.strip_prefix("[]") {
        rest = stripped.trim();
        depth += 1;
    }
    if depth == 0 || rest.is_empty() || rest.contains(char::is_whitespace) {
        return cleaned.to_string();
    }
    format!("{rest}{}", "[]".repeat(depth))
}

/// Whether a captured annotation is an inference keyword rather than a
/// concrete type (`var`, `auto`, `val`, `let`, `decltype(...)`).
///
/// Callers skip such annotations so inference falls back to the
/// initializer (`constructor_type` / `literal_type` / `call_target`).
fn is_inferred_type_keyword(ty: &str) -> bool {
    let mut normalized = ty.trim();
    normalized = normalized
        .strip_prefix("const ")
        .unwrap_or(normalized)
        .trim();
    normalized = normalized
        .trim_end_matches(['&', '*', ' '].as_slice())
        .trim();
    if normalized.starts_with("decltype") {
        return true;
    }
    matches!(normalized, "var" | "auto" | "val" | "let")
}

/// Extract type information from a function entity's struct fields.
///
/// Reads return type from `entity.return_type` and parameter types
/// from `entity.parameters` (filtering entries with type annotations).
/// Also stores the return type indexed by function name to enable
/// `call_target` resolution for variables assigned via `x = f()`.
///
/// When no explicit return type annotation is present, the extractor
/// checks for `return_body` metadata (set by the parser for languages
/// without return-type syntax) and uses it to derive a return type.
pub fn extract_function_types(entity: &Entity, ctx: &mut ScopedTypeContext) {
    if let Some(ref return_type) = entity.return_type {
        let cleaned = clean_annotation(return_type);
        if !cleaned.is_empty() {
            let normalized = normalize_go_slice_spelling(cleaned, ctx.language());
            let shape = parse_type_shape(&normalized, ctx.language());
            let binding = TypeBinding {
                type_name: normalized,
                type_entity_id: None,
                span: entity.span,
                origin: Some(InferenceOrigin::TypeAnnotation),
                shape,
            };
            ctx.add_return_type(entity.id, binding.clone());
            // Also store by name for local call_target resolution
            ctx.add_return_type_by_name(entity.name.clone(), entity.id, binding);
            // Explicit annotation wins; skip return_body processing.
            let param_bindings: Vec<TypeBinding> = entity
                .parameters
                .iter()
                .filter_map(|(_name, ty)| {
                    ty.as_ref().map(|type_name| {
                        let cleaned = clean_annotation(type_name);
                        let normalized = normalize_go_slice_spelling(cleaned, ctx.language());
                        TypeBinding {
                            type_name: normalized.clone(),
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(InferenceOrigin::TypeAnnotation),
                            shape: parse_type_shape(&normalized, ctx.language()),
                        }
                    })
                })
                .collect();
            if !param_bindings.is_empty() {
                ctx.add_parameter_types(entity.id, param_bindings);
            }
            return;
        }
    }

    // No explicit return annotation — try return_body metadata.
    if let Some(return_body) = entity.metadata.get("return_body") {
        let body = return_body.trim();
        if !body.is_empty() {
            if let Some(binding) = infer_return_from_body(body, entity, ctx) {
                ctx.add_return_type(entity.id, binding.clone());
                ctx.add_return_type_by_name(entity.name.clone(), entity.id, binding);
            }
        }
    }

    let param_bindings: Vec<TypeBinding> = entity
        .parameters
        .iter()
        .filter_map(|(_name, ty)| {
            ty.as_ref().map(|type_name| {
                let cleaned = clean_annotation(type_name);
                let normalized = normalize_go_slice_spelling(cleaned, ctx.language());
                TypeBinding {
                    type_name: normalized.clone(),
                    type_entity_id: None,
                    span: entity.span,
                    origin: Some(InferenceOrigin::TypeAnnotation),
                    shape: parse_type_shape(&normalized, ctx.language()),
                }
            })
        })
        .collect();
    if !param_bindings.is_empty() {
        ctx.add_parameter_types(entity.id, param_bindings);
    }
}

/// Try same-file `call_target` resolution: `x = f()` where `f` is defined
/// in the same file.
///
/// Returns `true` when a binding was recorded. A bare `Name(...)` call may
/// also carry `constructor_type` metadata (PascalCase methods in
/// C#/Java/Kotlin/Scala look like constructors to the extractor); the
/// function-return reading outranks the constructor reading
/// (`FunctionReturn` priority 6 over `ConstructorCall` priority 2), so this
/// runs before the constructor branch and a miss falls through to it.
fn try_resolve_call_target(entity: &Entity, ctx: &mut ScopedTypeContext) -> bool {
    let Some(call_target) = entity.metadata.get("call_target") else {
        return false;
    };
    let (func_name, args) = split_call_target(call_target);
    let language = ctx.language();
    let arg_shapes: Vec<Option<TypeShape>> = args
        .iter()
        .map(|arg| infer_arg_shape(ctx, language, arg))
        .collect();
    // Receiver-qualified targets (`obj.m`, `$this->m`, `A::m`) resolve
    // against the trailing name; the receiver is only consulted for
    // collection element access below.
    let simple_name = simple_callee_name(&func_name).to_string();
    let Some(return_binding) = ctx.resolve_return_by_name(&simple_name, &arg_shapes, language)
    else {
        // Member calls on a known receiver (`names.get(0)`,
        // `$this->combineInts(1)`) resolve element access against the
        // receiver binding before giving up, so same-file collection
        // reads bind without propagation.
        if let Some((receiver, method)) = split_receiver_method(&func_name)
            && let Some(receiver_binding) = ctx.get_variable_type(receiver.trim())
            && let Some(element) =
                collection_element_access(language, &receiver_binding.type_name, method.trim())
        {
            let type_name = type_shape_to_string(&element);
            ctx.add_variable_type(
                entity.name.clone(),
                TypeBinding {
                    type_name,
                    type_entity_id: None,
                    span: entity.span,
                    origin: Some(InferenceOrigin::FunctionReturn),
                    shape: Some(element),
                },
            );
            return true;
        }
        return false;
    };
    let binding = TypeBinding {
        type_name: return_binding.type_name.clone(),
        type_entity_id: return_binding.type_entity_id,
        span: entity.span,
        origin: Some(InferenceOrigin::FunctionReturn),
        shape: return_binding.shape.clone(),
    };
    ctx.add_variable_type(entity.name.clone(), binding);
    true
}

/// Try Scala collection-factory resolution: `xs = List(User(...), ...)`
/// where every element infers the same concrete type.
///
/// Collection factories (`List`, `Seq`, `Vector`, `Set`, `Array`) have no
/// same-file function entry, so the call-target path misses and the bare
/// constructor reading would degrade to `List`. When all call-site
/// elements agree, bind `List[User]` (shape `List<User>`) instead. Mixed
/// or unknown elements stay conservative and fall through.
fn try_resolve_scala_collection_constructor(entity: &Entity, ctx: &mut ScopedTypeContext) -> bool {
    if ctx.language() != Language::Scala {
        return false;
    }
    let Some(call_target) = entity.metadata.get("call_target") else {
        return false;
    };
    let language = ctx.language();
    let Some((type_name, shape)) = resolve_collection_factory_shape(language, call_target, |arg| {
        infer_arg_shape(ctx, language, arg)
    }) else {
        return false;
    };
    let binding = TypeBinding {
        type_name: type_name.clone(),
        type_entity_id: None,
        span: entity.span,
        origin: Some(InferenceOrigin::ConstructorCall),
        shape: Some(shape),
    };
    ctx.add_variable_type(entity.name.clone(), binding);
    try_bind_generic(ctx, &type_name);
    true
}

/// Instantiate a still-generic constructor type from call-site arguments.
///
/// `p = Pair(1, "one")` with `constructor_type = Pair<A, B>` yields
/// `Pair<Int, String>`; explicitly parameterized types (`ArrayList<String>`)
/// and partially resolved arguments keep the recorded reading.
fn instantiate_constructor_call(
    entity: &Entity,
    ctx: &ScopedTypeContext,
) -> Option<(String, TypeShape)> {
    let init_type = entity.metadata.get("constructor_type")?;
    let language = ctx.language();
    let ctor_shape = parse_type_shape(init_type, language)?;
    if !shape_contains_param(&ctor_shape) {
        return None;
    }
    let call_target = entity.metadata.get("call_target")?;
    let (_, args) = split_call_target(call_target);
    if args.is_empty() {
        return None;
    }
    let formal: Vec<TypeShape> = match &ctor_shape {
        TypeShape::Generic { args, .. } => args.clone(),
        _ => return None,
    };
    let actual: Vec<Option<TypeShape>> = args
        .iter()
        .map(|arg| infer_arg_shape(ctx, language, arg))
        .collect();
    let actual_refs: Vec<Option<&TypeShape>> = actual.iter().map(|s| s.as_ref()).collect();
    let substituted = substitute_call_return_type(&formal, &ctor_shape, &actual_refs, language)?;
    if shape_contains_param(&substituted) {
        return None;
    }
    Some((type_shape_to_string(&substituted), substituted))
}

/// Extract type information from a variable entity's metadata.
///
/// Checks metadata keys populated by the parser in priority order:
/// 1. `type_annotation` — explicit type annotation (High)
/// 2. `call_target` — function call like `x = f()` (via FunctionReturn;
///    outranks the constructor reading when both are present)
/// 3. `constructor_type` — constructor call like `x = MyClass()`
/// 4. `literal_type` — literal assignment like `x = 42`
pub fn extract_variable_type(entity: &Entity, ctx: &mut ScopedTypeContext) {
    // Composite destructuring names (`a, b = ...`) are owned by the
    // destructuring pass, which binds each part individually. Binding the
    // joined name here would leak a pseudo-variable into snapshots.
    if entity.name.contains(',') {
        return;
    }
    if let Some(type_name) = entity.metadata.get("type_annotation") {
        let cleaned = clean_annotation(type_name);
        if !cleaned.is_empty() && !is_inferred_type_keyword(cleaned) {
            let normalized = normalize_go_slice_spelling(cleaned, ctx.language());
            let binding = TypeBinding {
                type_name: normalized.clone(),
                type_entity_id: None,
                span: entity.span,
                origin: Some(InferenceOrigin::TypeAnnotation),
                shape: parse_type_shape(&normalized, ctx.language()),
            };
            ctx.add_variable_type(entity.name.clone(), binding);
            try_bind_generic(ctx, &normalized);
            return;
        }
    }

    // Same-file call resolution first: overloaded names resolve by
    // call-site argument shapes (`combine(1, 2)` picks the `(Int, Int)`
    // overload); a miss falls through to the constructor reading.
    if try_resolve_call_target(entity, ctx) {
        return;
    }

    // Scala collection factories (`List(User(...), ...)`): infer the
    // element type from uniform call-site elements before the bare
    // constructor reading applies.
    if try_resolve_scala_collection_constructor(entity, ctx) {
        return;
    }

    if let Some(init_type) = entity.metadata.get("constructor_type") {
        // Call-site instantiation first (`Pair(1, "one")` on `Pair<A, B>`
        // yields `Pair<Int, String>`); otherwise the recorded reading.
        if let Some((type_name, shape)) = instantiate_constructor_call(entity, ctx) {
            ctx.add_variable_type(
                entity.name.clone(),
                TypeBinding {
                    type_name: type_name.clone(),
                    type_entity_id: None,
                    span: entity.span,
                    origin: Some(InferenceOrigin::ConstructorCall),
                    shape: Some(shape),
                },
            );
            try_bind_generic(ctx, &type_name);
            return;
        }
        let binding = TypeBinding {
            type_name: init_type.clone(),
            type_entity_id: None,
            span: entity.span,
            origin: Some(InferenceOrigin::ConstructorCall),
            shape: parse_type_shape(init_type, ctx.language()),
        };
        ctx.add_variable_type(entity.name.clone(), binding);
        try_bind_generic(ctx, init_type);
        return;
    }

    if let Some(lit_type) = entity.metadata.get("literal_type") {
        let binding = TypeBinding {
            type_name: lit_type.clone(),
            type_entity_id: None,
            span: entity.span,
            origin: Some(InferenceOrigin::LiteralType),
            shape: parse_type_shape(lit_type, ctx.language()),
        };
        ctx.add_variable_type(entity.name.clone(), binding);
        try_bind_generic(ctx, lit_type);
    }
}

/// Extract type information from a field/property entity.
///
/// Checks metadata keys in priority order:
/// 1. `type_annotation` — explicit type annotation (High)
/// 2. `call_target` — call initializer like `val x = f()` (via
///    FunctionReturn; outranks the constructor reading when both present)
/// 3. `constructor_type` — initializer like `x = MyClass()`
/// 4. `literal_type` — literal initializer like `x = 42`
pub fn extract_field_type(entity: &Entity, ctx: &mut ScopedTypeContext) {
    // Same composite-name rule as the variable path: the destructuring
    // pass owns comma-joined names and binds each part individually.
    if entity.name.contains(',') {
        return;
    }
    if let Some(type_name) = entity.metadata.get("type_annotation") {
        let cleaned = clean_annotation(type_name);
        if !cleaned.is_empty() && !is_inferred_type_keyword(cleaned) {
            let normalized = normalize_go_slice_spelling(cleaned, ctx.language());
            let binding = TypeBinding {
                type_name: normalized.clone(),
                type_entity_id: None,
                span: entity.span,
                origin: Some(InferenceOrigin::TypeAnnotation),
                shape: parse_type_shape(&normalized, ctx.language()),
            };
            ctx.add_variable_type(entity.name.clone(), binding);
            return;
        }
    }

    // Call-initialized fields/properties (`val x = f()`): same overload-aware
    // resolution as the variable path, tried before the constructor reading.
    if try_resolve_call_target(entity, ctx) {
        return;
    }

    // Scala collection factories share the variable-path element inference.
    if try_resolve_scala_collection_constructor(entity, ctx) {
        return;
    }

    if let Some(init_type) = entity.metadata.get("constructor_type") {
        let binding = TypeBinding {
            type_name: init_type.clone(),
            type_entity_id: None,
            span: entity.span,
            origin: Some(InferenceOrigin::ConstructorCall),
            shape: parse_type_shape(init_type, ctx.language()),
        };
        ctx.add_variable_type(entity.name.clone(), binding);
        try_bind_generic(ctx, init_type);
        return;
    }

    if let Some(lit_type) = entity.metadata.get("literal_type") {
        let binding = TypeBinding {
            type_name: lit_type.clone(),
            type_entity_id: None,
            span: entity.span,
            origin: Some(InferenceOrigin::LiteralType),
            shape: parse_type_shape(lit_type, ctx.language()),
        };
        ctx.add_variable_type(entity.name.clone(), binding);
        try_bind_generic(ctx, lit_type);
    }
}

/// Infer a return type from the `return_body` metadata text.
///
/// The body text is produced by the parser-side normaliser and may be:
/// - A built-in type name from a literal (`str`, `int`, `number`, etc.)
/// - A class name from a constructor call (`User`)
/// - A raw expression the inferer should evaluate (concatenation, call)
///
/// Returns `None` only for truly unrecognisable expressions so the
/// function stays without a return type (conservative no-guess).
fn infer_return_from_body(
    body: &str,
    entity: &Entity,
    ctx: &ScopedTypeContext,
) -> Option<TypeBinding> {
    let language = ctx.language();

    // Built-in literal type names (`str`, `int`, `number`, `string`, etc.)
    // are already normalised by the parser — treat them as concrete types.
    if looks_like_builtin_type(body, language) {
        let shape = parse_type_shape(body, language);
        return Some(TypeBinding {
            type_name: body.to_string(),
            type_entity_id: None,
            span: entity.span,
            origin: Some(InferenceOrigin::GenericInference),
            shape,
        });
    }

    // Class-like name (PascalCase or already a known type).
    if looks_like_type_name(body) {
        let shape = parse_type_shape(body, language);
        return Some(TypeBinding {
            type_name: body.to_string(),
            type_entity_id: None,
            span: entity.span,
            origin: Some(InferenceOrigin::GenericInference),
            shape,
        });
    }

    // Call expression — resolve through same-file return types.
    if let Some(call_target) = body.strip_suffix("()") {
        let func_name = call_target
            .trim()
            .rsplit('.')
            .next()
            .unwrap_or(call_target.trim());
        if let Some(return_binding) = ctx.resolve_return_by_name(func_name, &[], language) {
            return Some(TypeBinding {
                type_name: return_binding.type_name.clone(),
                type_entity_id: return_binding.type_entity_id,
                span: entity.span,
                origin: Some(InferenceOrigin::FunctionReturn),
                shape: return_binding.shape.clone(),
            });
        }
    }

    // String concatenation: all-string operands → `str`.
    if is_string_concatenation(body, language) {
        let shape = parse_type_shape("str", language);
        return Some(TypeBinding {
            type_name: "str".to_string(),
            type_entity_id: None,
            span: entity.span,
            origin: Some(InferenceOrigin::GenericInference),
            shape,
        });
    }

    None
}

/// Check if a body text looks like a built-in type name for the given language.
fn looks_like_builtin_type(name: &str, language: cce_types::language::Language) -> bool {
    let name = name.trim();
    match language {
        cce_types::language::Language::Python => {
            matches!(
                name,
                "str"
                    | "int"
                    | "float"
                    | "bool"
                    | "list"
                    | "dict"
                    | "tuple"
                    | "set"
                    | "None"
                    | "bytes"
                    | "complex"
                    | "range"
            )
        }
        cce_types::language::Language::Lua => {
            matches!(
                name,
                "string" | "number" | "boolean" | "nil" | "table" | "function"
            )
        }
        _ => false,
    }
}

/// Check if a name looks like a type name (PascalCase).
fn looks_like_type_name(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty()
        && name.bytes().next().is_some_and(|b| b.is_ascii_uppercase())
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Check if a body text is a string concatenation expression.
fn is_string_concatenation(body: &str, language: cce_types::language::Language) -> bool {
    match language {
        cce_types::language::Language::Python => body.contains(".. ") || body.contains(" + "),
        cce_types::language::Language::Lua => body.contains(".."),
        _ => false,
    }
}

fn try_bind_generic(ctx: &mut ScopedTypeContext, type_name: &str) {
    if let Some(gt) = parse_generic_type(type_name) {
        match gt.args.len() {
            1 => {
                if let GenericTypeArg::Concrete(concrete) = &gt.args[0] {
                    for param in &["T", "E", "U", "Value"] {
                        if ctx.get_type_param_for_owner(&gt.base, param).is_none() {
                            ctx.bind_type_param_owned(
                                &gt.base,
                                (*param).to_string(),
                                concrete.clone(),
                            );
                        }
                    }
                }
            }
            2 => {
                let first = match &gt.args[0] {
                    GenericTypeArg::Concrete(s) => Some(s.clone()),
                    _ => None,
                };
                let second = match &gt.args[1] {
                    GenericTypeArg::Concrete(s) => Some(s.clone()),
                    _ => None,
                };
                if let Some(k) = first {
                    for param in &["K", "Key"] {
                        if ctx.get_type_param_for_owner(&gt.base, param).is_none() {
                            ctx.bind_type_param_owned(&gt.base, (*param).to_string(), k.clone());
                        }
                    }
                    if ctx.get_type_param_for_owner(&gt.base, "T").is_none() {
                        ctx.bind_type_param_owned(&gt.base, "T".to_string(), k);
                    }
                }
                if let Some(v) = second {
                    for param in &["V", "Value"] {
                        if ctx.get_type_param_for_owner(&gt.base, param).is_none() {
                            ctx.bind_type_param_owned(&gt.base, (*param).to_string(), v.clone());
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::language::Language;

    fn make_entity(
        id: u64,
        name: &str,
        return_type: Option<&str>,
        parameters: Vec<(&str, Option<&str>)>,
    ) -> Entity {
        Entity {
            id: cce_types::EntityId(id),
            name: name.to_string(),
            kind: cce_types::entity::EntityKind::Function,
            return_type: return_type.map(|s| s.to_string()),
            parameters: parameters
                .into_iter()
                .map(|(n, t)| (n.to_string(), t.map(|s| s.to_string())))
                .collect(),
            metadata: std::collections::HashMap::new(),
            span: cce_types::Span::default(),
            ..Default::default()
        }
    }

    fn make_variable_entity(id: u64, name: &str, metadata: Vec<(&str, &str)>) -> Entity {
        Entity {
            id: cce_types::EntityId(id),
            name: name.to_string(),
            kind: cce_types::entity::EntityKind::Variable,
            return_type: None,
            parameters: Vec::new(),
            metadata: metadata
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            span: cce_types::Span::default(),
            ..Default::default()
        }
    }

    // ==================== extract_function_types tests ====================

    #[test]
    fn test_extract_function_types_with_return_type() {
        let entity = make_entity(1, "foo", Some("String"), vec![]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_function_types(&entity, &mut ctx);
        let rt = ctx.get_return_type(cce_types::EntityId(1)).unwrap();
        assert_eq!(rt.type_name, "String");
        assert_eq!(rt.origin, Some(InferenceOrigin::TypeAnnotation));
    }

    #[test]
    fn test_extract_function_types_without_return_type() {
        let entity = make_entity(1, "foo", None, vec![]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_function_types(&entity, &mut ctx);
        assert!(ctx.get_return_type(cce_types::EntityId(1)).is_none());
    }

    #[test]
    fn test_extract_function_types_with_typed_parameters() {
        let entity = make_entity(
            1,
            "foo",
            None,
            vec![("x", Some("int")), ("y", Some("String"))],
        );
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_function_types(&entity, &mut ctx);
        let params = ctx.get_parameter_types(cce_types::EntityId(1)).unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].type_name, "int");
        assert_eq!(params[1].type_name, "String");
    }

    #[test]
    fn test_extract_function_types_with_untyped_parameters() {
        let entity = make_entity(1, "foo", None, vec![("x", None), ("y", None)]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_function_types(&entity, &mut ctx);
        assert!(ctx.get_parameter_types(cce_types::EntityId(1)).is_none());
    }

    #[test]
    fn test_extract_function_types_mixed_parameters() {
        let entity = make_entity(1, "foo", None, vec![("x", Some("int")), ("y", None)]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_function_types(&entity, &mut ctx);
        let params = ctx.get_parameter_types(cce_types::EntityId(1)).unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].type_name, "int");
    }

    #[test]
    fn test_extract_function_types_with_shape() {
        let entity = make_entity(1, "foo", Some("List<String>"), vec![]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_function_types(&entity, &mut ctx);
        let rt = ctx.get_return_type(cce_types::EntityId(1)).unwrap();
        assert!(rt.shape.is_some());
    }

    #[test]
    fn test_extract_function_types_strips_quoted_forward_ref() {
        let entity = make_entity(1, "duplicate", Some("\"Container\""), vec![]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_function_types(&entity, &mut ctx);
        let rt = ctx.get_return_type(cce_types::EntityId(1)).unwrap();
        assert_eq!(rt.type_name, "Container");
    }

    // ==================== extract_variable_type tests ====================

    #[test]
    fn test_extract_variable_type_type_annotation() {
        let entity = make_variable_entity(1, "x", vec![("type_annotation", "String")]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_variable_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "String");
        assert_eq!(binding.origin, Some(InferenceOrigin::TypeAnnotation));
    }

    #[test]
    fn test_extract_variable_type_unrecognized_key_yields_none() {
        // Legacy `variable_type` keys are no longer produced or consumed.
        let entity = make_variable_entity(1, "x", vec![("variable_type", "int")]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_variable_type(&entity, &mut ctx);
        assert!(ctx.get_variable_type("x").is_none());
    }

    #[test]
    fn test_extract_variable_type_constructor_call() {
        let entity = make_variable_entity(1, "x", vec![("constructor_type", "MyClass")]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_variable_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "MyClass");
        assert_eq!(binding.origin, Some(InferenceOrigin::ConstructorCall));
    }

    #[test]
    fn test_extract_variable_type_literal() {
        let entity = make_variable_entity(1, "x", vec![("literal_type", "int")]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_variable_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "int");
        assert_eq!(binding.origin, Some(InferenceOrigin::LiteralType));
    }

    #[test]
    fn test_extract_variable_type_no_metadata() {
        let entity = make_variable_entity(1, "x", vec![]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_variable_type(&entity, &mut ctx);
        assert!(ctx.get_variable_type("x").is_none());
    }

    #[test]
    fn test_extract_variable_type_skips_composite_destructuring_name() {
        let entity = make_variable_entity(1, "first, second", vec![("literal_type", "array")]);
        let mut ctx = ScopedTypeContext::new(Language::TypeScript);
        extract_variable_type(&entity, &mut ctx);
        assert!(ctx.get_variable_type("first, second").is_none());
    }

    #[test]
    fn test_extract_variable_type_member_get_binds_element() {
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_variable_type(
            "names".to_string(),
            TypeBinding {
                type_name: "ArrayList<String>".to_string(),
                type_entity_id: None,
                span: cce_types::Span::default(),
                origin: Some(InferenceOrigin::ConstructorCall),
                shape: parse_type_shape("ArrayList<String>", Language::Java),
            },
        );
        let entity = make_variable_entity(1, "first", vec![("call_target", "names.get(0)")]);
        extract_variable_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("first").unwrap();
        assert_eq!(binding.type_name, "String");
        assert_eq!(binding.origin, Some(InferenceOrigin::FunctionReturn));
    }

    #[test]
    fn test_extract_variable_type_unknown_member_stays_empty() {
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_variable_type(
            "names".to_string(),
            TypeBinding {
                type_name: "ArrayList<String>".to_string(),
                type_entity_id: None,
                span: cce_types::Span::default(),
                origin: Some(InferenceOrigin::ConstructorCall),
                shape: parse_type_shape("ArrayList<String>", Language::Java),
            },
        );
        let entity = make_variable_entity(1, "x", vec![("call_target", "names.frobnicate(0)")]);
        extract_variable_type(&entity, &mut ctx);
        assert!(ctx.get_variable_type("x").is_none());
    }

    #[test]
    fn test_extract_variable_type_priority_type_annotation_over_constructor() {
        let entity = make_variable_entity(
            1,
            "x",
            vec![
                ("type_annotation", "String"),
                ("constructor_type", "MyClass"),
            ],
        );
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_variable_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "String");
        assert_eq!(binding.origin, Some(InferenceOrigin::TypeAnnotation));
    }

    #[test]
    fn test_extract_variable_type_priority_constructor_over_literal() {
        let entity = make_variable_entity(
            1,
            "x",
            vec![("constructor_type", "MyClass"), ("literal_type", "int")],
        );
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_variable_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "MyClass");
        assert_eq!(binding.origin, Some(InferenceOrigin::ConstructorCall));
    }

    // ==================== extract_field_type tests ====================

    #[test]
    fn test_extract_field_type_type_annotation() {
        let entity = make_variable_entity(1, "name", vec![("type_annotation", "String")]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_field_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("name").unwrap();
        assert_eq!(binding.type_name, "String");
        assert_eq!(binding.origin, Some(InferenceOrigin::TypeAnnotation));
    }

    #[test]
    fn test_extract_field_type_constructor_call() {
        let entity = make_variable_entity(1, "user", vec![("constructor_type", "User")]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_field_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("user").unwrap();
        assert_eq!(binding.type_name, "User");
        assert_eq!(binding.origin, Some(InferenceOrigin::ConstructorCall));
    }

    #[test]
    fn test_extract_field_type_literal() {
        let entity = make_variable_entity(1, "count", vec![("literal_type", "int")]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_field_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("count").unwrap();
        assert_eq!(binding.type_name, "int");
        assert_eq!(binding.origin, Some(InferenceOrigin::LiteralType));
    }

    #[test]
    fn test_extract_field_type_legacy_key_yields_none() {
        // Legacy `field_type` keys are no longer produced or consumed.
        let entity = make_variable_entity(1, "name", vec![("field_type", "String")]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_field_type(&entity, &mut ctx);
        assert!(ctx.get_variable_type("name").is_none());
    }

    #[test]
    fn test_extract_field_type_no_metadata() {
        let entity = make_variable_entity(1, "name", vec![]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_field_type(&entity, &mut ctx);
        assert!(ctx.get_variable_type("name").is_none());
    }

    #[test]
    fn test_extract_field_type_priority_annotation_over_constructor() {
        let entity = make_variable_entity(
            1,
            "name",
            vec![("type_annotation", "String"), ("constructor_type", "Foo")],
        );
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_field_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("name").unwrap();
        assert_eq!(binding.type_name, "String");
    }

    // ==================== try_bind_generic tests ====================

    #[test]
    fn test_try_bind_generic_single_arg() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        try_bind_generic(&mut ctx, "List<String>");
        assert_eq!(ctx.get_type_param_for_owner("List", "T"), Some("String"));
    }

    #[test]
    fn test_try_bind_generic_two_args() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        try_bind_generic(&mut ctx, "HashMap<String, Integer>");
        assert_eq!(ctx.get_type_param_for_owner("HashMap", "K"), Some("String"));
        assert_eq!(
            ctx.get_type_param_for_owner("HashMap", "V"),
            Some("Integer")
        );
    }

    #[test]
    fn test_try_bind_generic_no_generic() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        try_bind_generic(&mut ctx, "String");
    }

    #[test]
    fn test_try_bind_generic_existing_binding_not_overridden() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        ctx.bind_type_param_owned("List", "T".to_string(), "int".to_string());
        try_bind_generic(&mut ctx, "List<String>");
        assert_eq!(ctx.get_type_param_for_owner("List", "T"), Some("int"));
    }

    // ==================== extract_variable_type with generic binding ====================

    #[test]
    fn test_extract_variable_type_binds_generic() {
        let entity = make_variable_entity(1, "items", vec![("type_annotation", "List<String>")]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_variable_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("items").unwrap();
        assert_eq!(binding.type_name, "List<String>");
        assert_eq!(ctx.get_type_param_for_owner("List", "T"), Some("String"));
    }

    // ==================== annotation cleanup + keyword fallback ====================

    #[test]
    fn test_extract_variable_type_strips_colon_prefix() {
        let entity = make_variable_entity(1, "name", vec![("type_annotation", ": string")]);
        let mut ctx = ScopedTypeContext::new(Language::TypeScript);
        extract_variable_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("name").unwrap();
        assert_eq!(binding.type_name, "string");
    }

    #[test]
    fn test_extract_field_type_strips_colon_prefix() {
        let entity = make_variable_entity(1, "name", vec![("type_annotation", ": string")]);
        let mut ctx = ScopedTypeContext::new(Language::TypeScript);
        extract_field_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("name").unwrap();
        assert_eq!(binding.type_name, "string");
    }

    #[test]
    fn test_extract_variable_type_var_falls_back_to_constructor() {
        let entity = make_variable_entity(
            1,
            "scores",
            vec![
                ("type_annotation", "var"),
                ("constructor_type", "ArrayList<String>"),
            ],
        );
        let mut ctx = ScopedTypeContext::new(Language::Java);
        extract_variable_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("scores").unwrap();
        assert_eq!(binding.type_name, "ArrayList<String>");
        assert_eq!(binding.origin, Some(InferenceOrigin::ConstructorCall));
    }

    #[test]
    fn test_extract_variable_type_auto_falls_back_to_literal() {
        let entity = make_variable_entity(
            1,
            "count",
            vec![("type_annotation", "auto"), ("literal_type", "int")],
        );
        let mut ctx = ScopedTypeContext::new(Language::Cpp);
        extract_variable_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("count").unwrap();
        assert_eq!(binding.type_name, "int");
        assert_eq!(binding.origin, Some(InferenceOrigin::LiteralType));
    }

    #[test]
    fn test_extract_variable_type_decltype_falls_back_to_call_target() {
        let mut ctx = ScopedTypeContext::new(Language::Cpp);
        let func = make_entity(9, "make_value", Some("int"), vec![]);
        extract_function_types(&func, &mut ctx);
        let entity = make_variable_entity(
            1,
            "other",
            vec![
                ("type_annotation", "decltype(count)"),
                ("call_target", "make_value()"),
            ],
        );
        extract_variable_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("other").unwrap();
        assert_eq!(binding.type_name, "int");
        assert_eq!(binding.origin, Some(InferenceOrigin::FunctionReturn));
    }

    #[test]
    fn test_extract_variable_type_bare_keyword_yields_no_binding() {
        let entity = make_variable_entity(1, "x", vec![("type_annotation", "var")]);
        let mut ctx = ScopedTypeContext::new(Language::Java);
        extract_variable_type(&entity, &mut ctx);
        assert!(ctx.get_variable_type("x").is_none());
    }

    #[test]
    fn test_constructor_call_site_instantiation() {
        // `p = Pair(1, "one")` on `Pair<A, B>` yields `Pair<Int, String>`.
        let mut ctx = ScopedTypeContext::new(Language::Scala);
        let entity = make_variable_entity(
            1,
            "p",
            vec![
                ("constructor_type", "Pair<A, B>"),
                ("call_target", "Pair(1, \"one\")"),
            ],
        );
        extract_variable_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("p").unwrap();
        assert_eq!(binding.type_name, "Pair<Int, String>");
        assert_eq!(binding.origin, Some(InferenceOrigin::ConstructorCall));
    }

    #[test]
    fn test_constructor_explicit_args_keep_reading() {
        // Explicitly parameterized constructors are not re-substituted.
        let mut ctx = ScopedTypeContext::new(Language::Java);
        let entity =
            make_variable_entity(1, "names", vec![("constructor_type", "ArrayList<String>")]);
        extract_variable_type(&entity, &mut ctx);
        assert_eq!(
            ctx.get_variable_type("names").unwrap().type_name,
            "ArrayList<String>"
        );
    }

    #[test]
    fn test_go_slice_prefix_normalizes_to_shape_spelling() {
        let entity = make_entity(1, "wrapInSlice", Some("[]T"), vec![]);
        let mut ctx = ScopedTypeContext::new(Language::Go);
        extract_function_types(&entity, &mut ctx);
        let rt = ctx.get_return_type(cce_types::EntityId(1)).unwrap();
        assert_eq!(rt.type_name, "T[]");
        assert_eq!(
            rt.shape.as_ref().map(type_shape_to_string),
            Some("T[]".to_string())
        );
    }

    #[test]
    fn test_constructor_unknown_args_keep_reading() {
        // Unresolvable arguments keep the recorded generic reading.
        let mut ctx = ScopedTypeContext::new(Language::Scala);
        let entity = make_variable_entity(
            1,
            "p",
            vec![
                ("constructor_type", "Pair<A, B>"),
                ("call_target", "Pair(x, y)"),
            ],
        );
        extract_variable_type(&entity, &mut ctx);
        assert_eq!(ctx.get_variable_type("p").unwrap().type_name, "Pair<A, B>");
    }
}
