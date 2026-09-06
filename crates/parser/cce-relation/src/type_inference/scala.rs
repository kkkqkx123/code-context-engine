//! Scala-specific type inference.
//!
//! Handles Scala-specific patterns:
//! - `val`/`var` declarations with optional type annotations
//! - `match` expression pattern narrowing
//! - Pattern matching with typed patterns (`case x: Type =>`)
//! - Constructor calls via `new ClassName()`
//! - Option/Some/None patterns

use cce_types::ControlFlowFactKind;
use cce_types::ControlFlowStore;
use cce_types::Language;
use cce_types::Span;
use cce_types::entity::{Entity, EntityKind};

use super::control_flow::shared::{extract_balanced_parens, is_valid_ident, strip_outer_parens};
use super::extractors::{extract_field_type, extract_function_types, extract_variable_type};
use super::traits::LanguageTypeInferer;
use super::types::{
    ScopedTypeContext, TypeBinding, TypeShape, add_polarity_aware_narrowings, declared_shape,
    parse_type_shape, subtract_union_members, type_shape_to_string,
};

/// Scala type inference implementation.
pub struct ScalaTypeInferer;

impl LanguageTypeInferer for ScalaTypeInferer {
    fn infer_declarations(&self, entities: &[Entity], ctx: &mut ScopedTypeContext) {
        for entity in entities {
            match entity.kind {
                EntityKind::Function | EntityKind::Method => {
                    extract_function_types(entity, ctx);
                }
                EntityKind::Variable => {
                    extract_variable_type(entity, ctx);

                    if let Some(var_type) = entity.metadata.get("var_type") {
                        let binding = TypeBinding {
                            type_name: var_type.clone(),
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(super::types::InferenceOrigin::TypeAnnotation),
                            shape: parse_type_shape(var_type, Language::Scala),
                        };
                        ctx.add_variable_type(entity.name.clone(), binding);
                    }
                    // Constructor calls (`new ClassName()`) are handled by the shared
                    // `extract_variable_type`, which binds `constructor_type` with a
                    // resolved shape only when no concrete annotation is present.
                    // No duplicate handling here so explicit annotations keep priority.
                }
                EntityKind::Field | EntityKind::Property => {
                    extract_field_type(entity, ctx);
                }
                _ => {}
            }
        }
    }

    fn infer_control_flow(
        &self,
        entities: &[Entity],
        control_flow: &ControlFlowStore,
        ctx: &mut ScopedTypeContext,
        _inference_ctx: &super::traits::InferenceContext<'_>,
    ) {
        for entity in entities {
            let Some(entity_cf) = control_flow.get(entity.id) else {
                continue;
            };
            for fact in &entity_cf.facts {
                match fact.kind {
                    ControlFlowFactKind::If | ControlFlowFactKind::Loop => {
                        let mut narrowed: Vec<(String, TypeBinding)> =
                            narrow_scala_if(&fact.text, ctx, &entity.parameters)
                                .into_iter()
                                .map(|result| (result.variable_name, result.narrowed_type))
                                .collect();
                        for (_, binding) in narrowed.iter_mut() {
                            if !binding.span.is_available() {
                                binding.span = entity.span;
                            }
                        }
                        add_polarity_aware_narrowings(
                            ctx,
                            &entity.parameters,
                            Language::Scala,
                            fact,
                            &narrowed,
                        );
                    }
                    ControlFlowFactKind::Match => {
                        for result in narrow_scala_match(&fact.text, ctx, &entity.parameters) {
                            ctx.add_narrowed_type_anchored(
                                result.variable_name,
                                result.narrowed_type,
                                entity.span,
                            );
                        }
                    }
                    _ => continue,
                }
            }
        }
    }
}

/// Result of a single narrowing operation.
#[derive(Debug, Clone)]
struct NarrowingResult {
    variable_name: String,
    narrowed_type: TypeBinding,
}

/// Narrow types from a Scala `if` condition.
///
/// Patterns:
/// - `if (x.isInstanceOf[Type])` → x: Type
/// - `if (!(x.isInstanceOf[Type]))` → x: declared-minus-Type (union only)
/// - `if (x.isInstanceOf[Type] == false)` → x: declared-minus-Type (union only)
/// - `if (x != null)` → x: declared (non-null)
/// - `if (x == null)` → x: null
fn narrow_scala_if(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    let text = text.trim();
    let mut results = vec![];

    if let Some(result) = narrow_scala_isinstanceof(text, ctx, params) {
        results.push(result);
    }

    if let Some(result) = narrow_scala_null_check(text, ctx, params) {
        results.push(result);
    }

    results
}

/// Parse `if (x.isInstanceOf[Type])`, dispatching negated forms to the
/// complement path.
fn narrow_scala_isinstanceof(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Option<NarrowingResult> {
    let text = strip_scala_condition_prefix(text)?;
    let text = text.trim();

    // Negated prefix: `!(x.isInstanceOf[T])` narrows the complement.
    if let Some(rest) = text.strip_prefix('!') {
        let rest = rest.trim();
        let inner = extract_balanced_parens(rest).unwrap_or_else(|| strip_outer_parens(rest));
        return narrow_scala_negated_isinstanceof(inner, ctx, params);
    }

    // Negated suffix: `x.isInstanceOf[T] == false` narrows the complement.
    // Handled before the positive path so the trailing `== false` can
    // never fall through into a positive binding.
    if let Some((lhs, rhs)) = text.split_once("==") {
        if rhs.trim() == "false" {
            return narrow_scala_negated_isinstanceof(lhs.trim(), ctx, params);
        }
        return None;
    }

    let (var_name, type_name) = split_scala_isinstanceof(text)?;

    Some(NarrowingResult {
        variable_name: var_name,
        narrowed_type: TypeBinding {
            type_name,
            type_entity_id: None,
            span: Span::default(),
            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
            shape: None,
        },
    })
}

/// Split `x.isInstanceOf[Type]` into the scrutinee and the tested type.
fn split_scala_isinstanceof(text: &str) -> Option<(String, String)> {
    let text = text.trim();
    let marker = ".isInstanceOf[";
    let marker_pos = text.find(marker)?;
    let var_name = text[..marker_pos].trim();

    if var_name.is_empty() || !is_valid_ident(var_name) {
        return None;
    }

    let bracket_start = marker_pos + marker.len();
    let bracket_end = text.rfind(']')?;
    if bracket_end <= bracket_start {
        return None;
    }
    let type_name = text[bracket_start..bracket_end].trim();

    if type_name.is_empty() {
        return None;
    }

    Some((var_name.to_string(), type_name.to_string()))
}

/// Parse `!(x.isInstanceOf[Type])` → x: declared-minus-Type.
///
/// Only fires when the declared shape is a union (or nullable wrapper)
/// that the exclusion can actually shrink; otherwise stays conservative.
fn narrow_scala_negated_isinstanceof(
    inner: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Option<NarrowingResult> {
    let (var_name, excluded) = split_scala_isinstanceof(inner.trim())?;
    let declared = declared_shape(ctx, params, Language::Scala, &var_name)?;
    let narrowed = subtract_union_members(&declared, &[excluded])?;
    Some(NarrowingResult {
        variable_name: var_name,
        narrowed_type: TypeBinding {
            type_name: type_shape_to_string(&narrowed),
            type_entity_id: None,
            span: Span::default(),
            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
            shape: Some(narrowed),
        },
    })
}

/// Parse Scala null checks: `x != null` → x: declared, `x == null` → x: null.
fn narrow_scala_null_check(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Option<NarrowingResult> {
    let text = strip_scala_condition_prefix(text)?;
    let text = text.trim();
    for (op, negated) in [("!=", true), ("==", false)] {
        let parts: Vec<&str> = text.splitn(2, op).collect();
        if parts.len() != 2 {
            continue;
        }
        let var_name = parts[0].trim();
        let rhs = parts[1].trim();
        if rhs != "null" || var_name.is_empty() || !is_valid_ident(var_name) {
            continue;
        }
        if !negated {
            return Some(NarrowingResult {
                variable_name: var_name.to_string(),
                narrowed_type: TypeBinding {
                    type_name: "null".to_string(),
                    type_entity_id: None,
                    span: Span::default(),
                    origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                    shape: None,
                },
            });
        }
        let declared = declared_shape(ctx, params, Language::Scala, var_name)?;
        let narrowed =
            subtract_union_members(&declared, &["null".to_string()]).unwrap_or(declared.clone());
        return Some(NarrowingResult {
            variable_name: var_name.to_string(),
            narrowed_type: TypeBinding {
                type_name: type_shape_to_string(&narrowed),
                type_entity_id: None,
                span: Span::default(),
                origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                shape: Some(narrowed),
            },
        });
    }
    None
}

/// Narrow types from a Scala `match` expression.
///
/// Patterns:
/// - `case x: Type =>` → x: Type
/// - `case Some(v) =>` → scrutinee: `Some[T]`, v: T (from `Option[T]`)
/// - `case None =>` → scrutinee: None
fn narrow_scala_match(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    let text = text.trim();
    let mut results = vec![];

    if let Some(brace_start) = text.find('{') {
        let scrutinee = extract_scala_match_scrutinee(&text[..brace_start]);
        let body = &text[brace_start + 1..];
        narrow_scala_match_arms(body, scrutinee.as_deref(), ctx, params, &mut results);
    }

    results
}

/// Extract the scrutinee of `x match`: the identifier before `match`.
fn extract_scala_match_scrutinee(header: &str) -> Option<String> {
    let header = header.trim();
    let match_pos = header.rfind("match")?;
    let candidate = header[..match_pos].trim();
    let name = candidate.split_whitespace().last().unwrap_or(candidate);
    let name = name.trim_matches(|c| c == '(' || c == ')').trim();
    if name.is_empty() || !is_valid_ident(name) {
        return None;
    }
    Some(name.to_string())
}

/// Extract variable bindings from Scala match arms.
fn narrow_scala_match_arms(
    arms_text: &str,
    scrutinee: Option<&str>,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
    results: &mut Vec<NarrowingResult>,
) {
    let mut remaining = arms_text;
    while let Some(case_pos) = remaining.find("case") {
        let after_case = &remaining[case_pos..];
        // Find the arrow separating pattern from body
        if let Some(arrow_pos) = after_case.find("=>") {
            let arm_text = after_case[..arrow_pos].trim();
            if let Some(result) = parse_scala_match_arm_pattern(arm_text) {
                results.push(result);
            }
            for result in parse_scala_option_arm_pattern(arm_text, scrutinee, ctx, params) {
                results.push(result);
            }
            remaining = after_case[arrow_pos + 2..].trim_start();
        } else {
            break;
        }
    }
}

/// Parse `case Some(v)` / `case None` arms against the scrutinee.
///
/// `Some(v)` binds the scrutinee to `Some[T]` and `v` to the option's
/// inner type resolved from the scrutinee declaration; `None` binds the
/// scrutinee to `None`. Arms without a resolvable option declaration
/// stay unbound.
fn parse_scala_option_arm_pattern(
    arm_text: &str,
    scrutinee: Option<&str>,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    let scrutinee = match scrutinee {
        Some(name) => name,
        None => return vec![],
    };
    let pattern = match arm_text.trim().strip_prefix("case") {
        Some(rest) => rest.trim(),
        None => return vec![],
    };
    if pattern == "None" {
        return vec![NarrowingResult {
            variable_name: scrutinee.to_string(),
            narrowed_type: TypeBinding {
                type_name: "None".to_string(),
                type_entity_id: None,
                span: Span::default(),
                origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                shape: Some(TypeShape::Named("None".to_string())),
            },
        }];
    }
    let inner_name = match pattern.strip_prefix("Some").map(str::trim) {
        Some(rest) => {
            let rest = rest.strip_prefix('(').unwrap_or(rest);
            let rest = rest.strip_suffix(')').unwrap_or(rest);
            rest.trim()
        }
        None => return vec![],
    };
    if inner_name.is_empty() || !is_valid_ident(inner_name) {
        return vec![];
    }
    let declared = match declared_shape(ctx, params, Language::Scala, scrutinee) {
        Some(shape) => shape,
        None => return vec![],
    };
    let inner = match scala_option_inner_type(&declared) {
        Some(inner) => inner,
        None => return vec![],
    };
    let inner_name_string = type_shape_to_string(&inner);
    let some_shape = TypeShape::Generic {
        base: "Some".to_string(),
        args: vec![inner.clone()],
    };
    vec![
        NarrowingResult {
            variable_name: scrutinee.to_string(),
            narrowed_type: TypeBinding {
                type_name: type_shape_to_string(&some_shape),
                type_entity_id: None,
                span: Span::default(),
                origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                shape: Some(some_shape),
            },
        },
        NarrowingResult {
            variable_name: inner_name.to_string(),
            narrowed_type: TypeBinding {
                type_name: inner_name_string,
                type_entity_id: None,
                span: Span::default(),
                origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                shape: Some(inner),
            },
        },
    ]
}

/// Resolve the inner type of an option-like declaration.
///
/// Accepts `Option[T]` / `Some[T]` generics and nullable `T | None`
/// unions; anything else stays conservative.
fn scala_option_inner_type(declared: &TypeShape) -> Option<TypeShape> {
    match declared {
        TypeShape::Generic { base, args }
            if (base == "Option" || base == "Some") && !args.is_empty() =>
        {
            args.first().cloned()
        }
        TypeShape::Union(members) => {
            let non_none: Vec<TypeShape> = members
                .iter()
                .filter(|m| !matches!(m, TypeShape::Named(n) if n == "None" || n == "NoneType"))
                .cloned()
                .collect();
            match non_none.len() {
                1 => non_none.into_iter().next(),
                _ => None,
            }
        }
        _ => {
            let rendered = type_shape_to_string(declared);
            parse_type_shape(&rendered, Language::Scala).and_then(|reparsed| {
                if &reparsed == declared {
                    None
                } else {
                    scala_option_inner_type(&reparsed)
                }
            })
        }
    }
}

/// Parse a single Scala match arm pattern: `case var: Type` → var: Type.
fn parse_scala_match_arm_pattern(text: &str) -> Option<NarrowingResult> {
    let text = text.trim();
    let text = text.strip_prefix("case")?.trim();

    // Pattern: `varName: Type`
    if let Some(colon_pos) = text.find(':') {
        let var_name = text[..colon_pos].trim();
        let type_name = text[colon_pos + 1..].trim().trim_end_matches("=>").trim();

        if !var_name.is_empty()
            && is_valid_ident(var_name)
            && !type_name.is_empty()
            && type_name != "_"
        {
            return Some(NarrowingResult {
                variable_name: var_name.to_string(),
                narrowed_type: TypeBinding {
                    type_name: type_name.to_string(),
                    type_entity_id: None,
                    span: Span::default(),
                    origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                    shape: None,
                },
            });
        }
    }

    None
}

/// Strip Scala condition prefixes.
fn strip_scala_condition_prefix(text: &str) -> Option<&str> {
    let text = text.trim();
    for prefix in &["if", "while", "else if", "return"] {
        if let Some(rest) = text.strip_prefix(prefix) {
            let rest = rest.trim();
            // Real facts carry the branch body after the condition, so
            // prefer balanced-paren extraction over naive outer stripping.
            return Some(extract_balanced_parens(rest).unwrap_or_else(|| strip_outer_parens(rest)));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::entity::EntityId;
    use cce_types::language::Language;

    fn dummy_span() -> Span {
        Span::default()
    }

    #[test]
    fn test_scala_method_signature() {
        let mut ctx = ScopedTypeContext::new(Language::Scala);
        let entities = vec![
            Entity::new(
                EntityId(1),
                EntityKind::Method,
                "getValue".to_string(),
                dummy_span(),
            )
            .with_return_type(Some("String".to_string())),
        ];

        ScalaTypeInferer.infer_declarations(&entities, &mut ctx);
        let rt = ctx.get_return_type(EntityId(1)).unwrap();
        assert_eq!(rt.type_name, "String");
    }

    #[test]
    fn test_scala_val_declaration() {
        let mut ctx = ScopedTypeContext::new(Language::Scala);
        let entities = vec![
            Entity::new(
                EntityId(2),
                EntityKind::Variable,
                "name".to_string(),
                dummy_span(),
            )
            .with_metadata("var_type", "String"),
        ];

        ScalaTypeInferer.infer_declarations(&entities, &mut ctx);
        let vt = ctx.get_variable_type("name").unwrap();
        assert_eq!(vt.type_name, "String");
        assert!(vt.origin.is_some());
    }

    #[test]
    fn test_scala_constructor_call() {
        let mut ctx = ScopedTypeContext::new(Language::Scala);
        let entities = vec![
            Entity::new(
                EntityId(3),
                EntityKind::Variable,
                "list".to_string(),
                dummy_span(),
            )
            .with_metadata("constructor_type", "List[Int]"),
        ];

        ScalaTypeInferer.infer_declarations(&entities, &mut ctx);
        let vt = ctx.get_variable_type("list").unwrap();
        assert_eq!(vt.type_name, "List[Int]");
        assert!(vt.origin.is_some());
    }

    #[test]
    fn test_scala_isinstanceof() {
        let ctx = ScopedTypeContext::new(Language::Scala);
        let results = narrow_scala_if("if (x.isInstanceOf[String])", &ctx, &[]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "String");
        assert!(results[0].narrowed_type.origin.is_some());
    }

    #[test]
    fn test_scala_negated_isinstanceof_complement() {
        let ctx = ScopedTypeContext::new(Language::Scala);
        let params = [("x".to_string(), Some("String | Integer".to_string()))];
        let results = narrow_scala_if("if (!(x.isInstanceOf[String]))", &ctx, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "Integer");
    }

    #[test]
    fn test_scala_negated_isinstanceof_plain_type_skipped() {
        let ctx = ScopedTypeContext::new(Language::Scala);
        let params = [("obj".to_string(), Some("Any".to_string()))];
        let results = narrow_scala_if("if (!obj.isInstanceOf[String])", &ctx, &params);
        assert!(results.is_empty());
    }

    #[test]
    fn test_scala_isinstanceof_false_suffix_complement() {
        let ctx = ScopedTypeContext::new(Language::Scala);
        let params = [("x".to_string(), Some("String | Integer".to_string()))];
        let results = narrow_scala_if("if (x.isInstanceOf[String] == false)", &ctx, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "Integer");
    }

    #[test]
    fn test_scala_isinstanceof_false_suffix_never_positive() {
        // The `== false` suffix must not fall through into a positive
        // binding when no union shape supports the complement.
        let ctx = ScopedTypeContext::new(Language::Scala);
        let params = [("obj".to_string(), Some("Any".to_string()))];
        let results = narrow_scala_if("if (obj.isInstanceOf[String] == false)", &ctx, &params);
        assert!(results.is_empty());
    }

    #[test]
    fn test_scala_not_null_narrows_declared() {
        let ctx = ScopedTypeContext::new(Language::Scala);
        let params = [("value".to_string(), Some("String".to_string()))];
        let results = narrow_scala_if("if (value != null)", &ctx, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "value");
        assert_eq!(results[0].narrowed_type.type_name, "String");
    }

    #[test]
    fn test_scala_equal_null_binds_null() {
        let ctx = ScopedTypeContext::new(Language::Scala);
        let results = narrow_scala_if("if (value == null)", &ctx, &[]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "value");
        assert_eq!(results[0].narrowed_type.type_name, "null");
    }

    #[test]
    fn test_scala_match_typed_pattern() {
        let ctx = ScopedTypeContext::new(Language::Scala);
        let results =
            narrow_scala_match("x match { case s: String => s, case _ => \"\" }", &ctx, &[]);
        assert!(!results.is_empty());
        assert_eq!(results[0].variable_name, "s");
        assert_eq!(results[0].narrowed_type.type_name, "String");
    }

    #[test]
    fn test_scala_match_wildcard_skipped() {
        let ctx = ScopedTypeContext::new(Language::Scala);
        let results = narrow_scala_match("x match { case _ => 0 }", &ctx, &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn test_scala_match_multi_arm() {
        let ctx = ScopedTypeContext::new(Language::Scala);
        let results = narrow_scala_match(
            "x match { case s: String => s.length, case n: Int => n }",
            &ctx,
            &[],
        );
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].variable_name, "s");
        assert_eq!(results[0].narrowed_type.type_name, "String");
        assert_eq!(results[1].variable_name, "n");
        assert_eq!(results[1].narrowed_type.type_name, "Int");
    }

    #[test]
    fn test_scala_match_some_arm_binds_inner() {
        let mut ctx = ScopedTypeContext::new(Language::Scala);
        ctx.add_variable_type(
            "opt".to_string(),
            TypeBinding {
                type_name: "Option[String]".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: parse_type_shape("Option[String]", Language::Scala),
            },
        );
        let results = narrow_scala_match(
            "opt match { case Some(v) => v, case None => \"\" }",
            &ctx,
            &[],
        );
        assert!(
            results
                .iter()
                .any(|r| r.variable_name == "v" && r.narrowed_type.type_name == "String"),
            "Some arm should bind v: String, got {results:?}"
        );
        assert!(
            results
                .iter()
                .any(|r| r.variable_name == "opt" && r.narrowed_type.type_name == "Some<String>"),
            "Some arm should bind scrutinee to Some<String>, got {results:?}"
        );
        assert!(
            results
                .iter()
                .any(|r| r.variable_name == "opt" && r.narrowed_type.type_name == "None"),
            "None arm should bind scrutinee to None, got {results:?}"
        );
    }

    #[test]
    fn test_scala_match_some_arm_without_option_stays_empty() {
        let ctx = ScopedTypeContext::new(Language::Scala);
        let params = [("opt".to_string(), Some("String".to_string()))];
        let results = narrow_scala_match("opt match { case Some(v) => v }", &ctx, &params);
        assert!(results.is_empty());
    }
}
