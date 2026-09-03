//! Metadata extraction from captures
//!
//! Dispatches to language-specific extraction for variable assignment types,
//! class bases, method types, enum variants, CSS values and impl-block metadata.
//! Fallback type inference is delegated to the `type_inference` submodule.

use cce_types::language::Language;
use cce_types::{Entity, EntityKind};

use crate::parser::extractor::capture as capture_module;
use crate::parser::extractor::utils;
use crate::tree_sitter_query::executor::QueryMatch;

use super::type_inference::{extract_fallback_type_annotation, is_valid_type_name};

/// Extract type annotation from AST node for an entity.
///
/// Uses `tree_sitter::Node` field-based access via `ast_accessor::extract_type_annotation`
/// instead of source-text heuristics. Returns `None` when the node cannot be
/// retrieved or no type field is found.
fn extract_type_from_ast_for_entity(
    mat: &QueryMatch,
    tree: &tree_sitter::Tree,
    source: &str,
) -> Option<String> {
    // Find the entity name capture to locate the AST node
    let name_capture = mat.captures.iter().find(|c| {
        c.name.ends_with(".name")
            || c.name.ends_with(".field.name")
            || c.name.ends_with(".property.name")
    })?;

    // Get the tree-sitter node using the capture's byte range
    let node = tree
        .root_node()
        .descendant_for_byte_range(name_capture.start_byte, name_capture.end_byte)?;

    // Walk up to find the declaration node that has a type field
    let mut current = Some(node);
    while let Some(n) = current {
        if let Some(type_text) =
            cce_parser_core::ast_accessor::extract_type_annotation(n, source.as_bytes())
        {
            return Some(type_text);
        }
        current = n.parent();
    }

    None
}

/// Extract language-specific metadata from match captures
///
/// Dispatches to specific extraction functions based on entity kind:
/// - Python method types
/// - Enum variant types
/// - CSS property values
/// - Rust impl block relationships
/// - Rust type parameter bounds
/// - Variable assignment types (constructor calls, literals)
/// - Variable type annotations (for type inference)
pub(crate) fn extract_metadata(
    mat: &QueryMatch,
    entity: &mut Entity,
    language: &Language,
    source: &str,
    tree: &tree_sitter::Tree,
) {
    if entity.kind == EntityKind::Class {
        let bases = capture_module::parser::extract_base_classes(mat);
        if !bases.is_empty() {
            entity.set_metadata("base_classes", bases.join(", "));
        }
    }

    if entity.kind == EntityKind::Method {
        if let Some(method_type) = capture_module::parser::extract_python_method_type(mat) {
            entity.set_metadata("method_type", method_type);
        }
    }

    if entity.kind == EntityKind::EnumVariant {
        if let Some(variant_type) = capture_module::parser::extract_enum_variant_type(mat) {
            entity.set_metadata("enum_variant_type", variant_type);
        }
    }

    if entity.kind == EntityKind::StyleProperty {
        if let Some(property_value) = capture_module::parser::extract_css_property_value(mat) {
            entity.set_metadata("property_value", property_value);
        }
    }

    if entity.kind == EntityKind::TraitImpl || entity.kind == EntityKind::InherentImpl {
        crate::parser::extractor::post_processing::extract_impl_block_metadata(mat, entity);
    }

    // Variable assignment type tracking
    if entity.kind == EntityKind::Variable {
        extract_variable_assignment_metadata(mat, entity, language, source);
    }

    // Field/Property fallback type inference via AST-based extraction
    if matches!(entity.kind, EntityKind::Field | EntityKind::Property)
        && !entity.metadata.contains_key("type_annotation")
        && !entity.metadata.contains_key("variable_type")
        && !entity.metadata.contains_key("field_type")
    {
        // Try AST-based type extraction first
        if let Some(type_text) = extract_type_from_ast_for_entity(mat, tree, source) {
            let trimmed = type_text.trim();
            if !trimmed.is_empty() {
                entity.set_metadata("type_annotation", trimmed.to_string());
                return;
            }
        }
        // Fall back to capture-based extraction
        if let Some(fallback) = extract_fallback_type_annotation(mat, entity, language, source) {
            let trimmed = fallback.trim();
            if !trimmed.is_empty() {
                entity.set_metadata("type_annotation", trimmed.to_string());
            }
        }
    }
}

/// Extract metadata for variable assignments to support type inference.
///
/// Checks the variable's initializer expression and records:
/// - `type_annotation`: when the variable has an explicit type annotation
/// - `var_type`: when the variable uses `var` keyword (Java/C#) with inferred type
/// - `inferred_type`: when the variable uses `:=` short declaration (Go)
/// - `explicit_type`: when the variable has an explicit type declaration (C#)
/// - `constructor_type`: when assigned via `new ClassName()` or `ClassName()`
/// - `literal_type`: when assigned via a literal value (int, string, bool, etc.)
pub(crate) fn extract_variable_assignment_metadata(
    mat: &QueryMatch,
    entity: &mut Entity,
    language: &Language,
    source: &str,
) {
    // Extract type annotation from tree-sitter captures if available.
    // Go: @entity.variable.type on long-form `var` declarations
    // Python: @entity.variable.typed.type on annotated assignments
    let type_annotation = utils::find_capture_by_name(&mat.captures, |name| {
        name.ends_with(".variable.type") || name.ends_with(".variable.typed.type")
    })
    .map(|c| c.text.clone());

    if let Some(ref type_text) = type_annotation {
        let trimmed = type_text.trim();
        if !trimmed.is_empty() {
            entity.set_metadata("type_annotation", trimmed.to_string());
        }
    } else if let Some(fallback) = extract_fallback_type_annotation(mat, entity, language, source) {
        let trimmed = fallback.trim();
        if !trimmed.is_empty() {
            entity.set_metadata("type_annotation", trimmed.to_string());
        }
    }

    // Language-specific variable type metadata
    match language {
        Language::Go => {
            extract_go_variable_metadata(mat, entity);
        }
        Language::Java => {
            extract_java_variable_metadata(mat, entity);
        }
        Language::CSharp => {
            extract_csharp_variable_metadata(mat, entity);
        }
        _ => {}
    }

    // Find the value capture (initializer expression)
    let value_text = utils::find_capture_by_name(&mat.captures, |name| {
        name.ends_with(".value")
            || name.ends_with(".const.value")
            || name.ends_with(".let.value")
            || name.ends_with(".var.value")
    })
    .map(|c| c.text.clone());

    let Some(value) = value_text else {
        return;
    };

    let trimmed = value.trim();

    // Check for constructor call: new ClassName() or ClassName()
    if let Some(type_name) = extract_constructor_type_from_expr(trimmed) {
        entity.set_metadata("constructor_type", type_name);
        return;
    }

    // Check for generic function call: foo(), module.func(), obj.method()
    if let Some(call_target) = extract_call_target_from_expr(trimmed) {
        entity.set_metadata("call_target", call_target);
        // Fall through to also capture literal if needed, but call_target takes precedence
        // Do not return: still check literal for chained cases? For now keep call_target only.
        return;
    }

    // Check for literal type
    if let Some(lit_type) = extract_literal_type(trimmed) {
        entity.set_metadata("literal_type", lit_type);
    }
}

/// Extract Go-specific variable metadata.
///
/// Go has two variable declaration forms:
/// - Long-form: `var x Type = expr` or `var x = expr` — type annotation already captured above
/// - Short-form: `x := expr` — type must be inferred from the expression
fn extract_go_variable_metadata(_mat: &QueryMatch, _entity: &mut Entity) {
    // Type inference for short-form declarations (`x := expr`) requires AST-based
    // analysis rather than source-text heuristics. The previous implementation used
    // `infer_go_type_from_expr` which was a string-based heuristic that has been
    // removed as part of the symbol resolution determinization effort.
    //
    // TODO: Implement AST-based type inference for Go short-form declarations
}

/// Extract Java-specific variable metadata.
///
/// Java distinguishes between:
/// - `var x = expr` — type inferred from expression (write `var_type`)
/// - `Type x = expr` — explicit type (write `type_annotation` from capture or source)
fn extract_java_variable_metadata(_mat: &QueryMatch, _entity: &mut Entity) {
    // Type inference for `var` declarations requires AST-based analysis rather than
    // source-text heuristics. The previous implementation used `infer_java_type_from_expr`
    // which was a string-based heuristic that has been removed as part of the symbol
    // resolution determinization effort.
    //
    // TODO: Implement AST-based type inference for Java `var` declarations
}

/// Extract C#-specific variable metadata.
///
/// C# distinguishes between:
/// - `var x = expr` — type inferred (write `var_type`)
/// - `Type x = expr` — explicit type (write `explicit_type`)
fn extract_csharp_variable_metadata(mat: &QueryMatch, entity: &mut Entity) {
    // Check the source text for `var` keyword or explicit type
    if let Some(main) = utils::find_capture_by_name(&mat.captures, |name| {
        name.contains("local_declaration_statement")
    }) {
        let source_text = main.text.trim();
        // Check if declaration starts with `var`
        if source_text.starts_with("var ") || source_text.starts_with("var\t") {
            // Type inference for `var` declarations requires AST-based analysis rather than
            // source-text heuristics. The previous implementation used `infer_csharp_type_from_expr`
            // which was a string-based heuristic that has been removed as part of the symbol
            // resolution determinization effort.
            //
            // TODO: Implement AST-based type inference for C# `var` declarations
        } else {
            // Explicit type declaration: `Type x = expr` or `Type x;`
            // Extract type from the source before the variable name
            if let Some(name_capture) =
                utils::find_capture_by_name(&mat.captures, |name| name.ends_with(".variable.name"))
            {
                let name_text = name_capture.text.trim();
                // Find the type portion: everything before the variable name
                if let Some(pos) = source_text.find(name_text) {
                    let type_portion = source_text[..pos].trim();
                    if !type_portion.is_empty() {
                        entity.set_metadata("explicit_type", type_portion.to_string());
                    }
                }
            }
        }
    }
}

/// Extract the type name from a constructor call expression.
///
/// Handles patterns like:
/// - `new ClassName()`
/// - `ClassName()`
/// - `module.ClassName()`
fn extract_constructor_type_from_expr(expr: &str) -> Option<String> {
    let trimmed = expr.trim();

    // new ClassName(...)
    if let Some(rest) = trimmed.strip_prefix("new ") {
        let type_name = rest.trim_end_matches('(').trim_end_matches(')').trim();
        if is_valid_type_name(type_name) {
            return Some(type_name.to_string());
        }
    }

    // ClassName(...) - function call that looks like a constructor
    if let Some(paren_pos) = trimmed.find('(') {
        let func_name = trimmed[..paren_pos].trim();
        // Constructor calls typically start with uppercase
        if is_valid_type_name(func_name)
            && func_name.chars().next().is_some_and(|c| c.is_uppercase())
        {
            return Some(func_name.to_string());
        }
    }

    None
}

/// Extract the call target from a function call expression for cross-file propagation.
///
/// Handles patterns like:
/// - `foo()`, `create_user()`, `module.func()`, `obj.method()`
///
/// Returns the function name portion before `(`.
fn extract_call_target_from_expr(expr: &str) -> Option<String> {
    let trimmed = expr.trim();
    // Skip literals and already handled constructors (uppercase check already gated)
    // Must contain '(' to be a call.
    let paren_pos = trimmed.find('(')?;
    let func_name = trimmed[..paren_pos].trim();
    if func_name.is_empty() {
        return None;
    }
    // Reject control flow keywords and invalid names.
    const KEYWORDS: &[&str] = &["if", "while", "for", "match", "return", "await", "yield"];
    if KEYWORDS.contains(&func_name) {
        return None;
    }
    // Allow qualified names with `.`, `::`, `/`
    if is_valid_type_name(func_name) {
        // Avoid capturing literals like `42(` which would be invalid.
        return Some(func_name.to_string());
    }
    None
}

/// Extract the type from a literal expression.
fn extract_literal_type(expr: &str) -> Option<String> {
    let trimmed = expr.trim();

    // Numeric literal
    if trimmed.parse::<f64>().is_ok()
        || trimmed.ends_with("f32")
        || trimmed.ends_with("f64")
        || trimmed.ends_with("i32")
        || trimmed.ends_with("i64")
        || trimmed.ends_with("u32")
        || trimmed.ends_with("u64")
    {
        return Some("number".to_string());
    }

    // String literal
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        || trimmed.starts_with("r\"")
        || trimmed.starts_with("r#")
    {
        return Some("string".to_string());
    }

    // Boolean literal
    if trimmed == "true" || trimmed == "false" {
        return Some("boolean".to_string());
    }

    // None/null
    if trimmed == "None" || trimmed == "null" || trimmed == "nil" {
        return Some("null".to_string());
    }

    // Array literal
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        return Some("array".to_string());
    }

    // Object literal
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Some("object".to_string());
    }

    None
}
