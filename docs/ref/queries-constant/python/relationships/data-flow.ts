/*
Python Data Flow-specific Tree-Sitter Query Patterns
用于追踪变量间的数据传递关系
*/
export default `
; Unified assignment data flow using alternation for different assignment types
(assignment
  left: [
    (identifier) @target.variable
    (attribute
      object: (identifier) @target.object
      attribute: (identifier) @target.property)
    (subscript
      object: (identifier) @target.object
      index: (identifier) @target.index)
  ]
  right: [
    (identifier) @source.variable
    (attribute
      object: (identifier) @source.object
      attribute: (identifier) @source.property)
    (subscript
      object: (identifier) @source.object
      index: (identifier) @source.index)
    (lambda) @source.lambda
  ]) @data.flow.assignment

; Annotated and augmented assignments with anchor
[
  (annotated_assignment
    left: (identifier) @target.variable
    right: (identifier) @source.variable)
  (augmented_assignment
    left: (identifier) @target.variable
    right: (identifier) @source.variable)
] @data.flow.special.assignment

; Comprehension-based assignments with anchor for precise matching
[
  (assignment
    left: (identifier) @target.variable
    right: (list_comprehension
      (identifier) @source.variable))) @data.flow.list.comprehension
  (assignment
    left: (identifier) @target.variable
    right: (dictionary_comprehension
      (identifier) @source.variable))) @data.flow.dict.comprehension
  (assignment
    left: (identifier) @target.variable
    right: (generator_expression
      (identifier) @source.variable))) @data.flow.generator.expression
] @data.flow.comprehension.assignment

; Parameter passing data flow using alternation
(arguments: (argument_list
    (identifier) @source.parameter)) @data.flow.parameter.passing

; Return value data flow with anchor for precise matching
(return_statement
  .
  [
    (identifier) @source.variable
    (attribute
      object: (identifier) @source.object
      attribute: (identifier) @source.property)
    (subscript
      object: (identifier) @source.object
      index: (identifier) @source.index)
  ]) @data.flow.return.value

; Pattern matching assignments
(assignment
  left: [
    (pattern_list
      (identifier) @target.variable))
    (tuple_pattern
      (identifier) @target.variable))
    (list_pattern
      (identifier) @target.variable))
  ]
  right: (identifier) @source.variable) @data.flow.pattern.assignment
`;