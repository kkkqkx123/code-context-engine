/*
TSX Component-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Function Components - Arrow functions with JSX return
(lexical_declaration
  (variable_declarator
    name: (identifier) @name.definition.function_component
    value: (arrow_function))) @definition.function_component

; Function Components - Function declarations
(function_declaration
  name: (identifier) @name.definition.function_component) @definition.function_component

; Export Statement Components
(export_statement
  (lexical_declaration
    (variable_declarator
      name: (identifier) @name.definition.exported_component
      value: (arrow_function)))) @definition.exported_component

; Class Components
(class_declaration
  name: (type_identifier) @name.definition.class_component) @definition.class_component

; HOC Components - Higher Order Components
(lexical_declaration
  (variable_declarator
    name: (identifier) @name.definition.hoc_component
    value: (call_expression
      function: (identifier)))) @definition.hoc_component

; Generic Components
(function_declaration
  name: (identifier) @name.definition.generic_component
  type_parameters: (type_parameters)) @definition.generic_component

; React.forwardRef components
(lexical_declaration
  (variable_declarator
    name: (identifier) @name.definition.forward_ref_component
    value: (call_expression
      function: (member_expression
        object: (identifier) @_react
        property: (property_identifier) @_forward_ref
        (#eq? @_react "React")
        (#eq? @_forward_ref "forwardRef"))))) @definition.forward_ref_component
`;