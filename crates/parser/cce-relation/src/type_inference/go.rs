//! Go-specific type inference.
//!
//! Reference: `docs/research/go-types-inference.md`

use cce_types::ControlFlowFactKind;
use cce_types::ControlFlowStore;
use cce_types::Span;
use cce_types::entity::{Entity, EntityKind};

use super::control_flow::shared::{is_valid_ident, strip_outer_parens};
use super::extractors::{extract_field_type, extract_function_types, extract_variable_type};
use super::traits::LanguageTypeInferer;
use super::types::{ScopedTypeContext, TypeBinding};

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
                    ControlFlowFactKind::If => {
                        for result in narrow_go_if(&fact.text) {
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
/// - `if err != nil` → err: error (true branch)
/// - `if err == nil` → conservative skip (we only narrow true branch)
/// - `if val != nil` → conservative skip (no concrete type)
fn narrow_go_if(text: &str) -> Vec<NarrowingResult> {
    let text = text.trim();
    let mut results = vec![];

    if let Some(result) = narrow_go_err_not_nil(text) {
        results.push(result);
    }

    results
}

/// Parse `if err != nil` → err: error.
///
/// Go convention: error interface values are checked with `!= nil`.
/// We narrow `err` to the `error` type in the true branch.
fn narrow_go_err_not_nil(text: &str) -> Option<NarrowingResult> {
    let text = strip_go_if_prefix(text)?;
    let text = text.trim().trim_end_matches('{').trim();

    let parts: Vec<&str> = text.splitn(2, "!=").collect();
    if parts.len() != 2 {
        return None;
    }
    let var_name = parts[0].trim();
    let rhs = parts[1].trim();

    if rhs == "nil" && !var_name.is_empty() && is_valid_ident(var_name) {
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
    } else {
        None
    }
}

/// Strip Go `if` prefix.
fn strip_go_if_prefix(text: &str) -> Option<&str> {
    let text = text.trim();
    text.strip_prefix("if")
        .map(|rest| strip_outer_parens(rest.trim()))
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
        let results = narrow_go_if("if err != nil {");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "err");
        assert_eq!(results[0].narrowed_type.type_name, "error");
        assert!(results[0].narrowed_type.origin.is_some());
    }

    #[test]
    fn test_go_err_not_nil_with_return() {
        let results = narrow_go_if("if err != nil");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "err");
    }

    #[test]
    fn test_go_err_equal_nil_skipped() {
        let results = narrow_go_if("if err == nil {");
        assert!(results.is_empty());
    }
}
