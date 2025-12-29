/*
TSX JSX-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; JSX Component Usage - Capture all components in JSX
(jsx_element
  open_tag: (jsx_opening_element
    name: (identifier) @name.definition.jsx_component)) @definition.jsx_element

; Self-closing JSX elements
(jsx_self_closing_element
  name: (identifier) @name.definition.jsx_self_closing_component) @definition.jsx_self_closing_element

; JSX attributes
(jsx_attribute
  (property_identifier) @name.definition.jsx_attribute) @definition.jsx_attribute

; JSX text content
(jsx_text) @definition.jsx_text

; JSX expressions (inside curly braces)
(jsx_expression
  (identifier) @name.definition.jsx_expression) @definition.jsx_expression
`;