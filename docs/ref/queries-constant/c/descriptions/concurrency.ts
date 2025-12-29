/*
C Concurrency Relationships-specific Tree-Sitter Query Patterns
用于识别和分析代码中的并发模式
优先级3
*/
export default `
; 线程创建并发关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.concurrency.function.from)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @relationship.concurrency.create.to
        (#match? @relationship.concurrency.create.to "^(pthread_create)$")
      ) @relationship.concurrency.thread.create
    )
  )
)

; 线程等待并发关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.concurrency.function.from)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @relationship.concurrency.wait.to
        (#match? @relationship.concurrency.wait.to "^(pthread_join)$")
      ) @relationship.concurrency.thread.wait
    )
  )
)

; 线程分离并发关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.concurrency.function.from)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @relationship.concurrency.detach.to
        (#match? @relationship.concurrency.detach.to "^(pthread_detach)$")
      ) @relationship.concurrency.thread.detach
    )
  )
)

; 线程退出并发关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.concurrency.function.from)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @relationship.concurrency.exit.to
        (#match? @relationship.concurrency.exit.to "^(pthread_exit)$")
      ) @relationship.concurrency.thread.exit
    )
  )
)

; 获取线程ID并发关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.concurrency.function.from)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @relationship.concurrency.id.to
        (#match? @relationship.concurrency.id.to "^(pthread_self)$")
      ) @relationship.concurrency.thread.id
    )
  )
)

; 互斥锁创建并发关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.concurrency.function.from)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @relationship.concurrency.mutex.to
        (#match? @relationship.concurrency.mutex.to "^(pthread_mutex_init)$")
      ) @relationship.concurrency.mutex.create
    )
  )
)

; 互斥锁操作并发关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.concurrency.function.from)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @relationship.concurrency.operation.to
        (#match? @relationship.concurrency.operation.to "^(pthread_mutex_lock|pthread_mutex_trylock|pthread_mutex_unlock|pthread_mutex_destroy)$")
      ) @relationship.concurrency.mutex.operation
    )
  )
)

; 条件变量操作并发关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.concurrency.function.from)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @relationship.concurrency.condition.to
        (#match? @relationship.concurrency.condition.to "^(pthread_cond_wait|pthread_cond_timedwait|pthread_cond_signal|pthread_cond_broadcast|pthread_cond_destroy)$")
      ) @relationship.concurrency.condition.operation
    )
  )
)

; 读写锁操作并发关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.concurrency.function.from)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @relationship.concurrency.rwlock.to
        (#match? @relationship.concurrency.rwlock.to "^(pthread_rwlock_rdlock|pthread_rwlock_wrlock|pthread_rwlock_unlock|pthread_rwlock_destroy)$")
      ) @relationship.concurrency.rwlock.operation
    )
  )
)

; 信号量操作并发关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.concurrency.function.from)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @relationship.concurrency.semaphore.to
        (#match? @relationship.concurrency.semaphore.to "^(sem_wait|sem_trywait|sem_post|sem_destroy)$")
      ) @relationship.concurrency.semaphore.operation
    )
  )
)
`;