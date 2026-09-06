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

pub mod bash;
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
pub mod lua;
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

pub use self::types::{
    InferenceOrigin, ScopedTypeContext, TypeBinding, TypeShape, binding_supersedes,
    bindings_supersede, origin_is_authoritative,
};
pub use cross_file::{CrossFilePropagator, propagate_variable_types};
pub use traits::InferenceContext;

// Internal imports used within this module (not re-exported)
use self::types::NestedPatternPart;
use traits::LanguageTypeInferer;

use cce_types::entity::EntityKind;
use cce_types::language::Language;

/// Per-file type inference context.
///
/// Built during symbol table construction and queried by the relation resolver
/// when disambiguating method calls on dynamically-typed receivers.
pub type TypeInferenceContext = ScopedTypeContext;

/// A same-file callable's formal parameters and return annotation.
type CalleeSignature<'a> = (&'a [(String, Option<String>)], Option<&'a str>);

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
    Bash(bash::BashTypeInferer),
    Lua(lua::LuaTypeInferer),
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
            Self::Bash(i) => i.infer_declarations(entities, ctx),
            Self::Lua(i) => i.infer_declarations(entities, ctx),
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
            Self::Bash(i) => i.infer_control_flow(entities, control_flow, ctx, inference_ctx),
            Self::Lua(i) => i.infer_control_flow(entities, control_flow, ctx, inference_ctx),
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
            Self::Bash(i) => i.collect_declarations(entities, ctx),
            Self::Lua(i) => i.collect_declarations(entities, ctx),
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
            Self::Bash(i) => i.resolve_references(entities, ctx),
            Self::Lua(i) => i.resolve_references(entities, ctx),
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
            Language::Bash => Some(Inferer::Bash(bash::BashTypeInferer)),
            Language::Lua => Some(Inferer::Lua(lua::LuaTypeInferer)),
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

    /// Check whether a bare name is shaped like a type expression
    /// (`ValueError`, `User`) rather than a value reference (`pair`, `items`).
    fn looks_like_type_name(name: &str) -> bool {
        let mut chars = name.trim().chars();
        match chars.next() {
            Some(c) if c.is_uppercase() => (),
            _ => return false,
        }
        chars.all(|c| c.is_alphanumeric() || c == '_')
    }

    /// Collect member name → type for a named interface, class, struct or
    /// type alias declared in the same file.
    ///
    /// Membership is decided by span containment, so object-literal members
    /// outside the declaration never leak in. A leading colon in captured
    /// annotation text is stripped. Returns `None` when the type is not
    /// declared locally or carries no member types.
    fn named_type_members(
        file: &cce_types::ParsedFile,
        type_name: &str,
    ) -> Option<std::collections::HashMap<String, String>> {
        let owner = file.entities.iter().find(|e| {
            e.name == type_name
                && matches!(
                    e.kind,
                    EntityKind::Interface
                        | EntityKind::Class
                        | EntityKind::Struct
                        | EntityKind::TypeAlias
                )
        })?;
        let mut members = std::collections::HashMap::new();
        for member in &file.entities {
            if !matches!(member.kind, EntityKind::Property | EntityKind::Field) {
                continue;
            }
            if member.span.start_byte < owner.span.start_byte
                || member.span.end_byte > owner.span.end_byte
            {
                continue;
            }
            let ty = member
                .metadata
                .get("type_annotation")
                .or_else(|| member.metadata.get("explicit_type"))?;
            let ty = ty.trim().trim_start_matches(':').trim();
            if ty.is_empty() {
                continue;
            }
            members.insert(member.name.clone(), ty.to_string());
        }
        if members.is_empty() {
            return None;
        }
        Some(members)
    }

    /// Bind top-level destructured names through same-file member types.
    fn bind_member_names(
        ctx: &mut ScopedTypeContext,
        file: &cce_types::ParsedFile,
        entity: &cce_types::Entity,
        members: &std::collections::HashMap<String, String>,
        names: &[String],
    ) {
        for part in names {
            if let Some(member_ty) = members.get(part) {
                ctx.add_variable_type(
                    part.clone(),
                    TypeBinding {
                        type_name: member_ty.clone(),
                        type_entity_id: None,
                        span: entity.span,
                        origin: Some(InferenceOrigin::DestructuringAssignment),
                        shape: crate::type_inference::types::parse_type_shape(
                            member_ty,
                            file.language,
                        ),
                    },
                );
            }
        }
    }

    /// Recover the nested destructuring pattern of a multi-binding entity.
    ///
    /// Slices the statement source by the entity span, keeps the assignment
    /// left-hand side, and parses grouping. Returns `None` when any step
    /// fails so the caller keeps the existing flat mapping.
    fn nested_pattern_parts(
        file: &cce_types::ParsedFile,
        entity: &cce_types::Entity,
    ) -> Option<Vec<NestedPatternPart>> {
        let text = file
            .source
            .get(entity.span.start_byte..entity.span.end_byte)?;
        let lhs = split_assignment_lhs(text)?;
        parse_nested_pattern_list(lhs)
    }

    /// Look up a parameter type in the closest enclosing function scope.
    ///
    /// Uses span containment (smallest enclosing function) rather than the
    /// `parent` link, which typically points at the module for locals.
    fn enclosing_param_type<'a>(
        file: &'a cce_types::ParsedFile,
        entity: &cce_types::Entity,
        name: &str,
    ) -> Option<&'a str> {
        file.entities
            .iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    EntityKind::Function | EntityKind::Method | EntityKind::Constructor
                ) && e.span.contains(&entity.span)
            })
            .min_by_key(|e| e.span.end_byte - e.span.start_byte)?
            .parameters
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, ty)| ty.as_deref())
    }

    /// Find a same-file callable (function, method or constructor) by name.
    ///
    /// Matches the stripped callee name (`obj.method` resolves against the
    /// `method` member); the first match wins. Method receivers are not
    /// disambiguated here — generic substitution only needs the formal
    /// parameter and return annotations.
    fn find_callee_signature<'a>(
        file: &'a cce_types::ParsedFile,
        name: &str,
    ) -> Option<CalleeSignature<'a>> {
        let simple = name.rsplit(['.', ':', '/']).next().unwrap_or(name).trim();
        file.entities
            .iter()
            .find(|e| {
                matches!(
                    e.kind,
                    EntityKind::Function | EntityKind::Method | EntityKind::Constructor
                ) && e.name == simple
            })
            .map(|e| (e.parameters.as_slice(), e.return_type.as_deref()))
    }

    /// Infer the type shape of one call-site argument expression.
    ///
    /// Bare identifiers resolve against enclosing-function parameters first
    /// (so `identity(x)` with `x: number` binds `T = number`), then fall
    /// back to literals, constructor bases and known variable bindings via
    /// the shared cross-file argument resolver.
    fn infer_call_arg_shape(
        file: &cce_types::ParsedFile,
        entity: &cce_types::Entity,
        ctx: &ScopedTypeContext,
        arg: &str,
    ) -> Option<TypeShape> {
        let trimmed = arg.trim();
        if !trimmed.is_empty()
            && trimmed
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
            && !trimmed.chars().next().is_some_and(|c| c.is_ascii_digit())
            && let Some(param_ty) = Self::enclosing_param_type(file, entity, trimmed)
            && let Some(shape) =
                crate::type_inference::types::parse_type_shape(param_ty, file.language)
        {
            return Some(shape);
        }
        crate::type_inference::cross_file::infer_arg_shape(ctx, file.language, arg)
    }

    /// Substitute a same-file generic call return using call-site arguments.
    ///
    /// For `y = identity(42)` with `identity<T>(x: T): T` in the same file,
    /// binds `T = number` from the argument and returns the substituted
    /// `number` shape. Returns `None` when the source is not a call with
    /// arguments, the callee is unknown, its return mentions no type
    /// parameter, or the substitution is not fully concrete — callers keep
    /// their existing unsubstituted fallback in all those cases.
    fn resolve_generic_call_shape(
        file: &cce_types::ParsedFile,
        entity: &cce_types::Entity,
        ctx: &ScopedTypeContext,
        source: &str,
    ) -> Option<TypeShape> {
        use crate::type_inference::generics::{
            shape_contains_param, split_call_target, substitute_call_return_type,
        };
        if !source.contains('(') {
            return None;
        }
        let (name, args) = split_call_target(source);
        if args.is_empty() {
            return None;
        }
        let (params, ret) = Self::find_callee_signature(file, &name)?;
        let return_text = ret?;
        let return_shape =
            crate::type_inference::types::parse_type_shape(return_text, file.language)?;
        if !shape_contains_param(&return_shape) {
            return None;
        }
        let formal_shapes: Vec<TypeShape> = params
            .iter()
            .map(|(_, ty)| {
                ty.as_deref()
                    .and_then(|text| {
                        crate::type_inference::types::parse_type_shape(text, file.language)
                    })
                    .unwrap_or(TypeShape::Named("unknown".to_string()))
            })
            .collect();
        let actual_shapes: Vec<Option<TypeShape>> = args
            .iter()
            .map(|arg| Self::infer_call_arg_shape(file, entity, ctx, arg))
            .collect();
        let actual_refs: Vec<Option<&TypeShape>> =
            actual_shapes.iter().map(|opt| opt.as_ref()).collect();
        let substituted = substitute_call_return_type(
            &formal_shapes,
            &return_shape,
            &actual_refs,
            file.language,
        )?;
        if shape_contains_param(&substituted) {
            return None;
        }
        Some(substituted)
    }

    /// Strip a stored call target to its callee name for name lookups.
    ///
    /// Stored targets may carry an argument list (`foo(a, b)`); lookups
    /// against return tables and variable bindings use `foo`.
    fn strip_call_target_name(source: &str) -> String {
        match source.find('(') {
            Some(pos) => source[..pos].trim().to_string(),
            None => source.to_string(),
        }
    }

    /// Resolve a destructuring-source expression to a concrete [`TypeShape`].
    ///
    /// identifier sources resolve against (in order) enclosing-function
    /// parameters, already-known variable bindings, and same-file function
    /// return types; a bare type-shaped name (`ValueError`) resolves to
    /// itself. Returns `None` when the source carries no usable type.
    fn resolve_source_shape(
        file: &cce_types::ParsedFile,
        returns_by_name: &std::collections::HashMap<&str, &str>,
        entity: &cce_types::Entity,
        ctx: &ScopedTypeContext,
        source: &str,
    ) -> Option<TypeShape> {
        let shape = crate::type_inference::types::parse_type_shape(source, file.language)?;
        let name = match &shape {
            TypeShape::Named(id) => id.clone(),
            _ => return Some(shape),
        };
        if let Some(param_ty) = Self::enclosing_param_type(file, entity, &name) {
            return crate::type_inference::types::parse_type_shape(param_ty, file.language);
        }
        if let Some(binding) = ctx.get_variable_type(&name) {
            return crate::type_inference::types::parse_type_shape(
                &binding.type_name,
                file.language,
            );
        }
        if let Some(ret) = returns_by_name.get(name.as_str()) {
            return crate::type_inference::types::parse_type_shape(ret, file.language);
        }
        if Self::looks_like_type_name(&name) {
            return Some(shape);
        }
        // Array-literal destructuring sources
        // (`const [first, second] = ["a", "b"]`): the literal carries no
        // type name, so resolve its element shape through the
        // call-argument literal path. Only array results with a known
        // element bind; anything else keeps the conservative `None`
        // fallback. Gated to JavaScript/TypeScript, whose element
        // vocabulary (`string`, `number`, `boolean`) matches
        // `infer_arg_shape`.
        if matches!(file.language, Language::JavaScript | Language::TypeScript)
            && let Some(TypeShape::Array(element)) =
                crate::type_inference::cross_file::infer_arg_shape(ctx, file.language, source)
            && !matches!(&*element, TypeShape::Named(name) if name == "unknown")
        {
            return Some(TypeShape::Array(element));
        }
        None
    }

    fn infer_variable_patterns(file: &cce_types::ParsedFile, ctx: &mut ScopedTypeContext) {
        let returns_by_name: std::collections::HashMap<&str, &str> = file
            .entities
            .iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    EntityKind::Function | EntityKind::Method | EntityKind::Constructor
                )
            })
            .filter_map(|e| e.return_type.as_deref().map(|r| (e.name.as_str(), r)))
            .collect();
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
                    // First resolvable candidate wins: a raw right-hand side
                    // (`make_pair()`) may not parse while `call_target`
                    // (`make_pair`) resolves through same-file returns.
                    // Generic calls try call-site substitution first
                    // (`p = makePair(42, "x")` binds `Pair<number, string>`).
                    let mut resolved = None;
                    for key in ["source_type", "call_target"] {
                        if let Some(candidate) = entity.metadata.get(key) {
                            if key == "call_target"
                                && let Some(shape) =
                                    Self::resolve_generic_call_shape(file, entity, ctx, candidate)
                            {
                                resolved = Some(shape);
                                break;
                            }
                            let lookup = if key == "call_target" {
                                Self::strip_call_target_name(candidate)
                            } else {
                                candidate.clone()
                            };
                            if let Some(shape) = Self::resolve_source_shape(
                                file,
                                &returns_by_name,
                                entity,
                                ctx,
                                &lookup,
                            ) {
                                resolved = Some(shape);
                                break;
                            }
                        }
                    }
                    if let Some(mut shape) = resolved {
                        // Loop and case subjects iterate their element type
                        // (`for a, b in pairs` destructures one pair, not the
                        // whole collection). Parts stay unbound when the
                        // element type cannot be determined.
                        if matches!(entity.subtype.as_deref(), Some("case") | Some("loop")) {
                            match crate::type_inference::types::element_type_at_depth(&shape, 1) {
                                Some(element) => shape = element,
                                None => continue,
                            }
                        } // Object destructuring against a named interface or
                        // class declared in the same file binds by member
                        // name (`const { name } = user` with `user: User`).
                        // Parts without a matching member stay unbound rather
                        // than guessed; tuple shapes keep positional mapping.
                        if let TypeShape::Named(type_name) = &shape {
                            if let Some(members) = Self::named_type_members(file, type_name) {
                                Self::bind_member_names(ctx, file, entity, &members, &parts);
                                continue;
                            }
                        }
                        // Nested tuple patterns (`a, (b, c) = t`) map by
                        // shape position through each grouping level.
                        // Statements that do not parse as nested patterns
                        // keep the existing flat mapping.
                        if let Some(nested) = Self::nested_pattern_parts(file, entity) {
                            if nested
                                .iter()
                                .any(|part| matches!(part, NestedPatternPart::Group(_)))
                            {
                                ctx.add_nested_destructuring_binding(&nested, &shape);
                                continue;
                            }
                        }
                        let pattern = crate::type_inference::types::Pattern::Tuple(parts.clone());
                        ctx.add_pattern_match_binding(&pattern, &shape);
                        for (i, part) in parts.iter().enumerate() {
                            ctx.add_destructuring_binding(part, &shape, Some(i));
                        }
                        continue;
                    }
                    // Constructor fallback: `const { x, y } = new Point()`
                    // carries `constructor_type` rather than a resolvable
                    // source, so bind parts by member name when the
                    // constructed type is declared in the same file.
                    if let Some(init_type) = entity.metadata.get("constructor_type") {
                        if let Some(members) = Self::named_type_members(file, init_type.trim()) {
                            Self::bind_member_names(ctx, file, entity, &members, &parts);
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
            } else if entity.metadata.contains_key("source_type")
                || entity.metadata.contains_key("call_target")
            {
                // Single pattern-bound variable (`except E as e`, `y = name`).
                // `case`/`loop` bindings iterate their subject, so they bind
                // the subject's element type when it can be determined. Other
                // singles bind only when the resolved shape is a bare type
                // name: generic shapes stay unbound rather than guessed.
                let is_element_binding =
                    matches!(entity.subtype.as_deref(), Some("case") | Some("loop"));
                if is_element_binding {
                    for key in ["source_type", "call_target"] {
                        let Some(source_str) = entity.metadata.get(key) else {
                            continue;
                        };
                        // Generic calls try call-site substitution first.
                        let shape = if key == "call_target" {
                            Self::resolve_generic_call_shape(file, entity, ctx, source_str).or_else(
                                || {
                                    let lookup = Self::strip_call_target_name(source_str);
                                    Self::resolve_source_shape(
                                        file,
                                        &returns_by_name,
                                        entity,
                                        ctx,
                                        &lookup,
                                    )
                                },
                            )
                        } else {
                            Self::resolve_source_shape(
                                file,
                                &returns_by_name,
                                entity,
                                ctx,
                                source_str,
                            )
                        };
                        let Some(shape) = shape else {
                            continue;
                        };
                        let Some(element) =
                            crate::type_inference::types::element_type_at_depth(&shape, 1)
                        else {
                            continue;
                        };
                        let type_name =
                            crate::type_inference::types::type_shape_to_string(&element);
                        let keep = ctx.get_variable_type(&entity.name).is_none_or(|existing| {
                            crate::type_inference::types::origin_supersedes(
                                InferenceOrigin::DestructuringAssignment,
                                existing.origin,
                            )
                        });
                        if keep {
                            ctx.add_variable_type(
                                entity.name.clone(),
                                TypeBinding {
                                    type_name,
                                    type_entity_id: None,
                                    span: entity.span,
                                    origin: Some(InferenceOrigin::DestructuringAssignment),
                                    shape: Some(element),
                                },
                            );
                        }
                        break;
                    }
                    continue;
                }
                for key in ["source_type", "call_target"] {
                    let Some(source_str) = entity.metadata.get(key) else {
                        continue;
                    };
                    let from_call =
                        key == "call_target" && !entity.metadata.contains_key("source_type");
                    // Generic calls try call-site substitution first
                    // (`y = identity(42)` binds `number`, not `T`).
                    let (shape, from_substitution) = if from_call {
                        match Self::resolve_generic_call_shape(file, entity, ctx, source_str) {
                            Some(shape) => (Some(shape), true),
                            None => {
                                let lookup = Self::strip_call_target_name(source_str);
                                (
                                    Self::resolve_source_shape(
                                        file,
                                        &returns_by_name,
                                        entity,
                                        ctx,
                                        &lookup,
                                    ),
                                    false,
                                )
                            }
                        }
                    } else {
                        (
                            Self::resolve_source_shape(
                                file,
                                &returns_by_name,
                                entity,
                                ctx,
                                source_str,
                            ),
                            false,
                        )
                    };
                    if let Some(shape) = shape {
                        // Bare type names bind as before. Fully substituted
                        // generic results (`Pair<number, string>`) bind too;
                        // every other compound shape stays unbound rather
                        // than guessed.
                        let bindable = matches!(&shape, TypeShape::Named(_))
                            || (from_substitution
                                && !crate::type_inference::generics::shape_contains_param(&shape));
                        if bindable {
                            let type_name =
                                crate::type_inference::types::type_shape_to_string(&shape);
                            let origin = if from_call {
                                InferenceOrigin::FunctionReturn
                            } else {
                                InferenceOrigin::DestructuringAssignment
                            };
                            // Never clobber a higher-priority binding (e.g. an
                            // explicit annotation recorded during declarations).
                            let keep = ctx.get_variable_type(&entity.name).is_none_or(|existing| {
                                crate::type_inference::types::origin_supersedes(
                                    origin,
                                    existing.origin,
                                )
                            });
                            if keep {
                                ctx.add_variable_type(
                                    entity.name.clone(),
                                    TypeBinding {
                                        type_name: type_name.clone(),
                                        type_entity_id: None,
                                        span: entity.span,
                                        origin: Some(origin),
                                        shape: Some(shape),
                                    },
                                );
                            }
                            break;
                        }
                    }
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
}

/// Left-hand side of the first top-level assignment in a statement.
///
/// Skips `==`, `!=`, `=>`, `<=` and `>=`, ignores nesting and quoted
/// regions, and strips declaration keywords plus a trailing top-level
/// type annotation. Returns `None` when no plain assignment is found.
fn split_assignment_lhs(statement: &str) -> Option<&str> {
    let bytes = statement.as_bytes();
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut brace = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' | b'\'' | b'`' => {
                i = skip_pattern_quoted(bytes, i);
                continue;
            }
            b'(' => paren += 1,
            b')' => paren = paren.saturating_sub(1),
            b'[' => bracket += 1,
            b']' => bracket = bracket.saturating_sub(1),
            b'{' => brace += 1,
            b'}' => brace = brace.saturating_sub(1),
            b'=' if paren == 0 && bracket == 0 && brace == 0 => {
                let prev = if i > 0 { bytes[i - 1] } else { 0 };
                let next = bytes.get(i + 1).copied().unwrap_or(0);
                if prev == b'=' || prev == b'!' || prev == b'<' || prev == b'>' {
                    i += 1;
                    continue;
                }
                if next == b'=' || next == b'>' {
                    i += 1;
                    continue;
                }
                return Some(strip_pattern_affixes(statement[..i].trim()));
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Strip declaration keywords and a trailing top-level type annotation.
fn strip_pattern_affixes(lhs: &str) -> &str {
    let mut rest = lhs.trim();
    for prefix in ["let mut ", "let ", "val ", "var ", "const "] {
        if let Some(stripped) = rest.strip_prefix(prefix) {
            rest = stripped.trim();
            break;
        }
    }
    let bytes = rest.as_bytes();
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut brace = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' | b'\'' | b'`' => {
                i = skip_pattern_quoted(bytes, i);
                continue;
            }
            b'(' => paren += 1,
            b')' => paren = paren.saturating_sub(1),
            b'[' => bracket += 1,
            b']' => bracket = bracket.saturating_sub(1),
            b'{' => brace += 1,
            b'}' => brace = brace.saturating_sub(1),
            b':' if paren == 0 && bracket == 0 && brace == 0 => {
                let prev = if i > 0 { bytes[i - 1] } else { 0 };
                let next = bytes.get(i + 1).copied().unwrap_or(0);
                if prev != b':' && next != b':' {
                    return rest[..i].trim();
                }
            }
            _ => {}
        }
        i += 1;
    }
    rest
}

/// Parse a comma-separated destructuring pattern with grouping.
fn parse_nested_pattern_list(text: &str) -> Option<Vec<NestedPatternPart>> {
    let mut parts = Vec::new();
    for item in split_top_level_commas(text)? {
        parts.push(parse_nested_pattern_part(item.trim())?);
    }
    if parts.is_empty() {
        return None;
    }
    // A lone outer group belongs to the pattern itself (`let (a, b) = t`),
    // so unwrap it instead of treating it as one nested element.
    while parts.len() == 1 {
        let inner = match parts.first() {
            Some(NestedPatternPart::Group(inner)) => inner.clone(),
            _ => break,
        };
        parts = inner;
    }
    Some(parts)
}

/// Parse one destructuring element: placeholder, group or plain name.
fn parse_nested_pattern_part(text: &str) -> Option<NestedPatternPart> {
    let text = text.trim();
    if text == "_" {
        return Some(NestedPatternPart::Wildcard);
    }
    let mut rest = text;
    for prefix in ["mut ", "ref "] {
        if let Some(stripped) = rest.strip_prefix(prefix) {
            rest = stripped.trim();
            break;
        }
    }
    if rest.starts_with('(') && rest.ends_with(')') && is_fully_wrapped(rest) {
        return parse_nested_pattern_list(&rest[1..rest.len() - 1]).map(NestedPatternPart::Group);
    }
    if is_pattern_ident(rest) {
        return Some(NestedPatternPart::Name(rest.to_string()));
    }
    None
}

/// Whether the outer parentheses wrap the whole text.
fn is_fully_wrapped(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    for (i, byte) in bytes.iter().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && i != bytes.len() - 1 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Split on depth-zero commas; `None` on unbalanced nesting or quotes.
fn split_top_level_commas(text: &str) -> Option<Vec<&str>> {
    let bytes = text.as_bytes();
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut brace = 0usize;
    let mut start = 0usize;
    let mut items = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' | b'\'' | b'`' => {
                i = skip_pattern_quoted(bytes, i);
                continue;
            }
            b'(' => paren += 1,
            b')' => {
                if paren == 0 {
                    return None;
                }
                paren -= 1;
            }
            b'[' => bracket += 1,
            b']' => {
                if bracket == 0 {
                    return None;
                }
                bracket -= 1;
            }
            b'{' => brace += 1,
            b'}' => {
                if brace == 0 {
                    return None;
                }
                brace -= 1;
            }
            b',' if paren == 0 && bracket == 0 && brace == 0 => {
                items.push(text[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if paren != 0 || bracket != 0 || brace != 0 {
        return None;
    }
    items.push(text[start..].trim());
    Some(items)
}

/// Advance past a quoted region starting at the quote character.
fn skip_pattern_quoted(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut j = start + 1;
    while j < bytes.len() {
        if bytes[j] == b'\\' {
            j += 2;
            continue;
        }
        if bytes[j] == quote {
            return j + 1;
        }
        j += 1;
    }
    j
}

/// Plain identifier check for recovered pattern names.
fn is_pattern_ident(text: &str) -> bool {
    crate::type_inference::control_flow::shared::is_valid_ident(text)
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
        // Equal-priority bindings replace: the incoming binding wins ties so
        // repeated merges converge instead of pinning the first value seen.
        assert_eq!(ctx1.get_variable_type("x").unwrap().type_name, "str");
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
        use crate::type_inference::types::InferenceOrigin;
        assert!(crate::type_inference::types::origin_supersedes(
            InferenceOrigin::TypeAnnotation,
            Some(InferenceOrigin::LiteralType),
        ));
        assert!(crate::type_inference::types::origin_supersedes(
            InferenceOrigin::ControlFlowNarrowing,
            Some(InferenceOrigin::ConstructorCall),
        ));
        assert!(!crate::type_inference::types::binding_supersedes(
            Some(InferenceOrigin::LiteralType),
            Some(InferenceOrigin::TypeAnnotation),
        ));
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

    #[test]
    fn test_parse_nested_pattern_list_flat_and_grouped() {
        let parts = parse_nested_pattern_list("a, (b, c)").expect("nested pattern");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], NestedPatternPart::Name("a".to_string()));
        assert_eq!(
            parts[1],
            NestedPatternPart::Group(vec![
                NestedPatternPart::Name("b".to_string()),
                NestedPatternPart::Name("c".to_string()),
            ])
        );
    }

    #[test]
    fn test_parse_nested_pattern_list_unwraps_lone_group() {
        let parts = parse_nested_pattern_list("(a, (b, c))").expect("unwrapped pattern");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], NestedPatternPart::Name("a".to_string()));
        assert!(matches!(parts[1], NestedPatternPart::Group(_)));
    }

    #[test]
    fn test_parse_nested_pattern_list_rejects_broken_input() {
        assert!(parse_nested_pattern_list("a, (b, c").is_none());
        assert!(parse_nested_pattern_list("a, foo(1)").is_none());
        assert!(parse_nested_pattern_list("").is_none());
        assert_eq!(
            parse_nested_pattern_list("_, b").expect("wildcard pattern"),
            vec![
                NestedPatternPart::Wildcard,
                NestedPatternPart::Name("b".to_string()),
            ]
        );
    }

    #[test]
    fn test_split_assignment_lhs_skips_comparisons() {
        assert_eq!(
            split_assignment_lhs("a, (b, c) = make()").unwrap(),
            "a, (b, c)"
        );
        assert_eq!(
            split_assignment_lhs("let (a, (b, c)): Pair = make();").unwrap(),
            "(a, (b, c))"
        );
        assert!(split_assignment_lhs("a == b").is_none());
    }

    #[test]
    fn test_nested_destructuring_end_to_end() {
        use cce_types::entity::{Entity, EntityKind};
        use cce_types::{Language, ParsedFile};

        let source = "a, (b, c) = make()";
        let mut file = ParsedFile::new(Language::Python, "demo.py".to_string(), source);
        let mut maker = Entity::new(
            EntityId(1),
            EntityKind::Function,
            "make".to_string(),
            Span {
                start_byte: 0,
                end_byte: source.len(),
                ..Span::default()
            },
        );
        maker.return_type = Some("Tuple[str, Tuple[int, bool]]".to_string());
        file.entities.push(maker);
        let mut multi = Entity::new(
            EntityId(2),
            EntityKind::Variable,
            "a, b, c".to_string(),
            Span {
                start_byte: 0,
                end_byte: source.len(),
                ..Span::default()
            },
        );
        multi
            .metadata
            .insert("call_target".to_string(), "make".to_string());
        file.entities.push(multi);

        let ctx = TypeInferenceEngine::infer_types(&file, &InferenceContext::new());
        assert_eq!(ctx.get_variable_type("a").unwrap().type_name, "str");
        assert_eq!(ctx.get_variable_type("b").unwrap().type_name, "int");
        assert_eq!(ctx.get_variable_type("c").unwrap().type_name, "bool");
    }

    #[test]
    fn test_generic_call_substitution_end_to_end() {
        use cce_types::entity::{Entity, EntityKind};
        use cce_types::{Language, ParsedFile};

        let source = "function identity<T>(x: T): T { return x; }\nconst y = identity(42);\nconst w = wrapInArray(\"a\");";
        let mut file = ParsedFile::new(Language::TypeScript, "demo.ts".to_string(), source);
        let mut identity = Entity::new(
            EntityId(1),
            EntityKind::Function,
            "identity".to_string(),
            Span {
                start_byte: 0,
                end_byte: 10,
                ..Span::default()
            },
        );
        identity.parameters = vec![("x".to_string(), Some("T".to_string()))];
        identity.return_type = Some("T".to_string());
        file.entities.push(identity);
        let mut wrap = Entity::new(
            EntityId(2),
            EntityKind::Function,
            "wrapInArray".to_string(),
            Span {
                start_byte: 0,
                end_byte: 10,
                ..Span::default()
            },
        );
        wrap.parameters = vec![("item".to_string(), Some("T".to_string()))];
        wrap.return_type = Some("Array<T>".to_string());
        file.entities.push(wrap);
        let mut y = Entity::new(
            EntityId(3),
            EntityKind::Variable,
            "y".to_string(),
            Span {
                start_byte: 45,
                end_byte: 46,
                ..Span::default()
            },
        );
        y.metadata
            .insert("call_target".to_string(), "identity(42)".to_string());
        file.entities.push(y);
        let mut w = Entity::new(
            EntityId(4),
            EntityKind::Variable,
            "w".to_string(),
            Span {
                start_byte: 60,
                end_byte: 61,
                ..Span::default()
            },
        );
        w.metadata
            .insert("call_target".to_string(), "wrapInArray(\"a\")".to_string());
        file.entities.push(w);

        let ctx = TypeInferenceEngine::infer_types(&file, &InferenceContext::new());
        assert_eq!(ctx.get_variable_type("y").unwrap().type_name, "number");
        assert_eq!(
            ctx.get_variable_type("w").unwrap().type_name,
            "Array<string>"
        );
    }
}
