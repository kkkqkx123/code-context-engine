//! CSS language query schemes
//!
//! Provides Tree-sitter query patterns for identifying and analyzing
//! CSS code entities, selectors, and rules.
//!
//! Tree-sitter version: 0.23.x

/// Get entity query for CSS
///
/// Returns Tree-sitter query patterns for identifying CSS entities:
/// - Rule sets (selectors + declarations)
/// - Selectors (class, id, tag, attribute, pseudo)
/// - Declarations (property + value)
/// - At rules (@media, @keyframes, @import, etc.)
pub fn entity_query() -> &'static str {
    r#"
; ============================================
; Rule Sets
; ============================================

; Rule set (selector + block)
(rule_set
  (selectors) @entity.style_rule.selectors
  (block) @entity.style_rule.block
) @entity.style_rule

; ============================================
; Selectors
; ============================================

; Class selector (.class)
(class_selector
  (class_name) @entity.style_selector.class.name
) @entity.style_selector.class

; ID selector (#id)
(id_selector
  (id_name) @entity.style_selector.id.name
) @entity.style_selector.id

; Tag/Type selector
(tag_name) @entity.style_selector.tag.name

; Universal selector (*)
(universal_selector) @entity.style_selector.universal

; Attribute selector ([attr], [attr=value])
(attribute_selector
  (attribute_name) @entity.style_selector.attribute.name
  (string_value)? @entity.style_selector.attribute.value
) @entity.style_selector.attribute

; Pseudo-class selector (:hover, :nth-child())
(pseudo_class_selector
  (class_name) @entity.style_selector.pseudo_class.name
  (arguments)? @entity.style_selector.pseudo_class.args
) @entity.style_selector.pseudo_class

; Pseudo-element selector (::before, ::after)
(pseudo_element_selector
  (tag_name) @entity.style_selector.pseudo_element.name
) @entity.style_selector.pseudo_element

; Nesting selector (&)
(nesting_selector) @entity.style_selector.nesting

; ============================================
; Selector Combinators
; ============================================

; Descendant selector (space)
(descendant_selector
  (_) @entity.style_selector.descendant.left
  (_) @entity.style_selector.descendant.right
) @entity.style_selector.descendant

; Child selector (>)
(child_selector
  (_) @entity.style_selector.child.left
  (_) @entity.style_selector.child.right
) @entity.style_selector.child

; Sibling selector (~)
(sibling_selector
  (_) @entity.style_selector.sibling.left
  (_) @entity.style_selector.sibling.right
) @entity.style_selector.sibling

; Adjacent sibling selector (+)
(adjacent_sibling_selector
  (_) @entity.style_selector.adjacent.left
  (_) @entity.style_selector.adjacent.right
) @entity.style_selector.adjacent

; ============================================
; Declarations
; ============================================

; Property declaration
(declaration
  (property_name) @entity.style_property.name
  (_) @entity.style_property.value
  (important)? @entity.style_property.important
) @entity.style_property

; ============================================
; Values
; ============================================

; Function call (rgb(), calc(), var())
(call_expression
  (function_name) @entity.style_value.function.name
  (arguments) @entity.style_value.function.args
) @entity.style_value.function

; String value
(string_value) @entity.style_value.string

; Color value
(color_value) @entity.style_value.color

; Integer with unit
(integer_value
  (unit)? @entity.style_value.unit
) @entity.style_value.integer

; Float with unit
(float_value
  (unit)? @entity.style_value.unit
) @entity.style_value.float

; Plain identifier value
(plain_value) @entity.style_value.plain

; ============================================
; At Rules
; ============================================

; @charset
(charset_statement
  (string_value) @entity.at.charset.encoding
) @entity.at.charset

; @import
(import_statement
  (string_value) @entity.at.import.path
) @entity.at.import

; @namespace
(namespace_statement
  (namespace_name) @entity.at.namespace.name
  (string_value) @entity.at.namespace.url
) @entity.at.namespace

; @media
(media_statement
  (block) @entity.at.media.block
) @entity.at.media

; @supports
(supports_statement
  (block) @entity.at.supports.block
) @entity.at.supports

; @keyframes
(keyframes_statement
  (keyframes_name) @entity.at.keyframes.name
  (keyframe_block_list) @entity.at.keyframes.blocks
) @entity.at.keyframes

; Keyframe block (from, to, percentage)
(keyframe_block
  (_) @entity.keyframe.selector
  (block) @entity.keyframe.block
) @entity.keyframe

; @scope
(scope_statement
  (block) @entity.at.scope.block
) @entity.at.scope

; Generic at rule (catch-all)
(at_rule
  (at_keyword) @entity.at.generic.keyword
  (block) @entity.at.generic.block
) @entity.at.generic
"#
}

/// Get structural query for CSS
///
/// Returns Tree-sitter query patterns for identifying structural relationships:
/// - Media contains rules
/// - Keyframes contains keyframe blocks
/// - Selector nesting
pub fn structural_query() -> &'static str {
    r#"
; ============================================
; Media Query Contains
; ============================================

; @media contains rule sets
(media_statement
  (block
    (rule_set) @entity.contains.media.rule
  )+ @entity.contains.media.rules
) @entity.contains.media.contains

; ============================================
; Keyframes Contains
; ============================================

; @keyframes contains keyframe blocks
(keyframes_statement
  (keyframes_name) @entity.contains.keyframes.name
  (keyframe_block_list
    (keyframe_block) @entity.contains.keyframes.block
  )+ @entity.contains.keyframes.blocks
) @entity.contains.keyframes.contains

; ============================================
; Supports Contains
; ============================================

; @supports contains rule sets
(supports_statement
  (block
    (rule_set) @entity.contains.supports.rule
  )+ @entity.contains.supports.rules
) @entity.contains.supports.contains

; ============================================
; Style Scope (for nested CSS)
; ============================================

; Nested rule set within parent
(rule_set
  (selectors) @entity.contains.style.parent.selector
  (block
    (rule_set) @entity.contains.style.nested.rule
  ) @entity.contains.style.parent.block
) @entity.contains.style.contains

; ============================================
; Selector Descendant
; ============================================

; Descendant selector relationship
(descendant_selector) @entity.style_selector.descendant

; Child selector relationship
(child_selector) @entity.style_selector.child

; Sibling selector relationship
(sibling_selector) @entity.style_selector.sibling

; Adjacent sibling selector relationship
(adjacent_sibling_selector) @entity.style_selector.adjacent
"#
}

/// Get comment query for CSS
///
/// Returns Tree-sitter query patterns for identifying CSS comments.
/// CSS only supports block comments (/* ... */).
pub fn comment_query() -> &'static str {
    r#"
; ============================================
; CSS Comments
; ============================================

; CSS block comment (/* ... */)
(comment) @comment.block
"#
}

/// Get dependency query for CSS
///
/// Returns Tree-sitter query patterns for identifying CSS dependencies:
/// - @import statements
/// - url() references
pub fn dependency_query() -> &'static str {
    r#"
; ============================================
; Import Dependencies
; ============================================

; @import with string path
(import_statement
  (string_value) @dependency.import.css.path
) @dependency.import.css

; @import with url()
(import_statement
  (call_expression
    (function_name) @dependency.import.css.url.func
    (#eq? @dependency.import.css.url.func "url")
    (arguments
      (string_value) @dependency.import.css.url.path
    )
  )
) @dependency.import.css.url

; ============================================
; URL References
; ============================================

; url() function call in property values
(call_expression
  (function_name) @dependency.url.func
  (#eq? @dependency.url.func "url")
  (arguments
    (string_value) @dependency.url.path
  )
) @dependency.url
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Query;

    /// Validate query syntax and return detailed error information
    fn validate_query_syntax(query_name: &str, query_str: &str) -> Result<(), String> {
        let lang = tree_sitter_css::LANGUAGE;
        match Query::new(&lang.into(), query_str) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Query '{}' syntax error: {:?}", query_name, e)),
        }
    }

    #[test]
    fn test_entity_query_syntax_valid() {
        let result = validate_query_syntax("entity_query", entity_query());
        assert!(
            result.is_ok(),
            "Entity query syntax validation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_structural_query_syntax_valid() {
        let result = validate_query_syntax("structural_query", structural_query());
        assert!(
            result.is_ok(),
            "Structural query syntax validation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_dependency_query_syntax_valid() {
        let result = validate_query_syntax("dependency_query", dependency_query());
        assert!(
            result.is_ok(),
            "Dependency query syntax validation failed: {:?}",
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
