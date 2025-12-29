/*
JavaScript Control Flow and Expression-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 统一的控制流语句查询 - 使用交替模式
[
  (if_statement
    condition: (_) @if.condition
    consequence: (_) @if.body
    alternative: (_)? @if.else)
  (for_statement
    initializer: (_)? @for.init
    condition: (_)? @for.condition
    update: (_)? @for.update
    body: (_) @for.body)
  (while_statement
    condition: (_) @while.condition
    body: (_) @while.body)
  (do_statement
    body: (_) @do.body
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

; Try-catch语句查询 - 提供更多上下文信息
(try_statement
  body: (_) @try.body
  (catch_clause
    parameter: (identifier)? @catch.param
    body: (_) @catch.body)*
  (finally_clause
    body: (_) @finally.body)?) @definition.try_catch

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
  (labeled_statement
    label: (statement_identifier) @label.name
    body: (_) @label.body)
  (with_statement
    object: (_) @with.object
    body: (_) @with.body)
  (debugger_statement)
] @definition.control_flow_simple

; 表达式语句
[
  (expression_statement
    (expression) @expression.value)
  (binary_expression
    left: (_) @binary.left
    right: (_) @binary.right)
  (unary_expression
    argument: (_) @unary.argument)
  (update_expression
    argument: (_) @update.argument)
  (logical_expression
    left: (_) @logical.left
    right: (_) @logical.right)
  (conditional_expression
    condition: (_) @conditional.condition
    consequence: (_) @conditional.consequence
    alternative: (_) @conditional.alternative)
  (assignment_expression
    left: (_) @assignment.left
    right: (_) @assignment.right)
  (augmented_assignment_expression
    left: (_) @augmented.left
    right: (_) @augmented.right)
  (sequence_expression
    (expression)+ @sequence.expressions)
  (yield_expression
    (expression)? @yield.expression)
  (await_expression
    value: (_) @await.value)
] @definition.expression
`;