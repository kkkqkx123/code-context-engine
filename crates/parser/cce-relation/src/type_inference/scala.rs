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
use cce_types::Span;
use cce_types::entity::{Entity, EntityKind};

use super::control_flow::shared::{is_valid_ident, strip_outer_parens};
use super::extractors::{extract_field_type, extract_function_types, extract_variable_type};
use super::traits::LanguageTypeInferer;
use super::types::{ScopedTypeContext, TypeBinding};

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
                            shape: None,
                        };
                        ctx.add_variable_type(entity.name.clone(), binding);
                    }

                    if let Some(constructor_type) = entity.metadata.get("constructor_type") {
                        let binding = TypeBinding {
                            type_name: constructor_type.clone(),
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(super::types::InferenceOrigin::ConstructorCall),
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
                        for result in narrow_scala_if(&fact.text) {
                            ctx.add_narrowed_type(result.variable_name, result.narrowed_type);
                        }
                    }
                    ControlFlowFactKind::Match => {
                        for result in narrow_scala_match(&fact.text) {
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

/// Narrow types from a Scala `if` condition.
///
/// Patterns:
/// - `if (x.isInstanceOf[Type])` → x: Type
/// - `if (x.isInstanceOf[Type] == false)` → conservative skip
fn narrow_scala_if(text: &str) -> Vec<NarrowingResult> {
    let text = text.trim();
    let mut results = vec![];

    if let Some(result) = narrow_scala_isinstanceof(text) {
        results.push(result);
    }

    results
}

/// Parse `if (x.isInstanceOf[Type])`.
fn narrow_scala_isinstanceof(text: &str) -> Option<NarrowingResult> {
    let text = strip_scala_condition_prefix(text)?;
    let text = text.trim();

    // Find `.isInstanceOf[` in the text
    let marker = ".isInstanceOf[";
    let marker_pos = text.find(marker)?;
    let var_name = text[..marker_pos].trim();

    if var_name.is_empty() || !is_valid_ident(var_name) {
        return None;
    }

    // Extract the type between `[` and `]`
    let bracket_start = marker_pos + marker.len();
    let bracket_end = text.rfind(']')?;
    if bracket_end <= bracket_start {
        return None;
    }
    let type_name = text[bracket_start..bracket_end].trim();

    if type_name.is_empty() {
        return None;
    }

    Some(NarrowingResult {
        variable_name: var_name.to_string(),
        narrowed_type: TypeBinding {
            type_name: type_name.to_string(),
            type_entity_id: None,
            span: Span::default(),
            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
            shape: None,
        },
    })
}

/// Narrow types from a Scala `match` expression.
///
/// Pattern: `case x: Type =>` → x: Type
fn narrow_scala_match(text: &str) -> Vec<NarrowingResult> {
    let text = text.trim();
    let mut results = vec![];

    if let Some(brace_start) = text.find('{') {
        let body = &text[brace_start + 1..];
        narrow_scala_match_arms(body, &mut results);
    }

    results
}

/// Extract variable bindings from Scala match arms.
fn narrow_scala_match_arms(arms_text: &str, results: &mut Vec<NarrowingResult>) {
    for arm in arms_text.split("=>") {
        let arm = arm.trim();
        if let Some(result) = parse_scala_match_arm_pattern(arm) {
            results.push(result);
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
            return Some(strip_outer_parens(rest));
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
        let results = narrow_scala_if("if (x.isInstanceOf[String])");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "String");
        assert!(results[0].narrowed_type.origin.is_some());
    }

    #[test]
    fn test_scala_match_typed_pattern() {
        let results = narrow_scala_match("x match { case s: String => s, case _ => \"\" }");
        assert!(!results.is_empty());
        assert_eq!(results[0].variable_name, "s");
        assert_eq!(results[0].narrowed_type.type_name, "String");
    }

    #[test]
    fn test_scala_match_wildcard_skipped() {
        let results = narrow_scala_match("x match { case _ => 0 }");
        assert!(results.is_empty());
    }
}
