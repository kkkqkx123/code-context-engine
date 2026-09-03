//! HTML language query schemes
//!
//! Provides Tree-sitter query patterns for identifying and analyzing
//! HTML code entities, elements, and embedded content.
//!
//! Tree-sitter version: 0.23.2

/// Get entity query for HTML
///
/// Returns Tree-sitter query patterns for identifying HTML entities:
/// - Elements (tags)
/// - Attributes
/// - Script elements
/// - Style elements
/// - Form elements (input, select, textarea, button, form)
/// - Link elements (a, link)
pub fn entity_query() -> &'static str {
    r#"
; ============================================
; HTML Elements
; ============================================

; Generic element with start and end tag
(element
  (start_tag
    (tag_name) @entity.element.name
  ) @entity.element.start_tag
  (end_tag)? @entity.element.end_tag
) @entity.element

; Self-closing element (void elements)
(self_closing_tag
  (tag_name) @entity.element.void.name
) @entity.element.void

; ============================================
; Special Elements
; ============================================

; Script element
(script_element
  (start_tag
    (tag_name) @entity.script.tag_name
    (attribute
      (attribute_name) @entity.script.attribute.name
      (attribute_value)? @entity.script.attribute.value
    )* @entity.script.attributes
  ) @entity.script.start_tag
  (raw_text)? @entity.script.content
  (end_tag)? @entity.script.end_tag
) @entity.script

; Style element
(style_element
  (start_tag
    (tag_name) @entity.style.tag_name
    (attribute
      (attribute_name) @entity.style.attribute.name
      (attribute_value)? @entity.style.attribute.value
    )* @entity.style.attributes
  ) @entity.style.start_tag
  (raw_text)? @entity.style.content
  (end_tag)? @entity.style.end_tag
) @entity.style

; ============================================
; Form Elements
; ============================================

; Form element
(element
  (start_tag
    (tag_name) @entity.form.tag_name
    (#eq? @entity.form.tag_name "form")
    (attribute
      (attribute_name) @entity.form.attribute.name
      (quoted_attribute_value
        (attribute_value) @entity.form.attribute.value
      )?
    )*
  ) @entity.form.start_tag
) @entity.form

; Input element
(self_closing_tag
  (tag_name) @entity.input.tag_name
  (#eq? @entity.input.tag_name "input")
  (attribute
    (attribute_name) @entity.input.attribute.name
    (quoted_attribute_value
      (attribute_value) @entity.input.attribute.value
    )?
  )*
) @entity.input

; Select element
(element
  (start_tag
    (tag_name) @entity.select.tag_name
    (#eq? @entity.select.tag_name "select")
    (attribute
      (attribute_name) @entity.select.attribute.name
      (quoted_attribute_value
        (attribute_value) @entity.select.attribute.value
      )?
    )*
  ) @entity.select.start_tag
) @entity.select

; Textarea element
(element
  (start_tag
    (tag_name) @entity.textarea.tag_name
    (#eq? @entity.textarea.tag_name "textarea")
    (attribute
      (attribute_name) @entity.textarea.attribute.name
      (quoted_attribute_value
        (attribute_value) @entity.textarea.attribute.value
      )?
    )*
  ) @entity.textarea.start_tag
) @entity.textarea

; Button element
(element
  (start_tag
    (tag_name) @entity.button.tag_name
    (#eq? @entity.button.tag_name "button")
    (attribute
      (attribute_name) @entity.button.attribute.name
      (quoted_attribute_value
        (attribute_value) @entity.button.attribute.value
      )?
    )*
  ) @entity.button.start_tag
) @entity.button

; ============================================
; Link Elements
; ============================================

; Anchor (a) element
(element
  (start_tag
    (tag_name) @entity.anchor.tag_name
    (#eq? @entity.anchor.tag_name "a")
    (attribute
      (attribute_name) @entity.anchor.attribute.name
      (quoted_attribute_value
        (attribute_value) @entity.anchor.attribute.value
      )?
    )*
  ) @entity.anchor.start_tag
) @entity.anchor

; ============================================
; Attributes
; ============================================

; Standard attribute
(attribute
  (attribute_name) @entity.attribute.name
  (attribute_value)? @entity.attribute.value
) @entity.attribute

; Attribute with quoted value
(attribute
  (attribute_name) @entity.attribute.quoted.name
  (quoted_attribute_value) @entity.attribute.quoted.value
) @entity.attribute.quoted

; ============================================
; Document Structure
; ============================================

; Document type declaration
(doctype) @entity.doctype

; ============================================
; Content
; ============================================

; Text content
(text) @entity.text

; Raw text (inside script/style)
(raw_text) @entity.raw_text
"#
}

/// Get comment query for HTML
///
/// Returns Tree-sitter query patterns for identifying HTML comments.
/// HTML comments use <!-- --> syntax.
pub fn comment_query() -> &'static str {
    r#"
; ============================================
; HTML Comments
; ============================================

; HTML block comment (<!-- -->)
(comment) @comment.block
"#
}

/// Get dependency query for HTML
///
/// Returns Tree-sitter query patterns for identifying HTML dependencies:
/// - Script src attributes
/// - Link href attributes
/// - Import maps
pub fn dependency_query() -> &'static str {
    r#"
; ============================================
; Script Dependencies
; ============================================

; External script (src attribute)
(script_element
  (start_tag
    (attribute
      (attribute_name) @dependency.script.src.attr_name
      (#eq? @dependency.script.src.attr_name "src")
      (quoted_attribute_value) @dependency.script.src.value
    )
  )
) @dependency.script.external

; Module script (type="module")
(script_element
  (start_tag
    (attribute
      (attribute_name) @dependency.script.module.attr_name
      (#eq? @dependency.script.module.attr_name "type")
      (quoted_attribute_value) @dependency.script.module.value
      (#match? @dependency.script.module.value "module")
    )
  )
) @dependency.script.module

; ============================================
; Link Dependencies
; ============================================

; Stylesheet link
(element
  (self_closing_tag
    (tag_name) @dependency.link.tag
    (#eq? @dependency.link.tag "link")
    (attribute
      (attribute_name) @dependency.link.rel.name
      (#eq? @dependency.link.rel.name "rel")
      (quoted_attribute_value) @dependency.link.rel.value
      (#match? @dependency.link.rel.value "stylesheet")
    )
    (attribute
      (attribute_name) @dependency.link.href.name
      (#eq? @dependency.link.href.name "href")
      (quoted_attribute_value) @dependency.link.href.value
    )
  )
) @dependency.link.stylesheet

; Icon/preconnect/preload links
(element
  (self_closing_tag
    (tag_name) @dependency.link.resource.tag
    (#eq? @dependency.link.resource.tag "link")
    (attribute
      (attribute_name) @dependency.link.resource.href.name
      (#eq? @dependency.link.resource.href.name "href")
      (quoted_attribute_value) @dependency.link.resource.href.value
    )
  )
) @dependency.link.resource
"#
}

/// Get embedded block query for HTML
///
/// Returns Tree-sitter query patterns for extracting embedded code blocks:
/// - Script blocks with attributes (type, src, etc.)
/// - Style blocks with attributes (type, media, etc.)
pub fn embedded_block_query() -> &'static str {
    r#"
; ============================================
; Embedded Script Block
; ============================================

(script_element
  (start_tag
    (tag_name) @embedded.script.tag_name
    (attribute
      (attribute_name) @embedded.script.attr.name
      (quoted_attribute_value
        (attribute_value) @embedded.script.attr.value
      )? @embedded.script.attr.value_full
    )* @embedded.script.attributes
  ) @embedded.script.start_tag
  (raw_text)? @embedded.script.content
  (end_tag)? @embedded.script.end_tag
) @embedded.script

; ============================================
; Embedded Style Block
; ============================================

(style_element
  (start_tag
    (tag_name) @embedded.style.tag_name
    (attribute
      (attribute_name) @embedded.style.attr.name
      (quoted_attribute_value
        (attribute_value) @embedded.style.attr.value
      )? @embedded.style.attr.value_full
    )* @embedded.style.attributes
  ) @embedded.style.start_tag
  (raw_text)? @embedded.style.content
  (end_tag)? @embedded.style.end_tag
) @embedded.style
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Query;

    /// Validate query syntax and return detailed error information
    fn validate_query_syntax(query_name: &str, query_str: &str) -> Result<(), String> {
        let lang = tree_sitter_html::LANGUAGE;
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

    #[test]
    fn test_embedded_block_query_syntax_valid() {
        let result = validate_query_syntax("embedded_block_query", embedded_block_query());
        assert!(
            result.is_ok(),
            "Embedded block query syntax validation failed: {:?}",
            result.err()
        );
    }
}
