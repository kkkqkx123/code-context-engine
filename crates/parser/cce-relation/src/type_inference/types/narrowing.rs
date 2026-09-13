//! Control-flow narrowing over type shapes.

use cce_types::language::Language;
use cce_types::{ControlFlowFact, ControlFlowFactKind};

use super::super::control_flow::shared::{
    extract_call_args, is_valid_ident, parse_type_arg, split_two_args,
};
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
/// A named (non-union) shape additionally resolves through the recorded
/// class hierarchy: base-class-typed variables narrow to the base plus the
/// subclasses carrying the field (see `narrow_base_class_by_field`).
///
/// The compared literal value refines index-gated candidates: when it names
/// one of the field-bearing members (case-insensitive, e.g. `"circle"` for
/// `Circle`), only that member is returned instead of every field-bearing
/// case. Matching never fires without the index gate, so unknown
/// declarations stay conservative.
pub fn narrow_discriminated_union(
    shape: &TypeShape,
    field_name: &str,
    field_value: &str,
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
                let value_matched: Vec<TypeShape> = matched
                    .iter()
                    .filter(|member| {
                        shape_members(member)
                            .iter()
                            .any(|name| discriminant_value_matches(field_value, name))
                    })
                    .cloned()
                    .collect();
                let selected = if value_matched.is_empty() {
                    matched
                } else {
                    value_matched
                };
                return match selected.len() {
                    1 => Some(selected.into_iter().next().expect("one")),
                    _ => Some(TypeShape::Union(selected)),
                };
            }
        }
        return None;
    }
    if let TypeShape::Named(name) = shape {
        let index = type_index?;
        return narrow_base_class_by_field(name, field_name, field_value, index);
    }
    None
}

/// Whether a compared discriminant literal names a candidate member type.
///
/// Compares case-insensitively against the member's simple name
/// (`"circle"` and `"Circle"` both match `Circle`; qualified
/// `app.Circle` matches on its `Circle` tail). Quote characters are
/// stripped so pre- or post-unquote literals behave identically. A blank
/// value never matches, keeping the caller on the field-presence set.
fn discriminant_value_matches(field_value: &str, member_name: &str) -> bool {
    let normalized = field_value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim();
    if normalized.is_empty() {
        return false;
    }
    let simple = member_name
        .rsplit(['.', ':'])
        .next()
        .unwrap_or(member_name)
        .trim();
    !simple.is_empty() && simple.eq_ignore_ascii_case(normalized)
}

/// Narrow a base-class-typed variable via a discriminant field comparison.
///
/// Mirrors the union arm's field-presence filter, refined by the compared
/// literal value when it names one of the field-bearing subclasses (e.g.
/// `Kind == "Circle"` narrows to `Circle` rather than every `Kind`-bearing
/// case). Without a value match the base plus those subclasses is returned
/// as a union. Returns `None` when no subclass carries the field. The base
/// itself is kept only on the value-agnostic path: without abstractness
/// tracking a concrete base may be instantiated directly, so dropping it
/// would be unsound; a value naming a subclass proves the base wrong.
fn narrow_base_class_by_field(
    base_name: &str,
    field_name: &str,
    field_value: &str,
    index: &TypeMemberIndex,
) -> Option<TypeShape> {
    let (base_simple, base_qualified) = match index.resolve_type(base_name) {
        Some(entry) => (entry.key.simple.clone(), entry.key.qualified.clone()),
        None => (base_name.to_string(), base_name.to_string()),
    };
    let mut carriers: Vec<TypeShape> = Vec::new();
    for sub in index.subclasses_of(&base_simple, &base_qualified) {
        if sub.key.simple != base_simple && index.visible_field_names(sub).contains(field_name) {
            carriers.push(TypeShape::Named(sub.key.simple.clone()));
        }
    }
    if carriers.is_empty() {
        return None;
    }
    let value_matched: Vec<TypeShape> = carriers
        .iter()
        .filter(|member| {
            shape_members(member)
                .iter()
                .any(|name| discriminant_value_matches(field_value, name))
        })
        .cloned()
        .collect();
    if !value_matched.is_empty() {
        return match value_matched.len() {
            1 => Some(value_matched.into_iter().next().expect("one")),
            _ => Some(TypeShape::Union(value_matched)),
        };
    }
    let mut members: Vec<TypeShape> = vec![TypeShape::Named(base_simple.clone())];
    members.extend(carriers);
    if members.len() > 1 {
        Some(TypeShape::Union(members))
    } else {
        None
    }
}

/// Filter a union type based on truthiness assumption.
///
/// Nullable wrappers (`Optional[T]` / `Option[T]` / `T?`) expand to their
/// inner type plus the language null singleton before filtering, so
/// truthiness on an `Optional` resolves to a real member (`None`,
/// `undefined`) instead of a placeholder. Returns `None` when nothing
/// remains or the shape is not filterable, so callers emit no binding
/// rather than a guess.
pub fn narrow_truthiness(
    shape: &TypeShape,
    assume_true: bool,
    language: Language,
) -> Option<TypeShape> {
    if let Some(expanded) = expand_nullable_for_truthiness(shape, language) {
        return narrow_truthiness(&expanded, assume_true, language);
    }
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

/// Expand a nullable wrapper into a synthetic union for truthiness
/// filtering. Returns `None` for non-nullable shapes or languages
/// without a known null singleton, leaving the caller conservative.
fn expand_nullable_for_truthiness(shape: &TypeShape, language: Language) -> Option<TypeShape> {
    let null_name = match language {
        Language::Python => "None",
        Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx => "undefined",
        Language::Rust => "None",
        Language::Go => "nil",
        Language::CSharp => "null",
        Language::Dart => "Null",
        Language::Kotlin => "null",
        _ => return None,
    };
    let null_member = TypeShape::Named(null_name.to_string());
    match shape {
        TypeShape::Generic { base, args }
            if (base == "Option" || base == "Optional") && args.len() == 1 =>
        {
            let mut members = args.clone();
            members.push(null_member);
            Some(TypeShape::Union(members))
        }
        TypeShape::Named(name) => {
            let trimmed = name.trim();
            if let Some(inner) = trimmed.strip_suffix('?') {
                let inner = inner.trim();
                if !inner.is_empty() {
                    return Some(TypeShape::Union(vec![
                        TypeShape::Named(inner.to_string()),
                        null_member,
                    ]));
                }
            }
            None
        }
        _ => None,
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
/// Guard-shaped `if` facts (no `else`, diverging then side) additionally
/// record post-guard fallthrough bindings (see
/// [`guard_fallthrough_bindings`)), so early-return style narrowing stays
/// visible instead of vanishing with the then side.
pub fn add_polarity_aware_narrowings(
    ctx: &mut ScopedTypeContext,
    params: &[(String, Option<String>)],
    language: Language,
    fact: &ControlFlowFact,
    results: &[(String, TypeBinding)],
    span_fallback: cce_types::Span,
) {
    let has_else = fact_has_else_branch(fact);
    // Resolve fallthrough bindings before recording anything: declared
    // lookup prefers narrowed bindings, so reading after the then-branch
    // inserts would subtract from the narrowed types themselves.
    let fallthroughs = if has_else {
        Vec::new()
    } else {
        guard_fallthrough_bindings(ctx, params, language, fact, results, span_fallback)
    };
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
    if !has_else {
        for (variable_name, binding) in fallthroughs {
            ctx.add_narrowed_type(variable_name, binding);
        }
    }
}

/// Post-guard fallthrough bindings for `if` facts without `else`.
///
/// Only fires when the then side diverges (early return / throw / break /
/// ...), so the code after the statement observes the negated condition.
/// Two sources, both recorded as visible then-side bindings because the
/// diverging then region binds no further uses of the variable:
///
/// - declared-minus-then for each recorded then narrowing (positive guards
///   like `if isinstance(x, Circle): return` leave `declared-minus-Circle`
///   behind);
/// - the positive type of a negated test (`!(x instanceof T)`, `x is not
///   T`, `x is! T`, `x !is T`, `not isinstance(x, T)`) for variables with
///   no then binding (e.g. plain `Object` declarations whose complement
///   is inexpressible).
///
/// Results identical to the declared type are dropped so vacuous guards
/// emit nothing. Must be computed before any then-branch insert (declared
/// lookup prefers narrowed bindings).
pub fn guard_fallthrough_bindings(
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
    language: Language,
    fact: &ControlFlowFact,
    results: &[(String, TypeBinding)],
    span_fallback: cce_types::Span,
) -> Vec<(String, TypeBinding)> {
    if fact.kind != ControlFlowFactKind::If || !guard_then_diverges(&fact.text) {
        return Vec::new();
    }
    let mut out: Vec<(String, TypeBinding)> = Vec::new();
    let mut seen: Vec<(String, String)> = results
        .iter()
        .map(|(name, binding)| (name.clone(), binding.type_name.clone()))
        .collect();
    for (variable_name, binding) in results {
        let Some(declared) = declared_shape(ctx, params, language, variable_name) else {
            continue;
        };
        let Some(complement) = else_branch_complement(&declared, &binding.type_name) else {
            continue;
        };
        // Identity results (string-equal or covering the same union
        // members across spellings) prove nothing; drop them.
        if type_shape_to_string(&complement) == type_shape_to_string(&declared)
            || covers_declared_members(&declared, &complement)
        {
            continue;
        }
        let complement_name = type_shape_to_string(&complement);
        if seen
            .iter()
            .any(|(name, ty)| name == variable_name && ty == &complement_name)
        {
            continue;
        }
        seen.push((variable_name.clone(), complement_name.clone()));
        out.push((
            variable_name.clone(),
            TypeBinding {
                type_name: complement_name,
                type_entity_id: None,
                span: binding.span,
                origin: Some(InferenceOrigin::ControlFlowNarrowing),
                shape: Some(complement),
            },
        ));
    }
    for (variable_name, mut positive) in negated_guard_positive(&fact.text, language) {
        // A variable with any then-side result keeps that side only;
        // mixing the positive into compound conditions (`!(x instanceof A)
        // || x == null`) would misread the disjunction.
        if results.iter().any(|(name, _)| name == &variable_name) {
            continue;
        }
        if !positive.span.is_available() {
            positive.span = span_fallback;
        }
        if seen
            .iter()
            .any(|(name, ty)| name == &variable_name && ty == &positive.type_name)
        {
            continue;
        }
        seen.push((variable_name.clone(), positive.type_name.clone()));
        out.push((variable_name, positive));
    }
    out
}

/// Positive type of a negated type test in guard position.
///
/// `if (!(obj instanceof String)) return ...` leaves `obj: String` behind
/// for the code after the statement, even though the then side (a plain
/// `Object` minus `String`) is inexpressible. Each entry parses one
/// language's negated form and returns `(variable, positive-binding)`;
/// unlisted languages stay conservative. Callers only use entries for
/// variables with no then-side result.
fn negated_guard_positive(text: &str, language: Language) -> Vec<(String, TypeBinding)> {
    let mut out = Vec::new();
    let mut push = |var: &str, ty: &str, out: &mut Vec<(String, TypeBinding)>| {
        let var = var.trim();
        let ty = ty.trim().trim_end_matches([')', '{', ';', ',']).trim();
        if var.is_empty() || !is_valid_ident(var) || ty.is_empty() {
            return;
        }
        // Multi-token tails (`String s`, `a && b`) are not plain types.
        if ty.split_whitespace().count() != 1 {
            return;
        }
        let shape = parse_type_shape(ty, language);
        out.push((
            var.to_string(),
            TypeBinding {
                type_name: ty.to_string(),
                type_entity_id: None,
                span: cce_types::Span::default(),
                origin: Some(InferenceOrigin::ControlFlowNarrowing),
                shape,
            },
        ));
    };
    match language {
        Language::Java
        | Language::TypeScript
        | Language::JavaScript
        | Language::Tsx
        | Language::Jsx => {
            if let Some(pos) = text.find("instanceof") {
                let head = &text[..pos];
                let Some(var) = head
                    .rsplit(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
                    .map(str::trim)
                    .find(|s| !s.is_empty())
                else {
                    return out;
                };
                let assertion = &head[..head.len() - var.len()];
                if !assertion.contains('!') {
                    return out;
                }
                let tail = &text[pos + "instanceof".len()..];
                let ty = tail.split_whitespace().next().unwrap_or("");
                push(var, ty, &mut out);
            }
        }
        Language::CSharp => {
            if let Some((head, tail)) = text.split_once(" is not ") {
                let var = head
                    .rsplit(|c: char| !(c.is_alphanumeric() || c == '_' || c == '@'))
                    .map(str::trim)
                    .find(|s| !s.is_empty())
                    .unwrap_or("")
                    .trim_start_matches('@');
                let excluded = tail.split_whitespace().next().unwrap_or("");
                if excluded == "null" {
                    if let Some(null) = null_singleton_name(language) {
                        push(var, null, &mut out);
                    }
                } else {
                    push(var, excluded, &mut out);
                }
            }
        }
        Language::Dart => {
            if let Some((head, tail)) = text.split_once("is!") {
                let var = head
                    .rsplit(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
                    .map(str::trim)
                    .find(|s| !s.is_empty())
                    .unwrap_or("");
                let excluded = tail.split_whitespace().next().unwrap_or("");
                push(var, excluded, &mut out);
            }
        }
        Language::Kotlin => {
            if let Some((head, tail)) = text.split_once(" !is ") {
                let var = head
                    .rsplit(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
                    .map(str::trim)
                    .find(|s| !s.is_empty())
                    .unwrap_or("");
                let excluded = tail.split_whitespace().next().unwrap_or("");
                push(var, excluded, &mut out);
            }
        }
        Language::Python => {
            if let Some(rest) = text
                .find("not isinstance(")
                .and_then(|pos| extract_call_args(&text[pos + "not ".len()..], "isinstance"))
            {
                if let Some((var, type_arg)) = split_two_args(rest) {
                    if let Some(ty) = parse_type_arg(type_arg.trim()) {
                        // Tuples widen (`(A, B)` → `A | B`); record only
                        // when the union actually informs (single types
                        // always do; unions are checked against declared
                        // by the caller via the identity rule below).
                        push(var, &ty, &mut out);
                    }
                }
            } else if let Some((head, _)) = text.split_once(" is not None") {
                let var = head
                    .rsplit(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .map(str::trim)
                    .find(|s| !s.is_empty())
                    .unwrap_or("");
                push(var, "None", &mut out);
            }
        }
        _ => {}
    }
    out
}
/// Whether the then side of an `if` fact diverges.
///
/// Heuristic over the then-body segment (text between the first `{` and
/// the last `}`): a diverging keyword (`return`, `throw`, `break`,
/// `continue`, `raise`, `panic!`, ...) means code after the statement only
/// runs when the condition is false. Nested bodies may over-approximate,
/// but the consequence is one extra visible narrowing row, never a wrong
/// variable binding.
fn guard_then_diverges(text: &str) -> bool {
    // Brace body (`if (c) { return ...; }`) or, for indentation-based
    // languages, the statement after the first colon (`if c: return ...`).
    let body = match text.find('{') {
        Some(brace) => &text[brace + 1..],
        None => match text.find(':') {
            Some(colon) => &text[colon + 1..],
            None => return false,
        },
    };
    const WORDS: &[&str] = &[
        "return",
        "throw",
        "break",
        "continue",
        "raise",
        "exit",
        "panic",
        "unreachable",
        "fatal",
    ];
    let mut token = String::new();
    let mut diverges = false;
    for ch in body.chars().chain(std::iter::once(' ')) {
        if ch.is_alphanumeric() || ch == '_' {
            token.push(ch);
        } else {
            if WORDS.contains(&token.as_str()) {
                diverges = true;
                break;
            }
            token.clear();
        }
    }
    diverges
}
/// Whether a narrowed shape covers exactly the declared members.
///
/// Compares member sets order-insensitively across union spellings
/// (`Union[A, B]` vs `A | B`), so vacuous checks (`isinstance(x, (A, B))`
/// on `Union[A, B]`) are detected regardless of surface syntax.
pub fn covers_declared_members(declared: &TypeShape, narrowed: &TypeShape) -> bool {
    match (union_member_set(declared), union_member_set(narrowed)) {
        (Some(before), Some(after)) => before == after,
        _ => false,
    }
}

/// Member render set of a union-like shape (`Union` or `Union[...]`).
fn union_member_set(shape: &TypeShape) -> Option<std::collections::BTreeSet<String>> {
    match shape {
        TypeShape::Union(members) => Some(
            members
                .iter()
                .map(type_shape_to_string)
                .collect::<std::collections::BTreeSet<_>>(),
        ),
        TypeShape::Generic { base, args } if base == "Union" => Some(
            args.iter()
                .map(type_shape_to_string)
                .collect::<std::collections::BTreeSet<_>>(),
        ),
        _ => None,
    }
}

/// Null singleton spelling per language for fallthrough bindings.
pub fn null_singleton_name(language: Language) -> Option<&'static str> {
    match language {
        Language::Python => Some("None"),
        Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx => {
            Some("undefined")
        }
        Language::Rust => Some("None"),
        Language::Go => Some("nil"),
        Language::CSharp => Some("null"),
        Language::Dart => Some("Null"),
        Language::Kotlin => Some("null"),
        Language::Java => Some("null"),
        _ => None,
    }
}

/// Subtract one member from a nullable `T?`-suffixed declaration.
///
/// `subtract_union_members` only understands `Union`/`Optional` shapes, so
/// `String?`-style declarations would stay conservative. A single exclusion
/// matching the inner type resolves to the language null singleton; a null
/// exclusion unwraps to the inner type; anything else stays conservative.
pub fn subtract_nullable_suffix(
    shape: &TypeShape,
    excluded: &str,
    language: Language,
) -> Option<TypeShape> {
    let TypeShape::Named(name) = shape else {
        return None;
    };
    let inner = name.trim().strip_suffix('?')?.trim();
    if inner.is_empty() {
        return None;
    }
    let normalized = excluded.trim().trim_matches('"').trim_matches('\'');
    if normalized == inner {
        return null_singleton_name(language).map(|null| TypeShape::Named(null.to_string()));
    }
    if is_null_exclusion(&[excluded.to_string()]) {
        return Some(TypeShape::Named(inner.to_string()));
    }
    None
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
            Language::CSharp => matches!(s, "null"),
            Language::Dart => matches!(s, "Null"),
            Language::Kotlin => matches!(s, "null"),
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

    #[test]
    fn test_narrow_truthiness_optional_assume_false_yields_none_member() {
        let shape = TypeShape::Generic {
            base: "Optional".to_string(),
            args: vec![TypeShape::Named("str".to_string())],
        };
        let result = narrow_truthiness(&shape, false, Language::Python).unwrap();
        assert_eq!(result, TypeShape::Named("None".to_string()));
    }

    #[test]
    fn test_narrow_truthiness_optional_assume_true_yields_inner() {
        let shape = TypeShape::Generic {
            base: "Optional".to_string(),
            args: vec![TypeShape::Named("str".to_string())],
        };
        let result = narrow_truthiness(&shape, true, Language::Python).unwrap();
        assert_eq!(result, TypeShape::Named("str".to_string()));
    }

    #[test]
    fn test_narrow_truthiness_nullable_suffix_assume_false() {
        let shape = TypeShape::Named("String?".to_string());
        let result = narrow_truthiness(&shape, false, Language::Dart).unwrap();
        assert_eq!(result, TypeShape::Named("Null".to_string()));
    }

    #[test]
    fn test_narrow_truthiness_nullable_suffix_assume_true() {
        let shape = TypeShape::Named("String?".to_string());
        let result = narrow_truthiness(&shape, true, Language::Dart).unwrap();
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

    // ==================== base-class discrimination tests ====================

    fn shape_hierarchy_index() -> TypeMemberIndex {
        use cce_types::Span;
        use cce_types::entity::{EntityId, EntityKind};

        use crate::symbol::Visibility;
        use crate::symbol_table::type_index::{MemberEntry, TypeEntry, TypeKey};

        fn field(name: &str, id: u64) -> MemberEntry {
            MemberEntry {
                entity_id: EntityId(id),
                name: name.to_string(),
                kind: EntityKind::Property,
                visibility: Visibility::Public,
                is_static: false,
                is_associated: false,
                span: Span::default(),
                file_path: "app.cs".to_string(),
                module_path: None,
                package: String::new(),
            }
        }

        let mut index = TypeMemberIndex::new();
        let mut add = |simple: &str, id: u64, supertypes: &[&str], fields: &[(&str, u64)]| {
            let key = TypeKey::new(
                format!("app.{simple}"),
                simple.to_string(),
                "app.cs".to_string(),
            );
            let mut entry = TypeEntry::new(
                EntityId(id),
                key.clone(),
                EntityKind::Class,
                Language::CSharp,
                Visibility::Public,
            );
            entry.supertypes = supertypes.iter().map(|s| s.to_string()).collect();
            index.insert_type(key.clone(), entry);
            for (name, fid) in fields {
                index.insert_member(&key, field(name, *fid));
            }
        };
        add("Shape", 1, &[], &[("Kind", 11)]);
        add("Circle", 2, &["Shape"], &[("Kind", 21)]);
        add("Rectangle", 3, &["Shape"], &[("Kind", 31)]);
        index
    }

    #[test]
    fn test_narrow_base_class_by_discriminant_field() {
        let index = shape_hierarchy_index();
        let shape = TypeShape::Named("Shape".to_string());
        // The literal names a subclass, so only it is returned.
        let result = narrow_discriminated_union(&shape, "Kind", "Circle", Some(&index))
            .expect("value-named subclass must narrow");
        assert_eq!(result, TypeShape::Named("Circle".to_string()));
    }

    #[test]
    fn test_narrow_base_class_unmatched_value_keeps_base_and_carriers() {
        let index = shape_hierarchy_index();
        let shape = TypeShape::Named("Shape".to_string());
        let result = narrow_discriminated_union(&shape, "Kind", "Hexagon", Some(&index))
            .expect("base with field-bearing subclasses must narrow");
        assert_eq!(
            result,
            TypeShape::Union(vec![
                TypeShape::Named("Shape".to_string()),
                TypeShape::Named("Circle".to_string()),
                TypeShape::Named("Rectangle".to_string()),
            ])
        );
    }

    #[test]
    fn test_discriminant_value_matches_member_name() {
        assert!(discriminant_value_matches("circle", "Circle"));
        assert!(discriminant_value_matches("\"Circle\"", "app.Circle"));
        assert!(!discriminant_value_matches("hexagon", "Circle"));
        assert!(!discriminant_value_matches("", "Circle"));
    }

    fn discriminant_union_index() -> TypeMemberIndex {
        use cce_types::Span;
        use cce_types::entity::{EntityId, EntityKind};

        use crate::symbol::Visibility;
        use crate::symbol_table::type_index::{MemberEntry, TypeEntry, TypeKey};

        let mut index = TypeMemberIndex::new();
        for (simple, id) in [("Circle", 21u64), ("Rectangle", 31u64)] {
            let key = TypeKey::new(simple.to_string(), simple.to_string(), "app.py".to_string());
            let entry = TypeEntry::new(
                EntityId(id),
                key.clone(),
                EntityKind::Class,
                Language::Python,
                Visibility::Public,
            );
            index.insert_type(key.clone(), entry);
            let _ = index.insert_member(
                &key,
                MemberEntry {
                    entity_id: EntityId(id + 100),
                    name: "kind".to_string(),
                    kind: EntityKind::Field,
                    visibility: Visibility::Public,
                    is_static: false,
                    is_associated: false,
                    span: Span::default(),
                    file_path: "app.py".to_string(),
                    module_path: None,
                    package: String::new(),
                },
            );
        }
        index
    }

    #[test]
    fn test_narrow_union_prefers_value_named_member() {
        let index = discriminant_union_index();
        let shape = TypeShape::Union(vec![
            TypeShape::Named("Circle".to_string()),
            TypeShape::Named("Rectangle".to_string()),
        ]);
        let result = narrow_discriminated_union(&shape, "kind", "circle", Some(&index))
            .expect("value-named member must narrow");
        assert_eq!(result, TypeShape::Named("Circle".to_string()));
    }

    #[test]
    fn test_narrow_union_unmatched_value_keeps_field_bearers() {
        let index = discriminant_union_index();
        let shape = TypeShape::Union(vec![
            TypeShape::Named("Circle".to_string()),
            TypeShape::Named("Rectangle".to_string()),
        ]);
        let result = narrow_discriminated_union(&shape, "kind", "hexagon", Some(&index))
            .expect("field-bearing members must narrow");
        assert_eq!(
            result,
            TypeShape::Union(vec![
                TypeShape::Named("Circle".to_string()),
                TypeShape::Named("Rectangle".to_string()),
            ])
        );
    }

    #[test]
    fn test_narrow_base_class_without_index_stays_unbound() {
        let shape = TypeShape::Named("Shape".to_string());
        assert!(narrow_discriminated_union(&shape, "Kind", "Circle", None).is_none());
    }

    #[test]
    fn test_narrow_base_class_no_carrying_subclass() {
        let index = shape_hierarchy_index();
        let shape = TypeShape::Named("Shape".to_string());
        assert!(narrow_discriminated_union(&shape, "Nope", "x", Some(&index)).is_none());
    }

    #[test]
    fn test_narrow_base_class_unknown_base_stays_unbound() {
        let index = shape_hierarchy_index();
        let shape = TypeShape::Named("Blob".to_string());
        assert!(narrow_discriminated_union(&shape, "Kind", "Circle", Some(&index)).is_none());
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
            cce_types::Span::default(),
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
            cce_types::Span::default(),
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
            cce_types::Span::default(),
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
