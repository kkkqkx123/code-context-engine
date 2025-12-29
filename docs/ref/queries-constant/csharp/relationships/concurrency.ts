/*
C# Concurrency Relationships-specific Tree-Sitter Query Patterns
用于识别同步、锁、通信、竞态条件等并发关系
*/
export default `
; async方法并发关系
(method_declaration
  (modifier) @async.modifier
  name: (identifier) @async.method.name
  body: (block) @async.method.body) @concurrency.relationship.async.method

; await表达式并发关系
(await_expression
  (invocation_expression
    function: (identifier) @awaited.function)) @concurrency.relationship.await.expression

; Task创建并发关系
(object_creation_expression
  type: (identifier) @task.type
  arguments: (argument_list
    (argument
      (lambda_expression
        parameters: (parameter_list)
        body: (block) @task.body)))) @concurrency.relationship.task.creation

; Task.Run并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @task.type
    name: (identifier) @task.run.method)
  arguments: (argument_list
    (argument
      (lambda_expression
        parameters: (parameter_list)
        body: (block) @task.action.body)))) @concurrency.relationship.task.run

; Task启动并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @task.object
    name: (identifier) @task.start.method)) @concurrency.relationship.task.start

; Task等待并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @task.object
    name: (identifier) @task.wait.method)) @concurrency.relationship.task.wait

; Task结果获取并发关系
(member_access_expression
  expression: (identifier) @task.object
  name: (identifier) @task.result.property) @concurrency.relationship.task.result

; 并行任务执行并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @task.type
    name: (identifier) @task.when.all.method)
  arguments: (argument_list
    (argument
      (identifier) @task.argument1)
    (argument
      (identifier) @task.argument2))) @concurrency.relationship.parallel.tasks

; 线程创建并发关系
(object_creation_expression
  type: (identifier) @thread.type
  arguments: (argument_list
    (argument
      (lambda_expression
        parameters: (parameter_list)
        body: (block) @thread.body)))) @concurrency.relationship.thread.creation

; 线程启动并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @thread.object
    name: (identifier) @thread.start.method)) @concurrency.relationship.thread.start

; 线程加入并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @thread.object
    name: (identifier) @thread.join.method)) @concurrency.relationship.thread.join

; lock语句并发关系
(lock_statement
  expression: (identifier) @lock.object
  body: (block) @lock.body) @concurrency.relationship.lock.statement

; Monitor.Enter并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @monitor.type
    name: (identifier) @monitor.enter.method)
  arguments: (argument_list
    (argument
      (identifier) @sync.object))) @concurrency.relationship.monitor.enter

; Monitor.Exit并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @monitor.type
    name: (identifier) @monitor.exit.method)
  arguments: (argument_list
    (argument
      (identifier) @sync.object))) @concurrency.relationship.monitor.exit

; Monitor.Wait并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @monitor.type
    name: (identifier) @monitor.wait.method)
  arguments: (argument_list
    (argument
      (identifier) @sync.object))) @concurrency.relationship.monitor.wait

; Monitor.Pulse并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @monitor.type
    name: (identifier) @monitor.pulse.method)
  arguments: (argument_list
    (argument
      (identifier) @sync.object))) @concurrency.relationship.monitor.pulse

; Mutex创建并发关系
(object_creation_expression
  type: (identifier) @mutex.type
  arguments: (argument_list)) @concurrency.relationship.mutex.creation

; Mutex等待并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @mutex.object
    name: (identifier) @mutex.wait.one.method)
  arguments: (argument_list)) @concurrency.relationship.mutex.wait

; Mutex释放并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @mutex.object
    name: (identifier) @mutex.release.method)
  arguments: (argument_list)) @concurrency.relationship.mutex.release

; Semaphore创建并发关系
(object_creation_expression
  type: (identifier) @semaphore.type
  arguments: (argument_list
    (argument
      (integer_literal) @initial.count)
    (argument
      (integer_literal) @max.count))) @concurrency.relationship.semaphore.creation

; Semaphore等待并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @semaphore.object
    name: (identifier) @semaphore.wait.method)) @concurrency.relationship.semaphore.wait

; Semaphore释放并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @semaphore.object
    name: (identifier) @semaphore.release.method)) @concurrency.relationship.semaphore.release

; SemaphoreSlim并发关系
(object_creation_expression
  type: (identifier) @semaphore.slim.type
  arguments: (argument_list
    (argument
      (integer_literal) @initial.count))) @concurrency.relationship.semaphore.slim

; ReaderWriterLock并发关系
(object_creation_expression
  type: (identifier) @reader.writer.lock.type) @concurrency.relationship.reader.writer.lock

; ReaderWriterLock读锁并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @rw.lock.object
    name: (identifier) @rw.lock.acquire.read.method)) @concurrency.relationship.read.lock

; ReaderWriterLock写锁并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @rw.lock.object
    name: (identifier) @rw.lock.acquire.write.method)) @concurrency.relationship.write.lock

; ReaderWriterLock释放锁并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @rw.lock.object
    name: (identifier) @rw.lock.release.method)) @concurrency.relationship.lock.release

; Barrier并发关系
(object_creation_expression
  type: (identifier) @barrier.type
  arguments: (argument_list
    (argument
      (identifier) @participants.count))) @concurrency.relationship.barrier

; Barrier同步并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @barrier.object
    name: (identifier) @barrier.signal.method)) @concurrency.relationship.barrier.signal

; CountDownLatch并发关系
(object_creation_expression
  type: (identifier) @countdown.event.type
  arguments: (argument_list
    (argument
      (integer_literal) @initial.count))) @concurrency.relationship.countdown.latch

; CountDownLatch等待并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @countdown.object
    name: (identifier) @countdown.wait.method)) @concurrency.relationship.countdown.wait

; CountDownLatch计数并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @countdown.object
    name: (identifier) @countdown.signal.method)) @concurrency.relationship.countdown.signal

; Concurrent集合并发关系
(object_creation_expression
  type: (identifier) @concurrent.collection.type) @concurrency.relationship.concurrent.collection

; ConcurrentDictionary访问并发关系
(member_access_expression
  expression: (identifier) @concurrent.dict.object
  name: (identifier) @concurrent.dict.method) @concurrency.relationship.concurrent.dict.access

; ConcurrentQueue操作并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @concurrent.queue.object
    name: (identifier) @concurrent.queue.method)
  arguments: (argument_list)) @concurrency.relationship.concurrent.queue.op

; ConcurrentStack操作并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @concurrent.stack.object
    name: (identifier) @concurrent.stack.method)
  arguments: (argument_list)) @concurrency.relationship.concurrent.stack.op

; ConcurrentBag操作并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @concurrent.bag.object
    name: (identifier) @concurrent.bag.method)
  arguments: (argument_list)) @concurrency.relationship.concurrent.bag.op

; Channel创建并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @channel.factory
    name: (identifier) @channel.create.method)) @concurrency.relationship.channel.creation

; Channel写入并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @channel.writer
    name: (identifier) @channel.write.method)
  arguments: (argument_list
    (argument
      (identifier) @message.to.write))) @concurrency.relationship.channel.write

; Channel读取并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @channel.reader
    name: (identifier) @channel.read.method)) @concurrency.relationship.channel.read

; Channel完成并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @channel.writer
    name: (identifier) @channel.complete.method)) @concurrency.relationship.channel.complete

; TaskCompletionSource并发关系
(object_creation_expression
  type: (identifier) @task.completion.source.type) @concurrency.relationship.task.completion.source

; TaskCompletionSource设置结果并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @tcs.object
    name: (identifier) @tcs.set.result.method)
  arguments: (argument_list
    (argument
      (identifier) @result.value))) @concurrency.relationship.tcs.set.result

; TaskCompletionSource设置异常并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @tcs.object
    name: (identifier) @tcs.set.exception.method)
  arguments: (argument_list
    (argument
      (identifier) @exception.to.set))) @concurrency.relationship.tcs.set.exception

; Parallel.Invoke并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @parallel.type
    name: (identifier) @parallel.invoke.method)
  arguments: (argument_list
    (argument
      (lambda_expression
        parameters: (parameter_list)
        body: (block) @parallel.action.body)))) @concurrency.relationship.parallel.invoke

; Parallel.For并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @parallel.type
    name: (identifier) @parallel.for.method)
  arguments: (argument_list
    (argument
      (identifier) @loop.start)
    (argument
      (identifier) @loop.end))) @concurrency.relationship.parallel.for

; Parallel.ForEach并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @parallel.type
    name: (identifier) @parallel.foreach.method)
  arguments: (argument_list
    (argument
      (identifier) @source.collection))) @concurrency.relationship.parallel.foreach

; PLINQ查询并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @enumerable.source
    name: (identifier) @plinq.extension.method)) @concurrency.relationship.plinq

; PLINQ并行处理并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @parallel.query
    name: (identifier) @parallel.processing.method)) @concurrency.relationship.plinq.processing

; CancellationToken并发关系
(object_creation_expression
  type: (identifier) @cancellation.token.source.type) @concurrency.relationship.cancellation.token.source

; CancellationToken取消并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @cts.object
    name: (identifier) @cts.cancel.method)) @concurrency.relationship.cancellation.cancel

; CancellationToken传递并发关系
(invocation_expression
  function: (identifier) @async.method.with.cancellation
  arguments: (argument_list
    (argument
      (identifier) @cancellation.token.argument))) @concurrency.relationship.cancellation.token.passing

; Interlocked操作并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @interlocked.type
    name: (identifier) @interlocked.method)
  arguments: (argument_list
    (argument
      (identifier) @target.variable))) @concurrency.relationship.interlocked.operation

; Interlocked CompareExchange并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @interlocked.type
    name: (identifier) @interlocked.compare.exchange.method)
  arguments: (argument_list
    (argument
      (identifier) @location1)
    (argument
      (identifier) @value)
    (argument
      (identifier) @comparand))) @concurrency.relationship.interlocked.compare.exchange

; Volatile读取并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @volatile.type
    name: (identifier) @volatile.read.method)
  arguments: (argument_list
    (argument
      (identifier) @volatile.field))) @concurrency.relationship.volatile.read

; Volatile写入并发关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @volatile.type
    name: (identifier) @volatile.write.method)
  arguments: (argument_list
    (argument
      (identifier) @volatile.field)
    (argument
      (identifier) @value.to.write))) @concurrency.relationship.volatile.write

; AsyncLocal并发关系
(object_creation_expression
  type: (identifier) @async.local.type) @concurrency.relationship.async.local

; AsyncLocal设置值并发关系
(assignment_expression
  left: (member_access_expression
    expression: (identifier) @async.local.object
    name: (identifier) @async.local.value.property)
  right: (identifier) @async.local.value) @concurrency.relationship.async.local.set

; Race condition detection - shared variable access without synchronization
(assignment_expression
  left: (identifier) @shared.variable
  right: (binary_expression
    left: (identifier) @shared.variable
    operator: (identifier) @read.modify.operator
    right: (identifier) @increment.value)) @concurrency.relationship.race.condition

; Potential deadlock - nested lock acquisitions
(lock_statement
  expression: (identifier) @first.lock
  body: (block
    (lock_statement
      expression: (identifier) @second.lock
      body: (block) @nested.lock.body))) @concurrency.relationship.potential.deadlock

; Double-checked locking pattern
(if_statement
  condition: (binary_expression
    left: (identifier) @double.checked.variable
    operator: "!="
    right: (null_literal))
  consequence: (block
    (lock_statement
      expression: (identifier) @sync.lock
      body: (block
        (if_statement
          condition: (binary_expression
            left: (identifier) @double.checked.variable
            operator: "=="
            right: (null_literal))
          consequence: (block) @double.checked.creation))))) @concurrency.relationship.double.checked.locking

; Producer-consumer pattern with concurrent collections
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @producer.thread
    name: (identifier) @producer.method)
  arguments: (argument_list
    (argument
      (identifier) @concurrent.collection))) @concurrency.relationship.producer

; Consumer pattern with concurrent collections
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @consumer.thread
    name: (identifier) @consumer.method)
  arguments: (argument_list
    (argument
      (identifier) @concurrent.collection))) @concurrency.relationship.consumer

; Actor pattern - message passing
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @actor.object
    name: (identifier) @actor.tell.method)
  arguments: (argument_list
    (argument
      (identifier) @message.to.send))) @concurrency.relationship.actor.message.passing

; Future/Promise pattern - Task continuation
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @task.object
    name: (identifier) @task.continue.with.method)
  arguments: (argument_list
    (argument
      (lambda_expression
        parameters: (parameter_list)
        body: (block) @continuation.body)))) @concurrency.relationship.task.continuation

; Async method with synchronization context
(method_declaration
  (modifier) @async.modifier
  name: (identifier) @async.method.name
  body: (block
    (await_expression
      (invocation_expression
        function: (identifier) @async.operation)))) @concurrency.relationship.async.with.context

; SemaphoreSlim WaitAsync concurrent relationship
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @semaphore.slim.object
    name: (identifier) @semaphore.wait.async.method)) @concurrency.relationship.semaphore.wait.async

; LockAsync pattern with SemaphoreSlim
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @semaphore.slim.object
    name: (identifier) @semaphore.wait.async.method)) @concurrency.relationship.lock.async

; AsyncLock implementation pattern
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @async.lock.object
    name: (identifier) @async.lock.acquire.method)) @concurrency.relationship.async.lock

; ThreadLocal storage concurrent relationship
(object_creation_expression
  type: (identifier) @thread.local.type
  arguments: (argument_list)) @concurrency.relationship.thread.local

; ThreadLocal value access concurrent relationship
(member_access_expression
  expression: (identifier) @thread.local.object
  name: (identifier) @thread.local.value.property) @concurrency.relationship.thread.local.value
`;