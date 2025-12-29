/*
Vue Template and Directive-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Template elements and tags
(template_element
  (element
    (start_tag
      (tag_name) @name.definition.tag))) @definition.template_element

; Vue directives (v-*)
(attribute
  (attribute_name) @name.definition.directive
  (#match? @name.definition.directive "^v-")) @definition.vue_directive

; Event handlers (@click, @submit, etc.)
(attribute
  (attribute_name) @name.definition.event_handler
  (#match? @name.definition.event_handler "^@")) @definition.event_handler

; Property bindings (:prop, :class, etc.)
(attribute
  (attribute_name) @name.definition.property_binding
  (#match? @name.definition.property_binding "^:")) @definition.property_binding

; Slot attributes
(attribute
  (attribute_name) @_attr
  (#eq? @_attr "slot")
  (attribute_value) @name.definition.slot_name) @definition.slot_attribute

; Ref attributes
(attribute
  (attribute_name) @_attr
  (#eq? @_attr "ref")
  (attribute_value) @name.definition.ref) @definition.ref_attribute

; Key attributes for list rendering
(attribute
  (attribute_name) @_attr
  (#eq? @_attr "key")
  (attribute_value) @name.definition.key) @definition.key_attribute

; Model bindings (v-model)
(attribute
  (attribute_name) @_attr
  (#eq? @_attr "v-model")
  (attribute_value) @name.definition.v_model) @definition.v_model_binding

; Show/Hide directives
(attribute
  (attribute_name) @_attr
  (#match? @_attr "v-show|v-hide")
  (attribute_value) @name.definition.show_hide) @definition.show_hide_directive

; If/Else directives
(attribute
  (attribute_name) @_attr
  (#match? @_attr "v-if|v-else-if|v-else")
  (attribute_value) @name.definition.conditional) @definition.conditional_directive

; For directives
(attribute
  (attribute_name) @_attr
  (#match? @_attr "v-for")
  (attribute_value) @name.definition.for_loop) @definition.for_directive
`;