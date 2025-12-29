/*
Kotlin Concurrency Relationships-specific Tree-Sitter Query Patterns
用于识别同步、锁、通信、竞态条件等并发关系
*/
export default `
; 协程创建关系 - 使用谓词过滤
(call_expression
  function: (simple_identifier) @coroutine.method
  arguments: (value_arguments
    (lambda_literal) @coroutine.handler)*
  (#match? @coroutine.method "^(launch|async)$")) @concurrency.coroutine.creation

; 协程作用域关系 - 使用谓词过滤
(call_expression
  function: (simple_identifier) @coroutine.scope
  arguments: (value_arguments
    (lambda_literal) @coroutine.handler)*
  (#match? @coroutine.scope "^(runBlocking|coroutineScope|supervisorScope)$")) @concurrency.coroutine.scope

; 协程取消关系 - 使用谓词过滤
(call_expression
  function: (navigation_expression
    left: (simple_identifier) @coroutine.job
    right: (simple_identifier) @cancel.method)
  (#match? @cancel.method "^cancel$")) @concurrency.coroutine.cancel

; 协程等待关系 - 使用谓词过滤
(call_expression
  function: (navigation_expression
    left: (simple_identifier) @coroutine.deferred
    right: (simple_identifier) @await.method)
  (#match? @await.method "^await$")) @concurrency.coroutine.await

; 通道创建关系 - 使用谓词过滤
(call_expression
  function: (simple_identifier) @channel.constructor
  arguments: (value_arguments
    (simple_identifier) @channel.capacity)*
  (#match? @channel.constructor "^Channel$")) @concurrency.channel.creation

; 通道操作关系 - 使用谓词过滤
(call_expression
  function: (navigation_expression
    left: (simple_identifier) @channel.object
    right: (simple_identifier) @channel.method)
  arguments: (value_arguments
    (simple_identifier) @channel.param)*
  (#match? @channel.method "^(send|receive|close)$")) @concurrency.channel.operation

; 互斥锁操作关系 - 使用谓词过滤
(call_expression
  function: (simple_identifier) @mutex.constructor
  arguments: (value_arguments
    (simple_identifier) @mutex.data)*
  (#match? @mutex.constructor "^Mutex$")) @concurrency.mutex.creation

(call_expression
  function: (navigation_expression
    left: (simple_identifier) @mutex.object
    right: (simple_identifier) @lock.method)
  arguments: (value_arguments
    (lambda_literal) @lock.handler)*
  (#match? @lock.method "^(lock|tryLock)$")) @concurrency.mutex.operation

; 原子操作关系 - 使用谓词过滤
(call_expression
  function: (navigation_expression
    left: (simple_identifier) @atomic.class
    right: (simple_identifier) @atomic.method)
  arguments: (value_arguments
    (simple_identifier) @atomic.param)*
  (#match? @atomic.method "^(Atomic.*|load|store|swap|compareAndSet|getAndSet|incrementAndGet)$")) @concurrency.atomic.operation

; 选择操作关系 - 使用谓词过滤
(call_expression
  function: (simple_identifier) @select.method
  arguments: (value_arguments
    (lambda_literal) @select.handler)*
  (#match? @select.method "^select$")) @concurrency.select.operation

; 流操作关系 - 使用谓词过滤
(call_expression
  function: (navigation_expression
    left: (simple_identifier) @flow.object
    right: (simple_identifier) @flow.method)
  arguments: (value_arguments
    (lambda_literal) @flow.handler)*
  (#match? @flow.method "^(flow|channelFlow|callbackFlow)$")) @concurrency.flow.operation

; 并发集合操作关系 - 使用谓词过滤
(call_expression
  function: (simple_identifier) @concurrent.constructor
  arguments: (value_arguments
    (simple_identifier) @concurrent.param)*
  (#match? @concurrent.constructor "^(ConcurrentHashMap|ConcurrentLinkedQueue|CopyOnWriteArrayList)$")) @concurrency.concurrent.collection

; 竞态条件检测 - 使用锚点确保精确匹配
(assignment
  left: (navigation_expression
    left: (simple_identifier) @shared.variable
    right: (simple_identifier) @shared.field)
  right: (binary_expression
    left: (navigation_expression
      left: (simple_identifier) @shared.variable
      right: (simple_identifier) @shared.field)
    operator: (_) @race.operator
    right: (simple_identifier) @race.value)) @concurrency.race.condition

; 线程本地变量关系 - 使用谓词过滤
(property_declaration
  name: (simple_identifier) @threadlocal.variable
  (modifiers
    (annotation
      name: (simple_identifier) @threadlocal.annotation
      (#match? @threadlocal.annotation "^ThreadLocal$")))) @concurrency.thread.local

; 屏障同步关系 - 使用谓词过滤
(call_expression
  function: (simple_identifier) @barrier.constructor
  arguments: (value_arguments
    (simple_identifier) @barrier.count
    (lambda_literal)? @barrier.action)*
  (#match? @barrier.constructor "^CyclicBarrier$")) @concurrency.barrier.creation

; 倒计时门闩关系 - 使用谓词过滤
(call_expression
  function: (simple_identifier) @countdown.constructor
  arguments: (value_arguments
    (simple_identifier) @countdown.count)*
  (#match? @countdown.constructor "^CountDownLatch$")) @concurrency.countdown.creation

; 信号量关系 - 使用谓词过滤
(call_expression
  function: (simple_identifier) @semaphore.constructor
  arguments: (value_arguments
    (simple_identifier) @semaphore.permits)*
  (#match? @semaphore.constructor "^Semaphore$")) @concurrency.semaphore.creation
`;