/*
HTML Element-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Document structure
(document) @definition.document

; Elements with content
(element
  (start_tag
    (tag_name) @name.definition.tag)
  (#not-eq? @name.definition.tag "script")
  (#not-eq? @name.definition.tag "style")) @definition.element

; Script elements
(script_element
  (start_tag
    (tag_name) @name.definition.script)
  (raw_text) @definition.script.content) @definition.script

; Style elements
(style_element
  (start_tag
    (tag_name) @name.definition.style)
  (raw_text) @definition.style.content) @definition.style

; All tag names (for both start and end tags)
(tag_name) @name.definition.tag

; Start tags
(start_tag
  (tag_name) @name.definition.start_tag) @definition.start_tag

; End tags
(end_tag
  (tag_name) @name.definition.end_tag) @definition.end_tag

; Self-closing tags
(self_closing_tag
  (tag_name) @name.definition.self_closing_tag) @definition.self_closing_tag

; Doctype declarations
(doctype) @definition.doctype

; Nested elements (parent with children)
(element
  (element)+) @definition.nested_elements

; Custom elements (with hyphens or colons)
(element
  (start_tag
    (tag_name) @name.definition.custom_element)
  (#match? @name.definition.custom_element "[-:]")) @definition.custom_element

; Form elements
(element
  (start_tag
    (tag_name) @name.definition.form_element)
  (#match? @name.definition.form_element "^(form|input|select|option|textarea|button|label|fieldset|legend)$")) @definition.form_element

; Table elements
(element
  (start_tag
    (tag_name) @name.definition.table_element)
  (#match? @name.definition.table_element "^(table|thead|tbody|tfoot|tr|th|td|caption|colgroup|col)$")) @definition.table_element

; List elements
(element
  (start_tag
    (tag_name) @name.definition.list_element)
  (#match? @name.definition.list_element "^(ul|ol|li|dl|dt|dd)$")) @definition.list_element

; Semantic elements
(element
  (start_tag
    (tag_name) @name.definition.semantic_element)
  (#match? @name.definition.semantic_element "^(header|nav|main|article|section|aside|footer|figure|figcaption|details|summary)$")) @definition.semantic_element

; Anchor elements
(element
  (start_tag
    (tag_name) @name.definition.anchor_element)
  (#eq? @name.definition.anchor_element "a")) @definition.anchor_element

; Image elements
(element
  (start_tag
    (tag_name) @name.definition.image_element)
  (#eq? @name.definition.image_element "img")) @definition.image_element

; Meta elements
(element
  (start_tag
    (tag_name) @name.definition.meta_element)
  (#eq? @name.definition.meta_element "meta")) @definition.meta_element

; Link elements
(element
  (start_tag
    (tag_name) @name.definition.link_element)
  (#eq? @name.definition.link_element "link")) @definition.link_element

; Title elements
(element
  (start_tag
    (tag_name) @name.definition.title_element)
  (#eq? @name.definition.title_element "title")) @definition.title_element

; Headings
(element
  (start_tag
    (tag_name) @name.definition.heading_element)
  (#match? @name.definition.heading_element "^h[1-6]$")) @definition.heading_element

; Content sections
(element
  (start_tag
    (tag_name) @name.definition.section_element)
  (#match? @name.definition.section_element "^(div|span|p|pre|blockquote|address)$")) @definition.section_element

; Void elements (self-closing)
(element
  (start_tag
    (tag_name) @name.definition.void_element)
  (#match? @name.definition.void_element "^(area|base|br|col|embed|hr|img|input|link|meta|param|source|track|wbr|!DOCTYPE|doctype)$")) @definition.void_element
`;