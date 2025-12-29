/*
Python Variable-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Unified variable assignments using alternation for different assignment types
(assignment
  left: [
    (identifier) @variable.name
    (attribute
      attribute: (identifier) @attribute.name)
    (subscript
      (identifier) @subscript.name)
  ]
  right: (expression) @variable.value
  type: (type)? @variable.type) @definition.variable

; Special assignment types with anchor for precise matching
[
  (augmented_assignment
    left: (identifier) @variable.name
    right: (expression) @variable.value)
  (named_expression
    name: (identifier) @named.expression.name
    value: (expression) @named.expression.value)
] @definition.special.assignment

; Pattern matching assignments using alternation
[
  (assignment
    left: (pattern_list
      (identifier) @pattern.variable.name))
  (assignment
    left: (tuple_pattern
      (identifier) @tuple.variable.name))
  (assignment
    left: (list_pattern
      (identifier) @list.variable.name))
] @definition.pattern.assignment

; Constants detection with predicate filtering
(assignment
  left: (identifier) @constant.name
  (#match? @constant.name "^[A-Z][A-Z0-9_]*$")) @definition.constant

; Typed variables with anchor for precise matching
(assignment
  left: (identifier) @typed.variable.name
  type: (type) @variable.type) @definition.typed.variable
`;