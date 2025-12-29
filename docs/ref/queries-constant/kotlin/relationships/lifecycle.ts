/*
Kotlin Lifecycle Relationships-specific Tree-Sitter Query Patterns
用于识别对象实例化、初始化、销毁、生命周期管理等关系
*/
export default `
; 异常处理流关系 - 使用锚点确保精确匹配
(try_expression
  body: (block) @source.try.block
  (catch_block
    (simple_identifier) @exception.parameter
    (block) @target.catch.block)*
  (finally_block
    (block) @target.finally.block)?) @control.flow.exception

; 多个catch块的异常流 - 使用量词操作符
(try_expression
  body: (block) @source.try.block
  (catch_block
    (simple_identifier) @exception.parameter1
    (block) @target.catch.block1)
  (catch_block
    (simple_identifier) @exception.parameter2
    (block) @target.catch.block2)*) @control.flow.multiple.exception

; 条件表达式嵌套流 - 使用锚点确保精确匹配
(if_expression
  condition: (simple_identifier) @source.condition
  consequence: (block
    .
    (if_expression
      condition: (simple_identifier) @source.nested.condition
      consequence: (_) @target.nested.block))) @control.flow.nested.conditional

; 循环嵌套流 - 使用锚点确保精确匹配
(for_statement
  body: (block
    .
    (while_statement
      condition: (simple_identifier) @source.while.condition
      body: (_) @target.while.block))) @control.flow.nested.loop

; When表达式流关系 - 使用锚点确保精确匹配
(when_expression
  value: (simple_identifier) @source.when.value
  body: (when_block
    (when_entry
      (expression) @case.condition
      (expression) @target.case.body)*)) @control.flow.when.case

; 守卫条件流关系 - 使用锚点确保精确匹配
(when_entry
  pattern: (simple_identifier) @source.guard.variable
  guard: (if_expression
    condition: (simple_identifier) @source.guard.condition)
  value: (_) @target.guard.body) @control.flow.guard

; 闭包控制流 - 使用锚点确保精确匹配
(lambda_literal
  parameters: (lambda_parameters
    (simple_identifier) @source.lambda.param)
  body: (block
    (return_expression
      (simple_identifier) @target.return.value))) @control.flow.lambda.return

; 异步块控制流 - 使用锚点确保精确匹配
(async_block
  body: (block
    (await_expression
      (call_expression) @source.awaited.call))) @control.flow.async.await

; 跳转表达式流关系 - 使用交替模式
[
  (break_expression
    (simple_identifier) @source.break.label)
  (continue_expression
    (simple_identifier) @source.continue.label)
  (return_expression
    (simple_identifier) @source.return.value)
  (throw_expression
    (simple_identifier) @source.throw.value)
] @control.flow.jump

; 标签块流关系 - 使用锚点确保精确匹配
(labeled_expression
  label: (simple_identifier) @source.block.label
  value: (block
    (break_expression
      (simple_identifier) @target.break.label))) @control.flow.labeled.break

; 协程控制流 - 使用谓词过滤
(call_expression
  function: (simple_identifier) @coroutine.function
  arguments: (value_arguments
    (lambda_literal) @coroutine.handler)*
  (#match? @coroutine.function "^(launch|async|runBlocking)$")) @control.flow.coroutine

; 流控制流 - 使用谓词过滤
(call_expression
  function: (navigation_expression
    left: (simple_identifier) @flow.object
    right: (simple_identifier) @flow.method)
  arguments: (value_arguments
    (lambda_literal) @flow.handler)*
  (#match? @flow.method "^(collect|emit|transform|filter|map)$")) @control.flow.flow.operation

; 同步块控制流 - 使用锚点确保精确匹配
(synchronized_statement
  body: (block
    (expression_statement
      (call_expression
        function: (simple_identifier) @target.synchronized.function)))) @control.flow.synchronized.block

; 条件编译流关系 - 使用谓词过滤
(annotation
  name: (simple_identifier) @cfg.name
  arguments: (argument_list
    (simple_identifier) @source.cfg.condition)*
  (#match? @cfg.name "^cfg$")) @control.flow.cfg.conditional
`;