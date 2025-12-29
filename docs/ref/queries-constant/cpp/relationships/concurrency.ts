/*
C++ Concurrency Relationships-specific Tree-Sitter Query Patterns
用于识别同步、锁、通信、竞态条件等并发关系
*/
export default `
; 线程创建
(declaration
  type: (qualified_identifier
    scope: (namespace_identifier) @std.scope
    name: (type_identifier) @thread.constructor)
  (#eq? @std.scope "std")
  (#match? @thread.constructor "^(thread|jthread)$")) @concurrency.relationship.thread.creation

; 线程加入
(call_expression
  function: (field_expression
    (identifier) @thread.object
    (field_identifier) @thread.method)
  (#eq? @thread.method "join")) @concurrency.relationship.thread.join
   
; 线程分离
(call_expression
  function: (field_expression
    (identifier) @thread.object
    (field_identifier) @thread.method)
  (#eq? @thread.method "detach"))@concurrency.relationship.thread.detach

; 互斥锁锁定
(call_expression
  function: (field_expression
    (identifier) @mutex.object
    (field_identifier) @mutex.method)
  (#eq? @mutex.method "lock"))@concurrency.relationship.mutex.lock

; 互斥锁解锁
(call_expression
  function: (field_expression
    (identifier) @mutex.object
    (field_identifier) @mutex.method)
  (#eq? @mutex.method "unlock"))@concurrency.relationship.mutex.unlock

; 互斥锁尝试锁定
(call_expression
  function: (field_expression
    (identifier) @mutex.object
    (field_identifier) @mutex.method)
  (#eq? @mutex.method "try_lock")) @concurrency.relationship.mutex.try_lock

; 锁守卫（RAII锁）
(declaration
  type: (qualified_identifier
    scope: (namespace_identifier) @std.scope
    name: (template_type
      name: (type_identifier) @lock_guard.type))
  declarator: (function_declarator
    declarator: (identifier) @lock_guard.variable
    parameters: (parameter_list
      (parameter_declaration
        (type_identifier) @locked.mutex)))
  (#match? @lock_guard.type "^(lock_guard|unique_lock|shared_lock)$")) @concurrency.relationship.lock_guard

; 条件变量等待
(call_expression
  function: (field_expression
    (identifier) @condition.variable
    (field_identifier) @condition.method)
  arguments: (argument_list
    (identifier) @locked.mutex)
  (#match? @condition.method "wait"))@concurrency.relationship.condition.wait 

; 条件变量通知
(call_expression
  function: (field_expression
    (identifier) @condition.variable
    (field_identifier) @condition.method)
  (#match? @condition.method "^(notify_one|notify_all)$"))@concurrency.relationship.condition.notify

; 原子操作
(call_expression
  function: (field_expression
    (identifier) @atomic.variable
    (field_identifier) @atomic.method)
  (#match? @atomic.method "^(store|load|exchange|fetch_add|fetch_sub|fetch_and|fetch_or|fetch_xor)$"))
  @concurrency.relationship.atomic.operation

; 原子比较交换
(call_expression
  function: (field_expression
    (identifier) @atomic.variable
    (field_identifier) @atomic.method)
  arguments: (argument_list
    (identifier) @expected.value
    (identifier) @desired.value)
  (#match? @atomic.method "compare_exchange"))@concurrency.relationship.atomic.compare_exchange

; 原子标志
(call_expression
  function: (field_expression
    (identifier) @atomic.flag
    (field_identifier) @flag.method)
  (#match? @flag.method "^(test_and_set|clear)$"))@concurrency.relationship.atomic.flag

; 信号量获取（C++20）
(call_expression
  function: (field_expression
    (identifier) @semaphore.object
    (field_identifier) @semaphore.method)
  (#match? @semaphore.method "acquire")) @concurrency.relationship.semaphore.acquire

; 信号量释放（C++20）
(call_expression
  function: (field_expression
    (identifier) @semaphore.object
    (field_identifier) @semaphore.method)
  arguments: (argument_list)
  (#match? @semaphore.method "release"))@concurrency.relationship.semaphore.release

; 闩器等待（C++20）
(call_expression
  function: (field_expression
    (identifier) @latch.object
    (field_identifier) @latch.method)
  arguments: (argument_list)
  (#match? @latch.method "wait")) @concurrency.relationship.latch.wait

; 闩器倒计时（C++20）
(call_expression
  function: (field_expression
    (identifier) @latch.object
    (field_identifier) @latch.method)
  arguments: (argument_list)
  (#match? @latch.method "count_down"))@concurrency.relationship.latch.count_down

; 屏障同步（C++20）
(call_expression
  function: (field_expression
    (identifier) @barrier.object
    (field_identifier) @barrier.method)
  arguments: (argument_list)
  (#match? @barrier.method "arrive_and_wait"))@concurrency.relationship.barrier.sync

; 共享互斥锁（读写锁）
(call_expression
  function: (field_expression
    (identifier) @shared.mutex
    (field_identifier) @shared.method)
  (#match? @shared.method "^(lock_shared|unlock_shared)$")) @concurrency.relationship.shared.mutex

; 异步任务创建
(call_expression
  function: (qualified_identifier
    scope: (namespace_identifier) @std.scope
    name: (identifier) @async.function)
  arguments: (argument_list
    (identifier) @async.task))
  (#match? @async.function "async")) @concurrency.relationship.async.task

; 异步任务等待
(call_expression
  function: (field_expression
    (identifier) @future.object
    (field_identifier) @future.method))
  arguments: (argument_list)
  (#match? @future.method "wait")) @concurrency.relationship.future.wait

; 异步任务获取结果
(call_expression
  function: (field_expression
    (identifier) @future.object
    (field_identifier) @future.method))
  arguments: (argument_list)
  (#match? @future.method "get")) @concurrency.relationship.future.get

; 共享异步任务获取结果
(call_expression
  function: (field_expression
    (identifier) @shared.future.object
    (field_identifier) @shared.future.method))
  arguments: (argument_list)
  (#match? @shared.future.method "get")) @concurrency.relationship.shared.future.get

; 承诺设置值
(call_expression
  function: (field_expression
    (identifier) @promise.object
    (field_identifier) @promise.method)
  arguments: (argument_list
    [
      (identifier) @promise.value
      (number_literal) @promise.value
      (string_literal) @promise.value
      (_) @promise.value
    ])
  (#match? @promise.method "set_value"))@concurrency.relationship.promise.set_value

; 打包任务
;; 已废弃

; 线程本地存储
(declaration
  (storage_class_specifier) @thread.local.specifier
  declarator: (init_declarator
    declarator: (identifier) @thread.local.variable)
  (#match? @thread.local.specifier "thread_local"))@concurrency.relationship.thread.local

; 内存屏障
(call_expression
  function: (qualified_identifier
    scope: (namespace_identifier) @std.scope
    name: (identifier) @atomic.function)
  arguments: (argument_list
    (identifier) @memory.order)
  (#match? @atomic.function "atomic_thread_fence")) @concurrency.relationship.memory.fence

; 类似生产者-消费者模式的同步模式
;; 已弃用

; 读写锁模式
(class_specifier
  name: (type_identifier) @read.write.lock.class
  body: (field_declaration_list
    (function_definition
      declarator: (function_declarator
        declarator: (field_identifier) @read.lock.method))
    (function_definition
      declarator: (function_declarator
        declarator: (field_identifier) @write.lock.method)))
  (#match? @read.lock.method "^(read|read_lock|acquire_read|lock_read|shared_lock|lock_shared)$")
  (#match? @write.lock.method "^(write|write_lock|acquire_write|lock_write|exclusive_lock|lock_exclusive)$")) @concurrency.relationship.read.write.lock

; 线程池模式
; 已弃用

; 并行算法
(call_expression
  function: (qualified_identifier
    scope: (namespace_identifier) @std.scope
    name: (identifier) @parallel.algorithm)
  arguments: (argument_list
    (identifier) @execution.policy
    (identifier) @algorithm.args))
  (#match? @parallel.algorithm "^(for_each|sort|transform|reduce)$")) @concurrency.relationship.parallel.algorithm

; 执行策略
(call_expression
  function: (qualified_identifier
    scope: (namespace_identifier) @execution.scope
    name: (identifier) @execution.policy))
  (#match? @execution.policy "^(par|par_unseq|seq|unseq)$")) @concurrency.relationship.execution.policy

; 协程创建（C++20）
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @coroutine.function)
  return_type: (type_identifier) @coroutine.return.type)
  (#match? @coroutine.return.type "^(task|generator|lazy)$")) @concurrency.relationship.coroutine

; 协程等待（C++20）
(await_expression
  (call_expression
    function: (field_expression
      argument: (identifier) @coroutine.object
      field: (field_identifier) @coroutine.method))
  (#match? @coroutine.method "^(get|wait|resume)$")) @concurrency.relationship.coroutine.await
`;