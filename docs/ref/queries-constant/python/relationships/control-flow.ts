/*
Python Control Flow and Expression-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Control flow statements using alternation to reduce redundancy
[
  (if_statement
    condition: (_) @if.condition
    consequence: (_) @if.body
    alternative: (_) @if.else)
  (for_statement
    left: (_) @for.target
    right: (_) @for.iterable
    body: (_) @for.body)
  (while_statement
    condition: (_) @while.condition
    body: (_) @while.body)
] @definition.control_statement

; Loop control statements
[
  (break_statement) @loop.break
  (continue_statement) @loop.continue
] @definition.loop.control

; Return, raise, and assert statements with context
[
  (return_statement
    (expression)? @return.expression) @definition.return
  (raise_statement
    (expression)? @raise.exception) @definition.raise
  (assert_statement
    condition: (_) @assert.condition
    message: (_) @assert.message) @definition.assert
] @definition.flow.control

; Expression statements with anchor for precise matching
(expression_statement
  (expression) @expression.content) @definition.expression

; Operations with field names for better context
[
  (binary_operator
    left: (_) @binary.left
    right: (_) @binary.right
    operator: (_) @binary.operator)
  (attribute
    object: (_) @attribute.object
    attribute: (_) @attribute.name)
  (subscript
    object: (_) @subscript.object
    index: (_) @subscript.index)
] @definition.operation
`;