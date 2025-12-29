# C语言函数调用关系Tree-Sitter查询规则测试用例

本文档为C语言函数调用关系的Tree-Sitter查询规则提供测试用例，每个查询规则后都附有相应的测试代码示例，方便在Tree-Sitter Playground中进行验证。

## 1. 函数调用关系 - 基本模式

### 查询规则
```
(call_expression
  function: (identifier) @call.function.name
  arguments: (argument_list
    (_) @call.argument)*) @call.relationship
```

### 测试用例
```c
#include <stdio.h>
#include <stdlib.h>

// 基本函数调用
void basic_function_calls() {
    printf("Hello, World!\n");
    int result = calculate(10, 20);
    process_data(&result, sizeof(result));
    free(memory_buffer);
}

// 带多个参数的函数调用
void multi_argument_calls() {
    int x = 10, y = 20, z = 30;
    complex_function(x, y, z, "parameter", &x);
    format_string("%d %s %f", 42, "test", 3.14);
}

// 嵌套函数调用
void nested_calls() {
    int result = calculate(add(5, 3), multiply(4, 6));
    printf("Result: %d\n", sqrt(pow(2, 3) + abs(-5)));
}

// 函数调用作为参数
void call_as_argument() {
    process_data(calculate(10, 20), sizeof(int));
    printf("Sum: %d\n", add(multiply(2, 3), divide(10, 2)));
}
```

## 2. 函数指针调用关系

### 查询规则
```
(call_expression
  function: (pointer_expression
    argument: (identifier) @call.function.pointer)
  arguments: (argument_list
    (_) @call.argument)*) @call.relationship.pointer
```

### 测试用例
```c
#include <stdio.h>

// 函数指针定义
typedef int (*operation_func)(int, int);
typedef void (*callback_func)(void*);

// 函数指针调用
void function_pointer_calls() {
    operation_func op = add;
    callback_func cb = process_callback;
    
    // 基本函数指针调用
    int result = (*op)(10, 20);
    int result2 = op(15, 25);  // 现代C语法
    
    // 带参数的函数指针调用
    (*cb)(data_buffer);
    cb(user_data);
    
    // 函数指针数组调用
    operation_func operations[] = {add, subtract, multiply, divide};
    for (int i = 0; i < 4; i++) {
        int result = (*operations[i])(100, i * 10);
        printf("Operation %d result: %d\n", i, result);
    }
}

// 结构体中的函数指针调用
void struct_function_pointer_calls() {
    struct Handler {
        void (*init)(void);
        void (*process)(int);
        void (*cleanup)(void);
    } handler;
    
    handler.init = system_init;
    handler.process = data_process;
    handler.cleanup = system_cleanup;
    
    (*handler.init)();
    (*handler.process)(42);
    (*handler.cleanup)();
}
```

## 3. 结构体方法调用关系

### 查询规则
```
(call_expression
  function: (field_expression
    argument: (identifier) @call.object
    field: (field_identifier) @call.method)
  arguments: (argument_list
    (_) @call.argument)*) @call.relationship.method
```

### 测试用例
```c
#include <stdio.h>
#include <stdlib.h>

// 结构体定义
struct Calculator {
    int (*add)(int, int);
    int (*subtract)(int, int);
    void (*reset)(struct Calculator*);
    int total;
};

struct StringHandler {
    char* (*concat)(const char*, const char*);
    int (*length)(const char*);
    void (*clear)(char*);
};

// 结构体方法调用
void struct_method_calls() {
    struct Calculator calc;
    struct StringHandler str_handler;
    
    // 初始化结构体方法指针
    calc.add = add_function;
    calc.subtract = subtract_function;
    calc.reset = reset_calculator;
    
    str_handler.concat = string_concat;
    str_handler.length = string_length;
    str_handler.clear = clear_string;
    
    // 调用结构体方法
    int sum = calc.add(10, 20);
    int diff = calc.subtract(30, 15);
    calc.reset(&calc);
    
    char* result = str_handler.concat("Hello, ", "World!");
    int len = str_handler.length(result);
    str_handler.clear(result);
    
    // 嵌套结构体方法调用
    struct {
        struct Calculator calc;
        struct StringHandler str;
    } composite;
    
    composite.calc.add = add_function;
    composite.str.concat = string_concat;
    
    int nested_result = composite.calc.add(5, 3);
    char* nested_str = composite.str.concat("Nested ", "call");
}
```

## 4. 递归调用关系

### 查询规则
```
(call_expression
  function: (identifier) @recursive.function.name
  arguments: (argument_list
    (_) @call.argument)*) @call.relationship.recursive
```

### 测试用例
```c
#include <stdio.h>

// 递归函数调用
int factorial(int n) {
    if (n <= 1) {
        return 1;
    }
    return n * factorial(n - 1);  // 递归调用
}

int fibonacci(int n) {
    if (n <= 1) {
        return n;
    }
    return fibonacci(n - 1) + fibonacci(n - 2);  // 递归调用
}

// 二叉树递归遍历
struct TreeNode {
    int value;
    struct TreeNode* left;
    struct TreeNode* right;
};

void traverse_tree(struct TreeNode* node) {
    if (node == NULL) {
        return;
    }
    
    printf("%d ", node->value);
    traverse_tree(node->left);   // 递归调用
    traverse_tree(node->right);  // 递归调用
}

// 快速排序递归调用
void quick_sort(int arr[], int low, int high) {
    if (low < high) {
        int pivot = partition(arr, low, high);
        quick_sort(arr, low, pivot - 1);   // 递归调用
        quick_sort(arr, pivot + 1, high);  // 递归调用
    }
}

// 复杂递归调用
int complex_recursive(int n, int* memo) {
    if (n <= 1) {
        return n;
    }
    
    if (memo[n] != -1) {
        return memo[n];
    }
    
    memo[n] = complex_recursive(n - 1, memo) + 
              complex_recursive(n - 2, memo);  // 递归调用
    return memo[n];
}
```

## 5. 链式调用关系

### 查询规则
```
(call_expression
  function: (field_expression
    argument: (call_expression
      function: (identifier) @chained.call.function
      arguments: (argument_list))
    field: (field_identifier) @chained.call.method)
  arguments: (argument_list
    (_) @call.argument)*) @call.relationship.chained
```

### 测试用例
```c
#include <stdio.h>

// 返回结构体指针的函数，支持链式调用
struct StringBuilder {
    char* buffer;
    size_t size;
    struct StringBuilder* (*append)(struct StringBuilder*, const char*);
    struct StringBuilder* (*clear)(struct StringBuilder*);
    char* (*toString)(struct StringBuilder*);
};

// 链式调用实现
void chained_calls_example() {
    struct StringBuilder* builder = create_string_builder();
    
    // 链式调用
    char* result = builder->append(
        builder->append(
            builder->append(builder, "Hello, "),
            "World"
        ),
        "!"
    )->toString(builder);
    
    // 更复杂的链式调用
    char* complex_result = builder
        ->append(builder, "Start")
        ->append(builder, " ")
        ->append(builder, "Middle")
        ->append(builder, " ")
        ->append(builder, "End")
        ->toString(builder);
    
    printf("Result: %s\n", result);
    printf("Complex Result: %s\n", complex_result);
}

// 数据库查询链式调用
struct Query {
    struct Query* (*where)(struct Query*, const char*, const char*);
    struct Query* (*orderBy)(struct Query*, const char*);
    struct Query* (*limit)(struct Query*, int);
    void (*execute)(struct Query*);
};

void database_chained_calls() {
    struct Query* query = create_query();
    
    // 数据库查询链式调用
    query->where(query, "age", "> 18")
         ->orderBy(query, "name")
         ->limit(query, 10)
         ->execute(query);
    
    // 复杂查询链式调用
    query->where(query, "status", "active")
         ->where(query, "created_at", "> '2023-01-01'")
         ->orderBy(query, "priority")
         ->orderBy(query, "created_at")
         ->limit(query, 100)
         ->execute(query);
}
```

## 6. 条件调用关系

### 查询规则
```
(call_expression
  function: (identifier) @conditional.call.function
  arguments: (argument_list
    (_) @call.argument)*) 
  (#match? @conditional.call.function "^(if|switch|select)$")) @call.relationship.conditional
```

### 测试用例
```c
#include <stdio.h>

// 条件函数调用
void conditional_calls() {
    int x = 10;
    
    // if函数调用（假设有if函数）
    if(x > 5, printf("x is greater than 5\n"));
    if(x == 10, printf("x equals 10\n"));
    
    // switch函数调用（假设有switch函数）
    switch(x, printf("Switching on x\n"));
    switch(x + 5, printf("Switching on x + 5\n"));
    
    // select函数调用（假设有select函数）
    select(x > 0, printf("Positive\n"), printf("Non-positive\n"));
    select(x % 2 == 0, printf("Even\n"), printf("Odd\n"));
}

// 复杂条件调用
void complex_conditional_calls() {
    int a = 10, b = 20, c = 30;
    
    // 嵌套条件调用
    if(a > b, 
       if(b > c, printf("a > b > c\n"), printf("a > b but b <= c\n")),
       printf("a <= b\n"));
    
    // 多参数条件调用
    switch(a + b + c, 
           printf("Sum is %d\n", a + b + c),
           printf("Processing sum\n"));
    
    // 复杂select调用
    select(a > b && b > c,
           printf("a > b > c\n"),
           select(a > c,
                  printf("a > c\n"),
                  printf("c is largest\n")));
}
```

## 7. 回调函数调用关系

### 查询规则
```
(call_expression
  function: (identifier) @callback.function
  arguments: (argument_list
    (identifier) @callback.argument)*) 
  (#match? @callback.function "^(callback|handler|invoke)$")) @call.relationship.callback
```

### 测试用例
```c
#include <stdio.h>
#include <stdlib.h>

// 回调函数类型定义
typedef void (*EventCallback)(void* data);
typedef int (*CompareCallback)(const void* a, const void* b);

// 回调函数调用
void callback_calls() {
    EventCallback event_cb = on_event;
    CompareCallback compare_cb = compare_function;
    
    // 基本回调调用
    callback(event_cb, user_data);
    handler(error_handler, error_context);
    invoke(custom_callback, callback_params);
    
    // 带多个参数的回调调用
    callback(event_cb, user_data, event_type, timestamp);
    handler(error_handler, error_context, error_code, error_message);
    invoke(custom_callback, callback_params, param1, param2);
}

// 事件系统回调调用
void event_system_callbacks() {
    // 注册和调用回调
    callback(on_click, button_data);
    callback(on_key_press, key_data);
    callback(on_mouse_move, mouse_data);
    
    // 错误处理回调
    handler(on_error, error_info);
    handler(on_warning, warning_info);
    handler(on_critical, critical_info);
    
    // 自定义回调调用
    invoke(on_data_ready, data_buffer);
    invoke(on_connection_lost, connection_info);
    invoke(on_timeout, timeout_context);
}

// 异步回调调用
void async_callbacks() {
    // 异步操作完成回调
    callback(on_operation_complete, operation_result);
    handler(on_file_loaded, file_data);
    invoke(on_network_response, response_data);
    
    // 进度回调
    callback(on_progress, progress_info);
    handler(on_download_progress, download_status);
    invoke(on_upload_progress, upload_status);
}
```

## 8. 宏函数调用关系

### 查询规则
```
(call_expression
  function: (identifier) @macro.function
  arguments: (argument_list
    (_) @macro.argument)*) 
  (#match? @macro.function "^[A-Z_][A-Z0-9_]*$")) @call.relationship.macro
```

### 测试用例
```c
#include <stdio.h>
#include <stdlib.h>

// 宏定义
#define MAX(a, b) ((a) > (b) ? (a) : (b))
#define MIN(a, b) ((a) < (b) ? (a) : (b))
#define ABS(x) ((x) < 0 ? -(x) : (x))
#define SQUARE(x) ((x) * (x))

#define PRINT_INT(x) printf("Value: %d\n", (x))
#define PRINT_STR(s) printf("String: %s\n", (s))
#define DEBUG_PRINT(fmt, ...) printf("[DEBUG] " fmt "\n", ##__VA_ARGS__)

#define ALLOCATE(type, count) ((type*)malloc(sizeof(type) * (count)))
#define SAFE_FREE(ptr) do { if(ptr) { free(ptr); ptr = NULL; } } while(0)

// 宏函数调用
void macro_function_calls() {
    int a = 10, b = 20;
    char* message = "Hello, World!";
    
    // 基本宏调用
    int max_val = MAX(a, b);
    int min_val = MIN(a, b);
    int abs_val = ABS(-5);
    int square_val = SQUARE(4);
    
    // 打印宏调用
    PRINT_INT(max_val);
    PRINT_STR(message);
    DEBUG_PRINT("Processing %d items", a + b);
    
    // 内存管理宏调用
    int* array = ALLOCATE(int, 100);
    SAFE_FREE(array);
    
    // 复杂宏调用
    int result = MAX(SQUARE(a), SQUARE(b));
    DEBUG_PRINT("Result: %d", result);
}

// 带可变参数的宏调用
void variadic_macro_calls() {
    // 可变参数宏调用
    DEBUG_PRINT("Simple message");
    DEBUG_PRINT("Value: %d", 42);
    DEBUG_PRINT("Values: %d, %d, %d", 1, 2, 3);
    DEBUG_PRINT("String: %s, Number: %d", "test", 100);
    
    // 复杂可变参数宏调用
    DEBUG_PRINT("Complex: %d + %d = %d", a, b, a + b);
    DEBUG_PRINT("Address: %p, Size: %zu", ptr, size);
}

// 条件宏调用
void conditional_macro_calls() {
#ifdef DEBUG
    DEBUG_PRINT("Debug mode enabled");
#endif
    
#ifdef VERBOSE
    DEBUG_PRINT("Verbose mode enabled");
    DEBUG_PRINT("Additional logging information");
#endif
    
    // 嵌套条件宏调用
#if defined(DEBUG) && defined(VERBOSE)
    DEBUG_PRINT("Debug and verbose modes enabled");
#endif
}
```

## 9. 综合测试用例

### 测试用例
```c
#include <stdio.h>
#include <stdlib.h>

// 综合函数调用示例
void comprehensive_function_calls() {
    // 基本函数调用
    int result = calculate(10, 20);
    printf("Result: %d\n", result);
    
    // 函数指针调用
    operation_func op = add;
    int ptr_result = (*op)(5, 3);
    
    // 结构体方法调用
    calculator.add(15, 25);
    string_handler.concat("Hello", "World");
    
    // 递归调用
    int fact = factorial(5);
    int fib = fibonacci(10);
    
    // 链式调用
    char* str = builder
        ->append("Start")
        ->append(" ")
        ->append("End")
        ->toString();
    
    // 条件调用
    if(result > 0, printf("Positive result\n"));
    switch(fact, printf("Factorial result\n"));
    
    // 回调调用
    callback(on_complete, &result);
    handler(on_error, error_info);
    
    // 宏调用
    int max_val = MAX(result, ptr_result);
    DEBUG_PRINT("Max value: %d", max_val);
    
    // 复杂组合调用
    process_data(
        calculator.add(
            MAX(result, ptr_result),
            MIN(fact, fib)
        ),
        callback(on_processed, &str)
    );
}

// 复杂嵌套调用示例
void complex_nested_calls() {
    // 多层嵌套调用
    int result = calculate(
        add(
            multiply(2, 3),
            divide(10, 2)
        ),
        subtract(
            factorial(4),
            fibonacci(5)
        )
    );
    
    // 函数指针与结构体方法组合
    operation_func ops[] = {add, subtract, multiply, divide};
    for (int i = 0; i < 4; i++) {
        calculator.process(
            (*ops[i])(result, i),
            callback(on_operation_complete, &i)
        );
    }
    
    // 链式调用与宏调用组合
    char* message = builder
        ->append("Result: ")
        ->append(string_from_int(MAX(result, 100)))
        ->toString();
    
    DEBUG_PRINT("Final message: %s", message);
}