//! Go-specific type inference.
//!
//! Reference: `docs/research/go-types-inference.md`

use cce_types::ControlFlowFactKind;
use cce_types::ControlFlowStore;
use cce_types::Language;
use cce_types::Span;
use cce_types::entity::{Entity, EntityKind};

use super::control_flow::shared::{extract_balanced_parens, is_valid_ident, strip_outer_parens};
use super::extractors::{extract_field_type, extract_function_types, extract_variable_type};
use super::traits::LanguageTypeInferer;
use super::types::{
    ScopedTypeContext, TypeBinding, add_polarity_aware_narrowings, declared_shape,
    type_shape_to_string,
};

/// Go type inference implementation.
///
/// Handles Go-specific patterns:
/// - Function signatures with receiver types
/// - Short variable declarations (`:=`)
/// - Receiver type binding for method call resolution
/// - Struct field types
pub struct GoTypeInferer;

impl LanguageTypeInferer for GoTypeInferer {
    fn infer_declarations(&self, entities: &[Entity], ctx: &mut ScopedTypeContext) {
        for entity in entities {
            match entity.kind {
                EntityKind::Function | EntityKind::Method => {
                    extract_function_types(entity, ctx);

                    if entity.kind == EntityKind::Method {
                        if let Some(receiver_type) = entity.metadata.get("receiver_type") {
                            if let Some(receiver_var) = extract_receiver_var_name(&entity.signature)
                            {
                                let binding = TypeBinding {
                                    type_name: receiver_type.clone(),
                                    type_entity_id: None,
                                    span: entity.span,
                                    origin: Some(super::types::InferenceOrigin::TypeAnnotation),
                                    shape: None,
                                };
                                ctx.add_variable_type(receiver_var, binding);
                            }
                        }
                    }
                }
                EntityKind::Variable => {
                    extract_variable_type(entity, ctx);

                    if let Some(inferred_type) = entity.metadata.get("inferred_type") {
                        let binding = TypeBinding {
                            type_name: inferred_type.clone(),
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(super::types::InferenceOrigin::GenericInference),
                            shape: None,
                        };
                        ctx.add_variable_type(entity.name.clone(), binding);
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
                            narrow_go_if(&fact.text, ctx, &entity.parameters)
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
                            Language::Go,
                            fact,
                            &narrowed,
                        );
                    }
                    ControlFlowFactKind::Match => {
                        for result in narrow_go_type_switch(&fact.text) {
                            ctx.add_narrowed_type(result.variable_name, result.narrowed_type);
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

/// Narrow types from a Go `if` condition.
///
/// Patterns:
/// - `if s, ok := value.(string); ok` → s: string (type assertion)
/// - `if err != nil` → err: error (only for error-like declarations or
///   err/Err-prefixed names; other `x != nil` stay conservative)
/// - `if err == nil` → conservative skip (we only narrow true branch)
fn narrow_go_if(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    let text = text.trim();
    let mut results = vec![];

    if let Some(result) = narrow_go_type_assertion(text) {
        results.push(result);
        return results;
    }

    if let Some(result) = narrow_go_err_not_nil(text, ctx, params) {
        results.push(result);
    }

    results
}

/// Parse `if s, ok := value.(Type); ok` → s: Type.
///
/// The comma-ok form binds the assertion target directly; a negated
/// check (`!ok`, typically an early return) is skipped.
fn narrow_go_type_assertion(text: &str) -> Option<NarrowingResult> {
    let cond = strip_go_if_prefix(text)?.trim();
    let (init, check) = cond.split_once(';')?;
    if check.trim() != "ok" {
        return None;
    }
    let init = init.trim();
    let assert_pos = init.rfind(".(")?;
    let after = &init[assert_pos + 2..];
    let close = after.find(')')?;
    let asserted = after[..close].trim();
    if asserted.is_empty() || asserted.contains([';', '{', '}', '"', '\'']) {
        return None;
    }
    let lhs = init[..assert_pos].trim();
    // Alias is the first name before `:=` (or `=`); the RHS must end with
    // the assertion, e.g. `s, ok := value.(string)`.
    let decl_end = lhs.find(":=").or_else(|| lhs.find('='))?;
    let alias = lhs[..decl_end].split(',').next()?.trim();
    if alias.is_empty() || !is_valid_ident(alias) {
        return None;
    }
    Some(NarrowingResult {
        variable_name: alias.to_string(),
        narrowed_type: TypeBinding {
            type_name: asserted.to_string(),
            type_entity_id: None,
            span: Span::default(),
            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
            shape: None,
        },
    })
}

/// Narrow types from a Go type switch.
///
/// `switch v := value.(type) { case string: ... }` binds the alias once
/// per single-type case. Multi-type cases keep the scrutinee type and
/// `default` carries no type, so both are skipped.
fn narrow_go_type_switch(text: &str) -> Vec<NarrowingResult> {
    let text = text.trim();
    let Some((header, body)) = text.split_once('{') else {
        return vec![];
    };
    let Some(rest) = header.trim().strip_prefix("switch").map(str::trim) else {
        return vec![];
    };
    if !rest.contains(".(type)") {
        return vec![];
    }
    let Some(decl_end) = rest.find(":=").or_else(|| rest.find('=')) else {
        return vec![];
    };
    let alias = rest[..decl_end].split(',').next().unwrap_or("").trim();
    if alias.is_empty() || !is_valid_ident(alias) {
        return vec![];
    }
    let mut results = vec![];
    for case in split_go_cases(body) {
        let after_case = case.trim().strip_prefix("case").unwrap_or("").trim();
        let case_type = after_case
            .split_once(':')
            .map(|(t, _)| t.trim())
            .unwrap_or("");
        if case_type.is_empty()
            || case_type == "default"
            || case_type == "nil"
            || case_type.contains(',')
            || !is_valid_go_type(case_type)
        {
            continue;
        }
        results.push(NarrowingResult {
            variable_name: alias.to_string(),
            narrowed_type: TypeBinding {
                type_name: case_type.to_string(),
                type_entity_id: None,
                span: Span::default(),
                origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                shape: None,
            },
        });
    }
    results
}

/// Split a type-switch body into its `case ...` segments.
///
/// Only `case` keywords bounded by non-identifier characters and followed
/// by whitespace qualify, so identifiers like `showcase` never split.
fn split_go_cases(body: &str) -> Vec<&str> {
    let mut starts: Vec<usize> = vec![];
    for (i, _) in body.match_indices("case") {
        let before_ok = body[..i]
            .chars()
            .last()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        let after_ok = body[i + 4..]
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace());
        if before_ok && after_ok {
            starts.push(i);
        }
    }
    starts
        .iter()
        .enumerate()
        .map(|(k, &s)| {
            let end = starts.get(k + 1).copied().unwrap_or(body.len());
            &body[s..end]
        })
        .collect()
}

/// Check that a type-switch case label looks like a Go type.
///
/// Accepts qualified, pointer, slice, and map forms (`pkg.Type`,
/// `*Greeter`, `[]int`, `map[string]int`) while rejecting string
/// literals and expression fragments.
fn is_valid_go_type(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() || text.contains(char::is_whitespace) {
        return false;
    }
    !text.contains(['"', '\'', ';', '{', '}', '(', ')', '=', '!'])
}

/// Parse `if err != nil` → err: error.
///
/// Go convention: error interface values are checked with `!= nil`.
/// The binding is kept only when the variable name suggests an error
/// (`err`/`Err` prefix) or the declared shape mentions `error`; other
/// `x != nil` checks stay conservative instead of mislabeling.
fn narrow_go_err_not_nil(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Option<NarrowingResult> {
    let text = strip_go_if_prefix(text)?;
    let text = text.trim();

    let parts: Vec<&str> = text.splitn(2, "!=").collect();
    if parts.len() != 2 {
        return None;
    }
    let var_name = parts[0].trim();
    let rhs = parts[1].trim();

    if rhs != "nil" || var_name.is_empty() || !is_valid_ident(var_name) {
        return None;
    }

    let name_suggests_error = var_name.starts_with("err") || var_name.starts_with("Err");
    let declared_is_error = declared_shape(ctx, params, Language::Go, var_name)
        .map(|shape| type_shape_to_string(&shape).contains("error"))
        .unwrap_or(false);
    if !(name_suggests_error || declared_is_error) {
        return None;
    }

    Some(NarrowingResult {
        variable_name: var_name.to_string(),
        narrowed_type: TypeBinding {
            type_name: "error".to_string(),
            type_entity_id: None,
            span: Span::default(),
            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
            shape: None,
        },
    })
}

/// Strip Go `if` prefix.
///
/// Fact text carries the branch body (`if err != nil { ... }`), so take
/// the balanced paren group when parenthesized; otherwise the condition
/// runs until the branch body `{`.
fn strip_go_if_prefix(text: &str) -> Option<&str> {
    let text = text.trim();
    let rest = text.strip_prefix("if")?.trim();
    if let Some(inner) = extract_balanced_parens(rest) {
        return Some(inner);
    }
    let cond = rest.split_once('{').map(|(c, _)| c).unwrap_or(rest);
    Some(strip_outer_parens(cond.trim()))
}

/// Extract the receiver variable name from a Go method signature.
///
/// Signature patterns:
/// - `func (r *Receiver) Method()` → `r`
/// - `func (s Receiver) Method()` → `s`
/// - `func Method()` → None (no receiver)
fn extract_receiver_var_name(signature: &str) -> Option<String> {
    let sig = signature.trim();
    let sig = sig.strip_prefix("func")?.trim();
    let paren_start = sig.find('(')?;
    let paren_end = sig.find(')')?;
    let receiver_content = sig[paren_start + 1..paren_end].trim();
    let var_name = receiver_content.split_whitespace().next()?;
    if var_name.is_empty()
        || !var_name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
    {
        return None;
    }
    Some(var_name.to_string())
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
    fn test_go_function_signature_extraction() {
        let mut ctx = ScopedTypeContext::new(Language::Go);
        let entities = vec![
            Entity::new(
                EntityId(1),
                EntityKind::Function,
                "Add".to_string(),
                dummy_span(),
            )
            .with_return_type(Some("int".to_string()))
            .with_parameters(vec![
                ("a".to_string(), Some("int".to_string())),
                ("b".to_string(), Some("int".to_string())),
            ]),
        ];

        GoTypeInferer.infer_declarations(&entities, &mut ctx);

        let return_type = ctx.get_return_type(EntityId(1)).unwrap();
        assert_eq!(return_type.type_name, "int");
    }

    #[test]
    fn test_go_short_variable_declaration() {
        let mut ctx = ScopedTypeContext::new(Language::Go);
        let entities = vec![
            Entity::new(
                EntityId(2),
                EntityKind::Variable,
                "name".to_string(),
                dummy_span(),
            )
            .with_metadata("inferred_type", "string"),
        ];

        GoTypeInferer.infer_declarations(&entities, &mut ctx);

        let var_type = ctx.get_variable_type("name").unwrap();
        assert_eq!(var_type.type_name, "string");
        assert!(var_type.origin.is_some());
    }

    #[test]
    fn test_go_receiver_type_binding() {
        let mut ctx = ScopedTypeContext::new(Language::Go);
        let mut entity = Entity::new(
            EntityId(3),
            EntityKind::Method,
            "GetValue".to_string(),
            dummy_span(),
        );
        entity.signature = "func (s *MyStruct) GetValue() int".to_string();
        entity
            .metadata
            .insert("receiver_type".to_string(), "*MyStruct".to_string());

        GoTypeInferer.infer_declarations(&[entity], &mut ctx);

        let receiver_type = ctx.get_variable_type("s").unwrap();
        assert_eq!(receiver_type.type_name, "*MyStruct");
        assert!(receiver_type.origin.is_some());
    }

    #[test]
    fn test_go_receiver_var_name_parsing() {
        assert_eq!(
            extract_receiver_var_name("func (r *Receiver) Method()"),
            Some("r".to_string())
        );
        assert_eq!(
            extract_receiver_var_name("func (s Receiver) Method()"),
            Some("s".to_string())
        );
        assert_eq!(extract_receiver_var_name("func Method()"), None);
        assert_eq!(
            extract_receiver_var_name("func (self *MyType) Do()"),
            Some("self".to_string())
        );
    }

    #[test]
    fn test_go_err_not_nil() {
        let ctx = ScopedTypeContext::new(Language::Go);
        let results = narrow_go_if("if err != nil {", &ctx, &[]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "err");
        assert_eq!(results[0].narrowed_type.type_name, "error");
        assert!(results[0].narrowed_type.origin.is_some());
    }

    #[test]
    fn test_go_err_not_nil_with_return() {
        let ctx = ScopedTypeContext::new(Language::Go);
        let results = narrow_go_if("if err != nil", &ctx, &[]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "err");
    }

    #[test]
    fn test_go_err_not_nil_with_body() {
        // Fact text carries the branch body; the condition must still parse.
        let ctx = ScopedTypeContext::new(Language::Go);
        let results = narrow_go_if("if err != nil { return err.Error() }", &ctx, &[]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "err");
        assert_eq!(results[0].narrowed_type.type_name, "error");
    }

    #[test]
    fn test_go_err_equal_nil_skipped() {
        let ctx = ScopedTypeContext::new(Language::Go);
        let results = narrow_go_if("if err == nil {", &ctx, &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn test_go_non_error_var_not_nil_skipped() {
        // Non-error variables must not be mislabeled as `error`.
        let ctx = ScopedTypeContext::new(Language::Go);
        let results = narrow_go_if("if val != nil {", &ctx, &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn test_go_declared_error_not_nil() {
        // A declared `error` shape keeps the binding even without an
        // err-like name.
        let ctx = ScopedTypeContext::new(Language::Go);
        let params = [("e".to_string(), Some("error".to_string()))];
        let results = narrow_go_if("if e != nil { return e }", &ctx, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "e");
        assert_eq!(results[0].narrowed_type.type_name, "error");
    }

    #[test]
    fn test_go_err_prefix_not_nil() {
        let ctx = ScopedTypeContext::new(Language::Go);
        let results = narrow_go_if("if ErrConn != nil { return ErrConn }", &ctx, &[]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "ErrConn");
        assert_eq!(results[0].narrowed_type.type_name, "error");
    }

    #[test]
    fn test_go_ok_assertion() {
        let ctx = ScopedTypeContext::new(Language::Go);
        let results = narrow_go_if("if s, ok := value.(string); ok { return s }", &ctx, &[]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "s");
        assert_eq!(results[0].narrowed_type.type_name, "string");
        assert!(results[0].narrowed_type.origin.is_some());
    }

    #[test]
    fn test_go_ok_assertion_int() {
        let ctx = ScopedTypeContext::new(Language::Go);
        let results = narrow_go_if("if n, ok := value.(int); ok { return n }", &ctx, &[]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "n");
        assert_eq!(results[0].narrowed_type.type_name, "int");
    }

    #[test]
    fn test_go_ok_assertion_negated_skipped() {
        // `!ok` (typically an early return) binds nothing.
        let ctx = ScopedTypeContext::new(Language::Go);
        let results = narrow_go_if("if s, ok := value.(string); !ok { return \"\" }", &ctx, &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn test_go_type_switch() {
        let results = narrow_go_type_switch(
            "switch v := value.(type) { case string: return v case int: return v }",
        );
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].variable_name, "v");
        assert_eq!(results[0].narrowed_type.type_name, "string");
        assert_eq!(results[1].variable_name, "v");
        assert_eq!(results[1].narrowed_type.type_name, "int");
    }

    #[test]
    fn test_go_type_switch_multi_and_default_skipped() {
        // Multi-type cases keep the scrutinee type; `default` carries none.
        let results = narrow_go_type_switch(
            "switch v := value.(type) { case string, int: return v case Greeter: return v default: return \"x\" }",
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "v");
        assert_eq!(results[0].narrowed_type.type_name, "Greeter");
    }

    #[test]
    fn test_go_type_switch_no_alias_skipped() {
        let results = narrow_go_type_switch("switch value.(type) { case string: return 1 }");
        assert!(results.is_empty());
    }
}
