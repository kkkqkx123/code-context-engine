/*
Vue Component-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Template section
(template_element) @definition.template

; Script section
(script_element) @definition.script

; Style section
(style_element) @definition.style

; Component tags in template (capitalized tags)
(element
  (start_tag
    (tag_name) @_tag
    (#match? @_tag "^[A-Z]"))) @definition.component_tag

; Slots
(slot_element) @definition.slot

; Template expressions
(interpolation) @definition.interpolation

; Comments in Vue file
(comment) @definition.comment

; Vue component exports in script
(export_statement
  (object
    (pair
      key: (property_identifier) @_key
      (#eq? @_key "name")
      value: (string (string_fragment)) @name.definition.component_name))) @definition.component_export

; Vue component props definition
(export_statement
  (object
    (pair
      key: (property_identifier) @_key
      (#eq? @_key "props")
      value: (object))) @definition.props_definition

; Vue component methods definition
(export_statement
  (object
    (pair
      key: (property_identifier) @_key
      (#eq? @_key "methods")
      value: (object))) @definition.methods_definition

; Vue component data definition
(export_statement
  (object
    (pair
      key: (property_identifier) @_key
      (#eq? @_key "data")
      value: (function))) @definition.data_definition

; Vue component computed properties
(export_statement
  (object
    (pair
      key: (property_identifier) @_key
      (#eq? @_key "computed")
      value: (object))) @definition.computed_definition

; Vue component lifecycle hooks
(export_statement
  (object
    (pair
      key: (property_identifier) @_key
      (#match? @_key "^(created|mounted|beforeDestroy|destroyed|beforeMount|updated|beforeUpdate|activated|deactivated)$")
      value: (function))) @definition.lifecycle_hook
`;