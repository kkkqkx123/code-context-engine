/*
Kotlin Control Flow and Pattern-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; When表达式查询 - 使用锚点确保精确匹配
(when_expression
  value: (_) @when.value
  body: (when_block
    (when_entry
      [
        (expression_list
          (expression) @case.value)*
        (range_expression) @case.range
      ]
      (expression) @case.body)*)) @definition.when.expression

; Try表达式查询 - 使用锚点确保精确匹配
(try_expression
  body: (block) @try.body
  (catch_block
    (simple_identifier) @catch.param
    (block) @catch.body)*
  (finally_block
    (block) @finally.body)?) @definition.try.expression

; 循环语句查询 - 使用交替模式
[
  (for_statement
    (variable_declaration
      name: (simple_identifier) @for.var)
    (expression) @for.iterable
    (block) @for.body)
  (while_statement
    (expression) @while.condition
    (block) @while.body)
  (do_while_statement
    (block) @do.body
    (expression) @do.condition)
] @definition.loop.statement

; 条件表达式查询
(if_expression
  condition: (expression) @if.condition
  consequence: (expression) @if.then
  alternative: (expression)? @if.else) @definition.if.expression

; 跳转表达式查询 - 使用交替模式
[
  (return_expression
    (expression)? @return.value)
  (break_expression
    (simple_identifier)? @break.label)
  (continue_expression
    (simple_identifier)? @continue.label)
  (throw_expression
    (expression) @throw.expr)
] @definition.jump.expression

; Lambda表达式查询 - 使用锚点确保精确匹配
(lambda_literal
  parameters: (lambda_parameters
    (variable_declaration
      name: (simple_identifier) @lambda.param)*)?
  body: (_) @lambda.body) @definition.lambda.expression

; 安全调用和Elvis表达式查询 - 使用交替模式
[
  (safe_call_expression
    receiver: (_) @safe.receiver
    (navigation_suffix
      (simple_identifier) @safe.method))
  (elvis_expression
    left: (expression) @elvis.condition
    right: (expression) @elvis.value))
] @definition.null.safety
`;