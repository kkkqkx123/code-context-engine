# C++ Concurrency Relationships Tree-Sitter查询规则测试用例

本文档为C++ Concurrency Relationships的Tree-Sitter查询规则提供测试用例，每个查询规则后都附有相应的测试代码示例，方便在TreeSitter Playground中进行验证。

## 1. 线程创建

### 测试用例
```cpp
#include <thread>
#include <iostream>

void workerFunction(int id) {
    std::cout << "Worker " << id << " is running" << std::endl;
}

int main() {
    std::thread t1(workerFunction, 1);
    std::thread t2(workerFunction, 2);
    
    t1.join();
    t2.join();
    
    return 0;
}
```

### 查询规则
```
(declaration
  type: (qualified_identifier
    scope: (namespace_identifier) @std.scope
    name: (type_identifier) @thread.constructor)) @concurrency.relationship.thread.creation
```

## 2. 线程加入

### 查询规则
```
(call_expression
  function: (field_expression
    (identifier) @thread.object
    (field_identifier) @thread.method)@concurrency.relationship.thread.join
  (#eq? @thread.method "join"))
```

### 测试用例
```cpp
#include <thread>
#include <iostream>

void task() {
    std::cout << "Task is running" << std::endl;
}

int main() {
    std::thread t(task);
    t.join();  // 等待线程完成
    return 0;
}
```

## 3. 线程分离

### 查询规则
```
(call_expression
  function: (field_expression
    (identifier) @thread.object
    (field_identifier) @thread.method)@concurrency.relationship.thread.detach
  (#eq? @thread.method "detach"))
```

### 测试用例
```cpp
#include <thread>
#include <iostream>

void backgroundTask() {
    std::cout << "Background task running" << std::endl;
}

int main() {
    std::thread t(backgroundTask);
    t.detach();  // 分离线程，让其独立运行
    return 0;
}
```

## 4. 互斥锁锁定

### 查询规则
```
(call_expression
  function: (field_expression
    (identifier) @mutex.object
    (field_identifier) @mutex.method)@concurrency.relationship.mutex.lock
  (#eq? @mutex.method "lock"))
```

### 测试用例
```cpp
#include <mutex>
#include <thread>

std::mutex mtx;
int sharedData = 0;

void incrementData() {
    mtx.lock();
    sharedData++;
    mtx.unlock();
}

int main() {
    std::thread t1(incrementData);
    std::thread t2(incrementData);
    
    t1.join();
    t2.join();
    
    return 0;
}
```

## 5. 互斥锁解锁

### 查询规则
```
(call_expression
  function: (field_expression
    (identifier) @mutex.object
    (field_identifier) @mutex.method)@concurrency.relationship.mutex.unlock
  (#eq? @mutex.method "unlock"))
```

### 测试用例
```cpp
#include <mutex>
#include <thread>

std::mutex mtx;
int sharedResource = 0;

void accessResource() {
    mtx.lock();
    sharedResource += 10;
    mtx.unlock();
}

int main() {
    std::thread t1(accessResource);
    std::thread t2(accessResource);
    
    t1.join();
    t2.join();
    
    return 0;
}
```

## 6. 互斥锁尝试锁定

### 查询规则
```
(call_expression
  function: (field_expression
    (identifier) @mutex.object
    (field_identifier) @mutex.method)@concurrency.relationship.mutex.try_lock
  (#eq? @mutex.method "try_lock"))
```

### 测试用例
```cpp
#include <mutex>
#include <iostream>

std::mutex mtx;

void tryLockExample() {
    if (mtx.try_lock()) {
        std::cout << "Lock acquired successfully" << std::endl;
        mtx.unlock();
    } else {
        std::cout << "Failed to acquire lock" << std::endl;
    }
}

int main() {
    std::thread t1(tryLockExample);
    std::thread t2(tryLockExample);
    
    t1.join();
    t2.join();
    
    return 0;
}
```

## 7. 锁守卫（RAII锁）

### 查询规则
```
(declaration
  type: (qualified_identifier
    scope: (identifier) @std.scope
    name: (type_identifier) @lock.guard.type)
 declarator: (init_declarator
    declarator: (identifier) @lock.guard.variable
    value: (call_expression
      function: (identifier) @locked.mutex)))
  (#match? @lock.guard.type "^(lock_guard|unique_lock|shared_lock)$")) @concurrency.relationship.lock.guard
```

### 测试用例
```cpp
#include <mutex>
#include <iostream>

std::mutex mtx;
int sharedData = 0;

void safeIncrement() {
    std::lock_guard<std::mutex> lock(mtx);
    sharedData++;
    std::cout << "Data incremented to: " << sharedData << std::endl;
}

int main() {
    std::thread t1(safeIncrement);
    std::thread t2(safeIncrement);
    
    t1.join();
    t2.join();
    
    return 0;
}
```

## 8. 条件变量等待

### 查询规则
```
(call_expression
  function: (field_expression
    (identifier) @condition.variable
    (field_identifier) @condition.method)
  arguments: (argument_list
    (identifier) @locked.mutex))
  (#match? @condition.method "wait")) @concurrency.relationship.condition.wait
```

### 测试用例
```cpp
#include <mutex>
#include <condition_variable>
#include <thread>
#include <iostream>

std::mutex mtx;
std::condition_variable cv;
bool ready = false;

void worker() {
    std::unique_lock<std::mutex> lock(mtx);
    cv.wait(lock, []{ return ready; });
    std::cout << "Worker thread proceeding" << std::endl;
}

int main() {
    std::thread t(worker);
    
    {
        std::lock_guard<std::mutex> lock(mtx);
        ready = true;
    }
    cv.notify_one();
    
    t.join();
    return 0;
}
```

## 9. 条件变量通知

### 查询规则
```
(call_expression
  function: (field_expression
    (identifier) @condition.variable
    (field_identifier) @condition.method))
  (#match? @condition.method "^(notify_one|notify_all)$")) @concurrency.relationship.condition.notify
```

### 测试用例
```cpp
#include <mutex>
#include <condition_variable>
#include <thread>
#include <iostream>

std::mutex mtx;
std::condition_variable cv;
bool dataReady = false;

void waitForData() {
    std::unique_lock<std::mutex> lock(mtx);
    cv.wait(lock, []{ return dataReady; });
    std::cout << "Data received, processing..." << std::endl;
}

void prepareData() {
    {
        std::lock_guard<std::mutex> lock(mtx);
        dataReady = true;
    }
    cv.notify_one();
    std::cout << "Data prepared, notification sent" << std::endl;
}

int main() {
    std::thread t1(waitForData);
    std::thread t2(prepareData);
    
    t1.join();
    t2.join();
    
    return 0;
}
```

## 10. 原子操作

### 查询规则
```
(call_expression
  function: (field_expression
    (identifier) @atomic.variable
    (field_identifier) @atomic.method))
  (#match? @atomic.method "^(store|load|exchange|fetch_add|fetch_sub|fetch_and|fetch_or|fetch_xor)$")) @concurrency.relationship.atomic.operation
```

### 测试用例
```cpp
#include <atomic>
#include <thread>
#include <iostream>

std::atomic<int> counter(0);

void incrementCounter() {
    counter.fetch_add(1);
}

void readCounter() {
    int value = counter.load();
    std::cout << "Counter value: " << value << std::endl;
}

int main() {
    std::thread t1(incrementCounter);
    std::thread t2(incrementCounter);
    std::thread t3(readCounter);
    
    t1.join();
    t2.join();
    t3.join();
    
    return 0;
}
```

## 11. 原子比较交换

### 查询规则
```
(call_expression
 function: (field_expression
    (identifier) @atomic.variable
    (field_identifier) @atomic.method)
  arguments: (argument_list
    (identifier) @expected.value
    (identifier) @desired.value))
  (#eq? @atomic.method "compare_exchange") @concurrency.relationship.atomic.compare_exchange
```

### 测试用例
```cpp
#include <atomic>
#include <iostream>

std::atomic<int> value(0);

void compareExchangeExample() {
    int expected = 0;
    int desired = 42;
    
    if (value.compare_exchange(expected, desired)) {
        std::cout << "Exchange successful" << std::endl;
    } else {
        std::cout << "Exchange failed, current value: " << expected << std::endl;
    }
}

int main() {
    compareExchangeExample();
    return 0;
}
```

## 12. 原子标志

### 查询规则
```
(call_expression
  function: (field_expression
    (identifier) @atomic.flag
    (field_identifier) @flag.method))
  (#match? @flag.method "^(test_and_set|clear)$")) @concurrency.relationship.atomic.flag
```

### 测试用例
```cpp
#include <atomic>
#include <thread>
#include <iostream>

std::atomic_flag flag = ATOMIC_FLAG_INIT;

void trySetFlag() {
    if (!flag.test_and_set()) {
        std::cout << "Flag was unset, now set" << std::endl;
    } else {
        std::cout << "Flag was already set" << std::endl;
    }
}

void clearFlag() {
    flag.clear();
    std::cout << "Flag cleared" << std::endl;
}

int main() {
    std::thread t1(trySetFlag);
    std::thread t2(trySetFlag);
    std::thread t3(clearFlag);
    
    t1.join();
    t2.join();
    t3.join();
    
    return 0;
}
```

## 13. 信号量获取（C++20）

### 查询规则
```
(call_expression
  function: (field_expression
    (identifier) @semaphore.object
    (field_identifier) @semaphore.method))
  (#match? @semaphore.method "acquire")) @concurrency.relationship.semaphore.acquire
```

### 测试用例
```cpp
#include <semaphore>
#include <thread>
#include <iostream>

std::counting_semaphore<3> sem(3);

void acquireSemaphore() {
    sem.acquire();
    std::cout << "Semaphore acquired" << std::endl;
    // 模拟工作
    sem.release();
}
```

## 14. 信号量释放（C++20）

### 查询规则
```
(call_expression
  function: (field_expression
    (identifier) @semaphore.object
    (field_identifier) @semaphore.method)
  arguments: (argument_list))
  (#match? @semaphore.method "release")) @concurrency.relationship.semaphore.release
```

### 测试用例
```cpp
#include <semaphore>
#include <thread>
#include <iostream>

std::counting_semaphore<1> sem(0);

void waitForSemaphore() {
    sem.acquire();
    std::cout << "Resource accessed" << std::endl;
}

void releaseSemaphore() {
    sem.release();
    std::cout << "Resource released" << std::endl;
}

int main() {
    std::thread t1(waitForSemaphore);
    std::thread t2(releaseSemaphore);
    
    t1.join();
    t2.join();
    
    return 0;
}
```

## 15. 闩器等待（C++20）

### 查询规则
```
(call_expression
  function: (field_expression
    (identifier) @latch.object
    (field_identifier) @latch.method)
 arguments: (argument_list))
  (#match? @latch.method "wait")) @concurrency.relationship.latch.wait
```

### 测试用例
```cpp
#include <latch>
#include <thread>
#include <iostream>

std::latch completionLatch(3);

void workerTask(int id) {
    std::cout << "Worker " << id << " completed task" << std::endl;
    completionLatch.count_down();
}

void waitForCompletion() {
    completionLatch.wait();
    std::cout << "All workers completed" << std::endl;
}

int main() {
    std::thread t1(workerTask, 1);
    std::thread t2(workerTask, 2);
    std::thread t3(workerTask, 3);
    std::thread t4(waitForCompletion);
    
    t1.join();
    t2.join();
    t3.join();
    t4.join();
    
    return 0;
}
```

## 16. 闩器倒计时（C++20）

### 查询规则
```
(call_expression
  function: (field_expression
    (identifier) @latch.object
    (field_identifier) @latch.method)
  arguments: (argument_list))
  (#match? @latch.method "count_down")) @concurrency.relationship.latch.count_down
```

### 测试用例
```cpp
#include <latch>
#include <thread>
#include <iostream>

std::latch startLatch(1);

void waitForStart() {
    startLatch.wait();
    std::cout << "Thread started" << std::endl;
}

void startAllThreads() {
    std::cout << "Starting all threads" << std::endl;
    startLatch.count_down();
}

int main() {
    std::thread t1(waitForStart);
    std::thread t2(waitForStart);
    std::thread t3(startAllThreads);
    
    t1.join();
    t2.join();
    t3.join();
    
    return 0;
}
```

## 17. 屏障同步（C++20）

### 查询规则
```
(call_expression
  function: (field_expression
    (identifier) @barrier.object
    (field_identifier) @barrier.method)
  arguments: (argument_list))
  (#match? @barrier.method "arrive_and_wait")) @concurrency.relationship.barrier.sync
```

### 测试用例
```cpp
#include <barrier>
#include <thread>
#include <iostream>

std::barrier syncPoint(3);

void synchronizedTask(int id) {
    std::cout << "Thread " << id << " reached barrier" << std::endl;
    syncPoint.arrive_and_wait();
    std::cout << "Thread " << id << " passed barrier" << std::endl;
}

int main() {
    std::thread t1(synchronizedTask, 1);
    std::thread t2(synchronizedTask, 2);
    std::thread t3(synchronizedTask, 3);
    
    t1.join();
    t2.join();
    t3.join();
    
    return 0;
}
```

## 18. 共享互斥锁（读写锁）

### 查询规则
```
(call_expression
  function: (field_expression
    (identifier) @shared.mutex
    (field_identifier) @shared.method))
  (#match? @shared.method "^(lock_shared|unlock_shared)$")) @concurrency.relationship.shared.mutex
```

### 测试用例
```cpp
#include <shared_mutex>
#include <thread>
#include <iostream>

std::shared_mutex rwMutex;
int sharedData = 0;

void reader() {
    rwMutex.lock_shared();
    std::cout << "Reading data: " << sharedData << std::endl;
    rwMutex.unlock_shared();
}

void writer() {
    rwMutex.lock();
    sharedData++;
    std::cout << "Writing data: " << sharedData << std::endl;
    rwMutex.unlock();
}

int main() {
    std::thread t1(reader);
    std::thread t2(reader);
    std::thread t3(writer);
    
    t1.join();
    t2.join();
    t3.join();
    
    return 0;
}
```

## 19. 异步任务创建

### 查询规则
```
(call_expression
  function: (qualified_identifier
    scope: (identifier) @std.scope
    name: (identifier) @async.function)
  arguments: (argument_list
    (identifier) @async.task))
  (#match? @async.function "async")) @concurrency.relationship.async.task
```

### 测试用例
```cpp
#include <future>
#include <iostream>

int asyncTask() {
    std::cout << "Async task running" << std::endl;
    return 42;
}

int main() {
    auto future = std::async(asyncTask);
    
    std::cout << "Main thread continues" << std::endl;
    
    int result = future.get();
    std::cout << "Async task result: " << result << std::endl;
    
    return 0;
}
```

## 20. 异步任务等待

### 查询规则
```
(call_expression
  function: (field_expression
    (identifier) @future.object
    (field_identifier) @future.method))
  (#match? @future.method "wait")) @concurrency.relationship.future.wait
```

### 测试用例
```cpp
#include <future>
#include <iostream>
#include <chrono>

void longRunningTask() {
    std::this_thread::sleep_for(std::chrono::seconds(2));
    std::cout << "Long task completed" << std::endl;
}

int main() {
    auto future = std::async(std::launch::async, longRunningTask);
    
    std::cout << "Waiting for task to complete..." << std::endl;
    future.wait();
    std::cout << "Task finished" << std::endl;
    
    return 0;
}
```

## 21. 异步任务获取结果

### 查询规则
```
(call_expression
 function: (field_expression
    (identifier) @future.object
    (field_identifier) @future.method))
  (#eq? @future.method "get") @concurrency.relationship.future.get
```

### 测试用例
```cpp
#include <future>
#include <iostream>

int computeResult() {
    return 100;
}

int main() {
    auto future = std::async(std::launch::async, computeResult);
    
    std::cout << "Doing other work..." << std::endl;
    
    int result = future.get();
    std::cout << "Computed result: " << result << std::endl;
    
    return 0;
}
```

## 22. 共享异步任务获取结果

### 查询规则
```
(call_expression
  function: (field_expression
    (identifier) @shared.future.object
    (field_identifier) @shared.future.method))
  (#eq? @shared.future.method "get") @concurrency.relationship.shared.future.get
```

### 测试用例
```cpp
#include <future>
#include <iostream>
#include <thread>

int sharedTask() {
    std::cout << "Shared task executing" << std::endl;
    return 200;
}

int main() {
    std::shared_future<int> sharedFuture = std::async(sharedTask);
    
    std::thread t1([&]() {
        int result = sharedFuture.get();
        std::cout << "Thread 1 got result: " << result << std::endl;
    });
    
    std::thread t2([&]() {
        int result = sharedFuture.get();
        std::cout << "Thread 2 got result: " << result << std::endl;
    });
    
    t1.join();
    t2.join();
    
    return 0;
}
```

## 23. 承诺设置值

### 查询规则
```
(call_expression
 function: (field_expression
    (identifier) @promise.object
    (field_identifier) @promise.method)
  arguments: (argument_list
    (identifier) @promise.value))
  (#eq? @promise.method "set_value") @concurrency.relationship.promise.set_value
```

### 测试用例
```cpp
#include <future>
#include <iostream>
#include <thread>

void setValueTask(std::promise<int> prom) {
    std::cout << "Setting promise value" << std::endl;
    prom.set_value(300);
}

int main() {
    std::promise<int> prom;
    std::future<int> fut = prom.get_future();
    
    std::thread t(setValueTask, std::move(prom));
    
    std::cout << "Waiting for promise value" << std::endl;
    int value = fut.get();
    std::cout << "Promise value: " << value << std::endl;
    
    t.join();
    return 0;
}
```

## 24. 打包任务

### 查询规则
```
(call_expression
 function: (qualified_identifier
    scope: (identifier) @std.scope
    name: (identifier) @packaged.task)
  arguments: (argument_list
    (identifier) @task.function))
  (#match? @packaged.task "packaged_task")) @concurrency.relationship.packaged.task
```

### 测试用例
```cpp
#include <future>
#include <iostream>
#include <functional>

int multiply(int a, int b) {
    return a * b;
}

int main() {
    std::packaged_task<int(int, int)> task(multiply);
    std::future<int> result = task.get_future();
    
    std::thread t(std::move(task), 6, 7);
    
    std::cout << "Waiting for packaged task result" << std::endl;
    int value = result.get();
    std::cout << "Packaged task result: " << value << std::endl;
    
    t.join();
    return 0;
}
```

## 25. 线程本地存储

### 查询规则
```
(declaration
  (storage_class_specifier) @thread.local.specifier
  declarator: (init_declarator
    declarator: (identifier) @thread.local.variable))
  (#match? @thread.local.specifier "thread_local")) @concurrency.relationship.thread.local
```

### 测试用例
```cpp
#include <iostream>
#include <thread>

thread_local int threadLocalVar = 0;

void threadFunction(int id) {
    threadLocalVar = id;
    std::cout << "Thread " << id << " threadLocalVar = " << threadLocalVar << std::endl;
}

int main() {
    std::thread t1(threadFunction, 1);
    std::thread t2(threadFunction, 2);
    
    t1.join();
    t2.join();
    
    std::cout << "Main thread threadLocalVar = " << threadLocalVar << std::endl;
    
    return 0;
}
```

## 26. 内存屏障

### 查询规则
```
(call_expression
  function: (qualified_identifier
    scope: (identifier) @std.scope
    name: (identifier) @atomic.function)
  arguments: (argument_list
    (identifier) @memory.order))
  (#match? @atomic.function "atomic_thread_fence")) @concurrency.relationship.memory.fence
```

### 测试用例
```cpp
#include <atomic>
#include <thread>
#include <iostream>

std::atomic<bool> flag(false);
int data = 0;

void writer() {
    data = 42;
    std::atomic_thread_fence(std::memory_order_release);
    flag.store(true, std::memory_order_relaxed);
}

void reader() {
    while (!flag.load(std::memory_order_relaxed)) {
        // 等待
    }
    std::atomic_thread_fence(std::memory_order_acquire);
    std::cout << "Data read: " << data << std::endl;
}

int main() {
    std::thread t1(writer);
    std::thread t2(reader);
    
    t1.join();
    t2.join();
    
    return 0;
}
```

## 27. 竞态条件检测

### 查询规则
```
(assignment_expression
  left: (field_expression
    (identifier) @shared.variable
    (field_identifier) @shared.field)
  right: (binary_expression
    left: (field_expression
      (identifier) @shared.variable
      (field_identifier) @shared.field)
    operator: (identifier) @operator
    right: (identifier) @increment.value)) @concurrency.relationship.race.condition
```

### 测试用例
```cpp
#include <thread>
#include <iostream>

struct Counter {
    int value;
};

void incrementCounter(Counter* counter) {
    // 这是一个潜在的竞态条件
    counter->value = counter->value + 1;
}

int main() {
    Counter counter{0};
    
    std::thread t1(incrementCounter, &counter);
    std::thread t2(incrementCounter, &counter);
    
    t1.join();
    t2.join();
    
    std::cout << "Final counter value: " << counter.value << std::endl;
    
    return 0;
}
```

## 28. 死锁模式（多个锁获取）

### 查询规则
```
(call_expression
  function: (field_expression
    (identifier) @first.lock
    (field_identifier) @lock.method)
  arguments: (argument_list))
  (#eq? @lock.method "lock") @concurrency.relationship.deadlock.pattern
```

### 测试用例
```cpp
#include <mutex>
#include <thread>
#include <iostream>

std::mutex mutex1, mutex2;

void thread1() {
    mutex1.lock();
    std::cout << "Thread 1 acquired mutex1" << std::endl;
    // 模拟一些工作
    mutex2.lock();
    std::cout << "Thread 1 acquired mutex2" << std::endl;
    
    mutex2.unlock();
    mutex1.unlock();
}

void thread2() {
    mutex2.lock();
    std::cout << "Thread 2 acquired mutex2" << std::endl;
    // 模拟一些工作
    mutex1.lock();
    std::cout << "Thread 2 acquired mutex1" << std::endl;
    
    mutex1.unlock();
    mutex2.unlock();
}

int main() {
    std::thread t1(thread1);
    std::thread t2(thread2);
    
    t1.join();
    t2.join();
    
    return 0;
}
```

## 29. 生产者-消费者模式

### 查询规则
```
(class_specifier
  name: (type_identifier) @producer.consumer.class
 body: (field_declaration_list
    (field_declaration
      declarator: (field_declarator
        declarator: (field_identifier) @queue.field))
    (field_declaration
      declarator: (field_declarator
        declarator: (field_identifier) @mutex.field))
    (field_declaration
      declarator: (field_declarator
        declarator: (field_identifier) @condition.field)))) @concurrency.relationship.producer.consumer
```

### 测试用例
```cpp
#include <queue>
#include <mutex>
#include <condition_variable>
#include <thread>
#include <iostream>

template<typename T>
class ProducerConsumerQueue {
private:
    std::queue<T> queue;
    std::mutex mtx;
    std::condition_variable cv;
    
public:
    void push(const T& item) {
        std::lock_guard<std::mutex> lock(mtx);
        queue.push(item);
        cv.notify_one();
    }
    
    T pop() {
        std::unique_lock<std::mutex> lock(mtx);
        cv.wait(lock, [this]{ return !queue.empty(); });
        T item = queue.front();
        queue.pop();
        return item;
    }
};

int main() {
    ProducerConsumerQueue<int> pcQueue;
    
    auto producer = [&]() {
        for (int i = 0; i < 5; ++i) {
            pcQueue.push(i);
            std::cout << "Produced: " << i << std::endl;
        }
    };
    
    auto consumer = [&]() {
        for (int i = 0; i < 5; ++i) {
            int item = pcQueue.pop();
            std::cout << "Consumed: " << item << std::endl;
        }
    };
    
    std::thread t1(producer);
    std::thread t2(consumer);
    
    t1.join();
    t2.join();
    
    return 0;
}
```

## 30. 读写锁模式

### 查询规则
```
(class_specifier
  name: (type_identifier) @read.write.lock.class
  body: (field_declaration_list
    (function_definition
      declarator: (function_declarator
        declarator: (field_identifier) @read.lock.method))
    (function_definition
      declarator: (function_declarator
        declarator: (field_identifier) @write.lock.method)))) @concurrency.relationship.read.write.lock
```

### 测试用例
```cpp
#include <shared_mutex>
#include <iostream>

class ReadWriteLock {
private:
    std::shared_mutex rwMutex;
    
public:
    void readLock() {
        rwMutex.lock_shared();
        std::cout << "Read lock acquired" << std::endl;
    }
    
    void readUnlock() {
        rwMutex.unlock_shared();
        std::cout << "Read lock released" << std::endl;
    }
    
    void writeLock() {
        rwMutex.lock();
        std::cout << "Write lock acquired" << std::endl;
    }
    
    void writeUnlock() {
        rwMutex.unlock();
        std::cout << "Write lock released" << std::endl;
    }
};

int main() {
    ReadWriteLock rwLock;
    
    rwLock.readLock();
    rwLock.readUnlock();
    
    rwLock.writeLock();
    rwLock.writeUnlock();
    
    return 0;
}
```

## 31. 线程池模式

### 查询规则
```
(class_specifier
  name: (type_identifier) @thread.pool.class
  body: (field_declaration_list
    (field_declaration
      declarator: (field_declarator
        declarator: (field_identifier) @workers.field))
    (field_declaration
      declarator: (field_declarator
        declarator: (field_identifier) @tasks.field))
    (field_declaration
      declarator: (field_declarator
        declarator: (field_identifier) @queue.mutex)))) @concurrency.relationship.thread.pool
```

### 测试用例
```cpp
#include <vector>
#include <thread>
#include <queue>
#include <mutex>
#include <functional>
#include <iostream>

class ThreadPool {
private:
    std::vector<std::thread> workers;
    std::queue<std::function<void()>> tasks;
    std::mutex queueMutex;
    
public:
    ThreadPool(size_t threads) {
        for (size_t i = 0; i < threads; ++i) {
            workers.emplace_back([this] {
                while (true) {
                    std::function<void()> task;
                    {
                        std::unique_lock<std::mutex> lock(queueMutex);
                        if (tasks.empty()) {
                            break;
                        }
                        task = tasks.front();
                        tasks.pop();
                    }
                    task();
                }
            });
        }
    }
    
    ~ThreadPool() {
        for (std::thread &worker : workers) {
            worker.join();
        }
    }
    
    void enqueue(std::function<void()> task) {
        {
            std::unique_lock<std::mutex> lock(queueMutex);
            tasks.push(task);
        }
    }
};

int main() {
    ThreadPool pool(4);
    
    for (int i = 0; i < 8; ++i) {
        pool.enqueue([i] {
            std::cout << "Task " << i << " executed" << std::endl;
        });
    }
    
    return 0;
}
```

## 32. 并行算法

### 查询规则
```
(call_expression
  function: (qualified_identifier
    scope: (identifier) @std.scope
    name: (identifier) @parallel.algorithm)
  arguments: (argument_list
    (identifier) @execution.policy
    (identifier) @algorithm.args))
  (#eq? @parallel.algorithm "for_each") @concurrency.relationship.parallel.algorithm
(call_expression
  function: (qualified_identifier
    scope: (identifier) @std.scope
    name: (identifier) @parallel.algorithm)
  arguments: (argument_list
    (identifier) @execution.policy
    (identifier) @algorithm.args))
  (#eq? @parallel.algorithm "sort") @concurrency.relationship.parallel.algorithm
(call_expression
  function: (qualified_identifier
    scope: (identifier) @std.scope
    name: (identifier) @parallel.algorithm)
  arguments: (argument_list
    (identifier) @execution.policy
    (identifier) @algorithm.args))
  (#eq? @parallel.algorithm "transform") @concurrency.relationship.parallel.algorithm
(call_expression
  function: (qualified_identifier
    scope: (identifier) @std.scope
    name: (identifier) @parallel.algorithm)
  arguments: (argument_list
    (identifier) @execution.policy
    (identifier) @algorithm.args))
  (#eq? @parallel.algorithm "reduce") @concurrency.relationship.parallel.algorithm
```

### 测试用例
```cpp
#include <vector>
#include <algorithm>
#include <execution>
#include <iostream>

int main() {
    std::vector<int> numbers = {5, 2, 8, 1, 9, 3};
    
    // 并行排序
    std::sort(std::execution::par, numbers.begin(), numbers.end());
    
    std::cout << "Sorted numbers: ";
    for (int num : numbers) {
        std::cout << num << " ";
    }
    std::cout << std::endl;
    
    // 并行for_each
    std::for_each(std::execution::par, numbers.begin(), numbers.end(), 
                  [](int& n) { n *= 2; });
    
    std::cout << "Doubled numbers: ";
    for (int num : numbers) {
        std::cout << num << " ";
    }
    std::cout << std::endl;
    
    return 0;
}
```

## 33. 执行策略

### 查询规则
```
(call_expression
  function: (qualified_identifier
    scope: (identifier) @execution.scope
    name: (identifier) @execution.policy))
  (#eq? @execution.policy "par") @concurrency.relationship.execution.policy
(call_expression
  function: (qualified_identifier
    scope: (identifier) @execution.scope
    name: (identifier) @execution.policy))
  (#eq? @execution.policy "par_unseq") @concurrency.relationship.execution.policy
(call_expression
  function: (qualified_identifier
    scope: (identifier) @execution.scope
    name: (identifier) @execution.policy))
  (#eq? @execution.policy "seq") @concurrency.relationship.execution.policy
(call_expression
  function: (qualified_identifier
    scope: (identifier) @execution.scope
    name: (identifier) @execution.policy))
  (#eq? @execution.policy "unseq") @concurrency.relationship.execution.policy
```

### 测试用例
```cpp
#include <vector>
#include <algorithm>
#include <execution>
#include <iostream>

int main() {
    std::vector<int> data = {1, 2, 3, 4, 5};
    
    // 使用并行执行策略
    auto policy = std::execution::par;
    
    std::for_each(policy, data.begin(), data.end(), 
                  [](int& x) { x *= x; });
    
    std::cout << "Squared values: ";
    for (int val : data) {
        std::cout << val << " ";
    }
    std::cout << std::endl;
    
    return 0;
}
```

## 34. 协程创建（C++20）

### 查询规则
```
(function_definition
 declarator: (function_declarator
    declarator: (identifier) @coroutine.function)
  return_type: (type_identifier) @coroutine.return.type)
  (#eq? @coroutine.return.type "task") @concurrency.relationship.coroutine
(function_definition
 declarator: (function_declarator
    declarator: (identifier) @coroutine.function)
  return_type: (type_identifier) @coroutine.return.type)
  (#eq? @coroutine.return.type "generator") @concurrency.relationship.coroutine
(function_definition
 declarator: (function_declarator
    declarator: (identifier) @coroutine.function)
  return_type: (type_identifier) @coroutine.return.type)
  (#eq? @coroutine.return.type "lazy") @concurrency.relationship.coroutine
```

### 测试用例
```cpp
#include <coroutine>
#include <iostream>

// 简化的协程返回类型
struct Task {
    struct promise_type {
        Task get_return_object() {
            return Task{std::coroutine_handle<promise_type>::from_promise(*this)};
        }
        std::suspend_never initial_suspend() { return {}; }
        std::suspend_never final_suspend() noexcept { return {}; }
        void return_void() {}
        void unhandled_exception() {}
    };
    
    std::coroutine_handle<promise_type> h;
    Task(std::coroutine_handle<promise_type> handle) : h(handle) {}
    ~Task() { if (h) h.destroy(); }
};

Task coroutineFunction() {
    std::cout << "Coroutine started" << std::endl;
    co_return;
}

int main() {
    auto task = coroutineFunction();
    return 0;
}
```

## 35. 协程等待（C++20）

### 查询规则
```
(await_expression
 (call_expression) @awaited.call) @concurrency.relationship.coroutine.await
```

### 测试用例
```cpp
#include <coroutine>
#include <iostream>

struct Awaiter {
    bool await_ready() { return false; }
    void await_suspend(std::coroutine_handle<>) {
        std::cout << "Coroutine suspended" << std::endl;
    }
    void await_resume() {
        std::cout << "Coroutine resumed" << std::endl;
    }
};

struct Task {
    struct promise_type {
        Task get_return_object() {
            return Task{std::coroutine_handle<promise_type>::from_promise(*this)};
        }
        std::suspend_never initial_suspend() { return {}; }
        std::suspend_never final_suspend() noexcept { return {}; }
        void return_void() {}
        void unhandled_exception() {}
    };
    
    std::coroutine_handle<promise_type> h;
    Task(std::coroutine_handle<promise_type> handle) : h(handle) {}
    ~Task() { if (h) h.destroy(); }
};

Task coroutineWithAwait() {
    std::cout << "Before await" << std::endl;
    co_await Awaiter{};
    std::cout << "After await" << std::endl;
}

int main() {
    auto task = coroutineWithAwait();
    return 0;
}
```
