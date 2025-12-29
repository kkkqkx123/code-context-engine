/*
TypeScript Control Flow Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
Based on tree-sitter-typescript grammar
*/
export default `
; 统一的控制流语句查询 - 使用交替模式
[
  (if_statement
    condition: (_) @if.condition
    consequence: (statement_block) @if.body
    alternative: (_)? @if.else)
  (for_statement
    initializer: (_)? @for.init
    condition: (_)? @for.condition
    update: (_)? @for.update
    body: (statement_block) @for.body)
  (for_in_statement
    left: (_) @for_in.left
    right: (_) @for_in.right
    body: (statement_block) @for_in.body)
  (for_of_statement
    left: (_) @for_of.left
    right: (_) @for_of.right
    body: (statement_block) @for_of.body)
  (while_statement
    condition: (_) @while.condition
    body: (statement_block) @while.body)
  (do_statement
    body: (statement_block) @do.body
    condition: (_) @do.condition)
] @definition.control_statement

; Switch语句查询 - 提供更多上下文信息
(switch_statement
  value: (_) @switch.value
  body: (switch_body
    (switch_case
      value: (_) @case.value
      body: (_)* @case.body)*
    (switch_default
      body: (_)* @default.body)?)) @definition.switch_statement

; Try-catch-finally语句查询 - 提供更多上下文信息
(try_statement
  body: (statement_block) @try.body
  (catch_clause
    parameter: (required_parameter
      name: (identifier) @catch.param)?
    body: (statement_block) @catch.body)*
  (finally_clause
    body: (statement_block) @finally.body)?) @definition.try_catch_finally

; 单独的控制流语句
[
  (throw_statement
    (expression)? @throw.expression)
  (return_statement
    (expression)? @return.expression)
  (break_statement
    label: (statement_identifier)? @break.label)
  (continue_statement
    label: (statement_identifier)? @continue.label)
  (debugger_statement)
  (yield_expression
    (expression)? @yield.expression)
  (await_expression
    value: (_) @await.value)
] @definition.control_flow_simple

; 三元表达式查询 - 提供更多上下文信息
(ternary_expression
  condition: (_) @ternary.condition
  consequence: (_) @ternary.consequence
  alternative: (_) @ternary.alternative) @definition.ternary
`;