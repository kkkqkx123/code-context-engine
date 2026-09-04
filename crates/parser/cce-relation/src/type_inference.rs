//! Lightweight type inference for dynamic languages.
//!
//! Provides per-file type context that the relation resolver uses to improve
//! disambiguation of method calls on dynamically-typed receivers. The engine
//! tracks:
//!
//! - Variable assignment types (from literals, function returns, constructor calls)
//! - Function parameter type annotations
//! - Function return type annotations
//! - Control flow narrowing (isinstance, typeof, if let, err != nil)
//!
//! The inference is intentionally conservative: it only records high-confidence
//! type bindings and never guesses. When inference fails, no binding is produced
//! so the resolver falls back to name-based resolution.

pub mod c;
pub mod control_flow;
pub mod cpp;
pub mod cross_file;
pub mod csharp;
pub mod dart;
pub mod extractors;
pub mod generics;
pub mod go;
pub mod java;
pub mod kotlin;
pub mod overload;
pub mod php;
pub mod python;
pub mod ruby;
pub mod rust;
pub mod scala;
pub mod traits;
pub mod type_shape;
pub mod types;
pub mod typescript;

pub use self::types::origin_priority;
pub use self::types::{InferenceOrigin, ScopeFrame, ScopedTypeContext, TypeBinding, TypeShape};
pub use cross_file::{CrossFilePropagator, propagate_variable_types};
pub use extractors::{extract_field_type, extract_function_types, extract_variable_type};
pub use traits::{InferenceContext, LanguageTypeInferer};

use cce_types::entity::EntityKind;
use cce_types::language::Language;

/// Per-file type inference context.
///
/// Built during symbol table construction and queried by the relation resolver
/// when disambiguating method calls on dynamically-typed receivers.
pub type TypeInferenceContext = ScopedTypeContext;

/// Lightweight type inference engine.
///
/// Analyzes parsed file entities to extract type information from:
/// - Type annotations on function parameters and return types
/// - Variable assignments with known types (literals, constructor calls)
/// - Control flow narrowing patterns
///
/// The engine is intentionally simple and conservative. It only produces
/// high-confidence type bindings that the resolver can use for disambiguation.
pub struct TypeInferenceEngine;

/// Static dispatch enum for language inferers.
///
/// Replaces `Box<dyn LanguageTypeInferer>` to avoid heap allocation and
/// vtable indirection on the per-file hot path. Each variant holds a
/// zero-sized inferer; dispatch is via `match` (monomorphized, inlined).
enum Inferer {
    C(c::CTypeInferer),
    Python(python::PythonTypeInferer),
    TypeScript(typescript::TypeScriptTypeInferer),
    Rust(rust::RustTypeInferer),
    Go(go::GoTypeInferer),
    Java(java::JavaTypeInferer),
    CSharp(csharp::CSharpTypeInferer),
    Cpp(cpp::CppTypeInferer),
    Kotlin(kotlin::KotlinTypeInferer),
    Scala(scala::ScalaTypeInferer),
    Ruby(ruby::RubyTypeInferer),
    Php(php::PhpTypeInferer),
    Dart(dart::DartTypeInferer),
}

impl LanguageTypeInferer for Inferer {
    fn infer_declarations(
        &self,
        entities: &[cce_types::entity::Entity],
        ctx: &mut ScopedTypeContext,
    ) {
        match self {
            Self::C(i) => i.infer_declarations(entities, ctx),
            Self::Python(i) => i.infer_declarations(entities, ctx),
            Self::TypeScript(i) => i.infer_declarations(entities, ctx),
            Self::Rust(i) => i.infer_declarations(entities, ctx),
            Self::Go(i) => i.infer_declarations(entities, ctx),
            Self::Java(i) => i.infer_declarations(entities, ctx),
            Self::CSharp(i) => i.infer_declarations(entities, ctx),
            Self::Cpp(i) => i.infer_declarations(entities, ctx),
            Self::Kotlin(i) => i.infer_declarations(entities, ctx),
            Self::Scala(i) => i.infer_declarations(entities, ctx),
            Self::Ruby(i) => i.infer_declarations(entities, ctx),
            Self::Php(i) => i.infer_declarations(entities, ctx),
            Self::Dart(i) => i.infer_declarations(entities, ctx),
        }
    }

    fn infer_control_flow(
        &self,
        entities: &[cce_types::entity::Entity],
        control_flow: &cce_types::ControlFlowStore,
        ctx: &mut ScopedTypeContext,
        inference_ctx: &traits::InferenceContext<'_>,
    ) {
        match self {
            Self::C(i) => i.infer_control_flow(entities, control_flow, ctx, inference_ctx),
            Self::Python(i) => i.infer_control_flow(entities, control_flow, ctx, inference_ctx),
            Self::TypeScript(i) => i.infer_control_flow(entities, control_flow, ctx, inference_ctx),
            Self::Rust(i) => i.infer_control_flow(entities, control_flow, ctx, inference_ctx),
            Self::Go(i) => i.infer_control_flow(entities, control_flow, ctx, inference_ctx),
            Self::Java(i) => i.infer_control_flow(entities, control_flow, ctx, inference_ctx),
            Self::CSharp(i) => i.infer_control_flow(entities, control_flow, ctx, inference_ctx),
            Self::Cpp(i) => i.infer_control_flow(entities, control_flow, ctx, inference_ctx),
            Self::Kotlin(i) => i.infer_control_flow(entities, control_flow, ctx, inference_ctx),
            Self::Scala(i) => i.infer_control_flow(entities, control_flow, ctx, inference_ctx),
            Self::Ruby(i) => i.infer_control_flow(entities, control_flow, ctx, inference_ctx),
            Self::Php(i) => i.infer_control_flow(entities, control_flow, ctx, inference_ctx),
            Self::Dart(i) => i.infer_control_flow(entities, control_flow, ctx, inference_ctx),
        }
    }

    fn collect_declarations(
        &self,
        entities: &[cce_types::entity::Entity],
        ctx: &mut ScopedTypeContext,
    ) {
        match self {
            Self::C(i) => i.collect_declarations(entities, ctx),
            Self::Python(i) => i.collect_declarations(entities, ctx),
            Self::TypeScript(i) => i.collect_declarations(entities, ctx),
            Self::Rust(i) => i.collect_declarations(entities, ctx),
            Self::Go(i) => i.collect_declarations(entities, ctx),
            Self::Java(i) => i.collect_declarations(entities, ctx),
            Self::CSharp(i) => i.collect_declarations(entities, ctx),
            Self::Cpp(i) => i.collect_declarations(entities, ctx),
            Self::Kotlin(i) => i.collect_declarations(entities, ctx),
            Self::Scala(i) => i.collect_declarations(entities, ctx),
            Self::Ruby(i) => i.collect_declarations(entities, ctx),
            Self::Php(i) => i.collect_declarations(entities, ctx),
            Self::Dart(i) => i.collect_declarations(entities, ctx),
        }
    }

    fn resolve_references(
        &self,
        entities: &[cce_types::entity::Entity],
        ctx: &mut ScopedTypeContext,
    ) {
        match self {
            Self::C(i) => i.resolve_references(entities, ctx),
            Self::Python(i) => i.resolve_references(entities, ctx),
            Self::TypeScript(i) => i.resolve_references(entities, ctx),
            Self::Rust(i) => i.resolve_references(entities, ctx),
            Self::Go(i) => i.resolve_references(entities, ctx),
            Self::Java(i) => i.resolve_references(entities, ctx),
            Self::CSharp(i) => i.resolve_references(entities, ctx),
            Self::Cpp(i) => i.resolve_references(entities, ctx),
            Self::Kotlin(i) => i.resolve_references(entities, ctx),
            Self::Scala(i) => i.resolve_references(entities, ctx),
            Self::Ruby(i) => i.resolve_references(entities, ctx),
            Self::Php(i) => i.resolve_references(entities, ctx),
            Self::Dart(i) => i.resolve_references(entities, ctx),
        }
    }
}

impl TypeInferenceEngine {
    fn get_inferer(language: Language) -> Option<Inferer> {
        match language {
            Language::C => Some(Inferer::C(c::CTypeInferer)),
            Language::Python => Some(Inferer::Python(python::PythonTypeInferer)),
            Language::JavaScript | Language::TypeScript | Language::Tsx | Language::Jsx => {
                Some(Inferer::TypeScript(typescript::TypeScriptTypeInferer))
            }
            Language::Rust => Some(Inferer::Rust(rust::RustTypeInferer)),
            Language::Go => Some(Inferer::Go(go::GoTypeInferer)),
            Language::Java => Some(Inferer::Java(java::JavaTypeInferer)),
            Language::CSharp => Some(Inferer::CSharp(csharp::CSharpTypeInferer)),
            Language::Cpp => Some(Inferer::Cpp(cpp::CppTypeInferer)),
            Language::Kotlin => Some(Inferer::Kotlin(kotlin::KotlinTypeInferer)),
            Language::Scala => Some(Inferer::Scala(scala::ScalaTypeInferer)),
            Language::Ruby => Some(Inferer::Ruby(ruby::RubyTypeInferer)),
            Language::Php => Some(Inferer::Php(php::PhpTypeInferer)),
            Language::Dart => Some(Inferer::Dart(dart::DartTypeInferer)),
            _ => None,
        }
    }

    /// Build a type inference context for a parsed file.
    ///
    /// Analyzes entities and their metadata to extract type information.
    /// Returns a context that can be queried by the relation resolver.
    ///
    /// `inference_ctx` provides access to shared resources like the type-member
    /// index for discriminated union narrowing.
    pub fn infer_types(
        file: &cce_types::ParsedFile,
        inference_ctx: &traits::InferenceContext<'_>,
    ) -> ScopedTypeContext {
        let mut ctx = ScopedTypeContext::new(file.language);

        if let Some(inferer) = Self::get_inferer(file.language) {
            inferer.infer_declarations(&file.entities, &mut ctx);
            Self::infer_variable_patterns(file, &mut ctx);

            // Control flow narrowing
            if !file.control_flow.is_empty() {
                inferer.infer_control_flow(
                    &file.entities,
                    &file.control_flow,
                    &mut ctx,
                    inference_ctx,
                );
            }
        }

        ctx
    }

    fn infer_variable_patterns(file: &cce_types::ParsedFile, ctx: &mut ScopedTypeContext) {
        for entity in &file.entities {
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
                    if let Some(source_type_str) = entity
                        .metadata
                        .get("source_type")
                        .or_else(|| entity.metadata.get("call_target"))
                    {
                        if let Some(shape) = crate::type_inference::types::parse_type_shape(
                            source_type_str,
                            file.language,
                        ) {
                            let pattern =
                                crate::type_inference::types::Pattern::Tuple(parts.clone());
                            ctx.add_pattern_match_binding(&pattern, &shape);
                            for (i, part) in parts.iter().enumerate() {
                                ctx.add_destructuring_binding(part, &shape, Some(i));
                            }
                            continue;
                        }
                    }
                    let generic = TypeShape::Generic {
                        base: "Tuple".to_string(),
                        args: vec![TypeShape::Named("unknown".to_string()); parts.len()],
                    };
                    let pattern = crate::type_inference::types::Pattern::Tuple(parts);
                    ctx.add_pattern_match_binding(&pattern, &generic);
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
                            .and_then(|s| {
                                crate::type_inference::types::parse_type_shape(s, file.language)
                            })
                            .unwrap_or(TypeShape::Named("unknown".to_string()));
                        let pattern = crate::type_inference::types::Pattern::Struct(fields);
                        ctx.add_pattern_match_binding(&pattern, &shape);
                    }
                }
            }
        }
    }

    /// Two-pass inference: first collect declarations, then resolve references.
    ///
    /// Enables forward references and recursive type handling by separating
    /// collection of type annotations from resolution of variable usages.
    pub fn infer_types_two_pass(
        file: &cce_types::ParsedFile,
        inference_ctx: &traits::InferenceContext<'_>,
    ) -> ScopedTypeContext {
        let mut ctx = ScopedTypeContext::new(file.language);
        if let Some(inferer) = Self::get_inferer(file.language) {
            Self::collect_declarations(file, &mut ctx, &inferer);
            Self::resolve_references(file, &mut ctx, &inferer);
            Self::infer_variable_patterns(file, &mut ctx);
            if !file.control_flow.is_empty() {
                inferer.infer_control_flow(
                    &file.entities,
                    &file.control_flow,
                    &mut ctx,
                    inference_ctx,
                );
            }
        }
        ctx
    }

    fn collect_declarations(
        file: &cce_types::ParsedFile,
        ctx: &mut ScopedTypeContext,
        inferer: &Inferer,
    ) {
        inferer.collect_declarations(&file.entities, ctx);
    }

    fn resolve_references(
        file: &cce_types::ParsedFile,
        ctx: &mut ScopedTypeContext,
        inferer: &Inferer,
    ) {
        inferer.resolve_references(&file.entities, ctx);
    }

    /// Incremental inference: only re-infer changed entities.
    pub fn infer_types_incremental(
        file: &cce_types::ParsedFile,
        changed_entity_ids: &[cce_types::entity::EntityId],
        existing_ctx: &ScopedTypeContext,
        inference_ctx: &traits::InferenceContext<'_>,
    ) -> ScopedTypeContext {
        let mut ctx = existing_ctx.clone();
        if changed_entity_ids.is_empty() {
            return ctx;
        }
        let changed_set: std::collections::HashSet<cce_types::entity::EntityId> =
            changed_entity_ids.iter().copied().collect();
        let changed_entities: Vec<cce_types::entity::Entity> = file
            .entities
            .iter()
            .filter(|e| changed_set.contains(&e.id))
            .cloned()
            .collect();
        if changed_entities.is_empty() {
            return ctx;
        }
        let inferer = Self::get_inferer(file.language);
        if let Some(inferer) = inferer {
            // For changed functions, use scoped inference: push scope, bind params, then infer
            for entity in &changed_entities {
                if entity.kind.is_function_like() {
                    ctx.push_scope();
                    for (param_name, param_type) in &entity.parameters {
                        if let Some(ty) = param_type {
                            let binding = TypeBinding {
                                type_name: ty.clone(),
                                type_entity_id: None,
                                span: entity.span,
                                origin: Some(InferenceOrigin::TypeAnnotation),
                                shape: crate::type_inference::types::parse_type_shape(
                                    ty,
                                    file.language,
                                ),
                            };
                            ctx.add_variable_type(param_name.clone(), binding);
                        }
                    }
                    inferer.infer_declarations(std::slice::from_ref(entity), &mut ctx);
                    ctx.pop_scope();
                } else {
                    inferer.infer_declarations(std::slice::from_ref(entity), &mut ctx);
                }
            }
            if !file.control_flow.is_empty() {
                inferer.infer_control_flow(
                    &changed_entities,
                    &file.control_flow,
                    &mut ctx,
                    inference_ctx,
                );
            }
        }
        ctx
    }

    /// Extract constructor call types from entity metadata.
    ///
    /// When a variable is assigned via a constructor call (e.g., `x = MyClass()`),
    /// the parser stores `"constructor_type"` in the variable entity's metadata.
    /// This method processes those entries and adds them to the type inference context.
    pub fn extract_constructor_call_types(
        file: &cce_types::ParsedFile,
        ctx: &mut ScopedTypeContext,
    ) {
        for entity in &file.entities {
            if entity.kind != EntityKind::Variable {
                continue;
            }
            if let Some(constructor_type) = entity.metadata.get("constructor_type") {
                let binding = TypeBinding {
                    type_name: constructor_type.clone(),
                    type_entity_id: None,
                    span: entity.span,
                    origin: Some(InferenceOrigin::ConstructorCall),
                    shape: crate::type_inference::types::parse_type_shape(
                        constructor_type,
                        file.language,
                    ),
                };
                ctx.add_variable_type(entity.name.clone(), binding);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;
    use cce_types::entity::EntityId;

    fn dummy_span() -> Span {
        Span::default()
    }

    #[test]
    fn test_scoped_type_context_basics() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        assert!(ctx.is_empty());

        let binding = TypeBinding {
            type_name: "builtins.int".to_string(),
            type_entity_id: None,
            span: dummy_span(),
            origin: None,
            shape: None,
        };
        ctx.add_variable_type("x".to_string(), binding.clone());

        assert!(!ctx.is_empty());
        assert_eq!(
            ctx.get_variable_type("x").unwrap().type_name,
            "builtins.int"
        );
        assert!(ctx.get_variable_type("y").is_none());
    }

    #[test]
    fn test_return_type_lookup() {
        let mut ctx = ScopedTypeContext::new(Language::TypeScript);
        let binding = TypeBinding {
            type_name: "Promise<string>".to_string(),
            type_entity_id: Some(EntityId(42)),
            span: dummy_span(),
            origin: None,
            shape: None,
        };
        ctx.add_return_type(EntityId(1), binding);

        let found = ctx.get_return_type(EntityId(1)).unwrap();
        assert_eq!(found.type_name, "Promise<string>");
        assert_eq!(found.type_entity_id, Some(EntityId(42)));
    }

    #[test]
    fn test_scope_push_pop() {
        let mut ctx = ScopedTypeContext::new(Language::Python);

        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "int".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );

        ctx.push_scope();
        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "str".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );

        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "str");

        ctx.pop_scope();
        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "int");
    }

    #[test]
    fn test_scope_shadowing() {
        let mut ctx = ScopedTypeContext::new(Language::Python);

        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "int".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );

        ctx.push_scope();
        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "str".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );

        ctx.push_scope();
        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "float".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );

        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "float");

        ctx.pop_scope();
        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "str");

        ctx.pop_scope();
        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "int");
    }

    #[test]
    fn test_merge_contexts_with_confidence() {
        let mut ctx1 = ScopedTypeContext::new(Language::Python);
        ctx1.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "int".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );

        let mut ctx2 = ScopedTypeContext::new(Language::Python);
        ctx2.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "str".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        ctx2.add_variable_type(
            "y".to_string(),
            TypeBinding {
                type_name: "float".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );

        ctx1.merge_from(&ctx2);
        assert_eq!(ctx1.get_variable_type("x").unwrap().type_name, "int");
        assert_eq!(ctx1.get_variable_type("y").unwrap().type_name, "float");
    }

    #[test]
    fn test_merge_contexts_high_overrides_medium() {
        let mut ctx1 = ScopedTypeContext::new(Language::Python);
        ctx1.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "int".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: Some(crate::type_inference::types::InferenceOrigin::LiteralType),
                shape: None,
            },
        );

        let mut ctx2 = ScopedTypeContext::new(Language::Python);
        ctx2.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "str".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: Some(crate::type_inference::types::InferenceOrigin::TypeAnnotation),
                shape: None,
            },
        );

        ctx1.merge_from(&ctx2);
        assert_eq!(ctx1.get_variable_type("x").unwrap().type_name, "str");
    }

    #[test]
    fn test_confidence_ordering() {
        assert!(
            crate::type_inference::types::origin_priority(Some(
                crate::type_inference::types::InferenceOrigin::TypeAnnotation
            )) > crate::type_inference::types::origin_priority(Some(
                crate::type_inference::types::InferenceOrigin::LiteralType
            ))
        );
        assert!(
            crate::type_inference::types::origin_priority(Some(
                crate::type_inference::types::InferenceOrigin::ControlFlowNarrowing
            )) > crate::type_inference::types::origin_priority(Some(
                crate::type_inference::types::InferenceOrigin::ConstructorCall
            ))
        );
    }

    #[test]
    fn test_narrowed_type_lookup() {
        let mut ctx = ScopedTypeContext::new(Language::Python);

        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "object".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );

        ctx.push_scope();
        ctx.add_narrowed_type(
            "x".to_string(),
            TypeBinding {
                type_name: "str".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );

        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "str");

        ctx.pop_scope();
        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "object");
    }

    #[test]
    fn test_narrowed_multiple_candidates() {
        let mut ctx = ScopedTypeContext::new(Language::Python);

        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "object".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );

        ctx.push_scope();
        ctx.add_narrowed_type(
            "x".to_string(),
            TypeBinding {
                type_name: "int".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        ctx.add_narrowed_type(
            "x".to_string(),
            TypeBinding {
                type_name: "str".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );

        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "str");
    }

    #[test]
    fn test_narrowed_does_not_shadow_other_variables() {
        let mut ctx = ScopedTypeContext::new(Language::Python);

        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "int".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        ctx.add_variable_type(
            "y".to_string(),
            TypeBinding {
                type_name: "str".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );

        ctx.push_scope();
        ctx.add_narrowed_type(
            "x".to_string(),
            TypeBinding {
                type_name: "bool".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );

        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "bool");
        assert_eq!(ctx.get_variable_type("y").unwrap().type_name, "str");
    }
}
