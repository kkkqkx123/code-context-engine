/*
Java Control Flow and Pattern-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Switch表达式和语句查询 - 使用交替模式
[
  (switch_expression) @control.switch.expr
  (switch_statement) @control.switch.stmt
] @definition.switch

; Switch块和分支查询 - 使用量词操作符
(switch_block
  (switch_block_statement_group
    (switch_label
      [
        (expression_list
          (identifier) @case.value)*
        (default_case) @default.case
      ]
    (block_statement_list
      (statement) @case.stmt)*)*) @definition.switch.block

; 模式匹配查询 - 使用交替模式
[
  (record_pattern) @pattern.record
  (type_pattern) @pattern.type
  (underscore_pattern) @pattern.underscore
] @definition.pattern

; Try语句查询 - 使用锚点确保精确匹配
(try_statement
  body: (block) @try.body
  (catch_clause
    parameter: (catch_formal_parameter
      name: (identifier) @catch.param)
    body: (block) @catch.body)*
  (finally_block
    (block) @finally.body)?) @definition.try.statement

; Try-with-resources查询
(try_with_resources_statement
  resources: (resource_list
    (resource
      name: (identifier) @resource.name
      value: (_) @resource.value)*)
  body: (block) @try.body
  (catch_clause
    parameter: (catch_formal_parameter
      name: (identifier) @catch.param)
    body: (block) @catch.body)*
  (finally_block
    (block) @finally.body)?) @definition.try.with.resources

; 循环语句查询 - 使用交替模式
[
  (for_statement
    initializer: (_) @for.init
    condition: (_) @for.condition
    update: (_) @for.update
    body: (statement) @for.body)
  (enhanced_for_statement
    name: (identifier) @for.var
    value: (_) @for.iterable
    body: (statement) @for.body)
  (while_statement
    condition: (_) @while.condition
    body: (statement) @while.body)
  (do_statement
    body: (statement) @do.body
    condition: (_) @do.condition)
] @definition.loop

; 条件语句查询
(if_statement
  condition: (parenthesized_expression
    (_) @if.condition)
  consequence: (statement) @if.then
  alternative: (statement)? @if.else) @definition.if.statement

; 跳转语句查询 - 使用交替模式
[
  (return_statement
    (expression)? @return.value)
  (break_statement
    (identifier)? @break.label)
  (continue_statement
    (identifier)? @continue.label)
  (yield_statement
    (expression) @yield.value)
] @definition.jump

; 异常处理查询 - 使用交替模式
[
  (throw_statement
    (expression) @throw.expr)
  (assert_statement
    condition: (_) @assert.condition
    message: (_)? @assert.message)
] @definition.exception

; 同步语句查询
(synchronized_statement
  (parenthesized_expression
    (identifier) @sync.lock)
  (block) @sync.body) @definition.synchronized

; 标签语句查询
(labeled_statement
  label: (identifier) @label.name
  statement: (_) @labeled.stmt) @definition.labeled

; 块和表达式语句查询 - 使用交替模式
[
  (block) @structure.block
  (expression_statement) @statement.expression
] @definition.structure
`;