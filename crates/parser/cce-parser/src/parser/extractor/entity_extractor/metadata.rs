//! Metadata extraction from captures
//!
//! Dispatches to language-specific extraction for variable assignment types,
//! class bases, method types, enum variants, CSS values and impl-block metadata.
//! Fallback type inference is delegated to the `type_inference` submodule.

use cce_types::language::Language;
use cce_types::{Entity, EntityKind, LiteralKind, classify_numeric_literal, literal_type_name};

use crate::parser::extractor::capture as capture_module;
use crate::parser::extractor::utils;
use crate::parser::extractor::utils::find_capture_by_name;
use crate::tree_sitter_query::executor::QueryMatch;

use super::type_inference::{is_valid_call_target_name, is_valid_type_name};

/// Node kinds that delimit value contexts during AST type lookup.
///
/// When walking up from a field or property name, crossing one of these
/// nodes means the name belongs to a value (object literal member, call
/// argument, array element, ...) rather than to the declaration that owns
/// the ancestor type annotation. Stopping there keeps e.g. TypeScript
/// object-literal keys from inheriting the outer variable's annotation.
fn is_value_boundary_kind(kind: &str) -> bool {
    kind == "object"
        || kind == "pair"
        || kind.starts_with("object_")
        || kind.starts_with("array")
        || kind.starts_with("template")
        || kind.contains("argument")
        || kind.ends_with("_expression")
        || kind.ends_with("_literal")
}

/// Whether a value text is a bare identifier (`radius`, not `a.b`, `f(x)`
/// or a literal) that can name an enclosing parameter or variable.
fn is_bare_identifier(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        && text
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
}

/// Node kinds that delimit declaration scopes during AST type lookup.
///
/// A field or property name must never inherit a type from an enclosing
/// function, method, or type definition: `self.radius` inside
/// `def __init__(...) -> None` is not `None`, and a member inside a class
/// body is not the class's own type arguments. Stopping at these nodes
/// keeps the walk-up in `extract_type_from_ast_for_entity` within the
/// declaration that actually owns the name.
fn is_scope_boundary_kind(kind: &str) -> bool {
    kind.contains("function")
        || kind.contains("method")
        || kind == "class_definition"
        || kind == "class_declaration"
        || kind == "class_body"
        || kind == "constructor"
        || kind == "lambda"
        || kind == "arrow_function"
        || kind == "function_expression"
}

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

    // Walk up to find the declaration node that has a type field,
    // stopping at value-context boundaries (see `is_value_boundary_kind`)
    // and at enclosing function/method/class scopes: a member must never
    // inherit the enclosing callable's return type (e.g. `self.radius`
    // inside `def __init__(...) -> None` is not `None`).
    let mut current = Some(node);
    while let Some(n) = current {
        // Scope and value boundaries are checked before reading the node's
        // own type fields so an enclosing callable's return type is never
        // mistaken for the member's annotation.
        if is_value_boundary_kind(n.kind()) || is_scope_boundary_kind(n.kind()) {
            return None;
        }
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
        extract_variable_assignment_metadata(mat, entity, language, source, tree);
    }

    // Field/Property type inference: AST-based extraction first, then
    // deterministic type captures (e.g. Kotlin property types, C field
    // types) which the AST accessor cannot see when the grammar carries the
    // type as an unfielded child.
    if matches!(entity.kind, EntityKind::Field | EntityKind::Property)
        && !entity.metadata.contains_key("type_annotation")
    {
        // Try AST-based type extraction first
        if let Some(type_text) = extract_type_from_ast_for_entity(mat, tree, source) {
            let trimmed = type_text.trim();
            if !trimmed.is_empty() {
                entity.set_metadata("type_annotation", trimmed.to_string());
            }
        }
        if !entity.metadata.contains_key("type_annotation") {
            if let Some(captured) = find_type_annotation_capture(mat) {
                let trimmed = captured.trim();
                if !trimmed.is_empty() {
                    entity.set_metadata("type_annotation", trimmed.to_string());
                }
            }
        }
    }

    // Field/Property initializer tracking (constructor calls, literals):
    // mirrors the variable path so annotated properties such as
    // `val c = Container("v")` feed type inference.
    if matches!(entity.kind, EntityKind::Field | EntityKind::Property) {
        if let Some(value) =
            capture_text_over_field_siblings(mat, tree, source, |name| name.ends_with(".value"))
        {
            extract_initializer_metadata(entity, &value, language);
            // Bare-identifier initializers (`self.radius = radius`): remember
            // the source name so inference can bind the enclosing
            // constructor parameter's type (`radius: float`).
            if !entity.metadata.contains_key("source_type") && is_bare_identifier(value.trim()) {
                entity.set_metadata("source_type", value.trim().to_string());
            }
        }
    }

    if *language == Language::Dart
        && matches!(
            entity.kind,
            EntityKind::Function | EntityKind::Method | EntityKind::Constructor
        )
    {
        extend_dart_signature_span(mat, entity, tree);
        extend_dart_return_type(mat, entity, source, tree);
    }

    // Ruby methods return the value of their last expression when no
    // explicit `return` type exists (Ruby has no return-type syntax, so
    // `entity.return_type` is always empty here). When that trailing
    // expression constructs an object (`User.new(...)`, `@user = User.new`),
    // record the class as the return type so cross-file propagation can
    // resolve callers like `user = load_user(...)`. Explicit YARD types
    // still win: the doc post-pass runs later and the inferer applies
    // `yard_return_type` after the annotation-derived binding.
    if *language == Language::Ruby
        && matches!(entity.kind, EntityKind::Function | EntityKind::Method)
        && entity.return_type.is_none()
        && let Some(implicit) = implicit_ruby_return_type(mat)
    {
        entity.return_type = Some(implicit);
    }

    // Python and Lua: extract the last return expression as `return_body`
    // metadata so the type-inference engine can derive a return type when
    // no explicit annotation exists.  The expression is normalised to a
    // type-like name (literals → built-in types, `new X(...)` → `X`);
    // compound expressions (arithmetic, calls) are stored as-is for the
    // inferer to evaluate.
    if matches!(*language, Language::Python | Language::Lua)
        && matches!(entity.kind, EntityKind::Function | EntityKind::Method)
        && entity.return_type.is_none()
        && let Some(body) = find_capture_by_name(&mat.captures, |name| name.ends_with(".body"))
    {
        if let Some(return_expr) = extract_last_return_expression(&body.text, *language) {
            entity.set_metadata("return_body", return_expr);
        }
    }
}

/// Take the first whitespace-separated token of a doc tag body as a type.
///
/// Strips a leading `?` (nullable shorthand) and cuts union suffixes at
/// `|`, so `?int`, `User|null` still yield a usable base type. Returns
/// `None` unless the remainder looks like a type name.
fn doc_tag_base_type(body: &str) -> Option<String> {
    let token = body.split_whitespace().next()?.trim();
    let token = token.strip_prefix('?').unwrap_or(token);
    let token = token.split('|').next()?.trim();
    if is_valid_type_name(token) {
        Some(token.to_string())
    } else {
        None
    }
}

/// Parse a Ruby YARD `@return` tag: `@return [Type] desc` or `@return Type`.
fn parse_yard_return(doc: &str) -> Option<String> {
    for line in doc.lines() {
        let text = line
            .trim()
            .trim_start_matches('#')
            .trim()
            .trim_start_matches('*')
            .trim();
        let rest = text.strip_prefix("@return")?;
        let rest = rest.trim();
        if rest.is_empty() {
            continue;
        }
        if let Some(bracketed) = rest.strip_prefix('[') {
            let inner = bracketed.split(']').next()?.trim();
            // Prefer the first listed type for multi-type tags.
            let first = inner.split(',').next()?.trim();
            if is_valid_type_name(first) {
                return Some(first.to_string());
            }
            continue;
        }
        if let Some(base) = doc_tag_base_type(rest) {
            return Some(base);
        }
    }
    None
}

/// Parse a PHPDoc `@return` tag from a method docblock.
fn parse_phpdoc_return(doc: &str) -> Option<String> {
    for line in doc.lines() {
        let text = line.trim().trim_start_matches('*').trim();
        let Some(rest) = text.strip_prefix("@return") else {
            continue;
        };
        if let Some(base) = doc_tag_base_type(rest.trim()) {
            return Some(base);
        }
    }
    None
}

/// Extract the last return expression from a function body text.
///
/// Walks the body text backwards to find the last `return` statement and
/// returns the expression after it.  For Python the expression is normalised
/// (literals → built-in type names); for Lua the raw expression is returned
/// since Lua has no type annotations.
fn extract_last_return_expression(body: &str, language: Language) -> Option<String> {
    let last_return_line = body
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| line.starts_with("return "))?;
    let expr = last_return_line.strip_prefix("return ")?.trim();
    let expr = expr.trim_end_matches(';').trim();
    if expr.is_empty() {
        return None;
    }
    match language {
        Language::Python => normalize_python_return_expression(expr),
        Language::Lua => normalize_lua_return_expression(expr),
        _ => Some(expr.to_string()),
    }
}

/// Normalise a Python return expression to a type-like name.
///
/// Literals map to built-in types (`"hello"` → `str`, `42` → `int`),
/// constructor calls map to the class (`User(...)` → `User`).
/// Compound expressions (arithmetic, calls, f-strings) are returned as-is
/// so the inferer can evaluate them with full context.
fn normalize_python_return_expression(expr: &str) -> Option<String> {
    if let Some(lit) = extract_literal_type(expr, &Language::Python) {
        return Some(lit);
    }
    // Constructor call: `User(...)` or `module.User(...)`.
    if let Some(name) = expr.strip_suffix("()") {
        let base = name.trim();
        let base = base.rsplit('.').next().unwrap_or(base).trim();
        if !base.is_empty() && is_valid_type_name(base) {
            return Some(base.to_string());
        }
    }
    // Assignment form: `x = expr` → normalise the RHS.
    if let Some((_lhs, rhs)) = expr.split_once('=') {
        let rhs = rhs.trim();
        if let Some(lit) = extract_literal_type(rhs, &Language::Python) {
            return Some(lit);
        }
    }
    // String concatenation: `"[" .. app_name .. "] " .. msg` → `str`.
    if expr.contains(".. ") || expr.contains(" + ") {
        // Mixed string concatenation — conservatively return `str` only when
        // all operands look like strings or string variables.
        let all_stringy = expr.split(".. ").all(|part| {
            let part = part.trim().trim_end_matches(" + ").trim();
            part.starts_with('"') || part.starts_with('\'') || is_valid_ident(part)
        });
        if all_stringy {
            return Some("str".to_string());
        }
    }
    Some(expr.to_string())
}

/// Normalise a Lua return expression to a type-like name.
///
/// Lua has no type annotations; string concatenation returns `string`
/// and all other expressions are returned as-is so the type-inference
/// engine can evaluate them with full context (literal bindings, call
/// targets, etc.).
fn normalize_lua_return_expression(expr: &str) -> Option<String> {
    // Lua string concatenation (`..`) always produces a string.
    if expr.contains(".. ") || expr.contains("..") {
        return Some("string".to_string());
    }
    Some(expr.to_string())
}

fn is_valid_ident(s: &str) -> bool {
    let s = s.trim();
    !s.is_empty()
        && s.bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Infer a Ruby method's return type from its trailing expression.
///
/// Ruby returns the value of the last evaluated expression, so a method
/// whose body ends with a constructor expression (`User.new(...)`,
/// optionally assigned or explicitly `return`ed) returns an instance of
/// that class. Only this strict constructor shape is accepted; anything
/// else yields `None` so unrelated methods keep no return type instead of
/// a wrong one.
fn implicit_ruby_return_type(mat: &QueryMatch) -> Option<String> {
    let body = utils::find_capture_by_name(&mat.captures, |name| name.ends_with(".body"))?;
    let last = body
        .text
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty() && !line.starts_with('#'))?;
    let expr = last.strip_prefix("return ").unwrap_or(last).trim();
    let expr = expr.strip_suffix(';').unwrap_or(expr).trim();
    // Reject comparisons and boolean operators before splitting off a
    // possible assignment, so `==`, `!=`, `=~`, `<=`, `>=`, `=>` are never
    // misread as `lhs = User.new`.
    if ["==", "!=", "=~", "<=", ">=", "=>", "||", "&&"]
        .iter()
        .any(|op| expr.contains(op))
    {
        return None;
    }
    let rhs = match expr.split_once('=') {
        Some((lhs, rhs)) => {
            let lhs = lhs.trim();
            let assignable = lhs.starts_with('@')
                || lhs
                    .chars()
                    .next()
                    .is_some_and(|c| c == '_' || c.is_ascii_lowercase());
            if !assignable || lhs.contains([' ', '\t', '(', '[', '.']) {
                return None;
            }
            rhs.trim()
        }
        None => expr,
    };
    if rhs.contains('=') {
        return None;
    }
    let (receiver, rest) = rhs.split_once(".new")?;
    let rest = rest.trim_start();
    if !(rest.is_empty() || rest.starts_with('(')) {
        return None;
    }
    if receiver.contains(char::is_whitespace) {
        return None;
    }
    let constant_path = !receiver.is_empty()
        && receiver
            .split("::")
            .all(|seg| seg.chars().next().is_some_and(|c| c.is_ascii_uppercase()));
    if !constant_path || !is_valid_type_name(receiver) {
        return None;
    }
    Some(receiver.to_string())
}

fn parse_phpdoc_var(doc: &str) -> Option<String> {
    for line in doc.lines() {
        let text = line.trim().trim_start_matches('*').trim();
        let Some(rest) = text.strip_prefix("@var") else {
            continue;
        };
        // `@var Type $name` or `@var Type`; the trailing name is ignored.
        if let Some(base) = doc_tag_base_type(rest.trim()) {
            return Some(base);
        }
    }
    None
}

/// Dart signature node kinds whose entity span only covers the signature.
/// The function body is a following sibling, unlike most languages where
/// the function node already spans its body.
const DART_SIGNATURE_KINDS: &[&str] = &[
    "function_signature",
    "method_signature",
    "constructor_signature",
];

/// Dart body node kinds that directly follow a signature node.
const DART_BODY_KINDS: &[&str] = &["function_body", "block"];

/// Locate the tree-sitter node for a capture byte range.
fn node_for_capture<'a>(
    tree: &'a tree_sitter::Tree,
    start_byte: usize,
    end_byte: usize,
) -> Option<tree_sitter::Node<'a>> {
    tree.root_node()
        .descendant_for_byte_range(start_byte, end_byte)
}

/// Extend a Dart function/method/constructor span to cover its body.
///
/// The entity query's main capture is the signature node while the body
/// (`function_body`) is a following sibling. Without the extension,
/// control-flow facts inside the body find no owning entity by span
/// containment and are dropped.
fn extend_dart_signature_span(mat: &QueryMatch, entity: &mut Entity, tree: &tree_sitter::Tree) {
    let Some(main) = capture_module::parser::find_main_capture(mat) else {
        return;
    };
    let Some(mut node) = node_for_capture(tree, main.start_byte, main.end_byte) else {
        return;
    };
    // Climb to the outermost signature node (method signatures wrap the
    // inner function signature).
    let mut signature = None;
    loop {
        if DART_SIGNATURE_KINDS.contains(&node.kind()) {
            signature = Some(node);
        }
        match node.parent() {
            Some(parent) if DART_SIGNATURE_KINDS.contains(&parent.kind()) => {
                node = parent;
            }
            _ => break,
        }
    }
    let Some(signature) = signature else {
        return;
    };
    let Some(parent) = signature.parent() else {
        return;
    };
    // Find the signature's index among its parent's children, then take the
    // first following named body sibling.
    let mut index = None;
    for i in 0..parent.child_count() {
        if let Some(child) = parent.child(i as u32) {
            if child.start_byte() == signature.start_byte()
                && child.end_byte() == signature.end_byte()
            {
                index = Some(i);
                break;
            }
        }
    }
    let Some(start) = index else {
        return;
    };
    for i in start + 1..parent.child_count() {
        let Some(child) = parent.child(i as u32) else {
            continue;
        };
        if !child.is_named() {
            continue;
        }
        if DART_BODY_KINDS.contains(&child.kind()) {
            if child.end_byte() > entity.span.end_byte {
                entity.span.end_byte = child.end_byte();
                let end = child.end_position();
                entity.span.end_position = cce_types::Position {
                    row: end.row,
                    column: end.column,
                };
            }
            return;
        }
    }
}

/// Extend a Dart return type over same-field sibling nodes.
///
/// The grammar reports generic return types as two `return_type` children
/// (base type plus type arguments) while the query binds one of them, so
/// `List<T>` would surface as `List`. Covering the full sibling range
/// restores the complete annotation from source.
fn extend_dart_return_type(
    mat: &QueryMatch,
    entity: &mut Entity,
    source: &str,
    tree: &tree_sitter::Tree,
) {
    if let Some(full) = capture_text_over_field_siblings(mat, tree, source, |name| {
        let lower = name.to_lowercase();
        lower.contains("return") || lower.contains("result")
    }) {
        entity.return_type = Some(full);
    }
}

/// Recover generic arguments for bare constructor calls from the class
/// declaration in the same file.
///
/// Runs as a post pass over all entities: match-level extraction strips
/// type arguments (`new Container()` and `new Container<String>()` both
/// record `constructor_type = Container`), so a bare call loses its
/// generics. When the variable has no explicit annotation, the call
/// carries no explicit type arguments, and a class with the same name
/// declares type parameters (`class Container<T>`), rewrite the metadata
/// to `Container<T>`. Explicit arguments are composed directly
/// (`Container` + `String` yields `Container<String>`) and never
/// overwritten.
///
/// Only the `<...>` or `[...]` group immediately following the class
/// name in its signature is accepted, so leading `template <typename T>`
/// headers (C++) or unrelated brackets cannot leak in.
pub(crate) fn resolve_constructor_type_params(entities: &mut [Entity]) {
    use std::collections::HashMap;

    let mut class_params: HashMap<String, Vec<String>> = HashMap::new();
    for entity in entities.iter() {
        if !matches!(
            entity.kind,
            EntityKind::Class | EntityKind::Struct | EntityKind::Interface
        ) {
            continue;
        }
        if class_params.contains_key(entity.name.as_str()) {
            continue;
        }
        if let Some(params) = class_type_params(&entity.name, &entity.signature) {
            class_params.insert(entity.name.clone(), params);
        }
    }
    if class_params.is_empty() {
        return;
    }

    for entity in entities.iter_mut() {
        if !matches!(
            entity.kind,
            EntityKind::Variable | EntityKind::Field | EntityKind::Property
        ) {
            continue;
        }
        // Explicitly written type arguments are concrete: compose them
        // onto a bare constructor type (`Container` + `String` yields
        // `Container<String>`) instead of leaving the call
        // unparameterized. Annotation precedence is decided downstream
        // (`type_annotation` still wins); metadata records the call
        // faithfully. Synthesis below only handles bare calls.
        if let Some(args) = entity.metadata.get("constructor_type_args").cloned() {
            let args = args.trim().to_string();
            if !args.is_empty()
                && let Some(ctor) = entity.metadata.get("constructor_type").cloned()
                && !ctor.contains('<')
            {
                entity.set_metadata("constructor_type", format!("{ctor}<{args}>"));
            }
            continue;
        }
        // An explicit annotation wins over synthesis, except for inference
        // keywords (`var`, `auto`, ...) which carry no concrete type.
        let annotated = entity
            .metadata
            .get("type_annotation")
            .is_some_and(|ann| !is_inferred_type_keyword(ann));
        if annotated {
            continue;
        }
        let Some(ctor) = entity.metadata.get("constructor_type").cloned() else {
            continue;
        };
        if ctor.contains('<') {
            continue;
        }
        let Some(params) = class_params.get(ctor.as_str()) else {
            continue;
        };
        if params.is_empty() {
            continue;
        }
        entity.set_metadata(
            "constructor_type",
            format!("{}<{}>", ctor, params.join(", ")),
        );
    }
}

/// Parse the type parameters declared on a class from its signature text.
///
/// Accepts the `<...>` group immediately following the class name
/// (`class Container<T>`, `static class Pair<A, B>`), as well as the
/// square-bracket form (`case class Pair[A, B]`, `type Pair[T1, T2]`),
/// returning each parameter's bare name (`T extends Bound` yields `T`).
/// Returns `None` when no such group exists or any parameter is not a
/// plain identifier.
fn class_type_params(class_name: &str, signature: &str) -> Option<Vec<String>> {
    let name_pos = signature.find(class_name)?;
    let after_name = &signature[name_pos + class_name.len()..];
    let after_name = after_name.trim_start();
    let inner = after_name
        .strip_prefix('<')
        .and_then(|s| match_closing_bracket(s, '<', '>'))
        .or_else(|| {
            after_name
                .strip_prefix('[')
                .and_then(|s| match_closing_bracket(s, '[', ']'))
        })?;
    let mut params = Vec::new();
    for part in split_top_level(inner, ',') {
        // Bare parameter name: first identifier token (`T`, `A`; bounds
        // like `T extends Bound` reduce to `T`).
        let name: String = part
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .find(|token| !token.is_empty() && !token.starts_with(|c: char| c.is_numeric()))
            .unwrap_or("")
            .to_string();
        if name.is_empty() || !is_valid_type_name(&name) {
            return None;
        }
        params.push(name);
    }
    if params.is_empty() {
        return None;
    }
    Some(params)
}

/// Slice the text up to the closing bracket at nesting depth zero.
///
/// Returns `None` when the bracket never closes, so callers keep their
/// previous behavior instead of guessing a truncated parameter list.
fn match_closing_bracket(text: &str, open: char, close: char) -> Option<&str> {
    let mut depth = 0usize;
    for (idx, ch) in text.char_indices() {
        if ch == open {
            depth += 1;
        } else if ch == close {
            if depth == 0 {
                return text.get(..idx);
            }
            depth -= 1;
        }
    }
    None
}

/// Whether a captured annotation is an inference keyword rather than a
/// concrete type (`var`, `auto`, `val`, `let`, `decltype(...)`).
fn is_inferred_type_keyword(ty: &str) -> bool {
    let mut normalized = ty.trim();
    normalized = normalized
        .strip_prefix("const ")
        .unwrap_or(normalized)
        .trim();
    normalized = normalized
        .trim_end_matches(['&', '*', ' '].as_slice())
        .trim();
    if normalized.starts_with("decltype") {
        return true;
    }
    matches!(normalized, "var" | "auto" | "val" | "let")
}

/// Split text on a separator that appears at nesting depth zero for
/// `<>`, `()`, and `[]`.
fn split_top_level(text: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (idx, ch) in text.char_indices() {
        match ch {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if ch == separator && depth == 0 {
            parts.push(text[start..idx].trim());
            start = idx + ch.len_utf8();
        }
    }
    parts.push(text[start..].trim());
    parts
}

/// Extract explicitly written type arguments from a constructor call
/// expression (`new Container<String>(...)` yields `String`).
///
/// Returns `None` for bare or diamond (`<>`) calls so callers can tell
/// "no information" apart from "concrete arguments".
fn extract_explicit_type_args(expr: &str) -> Option<String> {
    let trimmed = expr.trim();
    let head = trimmed.strip_prefix("new ").unwrap_or(trimmed);
    let head = head.split('(').next().unwrap_or(head);
    let left = head.find('<')?;
    let right = head.rfind('>')?;
    if right <= left {
        return None;
    }
    let inner = head[left + 1..right].trim();
    if inner.is_empty() {
        return None;
    }
    Some(inner.to_string())
}

/// Record doc-comment-derived types for languages whose inferers consume
/// them (Ruby YARD, PHPDoc).
///
/// Runs as a post pass once doc comments are attached, since match-level
/// extraction happens before comment association.
pub(crate) fn extract_doc_type_metadata(entities: &mut [Entity], language: &Language) {
    for entity in entities.iter_mut() {
        let Some(doc) = entity.doc_comment.as_deref() else {
            continue;
        };
        match language {
            Language::Ruby if entity.kind == EntityKind::Method => {
                if !entity.metadata.contains_key("yard_return_type")
                    && let Some(ty) = parse_yard_return(doc)
                {
                    entity.set_metadata("yard_return_type", ty);
                }
            }
            Language::Php if entity.kind == EntityKind::Method => {
                if !entity.metadata.contains_key("phpdoc_return_type")
                    && let Some(ty) = parse_phpdoc_return(doc)
                {
                    entity.set_metadata("phpdoc_return_type", ty);
                }
            }
            Language::Php if entity.kind == EntityKind::Variable => {
                if !entity.metadata.contains_key("phpdoc_var_type")
                    && let Some(ty) = parse_phpdoc_var(doc)
                {
                    entity.set_metadata("phpdoc_var_type", ty);
                }
            }
            _ => {}
        }
    }
}

/// Read a capture's source text, extended over following siblings that
/// share its tree-sitter field name.
///
/// Some grammars split one logical field into several same-field children
/// (e.g. tree-sitter-dart reports an initializer as multiple `value`
/// children, and generic return types as multiple `return_type` children)
/// while the query binds only one of them. Covering the full sibling range
/// restores the complete text from source. Single-child fields are
/// unaffected, so this is safe to apply uniformly.
fn capture_text_over_field_siblings(
    mat: &QueryMatch,
    tree: &tree_sitter::Tree,
    source: &str,
    predicate: impl Fn(&str) -> bool,
) -> Option<String> {
    let capture = utils::find_capture_by_name(&mat.captures, &predicate)?;
    let node = node_for_capture(tree, capture.start_byte, capture.end_byte)?;
    let parent = node.parent()?;
    let mut field_name = None;
    let mut position = None;
    for i in 0..parent.child_count() {
        if let Some(child) = parent.child(i as u32) {
            if child.start_byte() == node.start_byte() && child.end_byte() == node.end_byte() {
                field_name = parent.field_name_for_child(i as u32).map(str::to_string);
                position = Some(i);
                break;
            }
        }
    }
    let (field_name, position) = match (field_name, position) {
        (Some(name), Some(pos)) => (name, pos),
        // Unfielded value captures (e.g. Kotlin `(_)? @....value`) carry
        // the full initializer in the capture itself; use it directly.
        _ => {
            return source
                .get(capture.start_byte..capture.end_byte)
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty());
        }
    };
    let mut end_byte = node.end_byte();
    for i in position + 1..parent.child_count() {
        let same_field = parent
            .field_name_for_child(i as u32)
            .is_some_and(|name| name == field_name);
        if !same_field {
            break;
        }
        if let Some(child) = parent.child(i as u32) {
            if child.is_named() {
                end_byte = end_byte.max(child.end_byte());
            }
        }
    }
    source
        .get(node.start_byte()..end_byte)
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

/// Find an explicit type annotation capture for variable-like declarations.
///
/// Matches tree-sitter captures produced for annotated declarations across
/// languages, e.g. Go/Python variable types as well as Kotlin property and
/// C field types. Returns the raw captured text.
fn find_type_annotation_capture(mat: &QueryMatch) -> Option<&str> {
    utils::find_capture_by_name(&mat.captures, |name| {
        (name.contains(".variable.") && name.ends_with(".type"))
            || name.ends_with(".property.type")
            || name.ends_with(".field.type")
    })
    .map(|c| c.text.as_str())
}

/// Record initializer-derived metadata (constructor call, generic call
/// target, or literal type) for a declaration with the given value text.
fn extract_initializer_metadata(entity: &mut Entity, value_text: &str, language: &Language) {
    let trimmed = value_text.trim();
    if trimmed.is_empty() {
        return;
    }

    // Check for constructor call: new ClassName() or ClassName()
    if let Some(type_name) = extract_constructor_type_from_expr(trimmed) {
        entity.set_metadata("constructor_type", type_name);
        // Remember explicitly written type arguments (`new Foo<Bar>()`) so
        // the class-type-parameter post-pass only synthesizes generics for
        // bare calls (`new Foo()`), never overwriting concrete arguments.
        if let Some(args) = extract_explicit_type_args(trimmed) {
            entity.set_metadata("constructor_type_args", args);
        }
        // A bare `Name(...)` call may be a PascalCase method rather than a
        // constructor (C#/Java/Kotlin/Scala methods are capitalized). Keep
        // the call target with its argument list alongside so inference can
        // try same-file function-return resolution (with overload awareness)
        // before falling back to the constructor reading. Explicit `new`
        // expressions and qualified `X.new` forms stay constructor-only.
        let is_plain_call = !trimmed.starts_with("new ")
            && !trimmed.trim_end_matches(')').trim_end().ends_with(".new");
        if is_plain_call && let Some(call_target) = extract_call_target_from_expr(trimmed) {
            entity.set_metadata("call_target", call_target);
        }
        return;
    }

    // Check for generic function call: foo(), module.func(), obj.method()
    if let Some(call_target) = extract_call_target_from_expr(trimmed) {
        entity.set_metadata("call_target", call_target);
        // Fall through to also capture literal if needed, but call_target takes precedence
        // Do not return: still check literal for chained cases? For now keep call_target only.
        return;
    }

    // Simple binary `+` (`first + first`): record the operands so the
    // language inferer can apply string-concatenation / numeric rules.
    // Java-only: `+` semantics differ per language.
    if *language == Language::Java
        && let Some((lhs, rhs)) = extract_binary_plus_operands(trimmed)
    {
        entity.set_metadata("binary_plus", format!("{lhs} + {rhs}"));
        return;
    }

    // Check for literal type
    if let Some(lit_type) = extract_literal_type(trimmed, language) {
        entity.set_metadata("literal_type", lit_type);
    }
}

/// Extract operands of a simple `lhs + rhs` expression.
///
/// Only plain two-operand additions qualify: exactly one top-level `+`
/// (no `++`/`+=`), with both sides free of nesting, quotes and other
/// operators so downstream shape resolution stays deterministic.
fn extract_binary_plus_operands(expr: &str) -> Option<(String, String)> {
    let bytes = expr.as_bytes();
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut plus_pos: Option<usize> = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = quote {
            if b == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match b {
            b'\'' | b'"' => {
                quote = Some(b);
                i += 1;
            }
            b'(' | b'[' | b'{' => {
                depth += 1;
                i += 1;
            }
            b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b'+' => {
                if depth == 0 {
                    if bytes.get(i + 1) == Some(&b'+') || bytes.get(i + 1) == Some(&b'=') {
                        return None;
                    }
                    if plus_pos.is_some() {
                        return None;
                    }
                    plus_pos = Some(i);
                }
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
    if quote.is_some() || depth != 0 {
        return None;
    }
    let pos = plus_pos?;
    if pos == 0 || pos + 1 >= bytes.len() {
        return None;
    }
    if bytes.get(pos.saturating_sub(1)) == Some(&b'=') {
        return None;
    }
    let lhs = expr[..pos].trim();
    let rhs = expr[pos + 1..].trim();
    if !is_simple_plus_operand(lhs) || !is_simple_plus_operand(rhs) {
        return None;
    }
    Some((lhs.to_string(), rhs.to_string()))
}

/// Whether a `+` operand is a plain identifier (possibly qualified) or
/// numeric literal: no nesting, quotes, whitespace or further operators.
fn is_simple_plus_operand(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '.'))
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
    tree: &tree_sitter::Tree,
) {
    // Extract type annotation from tree-sitter captures if available.
    // Go: @entity.variable.type on long-form `var` declarations
    // Python: @entity.variable.typed.type on annotated assignments
    // Kotlin/Scala/Dart/C: corresponding `.variable.*.type` captures
    if let Some(type_text) = find_type_annotation_capture(mat) {
        let trimmed = type_text.trim();
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
            extract_java_variable_metadata(mat, entity, source, tree);
        }
        Language::CSharp => {
            extract_csharp_variable_metadata(mat, entity);
        }
        Language::Kotlin => {
            extract_kotlin_variable_metadata(mat, entity, source, tree);
        }
        _ => {}
    }

    // Find the value capture (initializer expression), covering split
    // same-field siblings (e.g. Dart generic instantiation parts).
    let value_text = capture_text_over_field_siblings(mat, tree, source, |name| {
        name.ends_with(".value")
            || name.ends_with(".const.value")
            || name.ends_with(".let.value")
            || name.ends_with(".var.value")
    });

    let Some(value) = value_text else {
        return;
    };

    extract_initializer_metadata(entity, &value, language);
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
/// - `x instanceof Type name` — pattern variable (write `source_type` from
///   the `right` operand; the query cannot capture it because tree-sitter-java
///   rejects `name:` alongside any sibling type child in one pattern)
fn extract_java_variable_metadata(
    mat: &QueryMatch,
    entity: &mut Entity,
    source: &str,
    tree: &tree_sitter::Tree,
) {
    // Type inference for `var` declarations requires AST-based analysis rather than
    // source-text heuristics. The previous implementation used `infer_java_type_from_expr`
    // which was a string-based heuristic that has been removed as part of the symbol
    // resolution determinization effort.
    //
    // TODO: Implement AST-based type inference for Java `var` declarations
    if entity.subtype.as_deref() != Some("case") {
        return;
    }
    if entity.metadata.contains_key("source_type") {
        return;
    }
    let Some(name_capture) = mat.captures.iter().find(|c| c.name.ends_with(".name")) else {
        return;
    };
    let Some(mut node) = tree
        .root_node()
        .descendant_for_byte_range(name_capture.start_byte, name_capture.end_byte)
    else {
        return;
    };
    while node.kind() != "instanceof_expression" {
        let Some(parent) = node.parent() else {
            return;
        };
        node = parent;
    }
    let type_node = node.child_by_field_name("right").or_else(|| {
        let mut cursor = node.walk();
        node.children(&mut cursor)
            .find(|child| child.is_named() && child.kind() != "identifier")
    });
    let Some(type_node) = type_node else {
        return;
    };
    let text =
        &source[type_node.start_byte().min(source.len())..type_node.end_byte().min(source.len())];
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        entity.set_metadata("source_type", trimmed.to_string());
    }
}

/// Extract Kotlin-specific variable metadata.
///
/// Destructuring declarations (`val (a, b) = expr`) fold into one
/// comma-separated `.multiple` entity; the query cannot capture the
/// right-hand side without ambiguity, so the source expression is attached
/// here from the enclosing `property_declaration`.
fn extract_kotlin_variable_metadata(
    mat: &QueryMatch,
    entity: &mut Entity,
    source: &str,
    tree: &tree_sitter::Tree,
) {
    if entity.subtype.as_deref() != Some("multiple") {
        return;
    }
    if entity.metadata.contains_key("source_type") {
        return;
    }
    let Some(name_capture) = mat.captures.iter().find(|c| c.name.ends_with(".name")) else {
        return;
    };
    let Some(mut node) = tree
        .root_node()
        .descendant_for_byte_range(name_capture.start_byte, name_capture.end_byte)
    else {
        return;
    };
    while node.kind() != "property_declaration" {
        let Some(parent) = node.parent() else {
            return;
        };
        node = parent;
    }
    let mut cursor = node.walk();
    let mut seen_multi = false;
    for child in node.children(&mut cursor) {
        if !seen_multi {
            if child.kind() == "multi_variable_declaration" {
                seen_multi = true;
            }
            continue;
        }
        if child.is_named() {
            let text =
                &source[child.start_byte().min(source.len())..child.end_byte().min(source.len())];
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                entity.set_metadata("source_type", trimmed.to_string());
            }
            return;
        }
    }
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

/// Drop generic arguments from a callee or type prefix.
///
/// Turns `Container<String>` into `Container` so generic constructor and
/// call expressions still resolve to their base type. Only cuts at the
/// first `<` when the prefix holds no whitespace, so comparisons such as
/// `a < b(c)` keep falling through to validation instead of resolving to
/// a bogus callee.
fn truncate_generic_args(name: &str) -> &str {
    if name.contains(char::is_whitespace) {
        return name;
    }
    match name.find('<') {
        Some(pos) => name[..pos].trim_end(),
        None => name,
    }
}

/// Split a constructor base like `ArrayList<String, Integer>` into its bare
/// name and explicit generic arguments, preserving them for inference.
///
/// Returns `None` when there are no brackets, the brackets are unbalanced
/// or empty (`ArrayList<>` degrades to the bare name), or the text contains
/// characters outside the type-argument vocabulary. Whitespace inside the
/// argument list is preserved; downstream shape parsing trims per argument.
fn split_explicit_generic_args(base: &str) -> Option<(&str, &str)> {
    let start = base.find('<')?;
    // Match the bracket opened at `start` against nesting depth so nested
    // generics (`HashMap<String, List<Integer>>`) stay intact.
    let bytes = base.as_bytes();
    let mut depth = 0usize;
    let mut end = None;
    for (i, b) in bytes.iter().enumerate().skip(start) {
        match b {
            b'<' => depth += 1,
            b'>' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end?;
    // Trailing text after the closing bracket (e.g. array suffixes) keeps
    // the conservative bare-name reading.
    if !base[end + 1..].trim().is_empty() {
        return None;
    }
    let name = base[..start].trim_end();
    let args = base[start + 1..end].trim();
    // Whitespace is allowed inside the argument list (`HashMap<String,
    // Integer>`) but never in the bare class name.
    if name.is_empty() || args.is_empty() || !is_valid_type_name(name) {
        return None;
    }
    if name.contains(char::is_whitespace) {
        return None;
    }
    if !args.chars().all(|c| {
        c.is_alphanumeric()
            || matches!(
                c,
                '_' | '.' | ':' | ',' | ' ' | '<' | '>' | '[' | ']' | '?' | '*' | '&'
            )
    }) {
        return None;
    }
    Some((name, args))
}

/// Reattach explicit generic arguments to a bare constructor name.
///
/// `ArrayList` + `new ArrayList<String>()` yields `ArrayList<String>` so
/// `var names`-style inference keeps the element type. Returns the bare
/// name unchanged when no usable argument list is present.
fn with_explicit_generic_args(bare: &str, base: &str) -> String {
    match split_explicit_generic_args(base) {
        Some((name, args)) if name == bare => format!("{bare}<{args}>"),
        _ => bare.to_string(),
    }
}

/// Extract the type name from a constructor call expression.
///
/// Handles patterns like:
/// - `new ClassName()`
/// - `new ClassName(args)`
/// - `ClassName()`
/// - `ClassName(args)`
/// - `module.ClassName()`
/// - `Container<T>(args)` (generic arguments are stripped)
/// - `ClassName.new(...)` (normalized to `ClassName`)
/// - `ClassName.new` (Ruby-style without parentheses)
fn extract_constructor_type_from_expr(expr: &str) -> Option<String> {
    let trimmed = expr.trim();

    // new ClassName(...) / new ClassName<T>(...) (explicit type arguments
    // are preserved so `new ArrayList<String>()` infers `ArrayList<String>`).
    if let Some(rest) = trimmed.strip_prefix("new ") {
        let base = rest.split('(').next().unwrap_or(rest).trim();
        // Multi-argument generics (`HashMap<String, Integer>`) carry
        // spaces the plain truncation rejects; parse them first.
        if let Some((name, args)) = split_explicit_generic_args(base) {
            return Some(format!("{name}<{args}>"));
        }
        let bare = truncate_generic_args(base).trim();
        if is_valid_type_name(bare) {
            return Some(with_explicit_generic_args(bare, base));
        }
    }

    // ClassName(...) - function call that looks like a constructor
    if let Some(paren_pos) = trimmed.find('(') {
        let base = trimmed[..paren_pos].trim();
        if let Some((name, args)) = split_explicit_generic_args(base) {
            if name.chars().next().is_some_and(|c| c.is_uppercase()) {
                return Some(format!("{name}<{args}>"));
            }
        }
        let func_name = truncate_generic_args(base).trim();
        // Ruby-style `ClassName.new(...)` normalizes to the class name.
        let func_name = func_name.strip_suffix(".new").unwrap_or(func_name);
        // Constructor calls typically start with uppercase
        if is_valid_type_name(func_name)
            && func_name.chars().next().is_some_and(|c| c.is_uppercase())
        {
            return Some(with_explicit_generic_args(func_name, base));
        }
    }

    // ClassName.new (Ruby-style without parentheses)
    if trimmed.ends_with(".new") {
        let type_name = trimmed.trim_end_matches(".new").trim();
        if is_valid_type_name(type_name)
            && type_name.chars().next().is_some_and(|c| c.is_uppercase())
        {
            return Some(type_name.to_string());
        }
    }

    None
}

/// Extract the call target from a function call expression for cross-file propagation.
///
/// Handles patterns like:
/// - `foo()`, `create_user()`, `module.func()`, `obj.method()`
///
/// Returns the function name portion before `(` with generic arguments
/// stripped, so `wrapInList(10)` and `make<int>(1)` resolve uniformly.
/// The balanced argument list is preserved (`foo(a, b)`) so call-site
/// generic substitution can recover argument expressions downstream;
/// over-long argument lists fall back to the bare name to bound metadata
/// size. Consumers that need only the callee must strip the argument list
/// (everything from the first `(`) before name lookups.
fn extract_call_target_from_expr(expr: &str) -> Option<String> {
    let trimmed = expr.trim();
    // Skip literals and already handled constructors (uppercase check already gated)
    // Must contain '(' to be a call.
    let paren_pos = trimmed.find('(')?;
    let func_name = truncate_generic_args(trimmed[..paren_pos].trim()).trim();
    if func_name.is_empty() {
        return None;
    }
    // Reject control flow keywords and invalid names.
    const KEYWORDS: &[&str] = &["if", "while", "for", "match", "return", "await", "yield"];
    if KEYWORDS.contains(&func_name) {
        return None;
    }
    // Allow qualified names with `.`, `::`, `/` plus receiver-qualified
    // calls (`$this->m()`, `$svc->m()`, `ptr->m()`, `user?.m()`). The
    // strict type-name check stays on the constructor path; call targets
    // only need a plausible trailing identifier.
    if !is_valid_call_target_name(func_name) {
        // Avoid capturing literals like `42(` which would be invalid.
        return None;
    }
    // Preserve the balanced argument list for call-site analysis.
    const MAX_CALL_TARGET_ARGS: usize = 8;
    const MAX_CALL_TARGET_ARGLEN: usize = 200;
    if let Some(args) = balanced_call_args(&trimmed[paren_pos..]) {
        let arg_count = args.matches(',').count() + 1;
        if !args.trim().is_empty()
            && arg_count <= MAX_CALL_TARGET_ARGS
            && args.len() <= MAX_CALL_TARGET_ARGLEN
        {
            return Some(format!("{}({})", func_name, args.trim()));
        }
    }
    Some(func_name.to_string())
}

/// Extract the balanced argument list (without outer parentheses) from text
/// starting at `(`.
///
/// Tracks string quotes and nested bracket pairs so commas and closing
/// brackets inside arguments do not end the scan early. Returns `None`
/// when the parentheses never balance.
fn balanced_call_args(text_from_paren: &str) -> Option<String> {
    if !text_from_paren.starts_with('(') {
        return None;
    }
    let mut stack: Vec<char> = Vec::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut end = None;
    for (idx, ch) in text_from_paren.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' | '`' => quote = Some(ch),
            '(' => stack.push(')'),
            '[' => stack.push(']'),
            '{' => stack.push('}'),
            ')' | ']' | '}' => {
                if stack.pop() != Some(ch) {
                    return None;
                }
                if stack.is_empty() {
                    end = Some(idx);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end?;
    Some(text_from_paren[1..end].to_string())
}

/// Extract the type from a literal expression using the target language's
/// own vocabulary (`int` for C++, `i32` for Rust, `number` only for
/// JavaScript/TypeScript).
fn extract_literal_type(expr: &str, language: &Language) -> Option<String> {
    let trimmed = expr.trim();

    // Numeric literal (underscores, base prefixes and Rust-style suffixes
    // included so `1_000`, `0xFF` and `10u32` still resolve).
    if let Some(kind) = classify_numeric_literal(trimmed) {
        return Some(literal_type_name(language, kind).to_string());
    }

    // String literal
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() > 3)
        || trimmed.starts_with("r\"")
        || trimmed.starts_with("r#")
    {
        return Some(literal_type_name(language, LiteralKind::String).to_string());
    }

    // Single-character literal (`'a'` in C-like languages).
    if trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() == 3 {
        return Some(literal_type_name(language, LiteralKind::Char).to_string());
    }

    // Boolean literal
    if trimmed == "true" || trimmed == "false" {
        return Some(literal_type_name(language, LiteralKind::Boolean).to_string());
    }

    // None/null
    if trimmed == "None" || trimmed == "null" || trimmed == "nil" {
        return Some(literal_type_name(language, LiteralKind::Null).to_string());
    }

    // Array literal
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        return Some(literal_type_name(language, LiteralKind::Array).to_string());
    }

    // Object literal
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Some(literal_type_name(language, LiteralKind::Object).to_string());
    }

    None
}

/// Normalize a JavaScript return-expression capture into a return type.
///
/// Plain JavaScript has no return-type annotations, so the query captures
/// the returned *expression*. A literal expression still carries a usable
/// type (`return 1` yields `number`); a construction expression yields its
/// class (`return new User(...)` yields `User`). Anything else (calls,
/// arithmetic, identifiers) is dropped so the inferer falls back to
/// `unknown` instead of treating `utils.alpha() + utils.beta()` as a
/// type name.
pub(crate) fn normalize_js_return_expression(expr: &str) -> Option<String> {
    if let Some(lit) = extract_literal_type(expr, &Language::JavaScript) {
        return Some(lit);
    }
    let head = expr.trim().strip_prefix("new ")?;
    let head = head.split('(').next().unwrap_or(head).trim();
    // Generic instantiation keeps only the base name; member construction
    // (`new ns.User()`) keeps the final segment.
    let base = head.split('<').next().unwrap_or(head).trim();
    let name = base.rsplit('.').next().unwrap_or(base).trim();
    if name.is_empty() || !is_valid_type_name(name) {
        return None;
    }
    Some(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_target_preserves_args() {
        assert_eq!(
            extract_call_target_from_expr("makePair(42, \"answer\")"),
            Some("makePair(42, \"answer\")".to_string())
        );
        assert_eq!(
            extract_call_target_from_expr("foo()"),
            Some("foo".to_string())
        );
        assert_eq!(
            extract_call_target_from_expr("module.func(a, b)"),
            Some("module.func(a, b)".to_string())
        );
    }

    #[test]
    fn test_call_target_nested_and_quoted() {
        assert_eq!(
            extract_call_target_from_expr("wrap(g(1, 2), [x, y])"),
            Some("wrap(g(1, 2), [x, y])".to_string())
        );
        assert_eq!(
            extract_call_target_from_expr("f(\"a)b\", 'c')"),
            Some("f(\"a)b\", 'c')".to_string())
        );
    }

    #[test]
    fn test_call_target_fallbacks() {
        // Unbalanced input keeps the bare name.
        assert_eq!(
            extract_call_target_from_expr("foo(a"),
            Some("foo".to_string())
        );
        // Keywords and literals still rejected.
        assert_eq!(extract_call_target_from_expr("if (x)"), None);
        assert_eq!(extract_call_target_from_expr("42(x)"), None);
        // Generic turbofish stripped from the name.
        assert_eq!(
            extract_call_target_from_expr("make<int>(1)"),
            Some("make(1)".to_string())
        );
    }

    #[test]
    fn test_call_target_receiver_qualified() {
        // PHP `$this->m()` / `$svc->m()` keep the receiver path; the
        // inference layer strips to the trailing identifier.
        assert_eq!(
            extract_call_target_from_expr("$this->combineInts(1, 2)"),
            Some("$this->combineInts(1, 2)".to_string())
        );
        assert_eq!(
            extract_call_target_from_expr("$svc->loadUser(\"a\")"),
            Some("$svc->loadUser(\"a\")".to_string())
        );
        // C++ member access and Kotlin safe-call pass validation too.
        assert_eq!(
            extract_call_target_from_expr("ptr->method(arg)"),
            Some("ptr->method(arg)".to_string())
        );
        assert_eq!(
            extract_call_target_from_expr("user?.getName()"),
            Some("user?.getName".to_string())
        );
        // Operator noise still rejected.
        assert_eq!(extract_call_target_from_expr("a + b(c)"), None);
        assert_eq!(extract_call_target_from_expr("a->"), None);
    }
}
