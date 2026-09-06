//! Control-flow narrowing over type shapes.

use cce_types::language::Language;
use cce_types::{ControlFlowFact, ControlFlowFactKind};

use super::binding::TypeBinding;
use super::context::ScopedTypeContext;
use super::origin::InferenceOrigin;
use super::shape::{TypeShape, parse_type_shape, shape_members, type_shape_to_string};
use crate::symbol_table::TypeMemberIndex;

/// Narrow a union type using a discriminated field value.
///
/// Deterministic strategy: only `TypeMemberIndex` field lookup is used.
/// Heuristic case-insensitive name matching (`Pass 2`) has been thoroughly
/// removed. If the index is unavailable or no member contains the field,
/// narrowing returns `None` instead of guessing via string containment.
pub fn narrow_discriminated_union(
    shape: &TypeShape,
    field_name: &str,
    _field_value: &str,
    type_index: Option<&TypeMemberIndex>,
) -> Option<TypeShape> {
    if let TypeShape::Union(members) = shape {
        if let Some(index) = type_index {
            let mut matched: Vec<TypeShape> = Vec::new();
            for member in members {
                let names = shape_members(member);
                for name in &names {
                    if let Some(entry) = index.get_type(name) {
                        if entry.fields.contains_key(field_name) {
                            matched.push(member.clone());
                            break;
                        }
                    }
                }
            }
            if !matched.is_empty() {
                return match matched.len() {
                    1 => Some(matched.into_iter().next().expect("one")),
                    _ => Some(TypeShape::Union(matched)),
                };
            }
        }
        return None;
    }
    None
}

/// Filter a union type based on truthiness assumption.
pub fn narrow_truthiness(
    shape: &TypeShape,
    assume_true: bool,
    language: Language,
) -> Option<TypeShape> {
    if let TypeShape::Union(members) = shape {
        let filtered: Vec<TypeShape> = if assume_true {
            members
                .iter()
                .filter(|m| !is_falsy_type(m, language))
                .cloned()
                .collect()
        } else {
            members
                .iter()
                .filter(|m| is_falsy_type(m, language))
                .cloned()
                .collect()
        };
        return match filtered.len() {
            0 => None,
            1 => filtered.into_iter().next(),
            _ => Some(TypeShape::Union(filtered)),
        };
    }
    // Non-union single type: if the type itself is falsy and we assume true,
    // narrowing removes it (returns None). Otherwise keep as is for conservative.
    if is_falsy_type(shape, language) {
        if assume_true {
            None
        } else {
            Some(shape.clone())
        }
    } else if assume_true {
        Some(shape.clone())
    } else {
        None
    }
}

/// Pure union subtraction for negated narrowing (`not isinstance(x, T)`,
/// `x is not None`, `x !== null`, `typeof x !== "string"`).
///
/// Accepts `Union(members)` and `Generic{Union | Optional, args}`
/// (`Optional` implies a `None` member). Members matching any entry of
/// `excluded` (compared by rendered type string, quote-insensitive) are
/// removed. Returns `None` when the shape is not union-like or nothing
/// remains, so callers emit no binding rather than a guess.
pub fn subtract_union_members(shape: &TypeShape, excluded: &[String]) -> Option<TypeShape> {
    // Nullable wrappers (`Option[T]`, `T?`) unwrap to their inner type when
    // the exclusion targets the null member; other exclusions against a
    // bare wrapper stay conservative.
    if is_null_exclusion(excluded) {
        if let Some(inner) = unwrap_nullable(shape) {
            return Some(inner);
        }
    }
    let mut members: Vec<TypeShape> = match shape {
        TypeShape::Union(members) => members.clone(),
        TypeShape::Generic { base, args } if base == "Union" => args.clone(),
        TypeShape::Generic { base, args } if base == "Optional" || base == "Option" => {
            let mut with_none = args.clone();
            if !with_none
                .iter()
                .any(|m| matches!(m, TypeShape::Named(n) if n == "None" || n == "NoneType"))
            {
                with_none.push(TypeShape::Named("None".to_string()));
            }
            with_none
        }
        _ => return None,
    };
    let normalize = |s: &str| {
        s.trim()
            .trim_matches('"')
            .trim_matches('\'')
            .trim()
            .to_string()
    };
    members.retain(|member| {
        let rendered = normalize(&type_shape_to_string(member));
        !excluded.iter().any(|e| normalize(e) == rendered)
    });
    match members.len() {
        0 => None,
        1 => Some(members.into_iter().next().expect("one member")),
        _ => Some(TypeShape::Union(members)),
    }
}

/// Whether an exclusion list targets the null member of a nullable type.
fn is_null_exclusion(excluded: &[String]) -> bool {
    excluded.iter().any(|entry| {
        matches!(
            entry.trim().trim_matches('"').trim_matches('\''),
            "null" | "None" | "NoneType" | "nil" | "Nil" | "NULL" | "Null"
        )
    })
}

/// Unwrap a nullable shape to its non-null inner type.
///
/// Handles `Option[T]` / `Optional[T]` generics and the `T?` suffix form;
/// anything else yields `None` so callers stay conservative.
fn unwrap_nullable(shape: &TypeShape) -> Option<TypeShape> {
    match shape {
        TypeShape::Generic { base, args }
            if (base == "Option" || base == "Optional") && args.len() == 1 =>
        {
            args.first().cloned()
        }
        TypeShape::Named(name) => {
            let trimmed = name.trim();
            if let Some(inner) = trimmed.strip_suffix('?') {
                let inner = inner.trim();
                if !inner.is_empty() {
                    return Some(TypeShape::Named(inner.to_string()));
                }
            }
            None
        }
        _ => None,
    }
}

/// Compute the else-branch complement of a positive narrowing.
///
/// Removes the narrowed member from the declared shape; returns `None`
/// when the declaration is not shaped for subtraction so the else side
/// stays unbound instead of guessed.
pub fn else_branch_complement(declared: &TypeShape, narrowed_type_name: &str) -> Option<TypeShape> {
    subtract_union_members(declared, &[narrowed_type_name.to_string()])
}

/// Whether an `if` fact carries an `else` continuation.
///
/// Prefers the byte range recorded at extraction time; only facts without
/// a recorded range fall back to scanning the fact text, so the two
/// detection paths can never disagree on recorded facts.
pub fn fact_has_else_branch(fact: &ControlFlowFact) -> bool {
    if fact.kind != ControlFlowFactKind::If {
        return false;
    }
    if fact.has_else_range() {
        return true;
    }
    super::super::control_flow::shared::has_else_branch(&fact.text)
}

/// Record positive narrowings with branch attribution.
///
/// Then-branch bindings always apply. When the fact carries an `else`
/// continuation, the complement of each positive narrowing is recorded on
/// the else side, resolved against the variable declaration. Variables
/// without a subtractable declaration keep the then-branch binding only.
pub fn add_polarity_aware_narrowings(
    ctx: &mut ScopedTypeContext,
    params: &[(String, Option<String>)],
    language: Language,
    fact: &ControlFlowFact,
    results: &[(String, TypeBinding)],
) {
    let has_else = fact_has_else_branch(fact);
    for (variable_name, binding) in results {
        // Resolve the complement before recording the then-branch binding:
        // declared lookup prefers narrowed bindings, so reading after the
        // insert would subtract from the narrowed type itself.
        let complement = if has_else {
            declared_shape(ctx, params, language, variable_name)
                .and_then(|declared| else_branch_complement(&declared, &binding.type_name))
        } else {
            None
        };
        ctx.add_narrowed_type(variable_name.clone(), binding.clone());
        if let Some(complement) = complement {
            let complement_name = type_shape_to_string(&complement);
            ctx.add_narrowed_type_in_branch(
                variable_name.clone(),
                TypeBinding {
                    type_name: complement_name,
                    type_entity_id: None,
                    span: binding.span,
                    origin: Some(InferenceOrigin::ControlFlowNarrowing),
                    shape: Some(complement),
                },
                BranchPolarity::Else,
            );
        }
    }
}

/// Look up the declared type of a variable for narrowing.
///
/// Parameter annotations of the enclosing function win (they describe the
/// value at every program point); otherwise fall back to an already-known
/// variable binding. Returns the parsed shape, or `None` when the variable
/// has no usable declared type.
pub fn declared_shape(
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
    language: Language,
    name: &str,
) -> Option<TypeShape> {
    if let Some(ty) = params
        .iter()
        .find(|(n, _)| n == name)
        .and_then(|(_, ty)| ty.as_deref())
    {
        if let Some(shape) = parse_type_shape(ty, language) {
            return Some(shape);
        }
    }
    ctx.get_variable_type(name).and_then(|binding| {
        binding
            .shape
            .clone()
            .or_else(|| parse_type_shape(&binding.type_name, language))
    })
}

/// Check if a type shape represents a falsy value for a given language.
///
/// Deterministic singleton check: only language-specific falsy singletons
/// (None/null/nil) are considered falsy. Broad types such as `bool`/`int`
/// /`boolean`/`number` are intentionally excluded because they contain both
/// truthy and falsy values and filtering them is heuristic. The truthiness
/// narrowing is therefore limited to `TypeIndex`-verified unions and explicit
/// singleton members, matching the AST-pattern deterministic contract in
/// `docs/plan/symbol-resolution-deterministic.md`.
pub fn is_falsy_type(shape: &TypeShape, language: Language) -> bool {
    let check_single = |s: &str| -> bool {
        match language {
            Language::Python => matches!(s, "None" | "NoneType"),
            Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx => {
                matches!(s, "null" | "undefined")
            }
            Language::Rust => matches!(s, "Option::None" | "None"),
            Language::Go => matches!(s, "nil"),
            _ => false,
        }
    };
    match shape {
        TypeShape::Named(s) => check_single(s.as_str()),
        TypeShape::Union(members) | TypeShape::Intersection(members) => {
            // Conservative: only if any member is falsy
            members.iter().any(|m| is_falsy_type(m, language))
        }
        TypeShape::Array(_) => false,
        TypeShape::Generic { base, .. } => check_single(base.as_str()),
        TypeShape::Reference { inner, .. } => is_falsy_type(inner, language),
        TypeShape::Param(_) => false,
        TypeShape::Wildcard { .. } => false,
    }
}

/// Branch side of a conditional used to attribute narrowed bindings.
///
/// Positive checks narrow the then-branch; the else-branch (when present)
/// observes the complement instead. Recording both sides keeps complement
/// reasoning branch-aware rather than leaking one side into the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BranchPolarity {
    /// The branch taken when the condition holds.
    Then,
    /// The branch taken when the condition does not hold.
    Else,
}

#[cfg(test)]
mod tests {

    use super::*;

    use cce_types::language::Language;

    // ==================== is_falsy_type tests ====================

    #[test]
    fn test_is_falsy_type_python() {
        assert!(is_falsy_type(
            &TypeShape::Named("None".to_string()),
            Language::Python
        ));
        assert!(is_falsy_type(
            &TypeShape::Named("NoneType".to_string()),
            Language::Python
        ));
        // Deterministic: broad types like bool/int/float are not singleton
        // falsy and therefore not considered falsy.
        assert!(!is_falsy_type(
            &TypeShape::Named("bool".to_string()),
            Language::Python
        ));
        assert!(!is_falsy_type(
            &TypeShape::Named("int".to_string()),
            Language::Python
        ));
        assert!(!is_falsy_type(
            &TypeShape::Named("float".to_string()),
            Language::Python
        ));
        assert!(!is_falsy_type(
            &TypeShape::Named("String".to_string()),
            Language::Python
        ));
    }

    #[test]
    fn test_is_falsy_type_typescript() {
        assert!(is_falsy_type(
            &TypeShape::Named("null".to_string()),
            Language::TypeScript
        ));
        assert!(is_falsy_type(
            &TypeShape::Named("undefined".to_string()),
            Language::TypeScript
        ));
        // Deterministic: boolean/number contain truthy values, not singleton falsy
        assert!(!is_falsy_type(
            &TypeShape::Named("boolean".to_string()),
            Language::TypeScript
        ));
        assert!(!is_falsy_type(
            &TypeShape::Named("number".to_string()),
            Language::TypeScript
        ));
        assert!(!is_falsy_type(
            &TypeShape::Named("string".to_string()),
            Language::TypeScript
        ));
    }

    #[test]
    fn test_is_falsy_type_rust() {
        assert!(is_falsy_type(
            &TypeShape::Named("Option::None".to_string()),
            Language::Rust
        ));
        assert!(is_falsy_type(
            &TypeShape::Named("None".to_string()),
            Language::Rust
        ));
        // Deterministic: bool is not singleton falsy (contains true)
        assert!(!is_falsy_type(
            &TypeShape::Named("bool".to_string()),
            Language::Rust
        ));
        assert!(!is_falsy_type(
            &TypeShape::Named("String".to_string()),
            Language::Rust
        ));
    }

    #[test]
    fn test_is_falsy_type_go() {
        assert!(is_falsy_type(
            &TypeShape::Named("nil".to_string()),
            Language::Go
        ));
        // Deterministic: error/bool are not singleton falsy
        assert!(!is_falsy_type(
            &TypeShape::Named("error".to_string()),
            Language::Go
        ));
        assert!(!is_falsy_type(
            &TypeShape::Named("bool".to_string()),
            Language::Go
        ));
        assert!(!is_falsy_type(
            &TypeShape::Named("string".to_string()),
            Language::Go
        ));
    }

    #[test]
    fn test_is_falsy_type_union() {
        let shape = TypeShape::Union(vec![
            TypeShape::Named("String".to_string()),
            TypeShape::Named("None".to_string()),
        ]);
        assert!(is_falsy_type(&shape, Language::Python));
    }

    #[test]
    fn test_is_falsy_type_array() {
        let shape = TypeShape::Array(Box::new(TypeShape::Named("int".to_string())));
        assert!(!is_falsy_type(&shape, Language::Python));
    }

    // ==================== narrow_truthiness tests ====================

    #[test]
    fn test_narrow_truthiness_union_assume_true() {
        let shape = TypeShape::Union(vec![
            TypeShape::Named("String".to_string()),
            TypeShape::Named("None".to_string()),
        ]);
        let result = narrow_truthiness(&shape, true, Language::Python).unwrap();
        assert_eq!(result, TypeShape::Named("String".to_string()));
    }

    #[test]
    fn test_narrow_truthiness_union_assume_false() {
        let shape = TypeShape::Union(vec![
            TypeShape::Named("String".to_string()),
            TypeShape::Named("None".to_string()),
        ]);
        let result = narrow_truthiness(&shape, false, Language::Python).unwrap();
        assert_eq!(result, TypeShape::Named("None".to_string()));
    }

    #[test]
    fn test_narrow_truthiness_single_falsy_assume_true() {
        let shape = TypeShape::Named("None".to_string());
        let result = narrow_truthiness(&shape, true, Language::Python);
        assert!(result.is_none());
    }

    #[test]
    fn test_narrow_truthiness_single_non_falsy_assume_false() {
        let shape = TypeShape::Named("String".to_string());
        let result = narrow_truthiness(&shape, false, Language::Python);
        assert!(result.is_none());
    }

    #[test]
    fn test_narrow_truthiness_single_non_falsy_assume_true() {
        let shape = TypeShape::Named("String".to_string());
        let result = narrow_truthiness(&shape, true, Language::Python).unwrap();
        assert_eq!(result, TypeShape::Named("String".to_string()));
    }

    // ==================== narrow_discriminated_union tests ====================

    #[test]
    fn test_narrow_discriminated_union_non_union() {
        let shape = TypeShape::Named("String".to_string());
        let result = narrow_discriminated_union(&shape, "kind", "success", None);
        assert!(result.is_none());
    }

    #[test]
    fn test_narrow_discriminated_union_exact_match() {
        let shape = TypeShape::Union(vec![
            TypeShape::Named("Success".to_string()),
            TypeShape::Named("Error".to_string()),
        ]);
        let result = narrow_discriminated_union(&shape, "kind", "success", None);
        // Deterministic narrowing without TypeMemberIndex now returns None
        assert!(result.is_none());
    }

    #[test]
    fn test_narrow_discriminated_union_case_insensitive() {
        let shape = TypeShape::Union(vec![
            TypeShape::Named("Success".to_string()),
            TypeShape::Named("Error".to_string()),
        ]);
        let result = narrow_discriminated_union(&shape, "kind", "SUCCESS", None);
        // Heuristic case-insensitive matching removed; without index returns None
        assert!(result.is_none());
    }

    #[test]
    fn test_narrow_discriminated_union_contains_match() {
        let shape = TypeShape::Union(vec![
            TypeShape::Named("SuccessResult".to_string()),
            TypeShape::Named("ErrorResult".to_string()),
        ]);
        let result = narrow_discriminated_union(&shape, "kind", "success", None);
        // Heuristic contains matching removed; without index returns None
        assert!(result.is_none());
    }

    #[test]
    fn test_narrow_discriminated_union_no_match() {
        let shape = TypeShape::Union(vec![
            TypeShape::Named("Success".to_string()),
            TypeShape::Named("Error".to_string()),
        ]);
        let result = narrow_discriminated_union(&shape, "kind", "pending", None);
        assert!(result.is_none());
    }

    #[test]
    fn test_narrow_discriminated_union_multiple_matches() {
        let shape = TypeShape::Union(vec![
            TypeShape::Named("Success".to_string()),
            TypeShape::Named("SuccessResult".to_string()),
        ]);
        let result = narrow_discriminated_union(&shape, "kind", "success", None);
        // Deterministic: without TypeMemberIndex no heuristic fallback
        assert!(result.is_none());
    }

    fn polarity_binding(type_name: &str) -> TypeBinding {
        TypeBinding {
            type_name: type_name.to_string(),
            origin: Some(InferenceOrigin::ControlFlowNarrowing),
            ..Default::default()
        }
    }

    fn polarity_fact(text: &str) -> ControlFlowFact {
        ControlFlowFact::new(ControlFlowFactKind::If, text, 0, text.len())
    }

    #[test]
    fn test_fact_has_else_branch_prefers_recorded_range() {
        let text = "if (x instanceof String) { use(x); }";
        let recorded = polarity_fact(text).with_else_range(text.len() - 2, text.len());
        assert!(fact_has_else_branch(&recorded));
        let plain = polarity_fact("if (x instanceof String) { use(x); }");
        assert!(!fact_has_else_branch(&plain));
        let non_if = ControlFlowFact::new(
            ControlFlowFactKind::Loop,
            "for (x in xs) { use(x); } else { other(); }",
            0,
            10,
        );
        assert!(!fact_has_else_branch(&non_if));
    }

    #[test]
    fn test_polarity_aware_narrowing_records_else_complement() {
        let mut ctx = ScopedTypeContext::new(Language::Java);
        let params = [("x".to_string(), Some("String | Integer".to_string()))];
        add_polarity_aware_narrowings(
            &mut ctx,
            &params,
            Language::Java,
            &polarity_fact("if (x instanceof String) { use(x); } else { other(x); }"),
            &[("x".to_string(), polarity_binding("String"))],
        );
        let then = ctx
            .get_narrowed_in_branch("x", BranchPolarity::Then)
            .expect("then binding exists");
        assert_eq!(then.type_name, "String");
        let otherwise = ctx
            .get_narrowed_in_branch("x", BranchPolarity::Else)
            .expect("else binding exists");
        assert_eq!(otherwise.type_name, "Integer");
        // Default lookup keeps then-branch semantics.
        assert_eq!(
            ctx.get_variable_type("x")
                .expect("variable exists")
                .type_name,
            "String"
        );
    }

    #[test]
    fn test_polarity_aware_narrowing_without_else_records_then_only() {
        let mut ctx = ScopedTypeContext::new(Language::Java);
        let params = [("x".to_string(), Some("String | Integer".to_string()))];
        add_polarity_aware_narrowings(
            &mut ctx,
            &params,
            Language::Java,
            &polarity_fact("if (x instanceof String) { use(x); }"),
            &[("x".to_string(), polarity_binding("String"))],
        );
        assert!(
            ctx.get_narrowed_in_branch("x", BranchPolarity::Then)
                .is_some()
        );
        assert!(
            ctx.get_narrowed_in_branch("x", BranchPolarity::Else)
                .is_none()
        );
    }

    #[test]
    fn test_polarity_aware_narrowing_non_union_else_stays_empty() {
        let mut ctx = ScopedTypeContext::new(Language::Java);
        let params = [("obj".to_string(), Some("Object".to_string()))];
        add_polarity_aware_narrowings(
            &mut ctx,
            &params,
            Language::Java,
            &polarity_fact("if (obj instanceof String) { use(obj); } else { other(obj); }"),
            &[("obj".to_string(), polarity_binding("String"))],
        );
        assert!(
            ctx.get_narrowed_in_branch("obj", BranchPolarity::Then)
                .is_some()
        );
        assert!(
            ctx.get_narrowed_in_branch("obj", BranchPolarity::Else)
                .is_none()
        );
    }

    #[test]
    fn test_else_branch_complement_subtracts_member() {
        let declared = parse_type_shape("String | Integer", Language::Java).expect("shape");
        let complement = else_branch_complement(&declared, "String").expect("complement");
        assert_eq!(type_shape_to_string(&complement), "Integer");
    }

    #[test]
    fn test_else_branch_complement_plain_type_is_none() {
        let declared = parse_type_shape("Object", Language::Java).expect("shape");
        assert!(else_branch_complement(&declared, "String").is_none());
    }

    #[test]
    fn test_subtract_null_from_option_unwraps_inner() {
        let declared = parse_type_shape("Option[String]", Language::Scala).expect("shape");
        let narrowed =
            subtract_union_members(&declared, &["None".to_string()]).expect("inner type");
        assert_eq!(type_shape_to_string(&narrowed), "String");
    }

    #[test]
    fn test_subtract_null_from_nullable_suffix_unwraps_inner() {
        let declared = parse_type_shape("String?", Language::Kotlin).expect("shape");
        let narrowed =
            subtract_union_members(&declared, &["null".to_string()]).expect("inner type");
        assert_eq!(type_shape_to_string(&narrowed), "String");
    }

    #[test]
    fn test_subtract_member_from_option_keeps_remainder() {
        let declared = parse_type_shape("Option[String]", Language::Scala).expect("shape");
        let narrowed =
            subtract_union_members(&declared, &["String".to_string()]).expect("remainder");
        assert_eq!(type_shape_to_string(&narrowed), "None");
    }

    #[test]
    fn test_subtract_null_from_plain_union_member() {
        let declared = parse_type_shape("String | None", Language::Python).expect("shape");
        let narrowed = subtract_union_members(&declared, &["None".to_string()]).expect("remaining");
        assert_eq!(type_shape_to_string(&narrowed), "String");
    }
}
