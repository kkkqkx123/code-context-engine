# C Functions Tree-Sitter查询规则测试用例

本文档为C Functions的Tree-Sitter查询规则提供测试用例，每个查询规则后都附有相应的测试代码示例，方便在TreeSitter Playground中进行验证。

## 1. 统一的函数查询

### 查询规则
```
[
  (function_definition
    declarator: (function_declarator
      declarator: (identifier) @function.name)
    body: (compound_statement) @function.body) @definition.function
  (declaration
    type: (primitive_type)
    declarator: (function_declarator
      declarator: (identifier) @function.name
      parameters: (parameter_list))) @definition.function.prototype
] @definition.function
```

### 测试用例
```c
// 函数定义
int add(int a, int b) {
    return a + b;
}

// 函数原型声明
void print_message(const char* message);
```

## 2. 带参数的函数查询

### 查询规则
```
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @function.name
    parameters: (parameter_list
      (parameter_declaration
        type: (primitive_type)
        declarator: (identifier) @param.name)*))
 body: (compound_statement) @function.body) @definition.function.with_params
```

### 测试用例
```c
int calculate(int x, int y, int z) {
    return (x + y) * z;
}

void process_data(int count, float value, char* buffer) {
    // 函数体
}
```

## 3. 函数调用查询

### 查询规则
```
(call_expression
  function: (identifier) @call.function
  arguments: (argument_list
    (_) @call.argument)*) @definition.function.call
```

### 测试用例
```c
int main() {
    int result = add(5, 3);
    print_message("Hello, World!");
    calculate(10, 20, 30);
    return 0;
}
```

## 4. 函数指针查询

### 查询规则
```
(declaration
  type: (primitive_type) @return.type
  declarator: (function_declarator
    declarator: (parenthesized_declarator
      (pointer_declarator
        declarator: (identifier) @function.pointer.name))
    parameters: (parameter_list))) @definition.function.pointer
```

### 测试用例
```c
int (*operation_func)(int, int);
void (*callback)(void);
char* (*string_processor)(const char*);
```

## 5. 递归函数查询

### 查询规则
```
(call_expression
  function: (identifier) @recursive.call
  arguments: (argument_list)) @definition.recursive.call
```

### 测试用例
```c
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
```

## 6. 内联函数查询

### 查询规则
```
(function_definition
  (storage_class_specifier) @func.type
  declarator: (function_declarator
    declarator: (identifier) @inline.function)
  body: (compound_statement) @inline.body
  (#match? @func.type "inline|static")) @definition.inline.function
```

### 测试用例
```c
inline int max(int a, int b) {
    return (a > b) ? a : b;
}

static inline void swap(int* a, int* b) {
    int temp = *a;
    *a = *b;
    *b = temp;
}

inline double square(double x) {
    return x * x;
}