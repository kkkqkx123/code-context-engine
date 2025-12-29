/*
HTML Attribute and Content-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Attributes
(attribute
  (attribute_name) @name.definition.attribute) @definition.attribute

; Attribute values
(attribute
  (attribute_name)
  (quoted_attribute_value
    (attribute_value) @definition.attribute_value))

; Text content
(text) @definition.text

; Raw text content (in script/style tags)
(raw_text) @definition.raw_text

; Entities (HTML character entities like &nbsp;, &#160;, &#xA0;)
(entity) @definition.entity
`;