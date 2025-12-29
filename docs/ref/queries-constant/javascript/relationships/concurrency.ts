/*
JavaScript Concurrency Relationships-specific Tree-Sitter Query Patterns
用于识别同步、锁、通信、竞态条件等并发关系
*/
export default `
; Promise创建（异步并发）
(call_expression
  function: (identifier) @promise.constructor
  (#match? @promise.constructor "Promise$")
  arguments: (argument_list
    (function_expression) @promise.executor)) @concurrency.relationship.promise.creation

; Promise链式调用（异步并发）
(call_expression
  function: (member_expression
    object: (call_expression) @source.promise
    property: (property_identifier) @promise.method
    (#match? @promise.method "^(then|catch|finally)$"))
  arguments: (argument_list
    (function_expression) @handler.function)) @concurrency.relationship.promise.chain

; Async函数定义
(async_function_declaration
  name: (identifier) @async.function) @concurrency.relationship.async.function

; Async函数调用
(call_expression
  function: (identifier) @async.function
  arguments: (argument_list
    (identifier) @async.parameter)) @concurrency.relationship.async.call

; Await表达式
(await_expression
  (call_expression) @awaited.call) @concurrency.relationship.await.expression

; 并行Promise执行
(call_expression
  function: (member_expression
    object: (identifier) @promise.object
    property: (property_identifier) @parallel.method
    (#match? @parallel.method "^(all|allSettled|race)$"))
  arguments: (argument_list
    (array
      (identifier) @parallel.promise))) @concurrency.relationship.parallel.execution

; Worker创建
(new_expression
  constructor: (identifier) @worker.constructor
  (#match? @worker.constructor "Worker$")
  arguments: (argument_list
    (string) @worker.script)) @concurrency.relationship.worker.creation

; Worker消息发送
(call_expression
  function: (member_expression
    object: (identifier) @worker.object
    property: (property_identifier) @worker.method
    (#match? @worker.method "^(postMessage|send)$"))
  arguments: (argument_list
    (identifier) @worker.message)) @concurrency.relationship.worker.communication

; Worker消息接收
(assignment_expression
  left: (identifier) @message.handler
  right: (member_expression
    object: (identifier) @worker.object
    property: (property_identifier) @worker.event
    (#match? @worker.event "onmessage$"))) @concurrency.relationship.worker.message.reception

; 共享数组缓冲区
(new_expression
  constructor: (identifier) @shared.array.constructor
  (#match? @shared.array.constructor "SharedArrayBuffer$")
  arguments: (argument_list
    (identifier) @buffer.size)) @concurrency.relationship.shared.array

; Atomics操作
(call_expression
  function: (member_expression
    object: (identifier) @atomics.object
    property: (property_identifier) @atomics.method
    (#match? @atomics.method "^(add|sub|and|or|xor|load|store|compareExchange)$"))
  arguments: (argument_list
    (identifier) @atomics.target
    (identifier) @atomics.value)) @concurrency.relationship.atomics.operation

; 锁机制模拟
(call_expression
  function: (member_expression
    object: (identifier) @lock.object
    property: (property_identifier) @lock.method
    (#match? @lock.method "^(acquire|release|tryAcquire)$"))) @concurrency.relationship.lock.operation

; 条件变量模拟
(call_expression
  function: (member_expression
    object: (identifier) @condition.variable
    property: (property_identifier) @condition.method
    (#match? @condition.method "^(wait|signal|signalAll)$"))) @concurrency.relationship.condition.variable

; 信号量模拟
(call_expression
  function: (member_expression
    object: (identifier) @semaphore.object
    property: (property_identifier) @semaphore.method
    (#match? @semaphore.method "^(acquire|release|availablePermits)$"))) @concurrency.relationship.semaphore.operation

; 竞态条件检测 - 简化版本
(assignment_expression
  left: (identifier) @shared.variable
  right: (identifier) @source.variable) @concurrency.relationship.race.condition
`;