# C语言并发关系Tree-Sitter查询规则测试用例

本文档为C语言并发关系的Tree-Sitter查询规则提供测试用例，每个查询规则后都附有相应的测试代码示例，方便在Tree-Sitter Playground中进行验证。

## 1. 线程创建并发关系

### 测试用例
```c
#include <pthread.h>
#include <windows.h>

void* thread_function(void* arg) {
    return NULL;
}

int main() {
    pthread_t thread;
    pthread_attr_t attr;
    
    // pthread_create测试
    pthread_create(&thread, &attr, thread_function, NULL);
    
    // CreateThread测试
    HANDLE hThread = CreateThread(
        NULL,                   // Security attributes
        0,                      // Stack size
        (LPTHREAD_START_ROUTINE)thread_function,  // Start function
        NULL,                   // Argument
        0,                      // Creation flags
        NULL                    // Thread ID
    );
    
    // _beginthreadex测试
    HANDLE hThread2 = (HANDLE)_beginthreadex(
        NULL,                   // Security
        0,                      // Stack size
        (LPTHREAD_START_ROUTINE)thread_function,  // Start function
        NULL,                   // Argument
        0,                      // Init flag
        NULL                    // Thread address
    );
    
    return 0;
}
```

### 查询规则
```
(call_expression) @concurrency.relationship.thread.creation
  function: (identifier) @thread.create.function
  (#match? @thread.create.function "^(pthread_create|CreateThread|_beginthreadex)$")
  arguments: (argument_list
    (identifier) @thread.handle
    (identifier) @thread.attributes
    (identifier) @thread.start.function
    (identifier) @thread.argument)
```



## 2. 线程等待并发关系

### 查询规则
```
(call_expression) @concurrency.relationship.thread.wait
  function: (identifier) @thread.wait.function
  (#match? @thread.wait.function "^(pthread_join)$")
  arguments: (argument_list
    (identifier) @thread.handle
    (identifier)? @thread.return.value)
```

### 测试用例
```c
#include <pthread.h>

void* thread_function(void* arg) {
    return NULL;
}

int main() {
    pthread_t thread;
    void* result;
    
    pthread_create(&thread, NULL, thread_function, NULL);
    
    // pthread_join测试
    pthread_join(thread, &result);
    
    return 0;
}
```

## 3. 线程分离并发关系

### 查询规则
```
(call_expression) @concurrency.relationship.thread.detach
  function: (identifier) @thread.detach.function
  (#match? @thread.detach.function "^(pthread_detach)$")
  arguments: (argument_list
    (identifier) @thread.handle)
```

### 测试用例
```c
#include <pthread.h>

void* thread_function(void* arg) {
    return NULL;
}

int main() {
    pthread_t thread;
    
    pthread_create(&thread, NULL, thread_function, NULL);
    
    // pthread_detach测试
    pthread_detach(thread);
    
    return 0;
}
```

## 4. 线程退出并发关系

### 查询规则
```
(call_expression) @concurrency.relationship.thread.exit
  function: (identifier) @thread.exit.function
  (#match? @thread.exit.function "^(pthread_exit)$")
  arguments: (argument_list
    (identifier)? @thread.exit.value)
```

### 测试用例
```c
#include <pthread.h>

void* thread_function(void* arg) {
    // pthread_exit测试
    pthread_exit(NULL);
    return NULL;
}

int main() {
    pthread_t thread;
    
    pthread_create(&thread, NULL, thread_function, NULL);
    
    return 0;
}
```

## 5. 获取线程ID并发关系

### 查询规则
```
(call_expression) @concurrency.relationship.thread.id
  function: (identifier) @thread.id.function
  (#match? @thread.id.function "^(pthread_self)$")
  arguments: (argument_list)
```

### 测试用例
```c
#include <pthread.h>
#include <stdio.h>

void* thread_function(void* arg) {
    // pthread_self测试
    pthread_t tid = pthread_self();
    printf("Thread ID: %lu\n", tid);
    return NULL;
}

int main() {
    pthread_t thread;
    
    pthread_create(&thread, NULL, thread_function, NULL);
    
    return 0;
}
```

## 6. 互斥锁初始化同步关系

### 查询规则
```
(call_expression) @concurrency.relationship.mutex.init
  function: (identifier) @mutex.init.function
  (#match? @mutex.init.function "^(pthread_mutex_init)$")
  arguments: (argument_list
    (pointer_expression argument: (identifier) @mutex.handle)
    (identifier)? @mutex.attributes)
```

### 测试用例
```c
#include <pthread.h>

int main() {
    pthread_mutex_t mutex;
    pthread_mutexattr_t attr;
    
    // pthread_mutex_init测试
    pthread_mutex_init(&mutex, &attr);
    
    return 0;
}
```

## 7. 互斥锁加锁同步关系

### 查询规则
```
(call_expression) @concurrency.relationship.mutex.lock
  function: (identifier) @mutex.lock.function
  (#match? @mutex.lock.function "^(pthread_mutex_lock)$")
  arguments: (argument_list
    (pointer_expression argument: (identifier) @mutex.handle))
```

### 测试用例
```c
#include <pthread.h>

int main() {
    pthread_mutex_t mutex;
    
    pthread_mutex_init(&mutex, NULL);
    
    // pthread_mutex_lock测试
    pthread_mutex_lock(&mutex);
    
    // Critical section
    // ...
    
    pthread_mutex_unlock(&mutex);
    
    return 0;
}
```

## 8. 互斥锁尝试加锁同步关系

### 查询规则
```
(call_expression) @concurrency.relationship.mutex.trylock
  function: (identifier) @mutex.trylock.function
  (#match? @mutex.trylock.function "^(pthread_mutex_trylock)$")
  arguments: (argument_list
    (pointer_expression argument: (identifier) @mutex.handle))
```

### 测试用例
```c
#include <pthread.h>
#include <stdio.h>

int main() {
    pthread_mutex_t mutex;
    
    pthread_mutex_init(&mutex, NULL);
    
    // pthread_mutex_trylock测试
    int result = pthread_mutex_trylock(&mutex);
    if (result == 0) {
        // Critical section
        // ...
        pthread_mutex_unlock(&mutex);
    } else {
        printf("Mutex is locked by another thread\n");
    }
    
    return 0;
}
```

## 9. 互斥锁解锁同步关系

### 查询规则
```
(call_expression) @concurrency.relationship.mutex.unlock
  function: (identifier) @mutex.unlock.function
  (#match? @mutex.unlock.function "^(pthread_mutex_unlock)$")
  arguments: (argument_list
    (pointer_expression argument: (identifier) @mutex.handle))
```

### 测试用例
```c
#include <pthread.h>

int main() {
    pthread_mutex_t mutex;
    
    pthread_mutex_init(&mutex, NULL);
    pthread_mutex_lock(&mutex);
    
    // Critical section
    // ...
    
    // pthread_mutex_unlock测试
    pthread_mutex_unlock(&mutex);
    
    return 0;
}
```

## 10. 互斥锁销毁同步关系

### 查询规则
```
(call_expression) @concurrency.relationship.mutex.destroy
  function: (identifier) @mutex.destroy.function
  (#match? @mutex.destroy.function "^(pthread_mutex_destroy)$")
  arguments: (argument_list
    (pointer_expression argument: (identifier) @mutex.handle))
```

### 测试用例
```c
#include <pthread.h>

int main() {
    pthread_mutex_t mutex;
    
    pthread_mutex_init(&mutex, NULL);
    pthread_mutex_lock(&mutex);
    pthread_mutex_unlock(&mutex);
    
    // pthread_mutex_destroy测试
    pthread_mutex_destroy(&mutex);
    
    return 0;
}
```

## 11. 条件变量初始化同步关系

### 查询规则
```
(call_expression) @concurrency.relationship.condition.init
  function: (identifier) @cond.init.function
  (#match? @cond.init.function "^(pthread_cond_init)$")
  arguments: (argument_list
    (pointer_expression argument: (identifier) @cond.handle)
    (identifier)? @cond.attributes)
```

### 测试用例
```c
#include <pthread.h>

int main() {
    pthread_cond_t cond;
    pthread_condattr_t attr;
    
    // pthread_cond_init测试
    pthread_cond_init(&cond, &attr);
    
    return 0;
}
```

## 12. 条件变量等待同步关系

### 查询规则
```
(call_expression) @concurrency.relationship.condition.wait
  function: (identifier) @cond.wait.function
  (#match? @cond.wait.function "^(pthread_cond_wait|pthread_cond_timedwait)$")
  arguments: (argument_list
    (pointer_expression argument: (identifier) @cond.handle)
    (pointer_expression argument: (identifier) @mutex.handle))
```

### 测试用例
```c
#include <pthread.h>
#include <time.h>

int main() {
    pthread_cond_t cond;
    pthread_mutex_t mutex;
    
    pthread_cond_init(&cond, NULL);
    pthread_mutex_init(&mutex, NULL);
    
    pthread_mutex_lock(&mutex);
    
    // pthread_cond_wait测试
    pthread_cond_wait(&cond, &mutex);
    
    // pthread_cond_timedwait测试
    struct timespec timeout;
    clock_gettime(CLOCK_REALTIME, &timeout);
    timeout.tv_sec += 5; // 5秒后超时
    pthread_cond_timedwait(&cond, &mutex, &timeout);
    
    pthread_mutex_unlock(&mutex);
    
    return 0;
}
```

## 13. 条件变量信号同步关系

### 查询规则
```
(call_expression) @concurrency.relationship.condition.signal
  function: (identifier) @cond.signal.function
  (#match? @cond.signal.function "^(pthread_cond_signal)$")
  arguments: (argument_list
    (pointer_expression argument: (identifier) @cond.handle))
```

### 测试用例
```c
#include <pthread.h>

int main() {
    pthread_cond_t cond;
    pthread_mutex_t mutex;
    
    pthread_cond_init(&cond, NULL);
    pthread_mutex_init(&mutex, NULL);
    
    pthread_mutex_lock(&mutex);
    
    // pthread_cond_signal测试
    pthread_cond_signal(&cond);
    
    pthread_mutex_unlock(&mutex);
    
    return 0;
}
```

## 14. 条件变量广播同步关系

### 查询规则
```
(call_expression) @concurrency.relationship.condition.broadcast
  function: (identifier) @cond.broadcast.function
  (#match? @cond.broadcast.function "^(pthread_cond_broadcast)$")
  arguments: (argument_list
    (pointer_expression argument: (identifier) @cond.handle))
```

### 测试用例
```c
#include <pthread.h>

int main() {
    pthread_cond_t cond;
    pthread_mutex_t mutex;
    
    pthread_cond_init(&cond, NULL);
    pthread_mutex_init(&mutex, NULL);
    
    pthread_mutex_lock(&mutex);
    
    // pthread_cond_broadcast测试
    pthread_cond_broadcast(&cond);
    
    pthread_mutex_unlock(&mutex);
    
    return 0;
}
```

## 15. 条件变量销毁同步关系

### 查询规则
```
(call_expression) @concurrency.relationship.condition.destroy
  function: (identifier) @cond.destroy.function
  (#match? @cond.destroy.function "^(pthread_cond_destroy)$")
  arguments: (argument_list
    (pointer_expression argument: (identifier) @cond.handle))
```

### 测试用例
```c
#include <pthread.h>

int main() {
    pthread_cond_t cond;
    
    pthread_cond_init(&cond, NULL);
    
    // pthread_cond_destroy测试
    pthread_cond_destroy(&cond);
    
    return 0;
}
```

## 16. 读写锁初始化同步关系

### 查询规则
```
(call_expression) @concurrency.relationship.rwlock.init
  function: (identifier) @rwlock.init.function
  (#match? @rwlock.init.function "^(pthread_rwlock_init)$")
  arguments: (argument_list
    (pointer_expression argument: (identifier) @rwlock.handle)
    (identifier)? @rwlock.attributes)
```

### 测试用例
```c
#include <pthread.h>

int main() {
    pthread_rwlock_t rwlock;
    pthread_rwlockattr_t attr;
    
    // pthread_rwlock_init测试
    pthread_rwlock_init(&rwlock, &attr);
    
    return 0;
}
```

## 17. 读写锁读锁同步关系

### 查询规则
```
(call_expression) @concurrency.relationship.rwlock.readlock
  function: (identifier) @rwlock.readlock.function
  (#match? @rwlock.readlock.function "^(pthread_rwlock_rdlock)$")
 arguments: (argument_list
    (pointer_expression argument: (identifier) @rwlock.handle))
```

### 测试用例
```c
#include <pthread.h>

int main() {
    pthread_rwlock_t rwlock;
    
    pthread_rwlock_init(&rwlock, NULL);
    
    // pthread_rwlock_rdlock测试
    pthread_rwlock_rdlock(&rwlock);
    
    // Read-only section
    // ...
    
    pthread_rwlock_unlock(&rwlock);
    
    return 0;
}
```

## 18. 读写锁写锁同步关系

### 查询规则
```
(call_expression) @concurrency.relationship.rwlock.writelock
  function: (identifier) @rwlock.writelock.function
  (#match? @rwlock.writelock.function "^(pthread_rwlock_wrlock)$")
 arguments: (argument_list
    (pointer_expression argument: (identifier) @rwlock.handle))
```

### 测试用例
```c
#include <pthread.h>

int main() {
    pthread_rwlock_t rwlock;
    
    pthread_rwlock_init(&rwlock, NULL);
    
    // pthread_rwlock_wrlock测试
    pthread_rwlock_wrlock(&rwlock);
    
    // Write section
    // ...
    
    pthread_rwlock_unlock(&rwlock);
    
    return 0;
}
```

## 19. 读写锁解锁同步关系

### 查询规则
```
(call_expression) @concurrency.relationship.rwlock.unlock
  function: (identifier) @rwlock.unlock.function
  (#match? @rwlock.unlock.function "^(pthread_rwlock_unlock)$")
  arguments: (argument_list
    (pointer_expression argument: (identifier) @rwlock.handle))
```

### 测试用例
```c
#include <pthread.h>

int main() {
    pthread_rwlock_t rwlock;
    
    pthread_rwlock_init(&rwlock, NULL);
    pthread_rwlock_wrlock(&rwlock);
    
    // Write section
    // ...
    
    // pthread_rwlock_unlock测试
    pthread_rwlock_unlock(&rwlock);
    
    return 0;
}
```

## 20. 读写锁销毁同步关系

### 查询规则
```
(call_expression) @concurrency.relationship.rwlock.destroy
  function: (identifier) @rwlock.destroy.function
  (#match? @rwlock.destroy.function "^(pthread_rwlock_destroy)$")
  arguments: (argument_list
    (pointer_expression argument: (identifier) @rwlock.handle))
```

### 测试用例
```c
#include <pthread.h>

int main() {
    pthread_rwlock_t rwlock;
    
    pthread_rwlock_init(&rwlock, NULL);
    pthread_rwlock_wrlock(&rwlock);
    pthread_rwlock_unlock(&rwlock);
    
    // pthread_rwlock_destroy测试
    pthread_rwlock_destroy(&rwlock);
    
    return 0;
}
```

## 21. 信号量初始化同步关系

### 查询规则
```
(call_expression) @concurrency.relationship.semaphore.init
 function: (identifier) @semaphore.init.function
  (#match? @semaphore.init.function "^(sem_init)$")
  arguments: (argument_list
    (pointer_expression argument: (identifier) @semaphore.handle)
    (identifier) @semaphore.pshared
    (identifier) @semaphore.value)
```

### 测试用例
```c
#include <semaphore.h>

int main() {
    sem_t sem;
    
    // sem_init测试
    sem_init(&sem, 0, 1); // 初始化为1，表示可用资源数
    
    return 0;
}
```

## 22. 信号量等待同步关系

### 查询规则
```
(call_expression) @concurrency.relationship.semaphore.wait
  function: (identifier) @semaphore.wait.function
  (#match? @semaphore.wait.function "^(sem_wait)$")
  arguments: (argument_list
    (pointer_expression argument: (identifier) @semaphore.handle))
```

### 测试用例
```c
#include <semaphore.h>

int main() {
    sem_t sem;
    
    sem_init(&sem, 0, 1);
    
    // sem_wait测试
    sem_wait(&sem);
    
    // Critical section
    // ...
    
    sem_post(&sem);
    
    return 0;
}
```

## 23. 信号量尝试等待同步关系

### 查询规则
```
(call_expression) @concurrency.relationship.semaphore.trywait
  function: (identifier) @semaphore.trywait.function
  (#match? @semaphore.trywait.function "^(sem_trywait)$")
  arguments: (argument_list
    (pointer_expression argument: (identifier) @semaphore.handle))
```

### 测试用例
```c
#include <semaphore.h>
#include <stdio.h>

int main() {
    sem_t sem;
    
    sem_init(&sem, 0, 1);
    
    // sem_trywait测试
    int result = sem_trywait(&sem);
    if (result == 0) {
        // Critical section
        // ...
        sem_post(&sem);
    } else {
        printf("Semaphore not available\n");
    }
    
    return 0;
}
```

## 24. 信号量信号同步关系

### 查询规则
```
(call_expression) @concurrency.relationship.semaphore.post
  function: (identifier) @semaphore.post.function
  (#match? @semaphore.post.function "^(sem_post)$")
  arguments: (argument_list
    (pointer_expression argument: (identifier) @semaphore.handle))
```

### 测试用例
```c
#include <semaphore.h>

int main() {
    sem_t sem;
    
    sem_init(&sem, 0, 0);
    sem_wait(&sem);
    
    // Critical section
    // ...
    
    // sem_post测试
    sem_post(&sem);
    
    return 0;
}
```

## 25. 信号量销毁同步关系

### 查询规则
```
(call_expression) @concurrency.relationship.semaphore.destroy
  function: (identifier) @semaphore.destroy.function
  (#match? @semaphore.destroy.function "^(sem_destroy)$")
  arguments: (argument_list
    (pointer_expression argument: (identifier) @semaphore.handle))
```

### 测试用例
```c
#include <semaphore.h>

int main() {
    sem_t sem;
    
    sem_init(&sem, 0, 1);
    sem_wait(&sem);
    sem_post(&sem);
    
    // sem_destroy测试
    sem_destroy(&sem);
    
    return 0;
}
```

## 26. 内存屏障并发关系

### 查询规则
```
(call_expression) @concurrency.relationship.memory.barrier
 function: (identifier) @memory.barrier.function
  (#match? @memory.barrier.function "^(atomic_thread_fence|__atomic_thread_fence)$")
  arguments: (argument_list
    (identifier)? @memory.order)
```

### 测试用例
```c
#include <stdatomic.h>

int main() {
    // atomic_thread_fence测试
    atomic_thread_fence(memory_order_seq_cst);
    
    // __atomic_thread_fence测试
    __atomic_thread_fence(__ATOMIC_SEQ_CST);
    
    return 0;
}
```
