//! Vue SFC language query schemes
//!
//! Provides Tree-sitter query patterns for identifying and analyzing
//! Vue Single File Components.
//!
//! Tree-sitter version: 0.0.3

/// Get entity query for Vue SFC
///
/// Returns Tree-sitter query patterns for identifying Vue entities:
/// - Template elements
/// - Components (custom elements)
/// - Vue directives (v-if, v-for, v-bind, v-on, etc.)
/// - Interpolations ({{ ... }})
/// - Script and Style blocks with language attributes
pub fn entity_query() -> &'static str {
    r#"
; ============================================
; Vue SFC Root
; ============================================

; Vue Single File Component root
(component) @entity.component.root

; ============================================
; Template Block
; ============================================

; Template element
(template_element
  (start_tag) @entity.template.start
  (end_tag)? @entity.template.end
) @entity.template

; ============================================
; Standard HTML Elements
; ============================================

; Regular HTML element
(element
  (start_tag
    (tag_name) @entity.element.name
  ) @entity.element.start_tag
  (end_tag)? @entity.element.end_tag
) @entity.element

; ============================================
; Vue Components (Custom Elements in templates)
; ============================================

; Vue component self-closing (PascalCase)
(self_closing_tag
  (tag_name) @entity.component.self_closing.name
) @entity.component.self_closing

; ============================================
; Vue Directives
; ============================================

; v-bind shorthand (:prop)
(directive_attribute
  (directive_name) @entity.directive.bind.shorthand
  (#eq? @entity.directive.bind.shorthand ":")
  (directive_argument) @entity.directive.bind.arg
  (quoted_attribute_value)? @entity.directive.bind.value
) @entity.directive.bind

; v-on shorthand (@event)
(directive_attribute
  (directive_name) @entity.directive.on.shorthand
  (#eq? @entity.directive.on.shorthand "@")
  (directive_argument) @entity.directive.on.arg
  (quoted_attribute_value)? @entity.directive.on.value
) @entity.directive.on

; v-model directive
(directive_attribute
  (directive_name) @entity.directive.model.name
  (#eq? @entity.directive.model.name "v-model")
  (directive_argument)? @entity.directive.model.arg
  (quoted_attribute_value)? @entity.directive.model.value
) @entity.directive.model

; v-if directive
(directive_attribute
  (directive_name) @entity.directive.if.name
  (#eq? @entity.directive.if.name "v-if")
  (quoted_attribute_value
    (attribute_value) @entity.directive.if.value
  )? @entity.directive.if.value_full
) @entity.directive.if

; v-else-if directive
(directive_attribute
  (directive_name) @entity.directive.else_if.name
  (#eq? @entity.directive.else_if.name "v-else-if")
  (quoted_attribute_value)? @entity.directive.else_if.value
) @entity.directive.else_if

; v-else directive
(directive_attribute
  (directive_name) @entity.directive.else.name
  (#eq? @entity.directive.else.name "v-else")
) @entity.directive.else

; v-for directive
(directive_attribute
  (directive_name) @entity.directive.for.name
  (#eq? @entity.directive.for.name "v-for")
  (quoted_attribute_value) @entity.directive.for.value
) @entity.directive.for

; v-show directive
(directive_attribute
  (directive_name) @entity.directive.show.name
  (#eq? @entity.directive.show.name "v-show")
  (quoted_attribute_value) @entity.directive.show.value
) @entity.directive.show

; v-slot directive
(directive_attribute
  (directive_name) @entity.directive.slot.name
  (#eq? @entity.directive.slot.name "v-slot")
  (directive_argument)? @entity.directive.slot.arg
  (quoted_attribute_value)? @entity.directive.slot.value
) @entity.directive.slot

; v-text directive
(directive_attribute
  (directive_name) @entity.directive.text.name
  (#eq? @entity.directive.text.name "v-text")
  (quoted_attribute_value) @entity.directive.text.value
) @entity.directive.text

; v-html directive
(directive_attribute
  (directive_name) @entity.directive.html.name
  (#eq? @entity.directive.html.name "v-html")
  (quoted_attribute_value) @entity.directive.html.value
) @entity.directive.html

; v-pre directive
(directive_attribute
  (directive_name) @entity.directive.pre.name
  (#eq? @entity.directive.pre.name "v-pre")
) @entity.directive.pre

; v-cloak directive
(directive_attribute
  (directive_name) @entity.directive.cloak.name
  (#eq? @entity.directive.cloak.name "v-cloak")
) @entity.directive.cloak

; v-once directive
(directive_attribute
  (directive_name) @entity.directive.once.name
  (#eq? @entity.directive.once.name "v-once")
) @entity.directive.once

; v-memo directive
(directive_attribute
  (directive_name) @entity.directive.memo.name
  (#eq? @entity.directive.memo.name "v-memo")
  (quoted_attribute_value) @entity.directive.memo.value
) @entity.directive.memo

; Generic directive (catch-all for v-* directives)
(directive_attribute
  (directive_name) @entity.directive.generic.name
  (directive_argument)? @entity.directive.generic.arg
  (quoted_attribute_value)? @entity.directive.generic.value
) @entity.directive.generic

; ============================================
; Interpolations
; ============================================

; Mustache interpolation {{ expression }}
(interpolation
  (raw_text) @entity.interpolation.content
) @entity.interpolation

; ============================================
; Script Block
; ============================================

; Script element
(script_element
  (start_tag) @entity.script.start_tag
  (raw_text)? @entity.script.content
  (end_tag)? @entity.script.end_tag
) @entity.script

; Script language attribute
(start_tag
  (attribute
    (attribute_name) @entity.script.lang.attr
    (#eq? @entity.script.lang.attr "lang")
    (quoted_attribute_value
      (attribute_value) @entity.script.lang.value
    )? @entity.script.lang.value_full
  ) @entity.script.lang
)

; Script setup attribute
(start_tag
  (attribute
    (attribute_name) @entity.script.setup.attr
    (#eq? @entity.script.setup.attr "setup")
  ) @entity.script.setup
)

; ============================================
; Style Block
; ============================================

; Style element
(style_element
  (start_tag) @entity.style.start_tag
  (raw_text)? @entity.style.content
  (end_tag)? @entity.style.end_tag
) @entity.style

; Style scoped attribute
(start_tag
  (attribute
    (attribute_name) @entity.style.scoped.attr
    (#eq? @entity.style.scoped.attr "scoped")
  ) @entity.style.scoped
)

; Style module attribute
(start_tag
  (attribute
    (attribute_name) @entity.style.module.attr
    (#eq? @entity.style.module.attr "module")
  ) @entity.style.module
)

; Style language attribute
(start_tag
  (attribute
    (attribute_name) @entity.style.lang.attr
    (#eq? @entity.style.lang.attr "lang")
    (quoted_attribute_value
      (attribute_value) @entity.style.lang.value
    )? @entity.style.lang.value_full
  ) @entity.style.lang
)

; ============================================
; Standard Attributes
; ============================================

; Regular attribute
(attribute
  (attribute_name) @entity.attribute.name
  (quoted_attribute_value
    (attribute_value) @entity.attribute.value
  )? @entity.attribute.value_full
) @entity.attribute

; ref attribute
(attribute
  (attribute_name) @entity.attribute.ref.name
  (#eq? @entity.attribute.ref.name "ref")
  (quoted_attribute_value
    (attribute_value) @entity.attribute.ref.value
  )? @entity.attribute.ref.value_full
) @entity.attribute.ref

; key attribute
(attribute
  (attribute_name) @entity.attribute.key.name
  (#eq? @entity.attribute.key.name "key")
  (quoted_attribute_value
    (attribute_value) @entity.attribute.key.value
  ) @entity.attribute.key.value_full
) @entity.attribute.key

; class attribute
(attribute
  (attribute_name) @entity.attribute.class.name
  (#eq? @entity.attribute.class.name "class")
  (quoted_attribute_value
    (attribute_value) @entity.attribute.class.value
  ) @entity.attribute.class.value_full
) @entity.attribute.class

; style attribute
(attribute
  (attribute_name) @entity.attribute.style.name
  (#eq? @entity.attribute.style.name "style")
  (quoted_attribute_value
    (attribute_value) @entity.attribute.style.value
  ) @entity.attribute.style.value_full
) @entity.attribute.style

; ============================================
; Slots
; ============================================

; Named slot element (<slot name="...">)
(element
  (start_tag
    (tag_name) @entity.slot.tag
    (#eq? @entity.slot.tag "slot")
  ) @entity.slot.start_tag
  (end_tag)? @entity.slot.end_tag
) @entity.slot

; Slot name attribute
(start_tag
  (attribute
    (attribute_name) @entity.slot.name.attr
    (#eq? @entity.slot.name.attr "name")
    (quoted_attribute_value
      (attribute_value) @entity.slot.name.value
    ) @entity.slot.name.value_full
  ) @entity.slot.name
)

; Slot content (element with slot attribute)
(element
  (start_tag
    (attribute
      (attribute_name) @entity.slot_content.attr
      (#eq? @entity.slot_content.attr "slot")
      (quoted_attribute_value
        (attribute_value) @entity.slot_content.value
      ) @entity.slot_content.value_full
    ) @entity.slot_content.attribute
  )
) @entity.slot_content

; ============================================
; Text Content
; ============================================

; Text nodes
(text) @entity.text

; Raw text (in script/style)
(raw_text) @entity.raw_text
"#
}

/// Get structural query for Vue
///
/// Returns Tree-sitter query patterns for identifying structural relationships:
/// - Component usage hierarchy
/// - Element contains
/// - Prop bindings
/// - Event bindings
/// - Slot usage
pub fn structural_query() -> &'static str {
    r#"
; ============================================
; Component Usage
; ============================================

; Self-closing component usage (PascalCase tag name)
(self_closing_tag
  (tag_name) @call.constructor.component.name
) @call.constructor.component

; ============================================
; Element Contains
; ============================================

; Parent element contains child elements
(element
  (start_tag (tag_name) @entity.contains.element.parent.name)
  (element
    (start_tag (tag_name) @entity.contains.element.child.name)
  )+ @entity.contains.element.children
) @entity.contains.element

; Element contains self-closing tag (component)
(element
  (start_tag (tag_name) @entity.contains.element.parent.name)
  (element
    (self_closing_tag
      (tag_name) @entity.contains.element.child.component
    )
  )+ @entity.contains.element.child.components
) @entity.contains.element.component

; ============================================
; Prop Bindings
; ============================================

; v-bind shorthand with argument (:prop)
(directive_attribute
  (directive_name) @entity.variable.parameter.prop.shorthand
  (#eq? @entity.variable.parameter.prop.shorthand ":")
  (directive_argument) @entity.variable.parameter.prop.name
  (quoted_attribute_value) @entity.variable.parameter.prop.value
) @entity.variable.parameter.prop

; Shorthand bind without value (boolean prop)
(directive_attribute
  (directive_name) @entity.variable.parameter.bool.shorthand
  (#eq? @entity.variable.parameter.bool.shorthand ":")
  (directive_argument) @entity.variable.parameter.bool.name
) @entity.variable.parameter.bool

; ============================================
; Event Bindings
; ============================================

; v-on shorthand with argument (@event)
(directive_attribute
  (directive_name) @call.callback.event.shorthand
  (#eq? @call.callback.event.shorthand "@")
  (directive_argument) @call.callback.event.name
  (quoted_attribute_value) @call.callback.event.handler
) @call.callback.event

; ============================================
; Template References
; ============================================

; ref attribute binding
(attribute
  (attribute_name) @entity.template.ref.attr
  (#eq? @entity.template.ref.attr "ref")
  (quoted_attribute_value
    (attribute_value) @entity.template.ref.value
  ) @entity.template.ref.value_full
) @entity.template.ref

; ============================================
; Class Bindings
; ============================================

; Static class attribute
(attribute
  (attribute_name) @entity.attribute.class.static.name
  (#eq? @entity.attribute.class.static.name "class")
  (quoted_attribute_value
    (attribute_value) @entity.attribute.class.static.value
  ) @entity.attribute.class.static.value_full
) @entity.attribute.class.static

; Dynamic class binding (:class)
(directive_attribute
  (directive_name) @entity.attribute.class.dynamic.shorthand
  (#eq? @entity.attribute.class.dynamic.shorthand ":")
  (directive_argument) @entity.attribute.class.dynamic.arg
  (#eq? @entity.attribute.class.dynamic.arg "class")
  (quoted_attribute_value) @entity.attribute.class.dynamic.value
) @entity.attribute.class.dynamic

; ============================================
; Style Bindings
; ============================================

; Static style attribute
(attribute
  (attribute_name) @entity.attribute.style.static.name
  (#eq? @entity.attribute.style.static.name "style")
  (quoted_attribute_value
    (attribute_value) @entity.attribute.style.static.value
  ) @entity.attribute.style.static.value_full
) @entity.attribute.style.static

; Dynamic style binding (:style)
(directive_attribute
  (directive_name) @entity.attribute.style.dynamic.shorthand
  (#eq? @entity.attribute.style.dynamic.shorthand ":")
  (directive_argument) @entity.attribute.style.dynamic.arg
  (#eq? @entity.attribute.style.dynamic.arg "style")
  (quoted_attribute_value) @entity.attribute.style.dynamic.value
) @entity.attribute.style.dynamic

; ============================================
; Slot Usage
; ============================================

; Named slot usage
(element
  (start_tag
    (attribute
      (attribute_name) @entity.attribute.slot.name
      (#eq? @entity.attribute.slot.name "slot")
      (quoted_attribute_value
        (attribute_value) @entity.attribute.slot.value
      ) @entity.attribute.slot.value_full
    ) @entity.attribute.slot
  )
) @entity.variable.parameter.slot

; v-slot directive
(directive_attribute
  (directive_name) @entity.directive.slot.name
  (#eq? @entity.directive.slot.name "v-slot")
  (directive_argument)? @entity.directive.slot.arg
) @entity.directive.slot

; ============================================
; Style Scope
; ============================================

; Scoped style block
(style_element
  (start_tag
    (attribute
      (attribute_name) @entity.attribute.style.scope.name
      (#eq? @entity.attribute.style.scope.name "scoped")
    )
  )
  (raw_text) @entity.style.scope.content
) @entity.style.scope
"#
}

/// Get comment query for Vue
///
/// Returns Tree-sitter query patterns for identifying Vue/HTML comments.
/// Vue uses HTML-style comments (<!-- -->) in template sections.
pub fn comment_query() -> &'static str {
    r#"
; ============================================
; Vue/HTML Comments
; ============================================

; HTML block comment in Vue template (<!-- -->)
(comment) @comment.block
"#
}

/// Get dependency query for Vue
///
/// Returns Tree-sitter query patterns for identifying Vue dependencies:
/// - Component imports (from script section)
/// - Module dependencies
pub fn dependency_query() -> &'static str {
    r#"
; ============================================
; Script Block Dependencies
; ============================================

; Note: Actual JavaScript/TypeScript imports in the script block
; are handled by the JavaScript/TypeScript query schemes.
; This section captures Vue-specific dependencies.

; ============================================
; Component Registration
; ============================================

; Self-closing component usage implies dependency
(self_closing_tag
  (tag_name) @dependency.import.component.name
) @dependency.import.component

; ============================================
; Script Content
; ============================================

; Script element content (for embedded parsing)
(script_element
  (raw_text) @entity.script.content
) @entity.script
"#
}

/// Get embedded block query for Vue
///
/// Returns Tree-sitter query patterns for extracting embedded code blocks:
/// - Script blocks with attributes (lang, setup)
/// - Style blocks with attributes (lang, scoped, module)
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
        let lang = tree_sitter_vue::language();
        match Query::new(&lang, query_str) {
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
