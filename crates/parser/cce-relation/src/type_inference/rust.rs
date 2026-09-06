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
    parse_type_shape, strip_references,
};
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
                        let type_name = if is_ref {
                            base.clone()
                        } else {
                            return_type.clone()
                        };
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
                            let type_name = if is_ref { base.clone() } else { ty.clone() };
                            let binding = TypeBinding {
                                type_name: type_name.clone(),
                                type_entity_id: None,
                                span: entity.span,
                                origin: Some(InferenceOrigin::TypeAnnotation),
                                shape: shape.clone(),
                            };
                            param_bindings.push(binding);
                            // Also bind variable for use in function body (with stripped type)
                            let var_binding = TypeBinding {
                                type_name: base.clone(),
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
                            let type_name = if is_ref {
                                base.clone()
                            } else {
                                self_type.clone()
                            };
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
                    // Handle variable types with reference stripping
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
                        let final_name = if is_ref {
                            base.clone()
                        } else {
                            type_name.clone()
                        };
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
                        let final_name = if is_ref {
                            base.clone()
                        } else {
                            init_type.clone()
                        };
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
                        let final_name = if is_ref {
                            base.clone()
                        } else {
                            lit_type.clone()
                        };
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
                        let mut narrowed: Vec<(String, TypeBinding)> = narrow_rust_if(&fact.text)
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
                        );
                    }
                    ControlFlowFactKind::Match => {
                        for result in narrow_rust_match(&fact.text) {
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
/// - `if let Some(val) = expr` → val: T (inner type of Option)
/// - `if let Ok(val) = expr` → val: T (Ok variant of Result)
/// - `if let Err(e) = expr` → e: E (Err variant of Result)
fn narrow_rust_if(text: &str) -> Vec<NarrowingResult> {
    let text = text.trim();
    narrow_rust_if_let(text)
}

/// Parse `if let Pattern(var) = expr` and extract the bound variable.
fn narrow_rust_if_let(text: &str) -> Vec<NarrowingResult> {
    let Some(text) = strip_rust_if_prefix(text) else {
        return vec![];
    };
    let text = text.trim();

    let Some(rest) = text.strip_prefix("let") else {
        return vec![];
    };
    let rest = rest.trim();

    parse_rust_let_pattern(rest)
}

/// Parse a Rust let-pattern like `Some(val)`, `Ok(val)`, `Err(e)`.
fn parse_rust_let_pattern(text: &str) -> Vec<NarrowingResult> {
    let Some(paren_start) = text.find('(') else {
        return vec![];
    };
    let constructor = text[..paren_start].trim();

    let Some(content) = extract_balanced_parens(&text[paren_start..]) else {
        return vec![];
    };
    let var_name = content.trim().to_string();

    if var_name.is_empty() || !is_valid_ident(&var_name) {
        return vec![];
    }

    let type_name = match constructor {
        "Some" => "Option::Some".to_string(),
        "Ok" => "Result::Ok".to_string(),
        "Err" => "Result::Err".to_string(),
        other => other.to_string(),
    };

    vec![NarrowingResult {
        variable_name: var_name,
        narrowed_type: TypeBinding {
            type_name,
            type_entity_id: None,
            span: Span::default(),
            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
            shape: None,
        },
    }]
}

/// Narrow types from a Rust `match` arm pattern.
fn narrow_rust_match(text: &str) -> Vec<NarrowingResult> {
    let text = text.trim();

    if let Some(brace_start) = text.find('{') {
        let body = &text[brace_start + 1..];
        narrow_rust_match_arms(body)
    } else {
        vec![]
    }
}

/// Extract variable bindings from match arm patterns.
fn narrow_rust_match_arms(arms_text: &str) -> Vec<NarrowingResult> {
    let mut results = vec![];
    for arm in arms_text.split("=>") {
        let arm = arm.trim();
        if let Some(result) = parse_rust_match_arm_pattern(arm) {
            results.push(result);
        }
    }
    results
}

/// Parse a single match arm pattern to extract variable bindings.
fn parse_rust_match_arm_pattern(text: &str) -> Option<NarrowingResult> {
    let text = text.trim();

    for constructor in &["Some", "Ok", "Err"] {
        if let Some(pos) = text.find(&format!("{constructor}(")) {
            let pattern_text = &text[pos..];
            let results = parse_rust_let_pattern(pattern_text);
            return results.into_iter().next();
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

    #[test]
    fn test_rust_if_let_some() {
        let results = narrow_rust_if("if let Some(val) = input {");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "val");
        assert_eq!(results[0].narrowed_type.type_name, "Option::Some");
    }

    #[test]
    fn test_rust_if_let_ok() {
        let results = narrow_rust_if("if let Ok(val) = result {");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "val");
        assert_eq!(results[0].narrowed_type.type_name, "Result::Ok");
    }

    #[test]
    fn test_rust_if_let_err() {
        let results = narrow_rust_if("if let Err(e) = result {");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "e");
        assert_eq!(results[0].narrowed_type.type_name, "Result::Err");
    }

    #[test]
    fn test_rust_if_let_custom_type() {
        let results = narrow_rust_if("if let Wrapper(inner) = data {");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "inner");
        assert_eq!(results[0].narrowed_type.type_name, "Wrapper");
    }

    #[test]
    fn test_rust_match_some_arm() {
        let results = narrow_rust_match("match opt { Some(val) => val, None => 0 }");
        assert!(!results.is_empty());
        assert_eq!(results[0].variable_name, "val");
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
        // Param should be stripped to inner type `str`
        let binding = ctx.get_variable_type("s").expect("param s");
        assert_eq!(binding.type_name, "str");
        assert!(matches!(binding.shape, Some(TypeShape::Reference { .. })));
        // Return type stripped as well
        let ret = ctx.get_return_type(EntityId(1)).expect("return");
        assert_eq!(ret.type_name, "str");
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
        assert_eq!(binding.type_name, "Vec<T>");
        if let Some(TypeShape::Reference { mutable, .. }) = &binding.shape {
            assert!(*mutable);
        } else {
            panic!("expected reference shape");
        }
    }
}
