/*
Rust Concurrency Relationships-specific Tree-Sitter Query Patterns
用于识别同步、锁、通信、竞态条件等并发关系
*/
export default `
; 线程创建（spawn）
(call_expression
  function: (scoped_identifier
    path: (scoped_identifier
      path: (identifier) @thread.module
      name: (identifier) @thread.submodule)
    name: (identifier) @thread.method)
  (#match? @thread.method "^spawn$")
  arguments: (arguments
    (closure_expression) @thread.handler)) @concurrency.relationship.thread.spawn

; 线程句柄管理
(call_expression
  function: (field_expression
    value: (identifier) @thread.handle
    field: (field_identifier) @thread.method)
  (#match? @thread.method "^(join|detach|is_finished)$")) @concurrency.relationship.thread.handle

; 通道创建
(call_expression
  function: (scoped_identifier
    path: (scoped_identifier
      path: (identifier) @channel.module
      name: (identifier) @channel.submodule)
    name: (identifier) @channel.method)
  (#match? @channel.method "^channel$")) @concurrency.relationship.channel.creation

; 通道发送
(call_expression
  function: (field_expression
    value: (identifier) @channel.sender
    field: (field_identifier) @send.method)
  (#match? @send.method "^send$")
  arguments: (arguments
    (identifier) @message.data)) @concurrency.relationship.channel.send

; 通道接收
(call_expression
  function: (field_expression
    value: (identifier) @channel.receiver
    field: (field_identifier) @recv.method)
  (#match? @recv.method "^recv$")) @concurrency.relationship.channel.receive

; 异步块创建
(async_block
  body: (block
    (await_expression
      (call_expression) @async.operation))) @concurrency.relationship.async.block

; 异步函数调用
(call_expression
  function: (identifier) @async.function
  (#match? @async.function "^(async|spawn|join|select)$")
  arguments: (arguments
    (closure_expression) @async.handler)) @concurrency.relationship.async.call

; Await表达式
(await_expression
  (call_expression) @awaited.call) @concurrency.relationship.await.expression

; 锁机制 - Mutex
(call_expression
  function: (scoped_identifier
    path: (scoped_identifier
      path: (identifier) @sync.module
      name: (identifier) @sync.submodule)
    name: (identifier) @mutex.constructor)
  (#match? @mutex.constructor "^Mutex::new$")
  arguments: (arguments
    (identifier) @mutex.data)) @concurrency.relationship.mutex.creation

; Mutex锁操作
(call_expression
  function: (field_expression
    value: (identifier) @mutex.object
    field: (field_identifier) @lock.method)
  (#match? @lock.method "^(lock|try_lock)$")) @concurrency.relationship.mutex.lock

; RwLock创建
(call_expression
  function: (scoped_identifier
    path: (scoped_identifier
      path: (identifier) @sync.module
      name: (identifier) @sync.submodule)
    name: (identifier) @rwlock.constructor)
  (#match? @rwlock.constructor "^RwLock::new$")
  arguments: (arguments
    (identifier) @rwlock.data)) @concurrency.relationship.rwlock.creation

; RwLock读写操作
(call_expression
  function: (field_expression
    value: (identifier) @rwlock.object
    field: (field_identifier) @rwlock.method)
  (#match? @rwlock.method "^(read|write|try_read|try_write)$")) @concurrency.relationship.rwlock.access

; Arc（原子引用计数）
(call_expression
  function: (scoped_identifier
    path: (scoped_identifier
      path: (identifier) @sync.module
      name: (identifier) @sync.submodule)
    name: (identifier) @arc.constructor)
  (#match? @arc.constructor "^Arc::new$")
  arguments: (arguments
    (identifier) @arc.data)) @concurrency.relationship.arc.creation

; Arc克隆
(call_expression
  function: (scoped_identifier
    path: (scoped_identifier
      path: (identifier) @sync.module
      name: (identifier) @sync.submodule)
    name: (identifier) @arc.method)
  (#match? @arc.method "^clone$")
  arguments: (arguments
    (identifier) @arc.source)) @concurrency.relationship.arc.clone

; 原子操作
(call_expression
  function: (scoped_identifier
    path: (scoped_identifier
      path: (identifier) @atomic.module
      name: (identifier) @atomic.submodule)
    name: (identifier) @atomic.method)
  (#match? @atomic.method "^(Atomic.*|load|store|swap|compare_and_swap|fetch_.*)$")
  arguments: (arguments
    (identifier) @atomic.target
    (identifier) @atomic.value)) @concurrency.relationship.atomic.operation

; Barrier同步
(call_expression
  function: (scoped_identifier
    path: (scoped_identifier
      path: (identifier) @sync.module
      name: (identifier) @sync.submodule)
    name: (identifier) @barrier.constructor)
  (#match? @barrier.constructor "^Barrier::new$")
  arguments: (arguments
    (identifier) @barrier.count)) @concurrency.relationship.barrier.creation

; Barrier等待
(call_expression
  function: (field_expression
    value: (identifier) @barrier.object
    field: (field_identifier) @barrier.method)
  (#match? @barrier.method "^wait$")) @concurrency.relationship.barrier.wait

; Condvar条件变量
(call_expression
  function: (scoped_identifier
    path: (scoped_identifier
      path: (identifier) @sync.module
      name: (identifier) @sync.submodule)
    name: (identifier) @condvar.constructor)
  (#match? @condvar.constructor "^Condvar::new$")) @concurrency.relationship.condvar.creation

; Condvar等待和通知
(call_expression
  function: (field_expression
    value: (identifier) @condvar.object
    field: (field_identifier) @condvar.method)
  (#match? @condvar.method "^(wait|wait_timeout|notify_one|notify_all)$")) @concurrency.relationship.condvar.operation

; 选择操作（select!宏）
(macro_invocation
  macro: (identifier) @select.macro
  (#match? @select.macro "^select$")
  arguments: (token_tree
    (identifier) @select.channel)) @concurrency.relationship.select.operation

; 竞态条件检测 - 共享变量写操作
(assignment_expression
  left: (field_expression
    value: (identifier) @shared.variable
    field: (field_identifier) @shared.field)
  right: (binary_expression
    left: (field_expression
      value: (identifier) @shared.variable
      field: (field_identifier) @shared.field)
    operator: (_) @operator
    right: (identifier) @increment.value)) @concurrency.relationship.race.condition

; 线程本地存储
(attribute_item
  attribute: (attribute
    name: (identifier) @thread_local.attr
    (#match? @thread_local.attr "^thread_local$"))) @concurrency.relationship.thread.local

; 无锁数据结构操作
(call_expression
  function: (scoped_identifier
    path: (identifier) @lockfree.module
    name: (identifier) @lockfree.method)
  (#match? @lockfree.method "^(push|pop|enqueue|dequeue|insert|remove)$")
  arguments: (arguments
    (identifier) @lockfree.data)) @concurrency.relationship.lockfree.operation

; 并行迭代器
(call_expression
  function: (field_expression
    value: (identifier) @parallel.iterator
    field: (field_identifier) @parallel.method)
  (#match? @parallel.method "^(par_iter|par_chunks|par_windows)$")) @concurrency.relationship.parallel.iterator

; 工作窃取
(call_expression
  function: (scoped_identifier
    path: (scoped_identifier
      path: (identifier) @thread_pool.module
      name: (identifier) @thread_pool.submodule)
    name: (identifier) @work_stealing.method)
  (#match? @work_stealing.method "^(spawn|join|scope)$")) @concurrency.relationship.work.stealing
`;