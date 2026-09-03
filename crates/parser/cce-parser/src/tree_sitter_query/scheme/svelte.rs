//! Svelte language query schemes
//!
//! Provides Tree-sitter query patterns for identifying and analyzing
//! Svelte components.
//!
//! Tree-sitter version: 0.10.2

/// Get entity query for Svelte
///
/// Returns Tree-sitter query patterns for identifying Svelte entities:
/// - Script and Style blocks
/// - Template elements
/// - Components (custom elements)
/// - Control flow blocks ({#if}, {#each}, {#await}, etc.)
/// - Reactive declarations ($:)
/// - Event handlers (on:click)
/// - Bindings (bind:value)
/// - Transitions/Animations (transition:, animate:)
pub fn entity_query() -> &'static str {
    r#"
; ============================================
; Document Root
; ============================================

; Document root
(document) @entity.document

; ============================================
; Script Element
; ============================================

; Script element with context (module or default)
(script_element
  (start_tag
    (attribute
      (attribute_name) @entity.script.context.attr
      (attribute_value)? @entity.script.context.value
    )? @entity.script.context
  ) @entity.script.start_tag
  (raw_text) @entity.script.content
  (end_tag) @entity.script.end_tag
) @entity.script

; ============================================
; Style Element
; ============================================

; Style element
(style_element
  (start_tag) @entity.style.start_tag
  (raw_text) @entity.style.content
  (end_tag) @entity.style.end_tag
) @entity.style

; ============================================
; HTML Elements
; ============================================

; Regular HTML element
(element
  (start_tag
    (tag_name) @entity.element.name
  ) @entity.element.start_tag
  (end_tag)? @entity.element.end_tag
) @entity.element

; Self-closing element
(self_closing_tag
  (tag_name) @entity.element.void.name
) @entity.element.void

; Start tag
(start_tag
  (tag_name) @entity.tag.start.name
) @entity.tag.start

; End tag
(end_tag
  (tag_name) @entity.tag.end.name
) @entity.tag.end

; ============================================
; Svelte Components (PascalCase)
; ============================================

; Component element (capitalized tag name)
(element
  (start_tag
    (tag_name) @entity.component.name
  ) @entity.component.start_tag
  (end_tag)? @entity.component.end_tag
) @entity.component

; Self-closing component
(self_closing_tag
  (tag_name) @entity.component.self_closing.name
) @entity.component.self_closing

; ============================================
; Control Flow Blocks
; ============================================

; {#if ...} block
(if_statement
  (if_start_expr) @entity.if.start
  (else_if_statement)* @entity.if.else_if
  (else_statement
    (else_expr) @entity.else.start
    (if_end_expr) @entity.if.end
  )? @entity.else
) @entity.if

; {:else if ...} block inside else_if_statement
(else_if_statement
  (else_if_expr) @entity.else_if.start
) @entity.else_if

; {#each ...} block
(each_statement
  (each_start_expr) @entity.each.start
  (each_end_expr) @entity.each.end
) @entity.each

; {:else} inside each (no items)
(else_each_statement
  (else_expr) @entity.each.else.start
  (each_end_expr) @entity.each.else.end
) @entity.each.else

; {#await ...} block
(await_statement
  (await_start_expr) @entity.await.start
  (then_statement)* @entity.await.then
  (catch_statement
    (catch_expr) @entity.catch.start
    (await_end_expr) @entity.await.end
  )? @entity.catch
) @entity.await

; {:then} block
(then_statement
  (then_expr) @entity.then.start
) @entity.then

; {:catch} block
(catch_statement
  (catch_expr) @entity.catch.start
) @entity.catch

; {#key ...} block
(key_statement
  (key_start_expr) @entity.key.start
  (key_end_expr) @entity.key.end
) @entity.key

; ============================================
; Expressions and Reactivity
; ============================================


; Reactive declaration ($: ...)
; Note: This is parsed as part of script content when extracted

; ============================================
; Attributes
; ============================================

; Standard attribute
(attribute
  (attribute_name) @entity.attribute.name
  (attribute_value)? @entity.attribute.value
) @entity.attribute

; Quoted attribute value
(quoted_attribute_value) @entity.attribute.quoted_value

; Expression attribute value (unquoted expression)
(expr_attribute_value
  (expression) @entity.attribute.expr_value
) @entity.attribute.expr

; ============================================
; Event Handlers (on:event)
; ============================================

; Event handler attribute (on:click, on:submit, etc.)
(attribute
  (attribute_name) @entity.event.handler.name
  (#match? @entity.event.handler.name "^on:")
  (attribute_value)? @entity.event.handler.value
) @entity.event.handler

; Event modifier attribute (on:click|preventDefault)
(attribute
  (attribute_name) @entity.event.modifier.name
  (#match? @entity.event.modifier.name "^on:")
) @entity.event.modifier

; ============================================
; Bindings (bind:property)
; ============================================

; Binding attribute (bind:value, bind:this, etc.)
(attribute
  (attribute_name) @entity.binding.name
  (#match? @entity.binding.name "^bind:")
  (attribute_value)? @entity.binding.value
) @entity.binding

; ============================================
; Transitions and Animations
; ============================================

; Transition attribute (transition:fade, etc.)
(attribute
  (attribute_name) @entity.transition.name
  (#match? @entity.transition.name "^transition:")
  (attribute_value)? @entity.transition.value
) @entity.transition

; In transition (in:fly, etc.)
(attribute
  (attribute_name) @entity.transition.in.name
  (#match? @entity.transition.in.name "^in:")
  (attribute_value)? @entity.transition.in.value
) @entity.transition.in

; Out transition (out:fade, etc.)
(attribute
  (attribute_name) @entity.transition.out.name
  (#match? @entity.transition.out.name "^out:")
  (attribute_value)? @entity.transition.out.value
) @entity.transition.out

; Animation attribute (animate:flip, etc.)
(attribute
  (attribute_name) @entity.animation.name
  (#match? @entity.animation.name "^animate:")
  (attribute_value)? @entity.animation.value
) @entity.animation

; ============================================
; Special Attributes
; ============================================

; Class directive (class:active={condition})
(attribute
  (attribute_name) @entity.class_directive.name
  (#match? @entity.class_directive.name "^class:")
  (attribute_value)? @entity.class_directive.value
) @entity.class_directive

; Style directive (style:color={value})
(attribute
  (attribute_name) @entity.style_directive.name
  (#match? @entity.style_directive.name "^style:")
  (attribute_value)? @entity.style_directive.value
) @entity.style_directive

; Use directive (use:action)
(attribute
  (attribute_name) @entity.use_directive.name
  (#match? @entity.use_directive.name "^use:")
  (attribute_value)? @entity.use_directive.value
) @entity.use_directive

; ============================================
; Content
; ============================================

; Text node
(text) @entity.text

; Raw text (inside script/style)
(raw_text) @entity.raw_text

; Raw text expression (inside blocks)
(raw_text_expr) @entity.raw_text_expr

; Raw text await (inside await block)
(raw_text_await) @entity.raw_text_await

; Raw text each (inside each block)
(raw_text_each) @entity.raw_text_each
"#
}

/// Get structural query for Svelte
///
/// Returns Tree-sitter query patterns for identifying structural relationships:
/// - Element contains
/// - Component usage
/// - Control flow nesting
/// - Event bindings
/// - Bindings
pub fn structural_query() -> &'static str {
    r#"
; ============================================
; Component Usage
; ============================================

; Component usage in template
(element
  (start_tag
    (tag_name) @call.constructor.component.name
  )
) @call.constructor.component

; Self-closing component usage
(self_closing_tag
  (tag_name) @call.constructor.component.self_closing.name
) @call.constructor.component.self_closing

; ============================================
; Element Contains
; ============================================

; Parent element contains child elements
(element
  (start_tag (tag_name) @entity.contains.element.parent.name)
  (element
    (start_tag (tag_name) @entity.contains.element.child.name)
  )+ @entity.contains.element.children
) @entity.contains.element.contains

; ============================================
; Event Bindings
; ============================================

; Event handler binding
(attribute
  (attribute_name) @call.callback.event.name
  (#match? @call.callback.event.name "^on:")
  (attribute_value)? @call.callback.event.handler
) @call.callback.event

; Event with modifier
(attribute
  (attribute_name) @call.callback.event.modifier.full_name
  (#match? @call.callback.event.modifier.full_name "^on:[^|]+\\|")
) @call.callback.event.modifier.with

; ============================================
; Two-way Bindings
; ============================================

; Property binding (bind:value)
(attribute
  (attribute_name) @entity.variable.parameter.bind.name
  (#match? @entity.variable.parameter.bind.name "^bind:[^=]+")
  (attribute_value)? @entity.variable.parameter.bind.value
) @entity.variable.parameter.bind

; Element binding (bind:this) - a template reference.
; The bound value is an expression (`{el}`), which the Svelte grammar
; parses as `expr_attribute_value`; the inner `raw_text_expr` holds the
; bare identifier without the braces.
(attribute
  (attribute_name) @entity.template_reference.name
  (#eq? @entity.template_reference.name "bind:this")
  (expr_attribute_value (expression (raw_text_expr) @entity.template_reference.value))
) @entity.template_reference

; ============================================
; Class Bindings
; ============================================

; Class directive (class:active={condition})
(attribute
  (attribute_name) @entity.attribute.class.directive.name
  (#match? @entity.attribute.class.directive.name "^class:")
  (attribute_value) @entity.attribute.class.directive.value
) @entity.attribute.class.directive

; ============================================
; Style Bindings
; ============================================

; Style directive (style:color={value})
(attribute
  (attribute_name) @entity.attribute.style.directive.name
  (#match? @entity.attribute.style.directive.name "^style:")
  (attribute_value) @entity.attribute.style.directive.value
) @entity.attribute.style.directive

; ============================================
; Transition/Animation Bindings
; ============================================

; Transition binding
(attribute
  (attribute_name) @entity.attribute.transition.name
  (#match? @entity.attribute.transition.name "^transition:")
  (attribute_value)? @entity.attribute.transition.value
) @entity.attribute.transition

; Animation binding
(attribute
  (attribute_name) @entity.attribute.animation.name
  (#match? @entity.attribute.animation.name "^animate:")
  (attribute_value)? @entity.attribute.animation.value
) @entity.attribute.animation

; ============================================
; Control Flow Structure
; ============================================

; If statement contains content
(if_statement
  (if_start_expr) @entity.control.flow.if.start
  (expression) @entity.control.flow.if.condition
  (raw_text_expr)? @entity.control.flow.if.content
  (else_if_statement)* @entity.control.flow.if.else_if
  (else_statement)? @entity.control.flow.if.else
) @entity.control.flow.if.block

; Each statement contains content
(each_statement
  (each_start_expr) @entity.control.flow.each.start
  (expression) @entity.control.flow.each.collection
  (raw_text_each)? @entity.control.flow.each.content
  (else_each_statement)? @entity.control.flow.each.empty
) @entity.control.flow.each.block

; Await statement contains content
(await_statement
  (await_start_expr) @entity.control.flow.await.start
  (expression) @entity.control.flow.await.promise
  (raw_text_await)? @entity.control.flow.await.pending
  (then_statement)? @entity.control.flow.await.resolved
  (catch_statement)? @entity.control.flow.await.rejected
) @entity.control.flow.await.block

; ============================================
; Style Scope (Svelte handles scoping automatically)
; ============================================

; Style block (implicitly scoped for components)
(style_element
  (raw_text) @entity.style.scope.content
) @entity.style.scope
"#
}

/// Get comment query for Svelte
///
/// Returns Tree-sitter query patterns for identifying Svelte/HTML comments.
/// Svelte uses HTML-style comments (<!-- -->) in template sections.
pub fn comment_query() -> &'static str {
    r#"
; ============================================
; Svelte/HTML Comments
; ============================================

; HTML block comment in Svelte template (<!-- -->)
(comment) @comment.block
"#
}

/// Get dependency query for Svelte
///
/// Returns Tree-sitter query patterns for identifying Svelte dependencies:
/// - Component imports
/// - Module dependencies
pub fn dependency_query() -> &'static str {
    r#"
; ============================================
; Component Dependencies
; ============================================

; Component usage implies dependency on that component
(element
  (start_tag
    (tag_name) @dependency.import.component.name
  )
) @dependency.import.component

(self_closing_tag
  (tag_name) @dependency.import.component.self_closing.name
) @dependency.import.component.self_closing

; ============================================
; Action Dependencies (use:action)
; ============================================

; use:action directive implies dependency
(attribute
  (attribute_name) @dependency.import.action.name
  (#match? @dependency.import.action.name "^use:")
) @dependency.import.action

; ============================================
; Transition/Animation Dependencies
; ============================================

; transition: implies dependency on transition function
(attribute
  (attribute_name) @dependency.import.transition.name
  (#match? @dependency.import.transition.name "^transition:")
) @dependency.import.transition

; animate: implies dependency on animation function
(attribute
  (attribute_name) @dependency.import.animation.name
  (#match? @dependency.import.animation.name "^animate:")
) @dependency.import.animation
"#
}

/// Get behavior query for Svelte
///
/// Returns Tree-sitter query patterns for capturing Svelte template behavior:
/// - Expression references
/// - Raw HTML rendering
pub fn behavior_query() -> String {
    let mut query = String::from(
        r#"
(expression) @behavior.data.reference
(html_expr) @behavior.data.reference
(const_expr) @behavior.data.bind
"#,
    );
    query.push_str(&super::common::bitwise_shift_operator_query(
        "binary_expression",
    ));
    query
}

/// Get embedded block query for Svelte
///
/// Returns Tree-sitter query patterns for extracting embedded code blocks:
/// - Script blocks with context attribute (module/default)
/// - Style blocks with attributes
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
      (attribute_value)? @embedded.script.attr.value
    )* @embedded.script.attributes
  ) @embedded.script.start_tag
  (raw_text) @embedded.script.content
  (end_tag) @embedded.script.end_tag
) @embedded.script

; ============================================
; Embedded Style Block
; ============================================

(style_element
  (start_tag
    (tag_name) @embedded.style.tag_name
    (attribute
      (attribute_name) @embedded.style.attr.name
      (attribute_value)? @embedded.style.attr.value
    )* @embedded.style.attributes
  ) @embedded.style.start_tag
  (raw_text) @embedded.style.content
  (end_tag) @embedded.style.end_tag
) @embedded.style
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Query;

    /// Validate query syntax and return detailed error information
    fn validate_query_syntax(query_name: &str, query_str: &str) -> Result<(), String> {
        let lang = tree_sitter_svelte::language();
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
