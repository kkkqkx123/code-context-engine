//! Rust-specific type inference.

use cce_types::ControlFlowFactKind;
use cce_types::ControlFlowStore;
use cce_types::Span;
use cce_types::entity::{Entity, EntityId, EntityKind};
use std::collections::HashMap;

use super::control_flow::shared::{extract_balanced_parens, is_valid_ident};
use super::extractors::{extract_field_type, extract_variable_type};
use super::traits::LanguageTypeInferer;
use super::types::{
    InferenceOrigin, ScopedTypeContext, TypeBinding, TypeShape, add_polarity_aware_narrowings,
    declared_shape, parse_type_shape, strip_references, type_shape_to_string,
};

/// Display name for a Rust type binding.
///
/// Reference types keep their borrow spelling (`&str`, `&mut Vec<T>`)
/// so the inferred name agrees with the structured shape instead of
/// degrading to the bare inner type. Lifetimes normalize away
/// (`&'a str` renders as `&str`, matching the shape parser).
fn rust_binding_name(original: &str, shape: &Option<TypeShape>) -> String {
    match shape {
        Some(TypeShape::Reference { .. }) => {
            type_shape_to_string(shape.as_ref().expect("matched reference shape"))
        }
        _ => original.to_string(),
    }
}
use cce_types::language::Language;

/// Rust type inference implementation.
pub struct RustTypeInferer;

impl LanguageTypeInferer for RustTypeInferer {
    fn infer_declarations(&self, entities: &[Entity], ctx: &mut ScopedTypeContext) {
        // First pass: collect impl block Self types
        let impl_self_types: HashMap<EntityId, String> = entities
            .iter()
            .filter(|e| e.kind.is_impl_block())
            .filter_map(|e| {
                e.metadata
                    .get("self_type")
                    .map(|self_type| (e.id, self_type.clone()))
            })
            .collect();

        for entity in entities {
            match entity.kind {
                EntityKind::Function | EntityKind::Method => {
                    // Handle Rust borrow/reference stripping for return and param types
                    if let Some(ref return_type) = entity.return_type {
                        let (base, is_mut, is_ref) = strip_references(return_type);
                        let shape = if is_ref {
                            Some(TypeShape::Reference {
                                inner: Box::new(TypeShape::Named(base.clone())),
                                mutable: is_mut,
                            })
                        } else {
                            parse_type_shape(return_type, ctx.language())
                        };
                        let type_name = rust_binding_name(return_type, &shape);
                        let binding = TypeBinding {
                            type_name,
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(InferenceOrigin::TypeAnnotation),
                            shape,
                        };
                        ctx.add_return_type(entity.id, binding);
                    }

                    let mut param_bindings: Vec<TypeBinding> = Vec::new();
                    for (param_name, param_type) in &entity.parameters {
                        if let Some(ty) = param_type {
                            let (base, is_mut, is_ref) = strip_references(ty);
                            let shape = if is_ref {
                                Some(TypeShape::Reference {
                                    inner: Box::new(TypeShape::Named(base.clone())),
                                    mutable: is_mut,
                                })
                            } else {
                                parse_type_shape(ty, ctx.language())
                            };
                            let type_name = rust_binding_name(ty, &shape);
                            let binding = TypeBinding {
                                type_name: type_name.clone(),
                                type_entity_id: None,
                                span: entity.span,
                                origin: Some(InferenceOrigin::TypeAnnotation),
                                shape: shape.clone(),
                            };
                            param_bindings.push(binding);
                            // Also bind variable for use in function body,
                            // keeping the borrow spelling for fidelity.
                            let var_binding = TypeBinding {
                                type_name,
                                type_entity_id: None,
                                span: entity.span,
                                origin: Some(InferenceOrigin::TypeAnnotation),
                                shape,
                            };
                            ctx.add_variable_type(param_name.clone(), var_binding);
                        }
                    }
                    if !param_bindings.is_empty() {
                        ctx.add_parameter_types(entity.id, param_bindings);
                    }

                    // Rust-specific: infer method receiver type from parent impl block
                    if let Some(parent_id) = entity.parent {
                        if let Some(self_type) = impl_self_types.get(&parent_id) {
                            let (base, is_mut, is_ref) = strip_references(self_type);
                            let shape = if is_ref {
                                Some(TypeShape::Reference {
                                    inner: Box::new(TypeShape::Named(base.clone())),
                                    mutable: is_mut,
                                })
                            } else {
                                parse_type_shape(self_type, ctx.language())
                            };
                            let type_name = rust_binding_name(self_type, &shape);
                            let binding = TypeBinding {
                                type_name,
                                type_entity_id: None,
                                span: entity.span,
                                origin: Some(InferenceOrigin::TypeAnnotation),
                                shape,
                            };
                            ctx.add_variable_type("Self".to_string(), binding);
                        }
                    }
                }
                EntityKind::Variable => {
                    // Handle variable types, keeping borrow spellings so the
                    // inferred name agrees with the reference shape.
                    let mut handled = false;
                    if let Some(type_name) = entity.metadata.get("type_annotation") {
                        let (base, is_mut, is_ref) = strip_references(type_name);
                        let shape = if is_ref {
                            Some(TypeShape::Reference {
                                inner: Box::new(TypeShape::Named(base.clone())),
                                mutable: is_mut,
                            })
                        } else {
                            parse_type_shape(type_name, ctx.language())
                        };
                        let final_name = rust_binding_name(type_name, &shape);
                        let binding = TypeBinding {
                            type_name: final_name,
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(InferenceOrigin::TypeAnnotation),
                            shape,
                        };
                        ctx.add_variable_type(entity.name.clone(), binding);
                        handled = true;
                    } else if let Some(init_type) = entity.metadata.get("constructor_type") {
                        let (base, is_mut, is_ref) = strip_references(init_type);
                        let shape = if is_ref {
                            Some(TypeShape::Reference {
                                inner: Box::new(TypeShape::Named(base.clone())),
                                mutable: is_mut,
                            })
                        } else {
                            parse_type_shape(init_type, ctx.language())
                        };
                        let final_name = rust_binding_name(init_type, &shape);
                        let binding = TypeBinding {
                            type_name: final_name,
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(InferenceOrigin::ConstructorCall),
                            shape,
                        };
                        ctx.add_variable_type(entity.name.clone(), binding);
                        handled = true;
                    } else if let Some(lit_type) = entity.metadata.get("literal_type") {
                        let (base, is_mut, is_ref) = strip_references(lit_type);
                        let shape = if is_ref {
                            Some(TypeShape::Reference {
                                inner: Box::new(TypeShape::Named(base.clone())),
                                mutable: is_mut,
                            })
                        } else {
                            parse_type_shape(lit_type, ctx.language())
                        };
                        let final_name = rust_binding_name(lit_type, &shape);
                        let binding = TypeBinding {
                            type_name: final_name,
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(InferenceOrigin::LiteralType),
                            shape,
                        };
                        ctx.add_variable_type(entity.name.clone(), binding);
                        handled = true;
                    }
                    if !handled {
                        extract_variable_type(entity, ctx);
                    }
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
                            narrow_rust_if(&fact.text, ctx, &entity.parameters)
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
                            Language::Rust,
                            fact,
                            &narrowed,
                            entity.span,
                        );
                    }
                    ControlFlowFactKind::Match => {
                        for result in narrow_rust_match(&fact.text, ctx, &entity.parameters) {
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

/// Narrow types from a Rust `if` condition.
///
/// Patterns:
/// - `if let Some(val) = expr` → val: T (payload of the scrutinee's
///   `Option<T>` declaration)
/// - `if let Ok(val) = expr` → val: T (payload of `Result<T, E>`)
/// - `if let Err(e) = expr` → e: E (payload of `Result<T, E>`)
///
/// Without a resolvable scrutinee declaration the known enum wrappers
/// (`Some`/`Ok`/`Err`) stay unbound instead of leaking the variant name
/// as the payload type. Custom constructors keep their name since the
/// binder genuinely holds that type.
fn narrow_rust_if(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    let text = text.trim();
    narrow_rust_if_let(text, ctx, params)
}

/// Parse `if let Pattern(var) = expr` and extract the bound variable.
fn narrow_rust_if_let(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    let Some(text) = strip_rust_if_prefix(text) else {
        return vec![];
    };
    let text = text.trim();

    let Some(rest) = text.strip_prefix("let") else {
        return vec![];
    };
    let rest = rest.trim();
    let Some((pattern_text, scrutinee)) = split_rust_let_binding(rest) else {
        return vec![];
    };
    let Some((constructors, binder)) = parse_rust_pattern_chain(pattern_text) else {
        return vec![];
    };
    let scrutinee_name = scrutinee
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim()
        .trim_start_matches('&')
        .trim();
    let payload = declared_shape(ctx, params, Language::Rust, scrutinee_name)
        .and_then(|shape| rust_pattern_payload(&shape, &constructors));
    let Some(payload) = payload else {
        // Known enum wrappers without a resolvable declaration stay
        // conservative: the binder holds the payload, not the variant.
        if matches!(
            constructors.last().map(String::as_str),
            Some("Some" | "Ok" | "Err")
        ) {
            return vec![];
        }
        let type_name = constructors.last().cloned().unwrap_or_default();
        if type_name.is_empty() {
            return vec![];
        }
        return vec![NarrowingResult {
            variable_name: binder,
            narrowed_type: TypeBinding {
                type_name,
                type_entity_id: None,
                span: Span::default(),
                origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                shape: None,
            },
        }];
    };
    let type_name = type_shape_to_string(&payload);
    vec![NarrowingResult {
        variable_name: binder,
        narrowed_type: TypeBinding {
            type_name,
            type_entity_id: None,
            span: Span::default(),
            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
            shape: Some(payload),
        },
    }]
}

/// Split `Pattern = expr` on the top-level `=` of a let binding.
fn split_rust_let_binding(text: &str) -> Option<(&str, &str)> {
    let mut depth = 0;
    for (i, ch) in text.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '=' if depth == 0 => {
                // Skip `==`, `=>`, `>=`, `<=`, `!=`.
                let bytes = text.as_bytes();
                let prev = if i > 0 { bytes[i - 1] } else { b' ' };
                let next = bytes.get(i + 1).copied().unwrap_or(b' ');
                if matches!(prev, b'=' | b'!' | b'>' | b'<') || matches!(next, b'=' | b'>') {
                    continue;
                }
                return Some((text[..i].trim(), text[i + 1..].trim()));
            }
            _ => {}
        }
    }
    None
}

/// Parse a Rust pattern into its constructor path and bound variable.
///
/// `Some(Ok(val))` → (`["Some", "Ok"]`, `val`); `ref`/`mut`/`&` markers on
/// the binder are stripped (`Some(ref name)` binds `name`).
fn parse_rust_pattern_chain(text: &str) -> Option<(Vec<String>, String)> {
    let mut constructors = Vec::new();
    let mut rest = text.trim();
    loop {
        rest = rest.trim();
        let Some(paren) = rest.find('(') else {
            break;
        };
        let head = rest[..paren].trim();
        // Reject bindings with result expressions before the pattern.
        if head.is_empty() || head.contains(|c: char| c.is_whitespace() || c == ',' || c == ';') {
            return None;
        }
        let constructor = head.rsplit("::").next().unwrap_or(head).trim().to_string();
        if constructor.is_empty() || !is_valid_ident(&constructor) {
            return None;
        }
        let inner = extract_balanced_parens(&rest[paren..])?;
        constructors.push(constructor);
        rest = inner;
    }
    if constructors.is_empty() {
        return None;
    }
    let mut binder = rest.trim();
    while let Some(stripped) = binder
        .strip_prefix("ref ")
        .or_else(|| binder.strip_prefix("mut "))
    {
        binder = stripped.trim();
    }
    binder = binder.trim_start_matches('&').trim();
    if binder.is_empty() || !is_valid_ident(binder) {
        return None;
    }
    Some((constructors, binder.to_string()))
}

/// Resolve the payload type at the end of a constructor path.
///
/// `Some` unwraps `Option<T>`; `Ok`/`Err` select from `Result<T, E>`.
/// Anything else (unknown declaration, mismatched constructor) yields
/// `None` so callers keep the constructor name instead of guessing.
fn rust_pattern_payload(shape: &TypeShape, constructors: &[String]) -> Option<TypeShape> {
    let mut current = shape.clone();
    for constructor in constructors {
        let args = match &current {
            TypeShape::Generic { base, args } => {
                if (base == "Option" || base == "Optional")
                    && constructor == "Some"
                    && args.len() == 1
                {
                    args[0].clone()
                } else if base == "Result" && args.len() == 2 {
                    match constructor.as_str() {
                        "Ok" => args[0].clone(),
                        "Err" => args[1].clone(),
                        _ => return None,
                    }
                } else {
                    return None;
                }
            }
            _ => return None,
        };
        current = args;
    }
    Some(current)
}

/// Narrow types from a Rust `match` arm pattern.
fn narrow_rust_match(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    let text = text.trim();

    let rest = text.strip_prefix("match").unwrap_or(text);
    if let Some(brace_start) = rest.find('{') {
        let scrutinee = rest[..brace_start]
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim()
            .trim_start_matches('&')
            .trim();
        let declared = declared_shape(ctx, params, Language::Rust, scrutinee);
        let body = &rest[brace_start + 1..];
        narrow_rust_match_arms(body, declared.as_ref())
    } else {
        vec![]
    }
}

/// Extract variable bindings from match arm patterns.
fn narrow_rust_match_arms(arms_text: &str, declared: Option<&TypeShape>) -> Vec<NarrowingResult> {
    let mut results = vec![];
    for arm in arms_text.split("=>") {
        let arm = arm.trim();
        if let Some(result) = parse_rust_match_arm_pattern(arm, declared) {
            results.push(result);
        }
    }
    results
}

/// Parse a single match arm pattern to extract variable bindings.
///
/// Nested patterns (`Some(Ok(val))`) resolve through the scrutinee
/// declaration; known enum wrappers (`Some`/`Ok`/`Err`) without one stay
/// unbound instead of leaking the variant name. Anything else stays
/// unbound: without the scrutinee declaration there is no payload shape
/// to resolve, so no binding is emitted.
fn parse_rust_match_arm_pattern(
    text: &str,
    declared: Option<&TypeShape>,
) -> Option<NarrowingResult> {
    let text = text.trim();

    for constructor in &["Some", "Ok", "Err"] {
        if let Some(pos) = text.find(&format!("{constructor}(")) {
            let pattern_text = &text[pos..];
            let (constructors, binder) = parse_rust_pattern_chain(pattern_text)?;
            let payload = declared.and_then(|shape| rust_pattern_payload(shape, &constructors))?;
            let type_name = type_shape_to_string(&payload);
            if type_name.is_empty() {
                return None;
            }
            return Some(NarrowingResult {
                variable_name: binder,
                narrowed_type: TypeBinding {
                    type_name,
                    type_entity_id: None,
                    span: Span::default(),
                    origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                    shape: Some(payload),
                },
            });
        }
    }
    None
}

/// Strip Rust `if` prefix.
fn strip_rust_if_prefix(text: &str) -> Option<&str> {
    let text = text.trim();
    text.strip_prefix("if").map(|rest| rest.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rust_ctx_with(
        params: Vec<(&str, &str)>,
    ) -> (ScopedTypeContext, Vec<(String, Option<String>)>) {
        let ctx = ScopedTypeContext::new(Language::Rust);
        let owned = params
            .into_iter()
            .map(|(name, ty)| (name.to_string(), Some(ty.to_string())))
            .collect();
        (ctx, owned)
    }

    #[test]
    fn test_rust_if_let_some_payload() {
        let (ctx, params) = rust_ctx_with(vec![("input", "Option<String>")]);
        let results = narrow_rust_if("if let Some(val) = input {", &ctx, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "val");
        assert_eq!(results[0].narrowed_type.type_name, "String");
    }

    #[test]
    fn test_rust_if_let_some_unknown_stays_unbound() {
        let (ctx, params) = rust_ctx_with(vec![]);
        let results = narrow_rust_if("if let Some(val) = input {", &ctx, &params);
        assert!(results.is_empty());
    }

    #[test]
    fn test_rust_if_let_ok_payload() {
        let (ctx, params) = rust_ctx_with(vec![("result", "Result<i32, String>")]);
        let results = narrow_rust_if("if let Ok(val) = result {", &ctx, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "val");
        assert_eq!(results[0].narrowed_type.type_name, "i32");
    }

    #[test]
    fn test_rust_if_let_err_payload() {
        let (ctx, params) = rust_ctx_with(vec![("result", "Result<i32, String>")]);
        let results = narrow_rust_if("if let Err(e) = result {", &ctx, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "e");
        assert_eq!(results[0].narrowed_type.type_name, "String");
    }

    #[test]
    fn test_rust_if_let_ref_binder() {
        let (ctx, params) = rust_ctx_with(vec![("x", "Option<String>")]);
        let results = narrow_rust_if("if let Some(ref name) = x {", &ctx, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "name");
        assert_eq!(results[0].narrowed_type.type_name, "String");
    }

    #[test]
    fn test_rust_if_let_custom_type() {
        let (ctx, params) = rust_ctx_with(vec![]);
        let results = narrow_rust_if("if let Wrapper(inner) = data {", &ctx, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "inner");
        assert_eq!(results[0].narrowed_type.type_name, "Wrapper");
    }

    #[test]
    fn test_rust_match_some_arm_payload() {
        let (ctx, params) = rust_ctx_with(vec![("opt", "Option<String>")]);
        let results = narrow_rust_match("match opt { Some(val) => val, None => 0 }", &ctx, &params);
        assert!(!results.is_empty());
        assert_eq!(results[0].variable_name, "val");
        assert_eq!(results[0].narrowed_type.type_name, "String");
    }

    #[test]
    fn test_rust_match_nested_payload() {
        let (ctx, params) = rust_ctx_with(vec![("opt", "Option<Result<i32, String>>")]);
        let results = narrow_rust_match(
            "match opt { Some(Ok(val)) => val, Some(Err(e)) => e, None => 0 }",
            &ctx,
            &params,
        );
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].variable_name, "val");
        assert_eq!(results[0].narrowed_type.type_name, "i32");
        assert_eq!(results[1].variable_name, "e");
        assert_eq!(results[1].narrowed_type.type_name, "String");
    }

    #[test]
    fn test_rust_strip_reference_str() {
        use crate::type_inference::types::{is_mut_reference, is_reference, strip_references};
        let (base, is_mut, is_ref) = strip_references("&str");
        assert_eq!(base, "str");
        assert!(!is_mut);
        assert!(is_ref);
        assert!(is_reference("&str"));
        assert!(!is_mut_reference("&str"));
    }

    #[test]
    fn test_rust_strip_mut_reference() {
        use crate::type_inference::types::{is_mut_reference, strip_references};
        let (base, is_mut, is_ref) = strip_references("&mut Vec<T>");
        assert_eq!(base, "Vec<T>");
        assert!(is_mut);
        assert!(is_ref);
        assert!(is_mut_reference("&mut Vec<T>"));
    }

    #[test]
    fn test_rust_lifetime_ignored() {
        use crate::type_inference::types::strip_references;
        let (base, _, is_ref) = strip_references("&'a str");
        assert_eq!(base, "str");
        assert!(is_ref);
    }

    #[test]
    fn test_rust_borrow_param_inference() {
        let mut ctx = ScopedTypeContext::new(cce_types::language::Language::Rust);
        let mut entity = Entity::new(
            EntityId(1),
            EntityKind::Function,
            "foo".to_string(),
            Span::default(),
        );
        entity.parameters = vec![("s".to_string(), Some("&str".to_string()))];
        entity.return_type = Some("&str".to_string());
        RustTypeInferer.infer_declarations(&[entity], &mut ctx);
        // Borrow spellings are preserved in the inferred name.
        let binding = ctx.get_variable_type("s").expect("param s");
        assert_eq!(binding.type_name, "&str");
        assert!(matches!(binding.shape, Some(TypeShape::Reference { .. })));
        // Return type keeps the borrow as well
        let ret = ctx.get_return_type(EntityId(1)).expect("return");
        assert_eq!(ret.type_name, "&str");
    }

    #[test]
    fn test_rust_mut_reference_inference() {
        let mut ctx = ScopedTypeContext::new(cce_types::language::Language::Rust);
        let mut entity = Entity::new(
            EntityId(2),
            EntityKind::Function,
            "bar".to_string(),
            Span::default(),
        );
        entity.parameters = vec![("v".to_string(), Some("&mut Vec<T>".to_string()))];
        RustTypeInferer.infer_declarations(&[entity], &mut ctx);
        let binding = ctx.get_variable_type("v").expect("param v");
        assert_eq!(binding.type_name, "&mut Vec<T>");
        if let Some(TypeShape::Reference { mutable, .. }) = &binding.shape {
            assert!(*mutable);
        } else {
            panic!("expected reference shape");
        }
    }
}
