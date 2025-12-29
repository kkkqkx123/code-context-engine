/*
C# Control Flow-specific Tree-Sitter Query Patterns
用于追踪控制流关系和条件分支
Optimized based on tree-sitter best practices
*/
export default `
; 条件语句查询 - 使用交替模式和字段名
[
  (if_statement
    condition: (_) @if.condition
    consequence: (block) @if.body
    alternative: (elseif_clause
      condition: (_) @elseif.condition
      consequence: (block) @elseif.body)*
    alternative: (block)? @if.else) @definition.control.if
  (switch_statement
    value: (_) @switch.value
    body: (switch_body
      (switch_section
        (switch_label) @case.label
        (statement) @case.statement)*)) @definition.control.switch
  (switch_expression
    (identifier) @switch.expression.value
    (switch_expression_arm
      pattern: (_) @switch.pattern
      result: (_) @switch.result)*)) @definition.control.switch.expression
] @definition.control.conditional

; 循环语句查询 - 使用交替模式和字段名
[
  (while_statement
    condition: (_) @while.condition
    body: (block) @while.body) @definition.control.while
  (do_statement
    body: (block) @do.body
    condition: (_) @do.condition) @definition.control.do
  (for_statement
    initializer: (_) @for.initializer
    condition: (_) @for.condition
    update: (_) @for.update
    body: (block) @for.body) @definition.control.for
  (foreach_statement
    type: (identifier) @foreach.type
    left: (identifier) @foreach.variable
    right: (_) @foreach.collection
    body: (block) @foreach.body) @definition.control.foreach
] @definition.control.loop

; 跳转语句查询 - 使用交替模式
[
  (break_statement) @definition.jump.break
  (continue_statement) @definition.jump.continue
  (return_statement
    expression: (_) @return.expression)? @definition.jump.return
  (goto_statement
    label: (identifier) @goto.label) @definition.jump.goto
  (yield_statement
    expression: (_) @yield.expression)? @definition.jump.yield
] @definition.control.jump

; 异常处理查询 - 使用交替模式和字段名
[
  (try_statement
    body: (block) @try.body
    (catch_clause
      type: (identifier) @catch.type
      name: (identifier)? @catch.name
      condition: (_) @catch.filter?
      body: (block) @catch.body)*
    (finally_clause
      body: (block) @finally.body)?) @definition.exception.try
  (throw_statement
    expression: (_) @throw.expression) @definition.exception.throw
  (throw_expression
    expression: (_) @throw.expression) @definition.exception.throw.expression
] @definition.control.exception

; 锁定和资源管理查询 - 使用交替模式
[
  (lock_statement
    expression: (_) @lock.expression
    body: (block) @lock.body) @definition.control.lock
  (using_statement
    declaration: (variable_declaration
      type: (identifier) @using.type
      declarators: (variable_declarator_list
        (variable_declarator
          name: (identifier) @using.variable
          value: (_) @using.value)))
    body: (block) @using.body) @definition.control.using
] @definition.control.resource

; 异步控制流查询 - 使用谓词过滤
[
  (await_expression
    expression: (_) @await.expression) @definition.async.await
  (method_declaration
    (modifier) @async.modifier
    name: (identifier) @async.method
    body: (block) @async.body
    (#match? @async.modifier "async")) @definition.async.method
] @definition.control.async

; Lambda和匿名方法查询 - 使用交替模式
[
  (lambda_expression
    parameters: (parameter_list
      (parameter
        name: (identifier) @lambda.param)*)?
    body: (_) @lambda.body) @definition.lambda
  (anonymous_method_expression
    parameters: (parameter_list
      (parameter
        name: (identifier) @anonymous.param)*)?
    body: (block) @anonymous.body) @definition.anonymous.method
] @definition.function.expression

; LINQ查询查询 - 使用锚点和字段名
(query_expression
  (from_clause
    type: (identifier) @linq.from.type
    identifier: (identifier) @linq.from.variable
    expression: (_) @linq.from.expression)
  (query_body
    (query_clause) @linq.clause*
    (select_clause
      expression: (_) @linq.select.expression)
    (group_clause
      group: (_) @linq.group.expression
      by: (_) @linq.group.by)?)) @definition.linq.query

; 模式匹配查询 - 使用交替模式
[
  (is_pattern_expression
    expression: (_) @pattern.expression
    pattern: (_) @pattern.pattern) @definition.pattern.matching
  (switch_statement
    value: (_) @pattern.switch.value
    body: (switch_body
      (switch_section
        (case_pattern_switch_label
          pattern: (_) @case.pattern)
        (when_clause
          condition: (_) @when.condition)?
        (statement) @case.statement)*)) @definition.pattern.switch
] @definition.control.pattern

; 标签语句查询 - 使用锚点确保精确匹配
(labeled_statement
  label: (identifier) @label.name
  statement: (_) @label.statement) @definition.labeled.statement

; 局部函数查询 - 使用锚点确保精确匹配
(local_function_statement
  name: (identifier) @local.function.name
  type: (identifier) @local.function.return.type
  parameters: (parameter_list
    (parameter
      name: (identifier) @param.name
      type: (identifier) @param.type)*)?
  body: (block) @local.function.body) @definition.local.function
`;