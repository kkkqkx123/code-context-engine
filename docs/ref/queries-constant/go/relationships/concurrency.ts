/*
Go Concurrency Relationships-specific Tree-Sitter Query Patterns
用于识别同步、锁、通信、竞态条件等并发关系
只包含基于抽象语法树的真实查询，不使用字符匹配
*/
export default `
; ===== 协程创建关系 =====

; 基本协程创建关系
(go_statement
  (call_expression
    function: (identifier) @goroutine.function
    arguments: (argument_list
      (identifier) @goroutine.param)*)) @concurrency.goroutine.creation

; 匿名函数协程创建关系
(go_statement
  (call_expression
    function: (func_literal
      body: (block) @goroutine.lambda.body))) @concurrency.goroutine.lambda.creation

; ===== 通道操作关系 =====

; 通道发送关系
(send_statement
  channel: (identifier) @channel.send
  value: (identifier) @channel.value) @concurrency.channel.send

; 通道接收关系
(unary_expression
  ["<-"] @channel.receive.op
  operand: (identifier) @channel.receive) @concurrency.channel.receive

; 通道创建关系
(call_expression
  function: (identifier) @channel.create.function
  arguments: (argument_list
    (channel_type) @channel.type))
  (#eq? @channel.create.function "make")) @concurrency.channel.creation

; 通道关闭关系
(call_expression
  function: (identifier) @channel.close.function
  arguments: (argument_list
    (identifier) @channel.object))
  (#eq? @channel.close.function "close")) @concurrency.channel.close

; ===== Select语句并发关系 =====

; 基本Select语句关系
(select_statement
  body: (block
    (comm_case
      [
        (send_statement
          channel: (identifier) @case.channel
          value: (identifier) @case.value)
        (expression_statement
          (unary_expression
            ["<-"] @case.receive.op
            operand: (identifier) @case.channel))
      ]
      (block) @case.body)*
    (default_case
      (block) @default.body)?)) @concurrency.select.operation

; ===== 同步原语关系 =====

; Mutex锁操作关系
(call_expression
  function: (selector_expression
    operand: (identifier) @mutex.object
    field: (field_identifier) @mutex.method)
  arguments: (argument_list
    (identifier) @mutex.param)*)) @concurrency.mutex.operation

; RWMutex操作关系
(call_expression
  function: (selector_expression
    operand: (identifier) @rwmutex.object
    field: (field_identifier) @rwmutex.method)
  arguments: (argument_list
    (identifier) @rwmutex.param)*)) @concurrency.rwmutex.operation

; WaitGroup操作关系
(call_expression
  function: (selector_expression
    operand: (identifier) @waitgroup.object
    field: (field_identifier) @waitgroup.method)
  arguments: (argument_list
    (identifier) @waitgroup.param)*)) @concurrency.waitgroup.operation

; Once操作关系
(call_expression
  function: (selector_expression
    operand: (identifier) @once.object
    field: (field_identifier) @once.method)
  arguments: (argument_list
    (identifier) @once.param)*)) @concurrency.once.operation

; Cond操作关系
(call_expression
  function: (selector_expression
    operand: (identifier) @cond.object
    field: (field_identifier) @cond.method)
  arguments: (argument_list
    (identifier) @cond.param)*)) @concurrency.cond.operation

; ===== 并发集合关系 =====

; Pool操作关系
(call_expression
  function: (selector_expression
    operand: (identifier) @pool.object
    field: (field_identifier) @pool.method)
  arguments: (argument_list
    (identifier) @pool.param)*)) @concurrency.pool.operation

; ===== 定时器关系 =====

; Timer操作关系
(call_expression
  function: [
    (identifier) @timer.function
    (selector_expression
      operand: (identifier) @timer.object
      field: (field_identifier) @timer.method)
  ]
  arguments: (argument_list
    (identifier) @timer.param)*)) @concurrency.timer.operation

; ===== Context操作关系 =====

; Context操作关系
(call_expression
  function: (selector_expression
    operand: (identifier) @context.object
    field: (field_identifier) @context.method)
  arguments: (argument_list
    (identifier) @context.param)*)) @concurrency.context.operation

; ===== 原子操作关系 =====

; 原子操作关系
(call_expression
  function: (selector_expression
    operand: (identifier) @atomic.object
    field: (field_identifier) @atomic.method)
  arguments: (argument_list
    (identifier) @atomic.param)*)) @concurrency.atomic.operation

; ===== 竞态条件检测 =====

; 共享变量访问竞态条件
(assignment_statement
  left: (expression_list
    (selector_expression
      operand: (identifier) @shared.object
      field: (field_identifier) @shared.field))
  right: (expression_list
    (binary_expression
      left: (selector_expression
        operand: (identifier) @shared.object
        field: (field_identifier) @shared.field)
      operator: (_) @race.operator
      right: (identifier) @race.value)))) @concurrency.race.condition

; ===== 死锁模式检测 =====

; 嵌套锁获取模式
(call_expression
  function: (selector_expression
    operand: (identifier) @first.lock
    field: (field_identifier) @lock.method))
(call_expression
  function: (selector_expression
    operand: (identifier) @second.lock
    field: (field_identifier) @lock.method)) @concurrency.deadlock.pattern

; ===== 并发模式关系 =====

; Worker池类型定义
(type_declaration
  (type_spec
    name: (type_identifier) @worker.pool.type
    type: (struct_type
      (field_declaration_list
        (field_declaration
          name: (field_identifier) @worker.pool.field
          type: (channel_type) @worker.pool.channel.type)*)))) @concurrency.worker.pool.pattern

; 并发集合类型定义
(type_declaration
  (type_spec
    name: (type_identifier) @concurrent.collection.type
    type: (struct_type
      (field_declaration_list
        (field_declaration
          name: (field_identifier) @concurrent.field
          type: (channel_type) @concurrent.channel.type)*)))) @concurrency.concurrent.collection.pattern
`;