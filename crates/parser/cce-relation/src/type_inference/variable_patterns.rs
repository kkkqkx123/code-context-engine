//! Variable pattern inference for destructuring and assignment types.
//!
//! Analyses variable entities with comma-separated names (destructuring
//! patterns), single pattern-bound variables, and struct-pattern
//! assignments to produce type bindings. This module is extracted from
//! `TypeInferenceEngine::infer_variable_patterns` to keep the engine
//! module focused on dispatch and public API.

use cce_types::entity::EntityKind;
use cce_types::language::Language;

use super::types::NestedPatternPart;
use super::types::{
    InferenceOrigin, ScopedTypeContext, TypeBinding, TypeShape, origin_supersedes,
    parse_type_shape, type_shape_to_string,
};

/// A same-file callable's formal parameters and return annotation.
type CalleeSignature<'a> = (&'a [(String, Option<String>)], Option<&'a str>);

/// Check whether a bare name is shaped like a type expression
/// (`ValueError`, `User`) rather than a value reference (`pair`, `items`).
fn looks_like_type_name(name: &str) -> bool {
    let mut chars = name.trim().chars();
    match chars.next() {
        Some(c) if c.is_uppercase() => (),
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Collect member name → type for a named interface, class, struct or
/// type alias declared in the same file.
///
/// Membership is decided by span containment, so object-literal members
/// outside the declaration never leak in. A leading colon in captured
/// annotation text is stripped. Returns `None` when the type is not
/// declared locally or carries no member types.
fn named_type_members(
    file: &cce_types::ParsedFile,
    type_name: &str,
) -> Option<std::collections::HashMap<String, String>> {
    let owner = file.entities.iter().find(|e| {
        e.name == type_name
            && matches!(
                e.kind,
                EntityKind::Interface
                    | EntityKind::Class
                    | EntityKind::Struct
                    | EntityKind::TypeAlias
            )
    })?;
    let mut members = std::collections::HashMap::new();
    for member in &file.entities {
        if !matches!(member.kind, EntityKind::Property | EntityKind::Field) {
            continue;
        }
        if member.span.start_byte < owner.span.start_byte
            || member.span.end_byte > owner.span.end_byte
        {
            continue;
        }
        let ty = member
            .metadata
            .get("type_annotation")
            .or_else(|| member.metadata.get("explicit_type"))?;
        let ty = ty.trim().trim_start_matches(':').trim();
        if ty.is_empty() {
            continue;
        }
        members.insert(member.name.clone(), ty.to_string());
    }
    if members.is_empty() {
        return None;
    }
    Some(members)
}

/// Bind top-level destructured names through same-file member types.
fn bind_member_names(
    ctx: &mut ScopedTypeContext,
    file: &cce_types::ParsedFile,
    entity: &cce_types::Entity,
    members: &std::collections::HashMap<String, String>,
    names: &[String],
) {
    for part in names {
        if let Some(member_ty) = members.get(part) {
            ctx.add_variable_type(
                part.clone(),
                TypeBinding {
                    type_name: member_ty.clone(),
                    type_entity_id: None,
                    span: entity.span,
                    origin: Some(InferenceOrigin::DestructuringAssignment),
                    shape: parse_type_shape(member_ty, file.language),
                },
            );
        }
    }
}

/// Recover the nested destructuring pattern of a multi-binding entity.
///
/// Slices the statement source by the entity span, keeps the assignment
/// left-hand side, and parses grouping. Returns `None` when any step
/// fails so the caller keeps the existing flat mapping.
fn nested_pattern_parts(
    file: &cce_types::ParsedFile,
    entity: &cce_types::Entity,
) -> Option<Vec<NestedPatternPart>> {
    let text = file
        .source
        .get(entity.span.start_byte..entity.span.end_byte)?;
    let lhs = split_assignment_lhs(text)?;
    parse_nested_pattern_list(lhs)
}

/// Look up a parameter type in the closest enclosing function scope.
///
/// Uses span containment (smallest enclosing function) rather than the
/// `parent` link, which typically points at the module for locals.
fn enclosing_param_type<'a>(
    file: &'a cce_types::ParsedFile,
    entity: &cce_types::Entity,
    name: &str,
) -> Option<&'a str> {
    file.entities
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                EntityKind::Function | EntityKind::Method | EntityKind::Constructor
            ) && e.span.contains(&entity.span)
        })
        .min_by_key(|e| e.span.end_byte - e.span.start_byte)?
        .parameters
        .iter()
        .find(|(n, _)| n == name)
        .and_then(|(_, ty)| ty.as_deref())
}

/// Find a same-file callable (function, method or constructor) by name.
///
/// Matches the stripped callee name (`obj.method` resolves against the
/// `method` member); the first match wins. Method receivers are not
/// disambiguated here — generic substitution only needs the formal
/// parameter and return annotations.
fn find_callee_signature<'a>(
    file: &'a cce_types::ParsedFile,
    name: &str,
) -> Option<CalleeSignature<'a>> {
    let simple = crate::type_inference::call_utils::simple_callee_name(name);
    file.entities
        .iter()
        .find(|e| {
            matches!(
                e.kind,
                EntityKind::Function | EntityKind::Method | EntityKind::Constructor
            ) && e.name == simple
        })
        .map(|e| (e.parameters.as_slice(), e.return_type.as_deref()))
}

/// Infer the type shape of one call-site argument expression.
///
/// Bare identifiers resolve against enclosing-function parameters first
/// (so `identity(x)` with `x: number` binds `T = number`), then fall
/// back to literals, constructor bases and known variable bindings via
/// the shared cross-file argument resolver.
fn infer_call_arg_shape(
    file: &cce_types::ParsedFile,
    entity: &cce_types::Entity,
    ctx: &ScopedTypeContext,
    arg: &str,
) -> Option<TypeShape> {
    let trimmed = arg.trim();
    if !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        && !trimmed.chars().next().is_some_and(|c| c.is_ascii_digit())
        && let Some(param_ty) = enclosing_param_type(file, entity, trimmed)
        && let Some(shape) = parse_type_shape(param_ty, file.language)
    {
        return Some(shape);
    }
    crate::type_inference::cross_file::infer_arg_shape(ctx, file.language, arg)
}

/// Substitute a same-file generic call return using call-site arguments.
///
/// For `y = identity(42)` with `identity<T>(x: T): T` in the same file,
/// binds `T = number` from the argument and returns the substituted
/// `number` shape. Returns `None` when the source is not a call with
/// arguments, the callee is unknown, its return mentions no type
/// parameter, or the substitution is not fully concrete — callers keep
/// their existing unsubstituted fallback in all those cases.
fn resolve_generic_call_shape(
    file: &cce_types::ParsedFile,
    entity: &cce_types::Entity,
    ctx: &ScopedTypeContext,
    source: &str,
) -> Option<TypeShape> {
    use crate::type_inference::call_utils::split_call_target;
    use crate::type_inference::generics::{shape_contains_param, substitute_call_return_type};
    if !source.contains('(') {
        return None;
    }
    let (name, args) = split_call_target(source);
    if args.is_empty() {
        return None;
    }
    let (params, ret) = find_callee_signature(file, &name)?;
    let return_text = ret?;
    let return_shape = parse_type_shape(return_text, file.language)?;
    if is_identity_any_shape(&return_shape) {
        return refine_identity_any_arg(file, entity, ctx, &args);
    }
    if !shape_contains_param(&return_shape) {
        return None;
    }
    let formal_shapes: Vec<TypeShape> = params
        .iter()
        .map(|(_, ty)| {
            ty.as_deref()
                .and_then(|text| parse_type_shape(text, file.language))
                .unwrap_or(TypeShape::Named("unknown".to_string()))
        })
        .collect();
    let actual_shapes: Vec<Option<TypeShape>> = args
        .iter()
        .map(|arg| infer_call_arg_shape(file, entity, ctx, arg))
        .collect();
    let actual_refs: Vec<Option<&TypeShape>> =
        actual_shapes.iter().map(|opt| opt.as_ref()).collect();
    let substituted =
        substitute_call_return_type(&formal_shapes, &return_shape, &actual_refs, file.language)?;
    if shape_contains_param(&substituted) {
        return None;
    }
    Some(substituted)
}

/// Whether a same-file return shape is a bare dynamic passthrough.
fn is_identity_any_shape(shape: &TypeShape) -> bool {
    match shape {
        TypeShape::Named(name) => {
            let normalized = name.trim();
            normalized == "Any" || normalized == "any"
        }
        _ => false,
    }
}

/// Refine a bare `Any` same-file return from a single known argument.
///
/// Only single-argument calls refine; multi-argument dynamic returns stay
/// conservative since the passthrough position is ambiguous.
fn refine_identity_any_arg(
    file: &cce_types::ParsedFile,
    entity: &cce_types::Entity,
    ctx: &ScopedTypeContext,
    args: &[String],
) -> Option<TypeShape> {
    use crate::type_inference::generics::shape_contains_param;
    let [single] = args else {
        return None;
    };
    let shape = infer_call_arg_shape(file, entity, ctx, single.trim())?;
    if matches!(&shape, TypeShape::Named(name) if name == "unknown") {
        return None;
    }
    if shape_contains_param(&shape) {
        return None;
    }
    Some(shape)
}

/// Strip a stored call target to its callee name for name lookups.
///
/// Stored targets may carry an argument list (`foo(a, b)`); lookups
/// against return tables and variable bindings use `foo`.
fn strip_call_target_name(source: &str) -> String {
    match source.find('(') {
        Some(pos) => source[..pos].trim().to_string(),
        None => source.to_string(),
    }
}

/// Resolve a stored call target through the overload-aware same-file index.
///
/// Parses the call-site argument expressions, infers their shapes, and
/// ranks same-name overloads via [`ScopedTypeContext::resolve_return_by_name`].
/// Returns the winning return name (source spelling) and shape (or the
/// overload-union name/shape when resolution is ambiguous). Returns
/// `None` when the target carries no arguments, no argument shape is
/// known, or the callee is unknown, so callers keep their
/// collapsed-name fallback.
fn resolve_same_file_overload_shape(
    file: &cce_types::ParsedFile,
    entity: &cce_types::Entity,
    ctx: &ScopedTypeContext,
    source: &str,
) -> Option<(String, TypeShape)> {
    use crate::type_inference::call_utils::split_call_target;
    let (name, args) = split_call_target(source);
    if args.is_empty() {
        return None;
    }
    let arg_shapes: Vec<Option<TypeShape>> = args
        .iter()
        .map(|arg| infer_call_arg_shape(file, entity, ctx, arg))
        .collect();
    if !arg_shapes.iter().any(Option::is_some) {
        return None;
    }
    let binding = ctx.resolve_return_by_name(&name, &arg_shapes, file.language)?;
    let shape = binding
        .shape
        .clone()
        .or_else(|| parse_type_shape(&binding.type_name, file.language))?;
    Some((binding.type_name.clone(), shape))
}

/// Resolve a Scala collection-factory call (`List(User(...))`) to its
/// element-instantiated shape.
///
/// Shares the extractor collection logic so the variable-pattern pass
/// keeps the precise binding instead of degrading to the bare
/// constructor-name fallback. Returns `None` for non-factory calls or
/// mixed/unknown elements.
fn resolve_collection_factory_shape(
    file: &cce_types::ParsedFile,
    entity: &cce_types::Entity,
    ctx: &ScopedTypeContext,
    source: &str,
) -> Option<(String, TypeShape)> {
    use crate::type_inference::generics::resolve_collection_factory_shape;
    resolve_collection_factory_shape(file.language, source, |arg| {
        infer_call_arg_shape(file, entity, ctx, arg)
    })
}

/// Resolve a destructuring-source expression to a concrete [`TypeShape`].
///
/// identifier sources resolve against (in order) enclosing-function
/// parameters, already-known variable bindings, and same-file function
/// return types; a bare type-shaped name (`ValueError`) resolves to
/// itself. The bare-name guess is skipped for failed call targets so a
/// constructor binding is never clobbered by its own base name.
/// Returns `None` when the source carries no usable type.
fn resolve_source_shape(
    file: &cce_types::ParsedFile,
    returns_by_name: &std::collections::HashMap<&str, &str>,
    entity: &cce_types::Entity,
    ctx: &ScopedTypeContext,
    source: &str,
    allow_bare_type_guess: bool,
) -> Option<TypeShape> {
    let shape = parse_type_shape(source, file.language)?;
    let name = match &shape {
        TypeShape::Named(id) => id.clone(),
        _ => return Some(shape),
    };
    if let Some(param_ty) = enclosing_param_type(file, entity, &name) {
        return parse_type_shape(param_ty, file.language);
    }
    if let Some(binding) = ctx.get_variable_type(&name) {
        return parse_type_shape(&binding.type_name, file.language);
    }
    if let Some(ret) = returns_by_name.get(name.as_str()) {
        return parse_type_shape(ret, file.language);
    }
    if allow_bare_type_guess && looks_like_type_name(&name) {
        return Some(shape);
    }
    // Array-literal destructuring sources
    if matches!(file.language, Language::JavaScript | Language::TypeScript)
        && let Some(TypeShape::Array(element)) =
            crate::type_inference::cross_file::infer_arg_shape(ctx, file.language, source)
        && !matches!(&*element, TypeShape::Named(name) if name == "unknown")
    {
        return Some(TypeShape::Array(element));
    }
    None
}

/// Infer type bindings for variable entities with pattern-based assignments.
///
/// Handles destructuring patterns (`a, b = f()`), single pattern-bound
/// variables (`except E as e`), struct patterns (`{ x, y } = Point()`),
/// and field assignments from bare identifiers (`self.x = x`).
pub fn infer_variable_patterns(file: &cce_types::ParsedFile, ctx: &mut ScopedTypeContext) {
    let returns_by_name: std::collections::HashMap<&str, &str> = file
        .entities
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                EntityKind::Function | EntityKind::Method | EntityKind::Constructor
            )
        })
        .filter_map(|e| e.return_type.as_deref().map(|r| (e.name.as_str(), r)))
        .collect();
    for entity in &file.entities {
        if matches!(entity.kind, EntityKind::Field | EntityKind::Property) {
            if ctx.get_variable_type(&entity.name).is_none()
                && let Some(source) = entity.metadata.get("source_type")
                && let Some(shape) =
                    resolve_source_shape(file, &returns_by_name, entity, ctx, source, true)
            {
                let type_name = type_shape_to_string(&shape);
                ctx.add_variable_type(
                    entity.name.clone(),
                    TypeBinding {
                        type_name,
                        type_entity_id: None,
                        span: entity.span,
                        origin: Some(InferenceOrigin::TypeAnnotation),
                        shape: Some(shape),
                    },
                );
            }
            continue;
        }
        if entity.kind != EntityKind::Variable {
            continue;
        }
        if entity.name.contains(',') {
            let parts: Vec<String> = entity
                .name
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if parts.len() > 1 {
                let mut resolved = None;
                for key in ["source_type", "call_target"] {
                    if let Some(candidate) = entity.metadata.get(key) {
                        if key == "call_target"
                            && let Some(shape) =
                                resolve_generic_call_shape(file, entity, ctx, candidate)
                        {
                            resolved = Some(shape);
                            break;
                        }
                        if key == "call_target"
                            && let Some((_, shape)) =
                                resolve_same_file_overload_shape(file, entity, ctx, candidate)
                        {
                            resolved = Some(shape);
                            break;
                        }
                        let lookup = if key == "call_target" {
                            strip_call_target_name(candidate)
                        } else {
                            candidate.clone()
                        };
                        if let Some(shape) = resolve_source_shape(
                            file,
                            &returns_by_name,
                            entity,
                            ctx,
                            &lookup,
                            key != "call_target",
                        ) {
                            resolved = Some(shape);
                            break;
                        }
                    }
                }
                if let Some(mut shape) = resolved {
                    if matches!(entity.subtype.as_deref(), Some("case") | Some("loop")) {
                        match crate::type_inference::types::element_type_at_depth(&shape, 1) {
                            Some(element) => shape = element,
                            None => continue,
                        }
                    }
                    if let TypeShape::Named(type_name) = &shape {
                        if let Some(members) = named_type_members(file, type_name) {
                            bind_member_names(ctx, file, entity, &members, &parts);
                            continue;
                        }
                    }
                    if let Some(nested) = nested_pattern_parts(file, entity) {
                        if nested
                            .iter()
                            .any(|part| matches!(part, NestedPatternPart::Group(_)))
                        {
                            ctx.add_nested_destructuring_binding(&nested, &shape, entity.span);
                            continue;
                        }
                    }
                    let pattern = crate::type_inference::types::Pattern::Tuple(parts.clone());
                    ctx.add_pattern_match_binding(&pattern, &shape, entity.span);
                    for (i, part) in parts.iter().enumerate() {
                        ctx.add_destructuring_binding(part, &shape, Some(i), entity.span);
                    }
                    continue;
                }
                if let Some(init_type) = entity.metadata.get("constructor_type") {
                    if let Some(members) = named_type_members(file, init_type.trim()) {
                        bind_member_names(ctx, file, entity, &members, &parts);
                        continue;
                    }
                }
                let generic = TypeShape::Generic {
                    base: "Tuple".to_string(),
                    args: vec![TypeShape::Named("unknown".to_string()); parts.len()],
                };
                let pattern = crate::type_inference::types::Pattern::Tuple(parts);
                ctx.add_pattern_match_binding(&pattern, &generic, entity.span);
            }
        } else if entity.metadata.contains_key("source_type")
            || entity.metadata.contains_key("call_target")
        {
            let is_element_binding =
                matches!(entity.subtype.as_deref(), Some("case") | Some("loop"));
            if is_element_binding {
                for key in ["source_type", "call_target"] {
                    let Some(source_str) = entity.metadata.get(key) else {
                        continue;
                    };
                    let shape = if key == "call_target" {
                        resolve_generic_call_shape(file, entity, ctx, source_str).or_else(|| {
                            let lookup = strip_call_target_name(source_str);
                            resolve_source_shape(
                                file,
                                &returns_by_name,
                                entity,
                                ctx,
                                &lookup,
                                false,
                            )
                        })
                    } else {
                        resolve_source_shape(file, &returns_by_name, entity, ctx, source_str, true)
                    };
                    let Some(shape) = shape else {
                        continue;
                    };
                    let Some(element) =
                        crate::type_inference::types::element_type_at_depth(&shape, 1)
                    else {
                        continue;
                    };
                    let type_name = type_shape_to_string(&element);
                    let keep = ctx.get_variable_type(&entity.name).is_none_or(|existing| {
                        origin_supersedes(InferenceOrigin::DestructuringAssignment, existing.origin)
                    });
                    if keep {
                        ctx.add_variable_type(
                            entity.name.clone(),
                            TypeBinding {
                                type_name,
                                type_entity_id: None,
                                span: entity.span,
                                origin: Some(InferenceOrigin::DestructuringAssignment),
                                shape: Some(element),
                            },
                        );
                    }
                    break;
                }
                continue;
            }
            for key in ["source_type", "call_target"] {
                let Some(source_str) = entity.metadata.get(key) else {
                    continue;
                };
                let from_call =
                    key == "call_target" && !entity.metadata.contains_key("source_type");
                let (shape, from_substitution, from_overload, from_collection, resolved_name) =
                    if from_call {
                        match resolve_generic_call_shape(file, entity, ctx, source_str) {
                            Some(shape) => (Some(shape), true, false, false, None),
                            None => match resolve_collection_factory_shape(
                                file, entity, ctx, source_str,
                            ) {
                                Some((type_name, shape)) => {
                                    (Some(shape), false, false, true, Some(type_name))
                                }
                                None => match resolve_same_file_overload_shape(
                                    file, entity, ctx, source_str,
                                ) {
                                    Some((type_name, shape)) => {
                                        (Some(shape), false, true, false, Some(type_name))
                                    }
                                    None => {
                                        let lookup = strip_call_target_name(source_str);
                                        (
                                            resolve_source_shape(
                                                file,
                                                &returns_by_name,
                                                entity,
                                                ctx,
                                                &lookup,
                                                false,
                                            ),
                                            false,
                                            false,
                                            false,
                                            None,
                                        )
                                    }
                                },
                            },
                        }
                    } else {
                        (
                            resolve_source_shape(
                                file,
                                &returns_by_name,
                                entity,
                                ctx,
                                source_str,
                                true,
                            ),
                            false,
                            false,
                            false,
                            None,
                        )
                    };
                if let Some(shape) = shape {
                    let bindable = matches!(&shape, TypeShape::Named(_))
                        || (from_call && matches!(&shape, TypeShape::Union(_)))
                        || ((from_substitution || from_overload || from_collection)
                            && !crate::type_inference::generics::shape_contains_param(&shape));
                    if bindable {
                        let type_name =
                            resolved_name.unwrap_or_else(|| type_shape_to_string(&shape));
                        let origin = if from_collection {
                            InferenceOrigin::ConstructorCall
                        } else if from_call {
                            InferenceOrigin::FunctionReturn
                        } else {
                            InferenceOrigin::DestructuringAssignment
                        };
                        let keep = ctx
                            .get_variable_type(&entity.name)
                            .is_none_or(|existing| origin_supersedes(origin, existing.origin));
                        if keep {
                            ctx.add_variable_type(
                                entity.name.clone(),
                                TypeBinding {
                                    type_name: type_name.clone(),
                                    type_entity_id: None,
                                    span: entity.span,
                                    origin: Some(origin),
                                    shape: Some(shape),
                                },
                            );
                        }
                        break;
                    }
                }
            }
        } else if entity.name.trim().starts_with('{')
            || entity.metadata.contains_key("pattern_struct")
        {
            if let Some(fields_str) = entity.metadata.get("pattern_fields") {
                let fields: Vec<String> = fields_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                if !fields.is_empty() {
                    let shape = entity
                        .metadata
                        .get("source_type")
                        .and_then(|s| parse_type_shape(s, file.language))
                        .unwrap_or(TypeShape::Named("unknown".to_string()));
                    let pattern = crate::type_inference::types::Pattern::Struct(fields);
                    ctx.add_pattern_match_binding(&pattern, &shape, entity.span);
                }
            }
        }
    }
}

// ─── Pattern parsing helpers ────────────────────────────────────────────────

/// Left-hand side of the first top-level assignment in a statement.
///
/// Skips `==`, `!=`, `=>`, `<=` and `>=`, ignores nesting and quoted
/// regions, and strips declaration keywords plus a trailing top-level
/// type annotation. Returns `None` when no plain assignment is found.
fn split_assignment_lhs(statement: &str) -> Option<&str> {
    let bytes = statement.as_bytes();
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut brace = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' | b'\'' | b'`' => {
                i = skip_pattern_quoted(bytes, i);
                continue;
            }
            b'(' => paren += 1,
            b')' => paren = paren.saturating_sub(1),
            b'[' => bracket += 1,
            b']' => bracket = bracket.saturating_sub(1),
            b'{' => brace += 1,
            b'}' => brace = brace.saturating_sub(1),
            b'=' if paren == 0 && bracket == 0 && brace == 0 => {
                let prev = if i > 0 { bytes[i - 1] } else { 0 };
                let next = bytes.get(i + 1).copied().unwrap_or(0);
                if prev == b'=' || prev == b'!' || prev == b'<' || prev == b'>' {
                    i += 1;
                    continue;
                }
                if next == b'=' || next == b'>' {
                    i += 1;
                    continue;
                }
                return Some(strip_pattern_affixes(statement[..i].trim()));
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Strip declaration keywords and a trailing top-level type annotation.
fn strip_pattern_affixes(lhs: &str) -> &str {
    let mut rest = lhs.trim();
    for prefix in ["let mut ", "let ", "val ", "var ", "const "] {
        if let Some(stripped) = rest.strip_prefix(prefix) {
            rest = stripped.trim();
            break;
        }
    }
    let bytes = rest.as_bytes();
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut brace = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' | b'\'' | b'`' => {
                i = skip_pattern_quoted(bytes, i);
                continue;
            }
            b'(' => paren += 1,
            b')' => paren = paren.saturating_sub(1),
            b'[' => bracket += 1,
            b']' => bracket = bracket.saturating_sub(1),
            b'{' => brace += 1,
            b'}' => brace = brace.saturating_sub(1),
            b':' if paren == 0 && bracket == 0 && brace == 0 => {
                let prev = if i > 0 { bytes[i - 1] } else { 0 };
                let next = bytes.get(i + 1).copied().unwrap_or(0);
                if prev != b':' && next != b':' {
                    return rest[..i].trim();
                }
            }
            _ => {}
        }
        i += 1;
    }
    rest
}

/// Parse a comma-separated destructuring pattern with grouping.
fn parse_nested_pattern_list(text: &str) -> Option<Vec<NestedPatternPart>> {
    let mut parts = Vec::new();
    for item in split_top_level_commas(text)? {
        parts.push(parse_nested_pattern_part(item.trim())?);
    }
    if parts.is_empty() {
        return None;
    }
    while parts.len() == 1 {
        let inner = match parts.first() {
            Some(NestedPatternPart::Group(inner)) => inner.clone(),
            _ => break,
        };
        parts = inner;
    }
    Some(parts)
}

/// Parse one destructuring element: placeholder, group or plain name.
fn parse_nested_pattern_part(text: &str) -> Option<NestedPatternPart> {
    let text = text.trim();
    if text == "_" {
        return Some(NestedPatternPart::Wildcard);
    }
    let mut rest = text;
    for prefix in ["mut ", "ref "] {
        if let Some(stripped) = rest.strip_prefix(prefix) {
            rest = stripped.trim();
            break;
        }
    }
    if rest.starts_with('(') && rest.ends_with(')') && is_fully_wrapped(rest) {
        return parse_nested_pattern_list(&rest[1..rest.len() - 1]).map(NestedPatternPart::Group);
    }
    if is_pattern_ident(rest) {
        return Some(NestedPatternPart::Name(rest.to_string()));
    }
    None
}

/// Whether the outer parentheses wrap the whole text.
fn is_fully_wrapped(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    for (i, byte) in bytes.iter().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && i != bytes.len() - 1 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Split on depth-zero commas; `None` on unbalanced nesting or quotes.
fn split_top_level_commas(text: &str) -> Option<Vec<&str>> {
    let bytes = text.as_bytes();
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut brace = 0usize;
    let mut start = 0usize;
    let mut items = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' | b'\'' | b'`' => {
                i = skip_pattern_quoted(bytes, i);
                continue;
            }
            b'(' => paren += 1,
            b')' => {
                if paren == 0 {
                    return None;
                }
                paren -= 1;
            }
            b'[' => bracket += 1,
            b']' => {
                if bracket == 0 {
                    return None;
                }
                bracket -= 1;
            }
            b'{' => brace += 1,
            b'}' => {
                if brace == 0 {
                    return None;
                }
                brace -= 1;
            }
            b',' if paren == 0 && bracket == 0 && brace == 0 => {
                items.push(text[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if paren != 0 || bracket != 0 || brace != 0 {
        return None;
    }
    items.push(text[start..].trim());
    Some(items)
}

/// Advance past a quoted region starting at the quote character.
fn skip_pattern_quoted(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut j = start + 1;
    while j < bytes.len() {
        if bytes[j] == b'\\' {
            j += 2;
            continue;
        }
        if bytes[j] == quote {
            return j + 1;
        }
        j += 1;
    }
    j
}

/// Plain identifier check for recovered pattern names.
fn is_pattern_ident(text: &str) -> bool {
    crate::type_inference::control_flow::shared::is_valid_ident(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;
    use cce_types::entity::{Entity, EntityId, EntityKind};
    use cce_types::{Language, ParsedFile};

    #[test]
    fn test_parse_nested_pattern_list_flat_and_grouped() {
        let parts = parse_nested_pattern_list("a, (b, c)").expect("nested pattern");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], NestedPatternPart::Name("a".to_string()));
        assert_eq!(
            parts[1],
            NestedPatternPart::Group(vec![
                NestedPatternPart::Name("b".to_string()),
                NestedPatternPart::Name("c".to_string()),
            ])
        );
    }

    #[test]
    fn test_parse_nested_pattern_list_unwraps_lone_group() {
        let parts = parse_nested_pattern_list("(a, (b, c))").expect("unwrapped pattern");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], NestedPatternPart::Name("a".to_string()));
        assert!(matches!(parts[1], NestedPatternPart::Group(_)));
    }

    #[test]
    fn test_parse_nested_pattern_list_rejects_broken_input() {
        assert!(parse_nested_pattern_list("a, (b, c").is_none());
        assert!(parse_nested_pattern_list("a, foo(1)").is_none());
        assert!(parse_nested_pattern_list("").is_none());
        assert_eq!(
            parse_nested_pattern_list("_, b").expect("wildcard pattern"),
            vec![
                NestedPatternPart::Wildcard,
                NestedPatternPart::Name("b".to_string()),
            ]
        );
    }

    #[test]
    fn test_split_assignment_lhs_skips_comparisons() {
        assert_eq!(
            split_assignment_lhs("a, (b, c) = make()").expect("test assignment must split"),
            "a, (b, c)"
        );
        assert_eq!(
            split_assignment_lhs("let (a, (b, c)): Pair = make();")
                .expect("test assignment must split"),
            "(a, (b, c))"
        );
        assert!(split_assignment_lhs("a == b").is_none());
    }

    #[test]
    fn test_nested_destructuring_end_to_end() {
        let source = "a, (b, c) = make()";
        let mut file = ParsedFile::new(Language::Python, "demo.py".to_string(), source);
        let mut maker = Entity::new(
            EntityId(1),
            EntityKind::Function,
            "make".to_string(),
            Span {
                start_byte: 0,
                end_byte: source.len(),
                ..Span::default()
            },
        );
        maker.return_type = Some("Tuple[str, Tuple[int, bool]]".to_string());
        file.entities.push(maker);
        let mut multi = Entity::new(
            EntityId(2),
            EntityKind::Variable,
            "a, b, c".to_string(),
            Span {
                start_byte: 0,
                end_byte: source.len(),
                ..Span::default()
            },
        );
        multi
            .metadata
            .insert("call_target".to_string(), "make".to_string());
        file.entities.push(multi);

        let mut ctx = ScopedTypeContext::new(Language::Python);
        infer_variable_patterns(&file, &mut ctx);
        assert_eq!(
            ctx.get_variable_type("a")
                .expect("variable 'a' must be bound")
                .type_name,
            "str"
        );
        assert_eq!(
            ctx.get_variable_type("b")
                .expect("variable 'b' must be bound")
                .type_name,
            "int"
        );
        assert_eq!(
            ctx.get_variable_type("c")
                .expect("variable 'c' must be bound")
                .type_name,
            "bool"
        );
    }

    #[test]
    fn test_generic_call_substitution_end_to_end() {
        let source = "function identity<T>(x: T): T { return x; }\nconst y = identity(42);\nconst w = wrapInArray(\"a\");";
        let mut file = ParsedFile::new(Language::TypeScript, "demo.ts".to_string(), source);
        let mut identity = Entity::new(
            EntityId(1),
            EntityKind::Function,
            "identity".to_string(),
            Span {
                start_byte: 0,
                end_byte: 10,
                ..Span::default()
            },
        );
        identity.parameters = vec![("x".to_string(), Some("T".to_string()))];
        identity.return_type = Some("T".to_string());
        file.entities.push(identity);
        let mut wrap = Entity::new(
            EntityId(2),
            EntityKind::Function,
            "wrapInArray".to_string(),
            Span {
                start_byte: 0,
                end_byte: 10,
                ..Span::default()
            },
        );
        wrap.parameters = vec![("item".to_string(), Some("T".to_string()))];
        wrap.return_type = Some("Array<T>".to_string());
        file.entities.push(wrap);
        let mut y = Entity::new(
            EntityId(3),
            EntityKind::Variable,
            "y".to_string(),
            Span {
                start_byte: 45,
                end_byte: 46,
                ..Span::default()
            },
        );
        y.metadata
            .insert("call_target".to_string(), "identity(42)".to_string());
        file.entities.push(y);
        let mut w = Entity::new(
            EntityId(4),
            EntityKind::Variable,
            "w".to_string(),
            Span {
                start_byte: 60,
                end_byte: 61,
                ..Span::default()
            },
        );
        w.metadata
            .insert("call_target".to_string(), "wrapInArray(\"a\")".to_string());
        file.entities.push(w);

        let mut ctx = ScopedTypeContext::new(Language::TypeScript);
        infer_variable_patterns(&file, &mut ctx);
        assert_eq!(
            ctx.get_variable_type("y")
                .expect("variable 'y' must be bound")
                .type_name,
            "number"
        );
        assert_eq!(
            ctx.get_variable_type("w")
                .expect("variable 'w' must be bound")
                .type_name,
            "Array<string>"
        );
    }

    #[test]
    fn test_any_return_refines_from_single_arg() {
        let source = "def identity(x: Any) -> Any:\n    return x\ny = identity(42)";
        let mut file = ParsedFile::new(Language::Python, "demo.py".to_string(), source);
        let mut identity = Entity::new(
            EntityId(1),
            EntityKind::Function,
            "identity".to_string(),
            Span {
                start_byte: 0,
                end_byte: 10,
                ..Span::default()
            },
        );
        identity.parameters = vec![("x".to_string(), Some("Any".to_string()))];
        identity.return_type = Some("Any".to_string());
        file.entities.push(identity);
        let mut y = Entity::new(
            EntityId(2),
            EntityKind::Variable,
            "y".to_string(),
            Span {
                start_byte: 45,
                end_byte: 46,
                ..Span::default()
            },
        );
        y.metadata
            .insert("call_target".to_string(), "identity(42)".to_string());
        file.entities.push(y);

        let mut ctx = ScopedTypeContext::new(Language::Python);
        infer_variable_patterns(&file, &mut ctx);
        assert_eq!(
            ctx.get_variable_type("y")
                .expect("variable 'y' must be bound")
                .type_name,
            "int"
        );
    }

    #[test]
    fn test_failed_call_fallback_does_not_guess_bare_type() {
        // `v = Widget(1)` with no resolvable `Widget` must not bind the
        // bare constructor name; the constructor reading owns it.
        let source = "v = Widget(1)";
        let mut file = ParsedFile::new(Language::Python, "demo.py".to_string(), source);
        let mut v = Entity::new(
            EntityId(3),
            EntityKind::Variable,
            "v".to_string(),
            Span {
                start_byte: 0,
                end_byte: 1,
                ..Span::default()
            },
        );
        v.metadata
            .insert("call_target".to_string(), "Widget(1)".to_string());
        file.entities.push(v);

        let mut ctx = ScopedTypeContext::new(Language::Python);
        infer_variable_patterns(&file, &mut ctx);
        assert!(ctx.get_variable_type("v").is_none());
    }
}
