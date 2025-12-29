# C语言语义关系Tree-Sitter查询规则测试用例

本文档为C语言语义关系的Tree-Sitter查询规则提供测试用例，每个查询规则后都附有相应的测试代码示例，方便在Tree-Sitter Playground中进行验证。

## 1. 函数调用关系

### 测试用例
```c
#include <stdio.h>
#include <stdlib.h>

void helper_function(int param) {
    printf("Helper function called with %d\n", param);
}

int calculate(int a, int b) {
    return a + b;
}

int main() {
    int x = 10;
    int y = 20;
    
    // 直接函数调用
    int result = calculate(x, y);
    
    // 调用helper函数
    helper_function(result);
    
    return 0;
}
```

### 查询规则
```
(call_expression
  function: [
    (identifier) @target.function
    (pointer_expression
      argument: (identifier) @function.pointer)
  ]
  arguments: (argument_list
    (identifier)* @source.parameter)) @semantic.relationship.function.call
```

## 2. 递归调用关系

### 查询规则
```
(call_expression
  function: (identifier) @recursive.function
  arguments: (argument_list))
  (#eq? @recursive.function @current.function) @semantic.relationship.recursive.call
```

### 测试用例
```c
#include <stdio.h>

int factorial(int n) {
    if (n <= 1) {
        return 1;
    }
    // 递归调用
    return n * factorial(n - 1);
}

int main() {
    int result = factorial(5);
    printf("Factorial of 5 is %d\n", result);
    return 0;
}
```

## 3. 回调函数模式

### 查询规则
```
[
  (assignment_expression
    left: (identifier) @callback.variable
    right: (identifier) @callback.function) @semantic.relationship.callback.assignment
  (field_declaration
    type: (pointer_type
      (function_type))
    declarator: (field_declarator
      declarator: (field_identifier) @callback.field)) @semantic.relationship.callback.field
] @semantic.relationship.callback.pattern
```

### 测试用例
```c
#include <stdio.h>

typedef void (*CallbackFunction)(int);

void my_callback(int value) {
    printf("Callback called with value: %d\n", value);
}

int main() {
    // 回调函数赋值
    CallbackFunction callback = my_callback;
    
    // 调用回调函数
    callback(42);
    
    return 0;
}
```

## 4. 结构体关系

### 查询规则
```
[
  (struct_specifier
    name: (type_identifier) @struct.name
    body: (field_declaration_list
      (field_declaration
        type: (type_identifier) @field.type
        declarator: (field_declarator
          declarator: (field_identifier) @field.name))*)) @semantic.relationship.struct.definition
  (struct_specifier
    name: (type_identifier) @nested.struct
    body: (field_declaration_list
      (field_declaration
        type: (struct_specifier
          name: (type_identifier) @inner.struct)
        declarator: (field_declarator
          declarator: (field_identifier) @field.name)))) @semantic.relationship.struct.nesting
] @semantic.relationship.struct
```

### 测试用例
```c
#include <stdio.h>

// 基本结构体定义
struct Point {
    int x;
    int y;
};

// 嵌套结构体
struct Rectangle {
    struct Point topLeft;
    struct Point bottomRight;
    int width;
    int height;
};

int main() {
    struct Point p = {10, 20};
    struct Rectangle rect = {{0, 0}, {100, 100}, 100, 100};
    
    printf("Point: (%d, %d)\n", p.x, p.y);
    printf("Rectangle: (%d, %d) to (%d, %d)\n", 
           rect.topLeft.x, rect.topLeft.y,
           rect.bottomRight.x, rect.bottomRight.y);
    
    return 0;
}
```

## 5. 指针关系

### 查询规则
```
(field_declaration
  type: (pointer_type
    (type_identifier) @pointed.type)
  declarator: (field_declarator
    declarator: (field_identifier) @pointer.field)) @semantic.relationship.pointer.field
```

### 测试用例
```c
#include <stdio.h>
#include <stdlib.h>

struct Node {
    int data;
    struct Node* next;  // 指向相同类型的指针
    char* name;         // 指向字符的指针
};

int main() {
    struct Node node;
    node.data = 42;
    node.next = NULL;
    node.name = "test";
    
    printf("Node data: %d\n", node.data);
    
    return 0;
}
```

## 6. 类型别名关系

### 查询规则
```
(type_definition
  type: (type_identifier) @original.type
  declarator: (type_identifier) @alias.type) @semantic.relationship.type.alias
```

### 测试用例
```c
#include <stdio.h>

// 基本类型别名
typedef unsigned int uint32_t;
typedef long long int64_t;

// 结构体类型别名
struct Person {
    char name[50];
    int age;
};

typedef struct Person PersonAlias;

int main() {
    uint32_t value = 42;
    int64_t big_value = 1234567890LL;
    
    PersonAlias person = {"Alice", 30};
    
    printf("Value: %u\n", value);
    printf("Big Value: %lld\n", big_value);
    printf("Person: %s, Age: %d\n", person.name, person.age);
    
    return 0;
}
```

## 7. 函数指针关系

### 查询规则
```
(type_definition
  type: (pointer_type
    (function_type
      parameters: (parameter_list
        (parameter_declaration
          type: (type_identifier) @param.type
          declarator: (identifier) @param.name))))
  declarator: (type_identifier) @function.pointer.type) @semantic.relationship.function.pointer.type
```

### 测试用例
```c
#include <stdio.h>

// 函数指针类型定义
typedef int (*BinaryOperation)(int, int);

int add(int a, int b) {
    return a + b;
}

int multiply(int a, int b) {
    return a * b;
}

int main() {
    BinaryOperation op1 = add;
    BinaryOperation op2 = multiply;
    
    int result1 = op1(5, 3);
    int result2 = op2(5, 3);
    
    printf("Addition: %d\n", result1);
    printf("Multiplication: %d\n", result2);
    
    return 0;
}
```

## 8. 预处理关系

### 查询规则
```
[
  (preproc_include
    path: (string_literal
      (string_content) @included.header)) @semantic.relationship.include
  (preproc_def
    name: (identifier) @macro.name
    value: (identifier)? @macro.value) @semantic.relationship.macro.definition
  (preproc_if
    condition: (identifier) @conditional.symbol) @semantic.relationship.conditional.compilation
] @semantic.relationship.preprocessor
```

### 测试用例
```c
#include <stdio.h>
#include <stdlib.h>

// 宏定义
#define MAX(a, b) ((a) > (b) ? (a) : (b))
#define BUFFER_SIZE 1024
#define DEBUG

#ifdef DEBUG
#define LOG(msg) printf("DEBUG: %s\n", msg)
#else
#define LOG(msg)
#endif

int main() {
    int x = 10;
    int y = 20;
    
    int max_value = MAX(x, y);
    char buffer[BUFFER_SIZE];
    
    printf("Max value: %d\n", max_value);
    LOG("Program started");
    
    return 0;
}
```

## 9. 全局变量关系

### 查询规则
```
(declaration
  type: (type_identifier) @global.variable.type
  declarator: (init_declarator
    declarator: (identifier) @global.variable.name)
  (#match? @global.variable.name "^[g_][a-zA-Z0-9_]*$")) @semantic.relationship.global.variable
```

### 测试用例
```c
#include <stdio.h>

// 全局变量（符合命名约定）
int g_counter = 0;
char g_buffer[256];
double g_precision = 0.001;

// 普通全局变量（不符合命名约定，不应匹配）
int global_var = 10;

static int s_local_counter = 0;  // 静态变量

int main() {
    g_counter++;
    sprintf(g_buffer, "Counter: %d", g_counter);
    printf("%s\n", g_buffer);
    printf("Precision: %.3f\n", g_precision);
    
    return 0;
}
```

## 10. 外部变量关系

### 查询规则
```
(declaration
  storage_class_specifier: (storage_class_specifier) @extern.specifier
  type: (type_identifier) @extern.variable.type
  declarator: (identifier) @extern.variable.name) @semantic.relationship.extern.variable
```

### 测试用例
```c
#include <stdio.h>

// 外部变量声明
extern int external_counter;
extern char* external_string;

int main() {
    // 使用外部变量
    printf("External counter: %d\n", external_counter);
    printf("External string: %s\n", external_string);
    
    return 0;
}
```

## 11. 内存管理关系

### 查询规则
```
(call_expression
  function: (identifier) @memory.function
  arguments: (argument_list
    (identifier) @memory.argument))
  (#match? @memory.function "^(malloc|calloc|realloc|free)$") @semantic.relationship.memory.management
```

### 测试用例
```c
#include <stdio.h>
#include <stdlib.h>

int main() {
    // malloc测试
    int* ptr1 = (int*)malloc(sizeof(int) * 10);
    
    // calloc测试
    int* ptr2 = (int*)calloc(5, sizeof(int));
    
    // realloc测试
    int* ptr3 = (int*)realloc(ptr1, sizeof(int) * 20);
    
    // 使用分配的内存
    if (ptr3 != NULL) {
        for (int i = 0; i < 20; i++) {
            ptr3[i] = i;
        }
    }
    
    // free测试
    free(ptr2);
    free(ptr3);
    
    return 0;
}
```

## 12. 错误处理模式

### 查询规则
```
[
  (return_statement
    (identifier) @error.code
    (#match? @error.code "^(ERROR|FAIL|INVALID|NULL)$")) @semantic.relationship.error.return
  (if_statement
    condition: (binary_expression
      left: (identifier) @checked.variable
      operator: ["==" "!="]
      right: (identifier) @error.value)
    consequence: (statement) @error.handling.block) @semantic.relationship.error.checking
] @semantic.relationship.error.handling
```

### 测试用例
```c
#include <stdio.h>
#include <stdlib.h>

#define ERROR -1
#define INVALID -2
#define NULL 0

int divide(int a, int b) {
    if (b == 0) {
        return ERROR;  // 错误返回
    }
    return a / b;
}

int* allocate_memory(int size) {
    int* ptr = (int*)malloc(sizeof(int) * size);
    if (ptr == NULL) {  // 错误检查
        printf("Memory allocation failed\n");
        return NULL;
    }
    return ptr;
}

int main() {
    int result = divide(10, 0);
    if (result == ERROR) {
        printf("Division by zero error\n");
    }
    
    int* ptr = allocate_memory(100);
    if (ptr == NULL) {  // 错误检查
        printf("Failed to allocate memory\n");
        return ERROR;  // 错误返回
    }
    
    free(ptr);
    return 0;
}
```

## 13. 资源管理模式

### 查询规则
```
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @resource.constructor)
  body: (compound_statement
    (declaration
      type: (type_identifier) @resource.type
      declarator: (init_declarator
        declarator: (identifier) @resource.variable
        value: (call_expression
          function: (identifier) @allocation.function))))) @semantic.relationship.resource.initialization
```

### 测试用例
```c
#include <stdio.h>
#include <stdlib.h>

typedef struct {
    int* data;
    int size;
} IntArray;

IntArray create_array(int size) {
    IntArray arr;
    arr.size = size;
    arr.data = (int*)malloc(sizeof(int) * size);  // 资源分配
    return arr;
}

int main() {
    IntArray my_array = create_array(10);
    
    if (my_array.data != NULL) {
        for (int i = 0; i < my_array.size; i++) {
            my_array.data[i] = i;
        }
        
        printf("Array created with size %d\n", my_array.size);
        free(my_array.data);
    }
    
    return 0;
}
```

## 14. 清理函数模式

### 查询规则
```
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @cleanup.function)
  parameters: (parameter_list
    (parameter_declaration
      type: (pointer_type)
      declarator: (identifier) @resource.parameter))) @semantic.relationship.cleanup.pattern
```

### 测试用例
```c
#include <stdio.h>
#include <stdlib.h>

typedef struct {
    int* data;
    int size;
} IntArray;

void destroy_array(IntArray* array) {
    if (array != NULL && array->data != NULL) {
        free(array->data);
        array->data = NULL;
        array->size = 0;
    }
}

int main() {
    IntArray my_array;
    my_array.size = 10;
    my_array.data = (int*)malloc(sizeof(int) * my_array.size);
    
    if (my_array.data != NULL) {
        for (int i = 0; i < my_array.size; i++) {
            my_array.data[i] = i;
        }
        
        printf("Array created with size %d\n", my_array.size);
        
        // 清理资源
        destroy_array(&my_array);
    }
    
    return 0;
}