//! TSX/JSX language query schemes
//!
//! Provides Tree-sitter query patterns for identifying and analyzing
//! TSX (TypeScript with JSX) and JSX code.
//!
//! TSX extends TypeScript with JSX support for component-based UI.

use super::javascript;
use super::typescript;

/// Get shared JSX entity query patterns
///
/// Returns patterns for JSX-specific entities:
/// - JSX elements (tags)
/// - JSX attributes (including spread attributes)
/// - JSX expressions
/// - JSX text
fn jsx_entity_shared() -> &'static str {
    r#"
; ============================================
; JSX Elements
; ============================================

; JSX opening element
(jsx_opening_element
  name: (identifier) @entity.jsx.element.name
  attribute: (jsx_attribute)* @entity.jsx.element.attributes
) @entity.jsx.element.opening

; JSX closing element
(jsx_closing_element
  name: (identifier) @entity.jsx.element.close_name
) @entity.jsx.element.closing

; JSX self-closing element
(jsx_self_closing_element
  name: (identifier) @entity.jsx.element.self_closing.name
  attribute: (jsx_attribute)* @entity.jsx.element.self_closing.attributes
) @entity.jsx.element.self_closing

; JSX element (wrapping opening, children, closing)
(jsx_element
  (jsx_opening_element) @entity.jsx.element.full.opening
  (jsx_closing_element)? @entity.jsx.element.full.closing
) @entity.jsx.element.full

; ============================================
; JSX Components (PascalCase)
; ============================================

; Component element (starts with uppercase)
(jsx_opening_element
  name: (identifier) @entity.jsx.component.opening.name
  (#match? @entity.jsx.component.opening.name "^[A-Z]")
) @entity.jsx.component.opening

; Self-closing component
(jsx_self_closing_element
  name: (identifier) @entity.jsx.component.self_closing.name
  (#match? @entity.jsx.component.self_closing.name "^[A-Z]")
) @entity.jsx.component.self_closing

; Closing component tag
(jsx_closing_element
  name: (identifier) @entity.jsx.component.closing.name
  (#match? @entity.jsx.component.closing.name "^[A-Z]")
) @entity.jsx.component.closing

; ============================================
; JSX Attributes
; ============================================

; Standard JSX attribute
(jsx_attribute
  (property_identifier) @entity.jsx.attribute.name
) @entity.jsx.attribute

; JSX expression in attribute value
(jsx_expression
  (_) @entity.jsx.attribute.expr.value
) @entity.jsx.attribute.expr

; ============================================
; JSX Expressions
; ============================================

; JSX expression container { ... }
(jsx_expression
  (expression) @entity.jsx.expression.content
) @entity.jsx.expression

; Conditional expression in JSX
(jsx_expression
  (ternary_expression) @entity.jsx.expression.conditional
) @entity.jsx.expression.ternary

; Logical expression in JSX (&& operator)
(jsx_expression
  (binary_expression) @entity.jsx.expression.logical
) @entity.jsx.expression.logical

; ============================================
; JSX Text
; ============================================

; JSX text content
(jsx_text) @entity.jsx.text

; HTML character reference (&nbsp;)
(html_character_reference) @entity.jsx.html_entity

; ============================================
; Special JSX Attributes
; ============================================

; key attribute
(jsx_attribute
  (property_identifier) @entity.jsx.key.name
  (#eq? @entity.jsx.key.name "key")
) @entity.jsx.key

; ref attribute (callback ref)
(jsx_attribute
  (property_identifier) @entity.jsx.ref.attr.name
  (#eq? @entity.jsx.ref.attr.name "ref")
  (jsx_expression
    (_) @entity.jsx.ref.callback
  )
) @entity.jsx.ref.callback_attr

; ref attribute (string ref - deprecated)
(jsx_attribute
  (property_identifier) @entity.jsx.ref.string.name
  (#eq? @entity.jsx.ref.string.name "ref")
  (string)
) @entity.jsx.ref.string_attr

; className attribute (React convention)
(jsx_attribute
  (property_identifier) @entity.jsx.className.name
  (#eq? @entity.jsx.className.name "className")
) @entity.jsx.className

; style attribute
(jsx_attribute
  (property_identifier) @entity.jsx.style.attr.name
  (#eq? @entity.jsx.style.attr.name "style")
) @entity.jsx.style.attr

; onClick, onSubmit, etc. event handlers
(jsx_attribute
  (property_identifier) @entity.jsx.event.name
  (#match? @entity.jsx.event.name "^on[A-Z]")
) @entity.jsx.event.handler

; dangerouslySetInnerHTML
(jsx_attribute
  (property_identifier) @entity.jsx.dangerous.name
  (#eq? @entity.jsx.dangerous.name "dangerouslySetInnerHTML")
) @entity.jsx.dangerous
"#
}

/// Get JSX structural query patterns
///
/// Returns patterns for JSX structural relationships:
/// - Component usage
/// - Element nesting
/// - Prop passing
fn jsx_structural_shared() -> &'static str {
    r#"
; ============================================
; Component Usage
; ============================================

; Component element usage
(jsx_element
  (jsx_opening_element
    name: (identifier) @call.constructor.component.name
    (#match? @call.constructor.component.name "^[A-Z]")
  )
) @call.constructor.component

; Self-closing component usage
(jsx_self_closing_element
  name: (identifier) @call.constructor.component.self_closing.name
  (#match? @call.constructor.component.self_closing.name "^[A-Z]")
) @call.constructor.component.self_closing

; ============================================
; Element Contains
; ============================================

; JSX element contains child elements
(jsx_element
  (jsx_opening_element
    name: (identifier) @entity.jsx.parent.name
  )
  (jsx_element
    (jsx_opening_element
      name: (identifier) @entity.jsx.child.name
    )
  )+ @entity.jsx.children
) @entity.jsx.contains

; JSX element contains expression
(jsx_element
  (jsx_opening_element)
  (jsx_expression) @entity.jsx.expression.child
) @entity.jsx.contains.expr

; ============================================
; Prop Bindings
; ============================================

; JSX attribute as prop
(jsx_attribute
  (property_identifier) @entity.variable.parameter.prop.name
) @entity.variable.parameter.prop

; ============================================
; Event Bindings
; ============================================

; Event handler binding (onClick, onSubmit, etc.)
(jsx_attribute
  (property_identifier) @call.callback.event.name
  (#match? @call.callback.event.name "^on[A-Z]")
  (jsx_expression
    (_) @call.callback.event.handler
  )
) @call.callback.event

; ============================================
; Template References
; ============================================

; ref attribute binding
(jsx_attribute
  (property_identifier) @entity.template.name
  (#eq? @entity.template.name "ref")
) @entity.template

; ============================================
; Class Bindings
; ============================================

; className attribute (React)
(jsx_attribute
  (property_identifier) @entity.attribute.class.name
  (#eq? @entity.attribute.class.name "className")
) @entity.attribute.class

; class attribute (standard HTML)
(jsx_attribute
  (property_identifier) @entity.attribute.class.name
  (#eq? @entity.attribute.class.name "class")
) @entity.attribute.class

; ============================================
; Style Bindings
; ============================================

; style attribute binding
(jsx_attribute
  (property_identifier) @entity.attribute.style.name
  (#eq? @entity.attribute.style.name "style")
) @entity.attribute.style
"#
}

/// Get JSX dependency query patterns
///
/// Returns patterns for JSX-specific dependencies
fn jsx_dependency_shared() -> &'static str {
    r#"
; ============================================
; Component Dependencies from JSX
; ============================================

; Component usage implies dependency
(jsx_opening_element
  name: (identifier) @dependency.import.component.name
  (#match? @dependency.import.component.name "^[A-Z]")
) @dependency.import.component

(jsx_self_closing_element
  name: (identifier) @dependency.import.component.self_closing.name
  (#match? @dependency.import.component.self_closing.name "^[A-Z]")
) @dependency.import.component.self_closing

; ============================================
; JSX Namespace Dependencies
; ============================================

; JSX namespace component (<MyLibrary.Component />)
(jsx_opening_element
  name: (jsx_namespace_name
    (identifier) @entity.jsx.namespace.dependency.ns
    (identifier) @entity.jsx.namespace.dependency.name
  )
) @entity.jsx.namespace.dependency

(jsx_self_closing_element
  name: (jsx_namespace_name
    (identifier) @entity.jsx.namespace.dependency.ns
    (identifier) @entity.jsx.namespace.dependency.self_closing.name
  )
) @entity.jsx.namespace.dependency.self_closing
"#
}

// ============================================================================
// TSX Public API
// ============================================================================

/// Get entity query for TSX
///
/// Returns Tree-sitter query patterns combining TypeScript entities with JSX entities
pub fn entity_query() -> String {
    let mut query = String::new();
    // TypeScript entities (includes JavaScript shared entities)
    query.push_str(&typescript::entity_query());
    // JSX-specific entities
    query.push_str(jsx_entity_shared());
    query
}

/// Get comment query for TSX
///
/// Returns Tree-sitter query patterns for identifying TSX comments.
/// TSX supports both JavaScript comments (//, /* */) and JSX HTML comments (<!-- -->).
pub fn comment_query() -> &'static str {
    r#"
; ============================================
; JavaScript Comments
; ============================================

; JavaScript/TypeScript comments (both line and block)
(comment) @comment

; ============================================
; JSX HTML Comments
; ============================================

; HTML-style comment in JSX (<!-- -->)
(html_comment) @comment.block
"#
}

/// Get call query for TSX
///
/// Returns Tree-sitter query patterns for identifying TSX call relationships.
/// Reuses TypeScript call patterns.
pub fn call_query() -> String {
    typescript::call_query()
}

/// Get structural query for TSX
///
/// Returns Tree-sitter query patterns for identifying TSX structural relationships
/// including JSX component usage and element nesting
pub fn structural_query() -> String {
    let mut query = String::new();
    // TypeScript/JSX structural patterns
    query.push_str(jsx_structural_shared());
    query
}

/// Get dependency query for TSX
///
/// Returns Tree-sitter query patterns for identifying TSX dependencies:
/// - TypeScript imports
/// - JSX component dependencies
pub fn dependency_query() -> String {
    let mut query = String::new();
    // TypeScript dependencies
    query.push_str(&typescript::dependency_query());
    // JSX-specific dependencies
    query.push_str(jsx_dependency_shared());
    query
}

/// Get behavior query for TSX
pub fn behavior_query() -> String {
    javascript::behavior_query()
}

/// Get control-flow query for TSX
pub fn control_flow_query() -> &'static str {
    javascript::control_flow_query()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Query;

    /// Validate query syntax and return detailed error information
    fn validate_query_syntax(query_name: &str, query_str: &str) -> Result<(), String> {
        let lang = tree_sitter_typescript::LANGUAGE_TSX;
        match Query::new(&lang.into(), query_str) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Query '{}' syntax error: {:?}", query_name, e)),
        }
    }

    #[test]
    fn test_entity_query_syntax_valid() {
        let result = validate_query_syntax("entity_query", &entity_query());
        assert!(
            result.is_ok(),
            "Entity query syntax validation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_structural_query_syntax_valid() {
        let result = validate_query_syntax("structural_query", &structural_query());
        assert!(
            result.is_ok(),
            "Structural query syntax validation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_call_query_syntax_valid() {
        let result = validate_query_syntax("call_query", &call_query());
        assert!(
            result.is_ok(),
            "Call query syntax validation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_dependency_query_syntax_valid() {
        let result = validate_query_syntax("dependency_query", &dependency_query());
        assert!(
            result.is_ok(),
            "Dependency query syntax validation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_behavior_query_syntax_valid() {
        let result = validate_query_syntax("behavior_query", &behavior_query());
        assert!(
            result.is_ok(),
            "Behavior query syntax validation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_control_flow_query_syntax_valid() {
        let result = validate_query_syntax("control_flow_query", control_flow_query());
        assert!(
            result.is_ok(),
            "Control-flow query syntax validation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_comment_query_syntax_valid() {
        let result = validate_query_syntax("comment_query", comment_query());
        assert!(
            result.is_ok(),
            "Comment query syntax validation failed: {:?}",
            result.err()
        );
    }
}
