/*
JavaScript Property-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Public field definitions - important for class structure
(public_field_definition
  (property_identifier) @name.definition.property) @definition.property

; Private field definitions
(private_field_definition
  (private_property_identifier) @name.definition.private_property) @definition.private_property

; Static field definitions
(public_field_definition
  "static"
  (property_identifier) @name.definition.static_property) @definition.static_property

; Property definitions in object literals
(pair
  key: (property_identifier) @name.definition.object_property
  value: (_) @definition.object_property) @definition.object_property

; Computed property names
(pair
  key: (computed_property_name) @name.definition.computed_property
  value: (_) @definition.computed_property) @definition.computed_property

; Method shorthand in object literals
(method_definition
  name: (property_identifier) @name.definition.object_method) @definition.object_method

; Getter in object literals
(method_definition
  name: (property_identifier) @name.definition.object_getter
  (#match? @name.definition.object_getter "^get")) @definition.object_getter

; Setter in object literals
(method_definition
  name: (property_identifier) @name.definition.object_setter
  (#match? @name.definition.object_setter "^set")) @definition.object_setter

; Class property with type annotation (TypeScript style but may appear in JS with JSDoc)
(public_field_definition
  (property_identifier) @name.definition.typed_property
  (type_annotation)) @definition.typed_property
`;