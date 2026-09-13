use cce_types::entity::EntityId;

use super::super::call_utils::split_call_target;
use super::super::generics::{shape_contains_param, substitute_call_return_type};
use super::super::propagator::CrossFilePropagator;
use super::super::types::{TypeBinding, TypeShape, parse_type_shape, type_shape_to_string};

/// Refine a generic callee return type using call-site argument expressions.
///
/// Looks up the callee's return and formal parameter bindings in the
/// propagator, resolves each argument expression with `resolve_arg`, and
/// substitutes type parameters (`T` in `identity<T>(x: T): T` called as
/// `identity(42)` yields `number`). A bare `Any`/`any` return is treated
/// as an identity passthrough: a single call-site argument with a known
/// shape refines to that shape. Returns `None` when there is nothing
/// to refine (no arguments, non-generic return, unknown formals) or when
/// the result is not fully concrete, so callers keep the unsubstituted
/// return type as their fallback.
pub fn refine_generic_call<F>(
    propagator: &CrossFilePropagator,
    language: cce_types::language::Language,
    callee_name: &str,
    arg_exprs: &[String],
    resolve_arg: &mut F,
) -> Option<(String, TypeShape)>
where
    F: FnMut(&str) -> Option<TypeShape>,
{
    if arg_exprs.is_empty() {
        return None;
    }
    let return_binding = propagator.get_return_type_by_name(callee_name)?;
    let return_shape = return_binding
        .shape
        .clone()
        .or_else(|| parse_type_shape(&return_binding.type_name, language))?;
    if is_identity_any_return(&return_shape) {
        return refine_identity_any_return(arg_exprs, resolve_arg);
    }
    if !shape_contains_param(&return_shape) {
        return None;
    }
    let param_bindings = propagator.get_parameter_types_by_name(callee_name)?;
    if param_bindings.is_empty() {
        return None;
    }
    let formal_shapes: Vec<TypeShape> = param_bindings
        .iter()
        .map(|binding| {
            binding
                .shape
                .clone()
                .or_else(|| parse_type_shape(&binding.type_name, language))
                .unwrap_or(TypeShape::Named("unknown".to_string()))
        })
        .collect();
    let actual_shapes: Vec<Option<TypeShape>> =
        arg_exprs.iter().map(|arg| resolve_arg(arg)).collect();
    let actual_refs: Vec<Option<&TypeShape>> =
        actual_shapes.iter().map(|opt| opt.as_ref()).collect();
    let substituted =
        substitute_call_return_type(&formal_shapes, &return_shape, &actual_refs, language)?;
    if shape_contains_param(&substituted) {
        return None;
    }
    Some((type_shape_to_string(&substituted), substituted))
}

/// Whether a return shape is a bare dynamic passthrough (`Any`/`any`).
///
/// Such returns carry no type parameters, so the generic substitution
/// path skips them. Callers treat a single known argument shape as the
/// refined result instead of leaking the bare dynamic name.
fn is_identity_any_return(shape: &TypeShape) -> bool {
    match shape {
        TypeShape::Named(name) => {
            let normalized = name.trim();
            normalized == "Any" || normalized == "any"
        }
        _ => false,
    }
}

/// Refine a bare `Any` return from a single known call-site argument.
///
/// Only single-argument calls refine; multi-argument dynamic returns stay
/// conservative since the passthrough position is ambiguous.
fn refine_identity_any_return<F>(
    arg_exprs: &[String],
    resolve_arg: &mut F,
) -> Option<(String, TypeShape)>
where
    F: FnMut(&str) -> Option<TypeShape>,
{
    let [single] = arg_exprs else {
        return None;
    };
    let shape = resolve_arg(single.trim())?;
    if matches!(&shape, TypeShape::Named(name) if name == "unknown") {
        return None;
    }
    if shape_contains_param(&shape) {
        return None;
    }
    Some((type_shape_to_string(&shape), shape))
}

/// Whether writing `candidate_shape` would downgrade a concrete binding.
///
/// A binding whose shape is fully resolved must never be overwritten by a
/// shape that still mentions a type parameter (e.g. same-file
/// `Pair<number, string>` must survive a later cross-file pass that only
/// knows the unsubstituted `Pair<A, B>`). Unknown shapes on either side
/// keep the existing priority logic.
pub fn candidate_downgrades_existing(
    existing: Option<&TypeBinding>,
    candidate_shape: Option<&TypeShape>,
) -> bool {
    match (
        existing.and_then(|binding| binding.shape.as_ref()),
        candidate_shape,
    ) {
        (Some(existing_shape), Some(candidate)) => {
            !shape_contains_param(existing_shape) && shape_contains_param(candidate)
        }
        _ => false,
    }
}

/// Resolve the propagated type for a single `x = f(...)` call target.
///
/// Tries call-site generic refinement first (`y = identity(42)` yields
/// `number`); falls back to the callee's unsubstituted return type.
/// Returns the type name, the (optional) defining entity id, and the
/// (optional) structured shape, or `None` when the callee is unknown.
/// Chain targets (`a.b().c()`) are not handled here; use
/// [`super::call_parsing::parse_call_chain`] for those.
pub fn resolve_single_call_binding<F>(
    propagator: &CrossFilePropagator,
    language: cce_types::language::Language,
    target: &str,
    resolve_arg: &mut F,
) -> Option<(String, Option<EntityId>, Option<TypeShape>)>
where
    F: FnMut(&str) -> Option<TypeShape>,
{
    let (name, args) = split_call_target(target);
    let simple = super::super::call_utils::simple_callee_name(&name);
    if simple.is_empty() {
        return None;
    }
    if !args.is_empty()
        && let Some((refined_name, refined_shape)) =
            refine_generic_call(propagator, language, simple, &args, resolve_arg)
    {
        return Some((refined_name, None, Some(refined_shape)));
    }
    // Overloaded names dispatch on call-site shapes before the collapsed
    // name slot is consulted; failures fall through to legacy lookup.
    if !args.is_empty()
        && let Some(winner) =
            propagator.resolve_overload_by_name(simple, &args, language, resolve_arg)
    {
        return Some((
            winner.type_name.clone(),
            winner.type_entity_id,
            winner.shape.clone(),
        ));
    }
    // Same-name callees with a different arity must not propagate: a call
    // like `ApplyTwice(5)` must never inherit the return type of an
    // unrelated `ApplyTwice(fn, value)` overload from another file.
    if !args.is_empty()
        && let Some(params) = propagator.get_parameter_types_by_name(simple)
        && !params.is_empty()
        && params.len() != args.len()
    {
        return None;
    }
    let return_binding = propagator.get_return_type_by_name(simple)?;
    Some((
        return_binding.type_name.clone(),
        return_binding.type_entity_id,
        return_binding.shape.clone(),
    ))
}
