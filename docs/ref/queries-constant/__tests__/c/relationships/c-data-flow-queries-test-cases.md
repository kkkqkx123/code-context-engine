# C语言数据流关系Tree-Sitter查询规则测试用例

本文档为C语言数据流关系的Tree-Sitter查询规则提供测试用例，每个查询规则后都附有相应的测试代码示例，方便在Tree-Sitter Playground中进行验证。

## 1. 统一的赋值数据流查询

### 查询规则
```
[
  (assignment_expression
    left: [
      (identifier) @target.variable
      (field_expression
        argument: (identifier) @target.object
        field: (field_identifier) @target.field)
      (subscript_expression
        argument: (identifier) @target.array
        index: (identifier) @target.index)
      (pointer_expression
        argument: (identifier) @target.pointer)
    ]
    right: [
      (identifier) @source.variable
      (call_expression
        function: (identifier) @source.function)
      (binary_expression
        left: (_) @binary.left
        right: (_) @binary.right)
      (unary_expression
        argument: (_) @unary.argument)
      (number_literal) @source.literal
    ]) @data.flow.assignment
  (init_declarator
    declarator: (identifier) @target.variable
    value: [
      (identifier) @source.variable
      (call_expression
        function: (identifier) @source.function)
      (binary_expression
        left: (_) @binary.left
        right: (_) @binary.right)
      (unary_expression
        argument: (_) @unary.argument)
      (number_literal) @source.literal
    ]) @data.flow.assignment
]
```

### 测试用例
```c
// 基本变量赋值
void basic_assignment() {
    int a, b, c;
    a = b;           // 变量到变量
    c = 42;          // 字面量到变量
    a = calculate(); // 函数返回值到变量
}

// 结构体字段赋值
void struct_field_assignment() {
    struct Point {
        int x;
        int y;
    } p1, p2;
    
    p1.x = p2.x;     // 结构体字段到结构体字段
    p1.y = 100;      // 字面量到结构体字段
}

// 数组元素赋值
void array_assignment() {
    int array1[10], array2[10];
    int i = 5;
    
    array1[i] = array2[i];  // 数组元素到数组元素
    array1[0] = 42;         // 字面量到数组元素
}

// 指针赋值
void pointer_assignment() {
    int x, y;
    int* ptr1, *ptr2;
    
    ptr1 = &x;       // 地址到指针
    ptr2 = ptr1;     // 指针到指针
    *ptr1 = y;       // 变量到指针解引用
}

// 复杂表达式赋值
void complex_assignment() {
    int a, b, c, d;
    
    a = b + c;       // 二元表达式到变量
    b = -a;          // 一元表达式到变量
    c = (a + b) * (c - d); // 复杂二元表达式到变量
}

// 声明初始化
void initialization_assignment() {
    int a = b;           // 变量初始化
    int b = 42;          // 字面量初始化
    int c = calculate(); // 函数返回值初始化
    int d = a + b;       // 表达式初始化
}
```

## 2. 复合赋值数据流

### 查询规则
```
(assignment_expression
  left: (identifier) @target.variable
  right: [
    (identifier) @source.variable1
    (binary_expression
      left: (identifier) @source.variable1
      operator: ["+" "-" "*" "/" "%" "&" "|" "^" "<<" ">>"] @compound.operator
      right: (identifier) @source.variable2)
  ]) @data.flow.compound.assignment
```

### 测试用例
```c
// 基本复合赋值
void compound_assignment() {
    int a, b, c;
    
    a += b;        // a = a + b
    a -= b;        // a = a - b
    a *= b;        // a = a * b
    a /= b;        // a = a / b
    a %= b;        // a = a % b
}

// 位运算复合赋值
void bitwise_compound_assignment() {
    int a, b;
    
    a &= b;        // a = a & b
    a |= b;        // a = a | b
    a ^= b;        // a = a ^ b
    a <<= b;       // a = a << b
    a >>= b;       // a = a >> b
}

// 复合赋值链
void compound_assignment_chain() {
    int a = 10, b = 20, c = 30;
    
    a += b += c;   // 先执行 b = b + c，然后 a = a + b
    a *= b -= c;   // 先执行 b = b - c，然后 a = a * b
}

// 复合赋值与表达式
void compound_with_expressions() {
    int a, b, c, d;
    
    a += b * c;    // a = a + (b * c)
    a -= b / c;    // a = a - (b / c)
    a *= b + c;    // a = a * (b + c)
}
```

## 3. 增量/减量数据流

### 查询规则
```
[
  (update_expression
    argument: (identifier) @variable
    operator: "++") @data.flow.increment
  (update_expression
    argument: (identifier) @variable
    operator: "--") @data.flow.decrement
] @data.flow.update
```

### 测试用例
```c
// 基本增量/减量
void increment_decrement() {
    int a = 10, b = 20;
    
    a++;           // 后置增量
    ++a;           // 前置增量
    b--;           // 后置减量
    --b;           // 前置减量
}

// 增量/减量在表达式中
void increment_in_expression() {
    int a = 10, b = 20, c;
    
    c = a++;       // 先使用a的值，然后增量
    c = ++a;       // 先增量，然后使用a的值
    c = b--;       // 先使用b的值，然后减量
    c = --b;       // 先减量，然后使用b的值
}

// 循环中的增量/减量
void increment_in_loop() {
    int i;
    for (i = 0; i < 10; i++) {
        // 循环体
    }
    
    while (i > 0) {
        i--;
    }
}

// 复杂增量/减量表达式
void complex_increment() {
    int a = 10, b = 20, c = 30;
    
    a += ++b;      // 先增量b，然后加到a
    a -= b--;      // 先使用b，然后减量，再从a中减去
    c = ++a + b++; // 复杂的增量表达式组合
}
```

## 4. 函数调用数据流

### 查询规则
```
(call_expression
  function: [
    (identifier) @target.function
    (pointer_expression
      argument: (identifier) @target.function.pointer)
    (field_expression
      argument: (identifier) @target.object
      field: (field_identifier) @target.method)
  ]
  arguments: (argument_list
    (_) @source.parameter)*) @data.flow.parameter.passing
```

### 测试用例
```c
// 基本函数调用
void basic_function_call() {
    int a = 10, b = 20;
    int result;
    
    result = add(a, b);        // 传递参数给函数
    printf("Result: %d\n", result);
}

// 函数指针调用
void function_pointer_call() {
    int (*operation)(int, int) = add;
    int a = 10, b = 20;
    int result;
    
    result = operation(a, b);  // 通过函数指针调用
}

// 结构体方法调用
void struct_method_call() {
    struct Calculator {
        int (*add)(int, int);
        int (*multiply)(int, int);
    } calc;
    
    int a = 10, b = 20;
    int result;
    
    calc.add = add_function;
    result = calc.add(a, b);   // 通过结构体方法调用
}

// 复杂参数传递
void complex_parameter_passing() {
    int array[10];
    struct Point p = {10, 20};
    
    process_array(array, 10);           // 传递数组
    process_point(&p);                  // 传递结构体指针
    calculate(add(5, 3), multiply(4, 6)); // 嵌套函数调用作为参数
}

// 多参数函数调用
void multi_parameter_call() {
    int a = 10, b = 20, c = 30;
    char* message = "Hello";
    
    complex_function(a, b, c, message, &a); // 多个不同类型的参数
}
```

## 5. 返回值数据流

### 查询规则
```
(return_statement
  .
  [
    (identifier) @source.variable
    (call_expression
      function: (identifier) @source.function)
    (field_expression
      argument: (identifier) @source.object
      field: (field_identifier) @source.field)
    (binary_expression
      left: (_) @binary.left
      right: (_) @binary.right)
    (number_literal) @source.literal
  ]) @data.flow.return.value
```

### 测试用例
```c
// 基本返回值
int basic_return() {
    int result = 42;
    return result;        // 返回变量
}

int return_literal() {
    return 100;           // 返回字面量
}

// 返回函数调用结果
int return_function_call() {
    return calculate(10, 20);  // 返回函数调用结果
}

// 返回结构体字段
struct Point return_field() {
    struct Point p = {10, 20};
    return p.x;          // 返回结构体字段
}

// 返回表达式结果
int return_expression() {
    int a = 10, b = 20;
    return a + b;        // 返回表达式结果
}

// 复杂返回表达式
int complex_return() {
    int a = 10, b = 20, c = 30;
    return (a + b) * (c - a);  // 返回复杂表达式
}

// 条件返回
int conditional_return() {
    int x = 10;
    if (x > 5) {
        return x * 2;    // 条件返回
    } else {
        return x / 2;
    }
}
```

## 6. 初始化数据流

### 查询规则
```
[
  (init_declarator
    declarator: (identifier) @target.variable
    value: (initializer_list
      (_) @source.variable)*)) @data.flow.struct.initialization
  (init_declarator
    declarator: (array_declarator
      declarator: (identifier) @target.array)
    value: (initializer_list
      (_) @source.variable)*)) @data.flow.array.initialization
] @data.flow.initialization
```

### 测试用例
```c
// 结构体初始化
void struct_initialization() {
    int a = 10, b = 20;
    
    struct Point {
        int x;
        int y;
    } p1 = {a, b};        // 使用变量初始化结构体
    
    struct Rectangle {
        struct Point top_left;
        struct Point bottom_right;
    } rect = {{a, b}, {a + 100, b + 100}}; // 嵌套结构体初始化
}

// 数组初始化
void array_initialization() {
    int a = 1, b = 2, c = 3, d = 4, e = 5;
    
    int numbers[5] = {a, b, c, d, e};  // 使用变量初始化数组
    
    int matrix[2][2] = {{a, b}, {c, d}}; // 二维数组初始化
    
    char chars[4] = {'A', 'B', 'C', 'D'}; // 字符数组初始化
}

// 复杂初始化
void complex_initialization() {
    int x = 10, y = 20;
    
    struct Complex {
        int values[3];
        struct Point {
            int x, y;
        } point;
    } complex = {{x, y, x + y}, {x * 2, y * 2}}; // 复杂嵌套初始化
}

// 部分初始化
void partial_initialization() {
    int a = 10, b = 20;
    
    int numbers[5] = {a, b};  // 部分初始化，其余为0
    
    struct Point {
        int x;
        int y;
        int z;
    } p = {a};  // 只初始化第一个字段，其余为0
}
```

## 7. 指针操作数据流

### 查询规则
```
[
  (assignment_expression
    left: (identifier) @target.pointer
    right: (pointer_expression
      argument: (identifier) @source.variable)) @data.flow.address.assignment
  (assignment_expression
    left: (pointer_expression
      argument: (identifier) @target.pointer)
    right: (identifier) @source.variable) @data.flow.pointer.assignment
  (init_declarator
    declarator: (pointer_declarator
      declarator: (identifier) @target.pointer)
    value: (pointer_expression
      argument: (identifier) @source.variable)) @data.flow.address.assignment
] @data.flow.pointer.operation
```

### 测试用例
```c
// 基本指针操作
void basic_pointer_operations() {
    int x = 10, y = 20;
    int* ptr1, *ptr2;
    
    ptr1 = &x;       // 取地址赋值给指针
    ptr2 = ptr1;     // 指针赋值给指针
    *ptr1 = y;       // 通过指针解引用赋值
}

// 指针初始化
void pointer_initialization() {
    int x = 10;
    int* ptr = &x;   // 指针初始化为变量地址
    
    int* ptr2 = ptr; // 指针初始化为另一个指针
}

// 复杂指针操作
void complex_pointer_operations() {
    int array[10] = {1, 2, 3, 4, 5};
    int* ptr;
    
    ptr = array;     // 数组名赋值给指针
    ptr = &array[0]; // 数组元素地址赋值给指针
    
    *ptr = 100;      // 通过指针修改数组元素
    *(ptr + 1) = 200; // 指针算术后解引用
}

// 指针与结构体
void pointer_with_struct() {
    struct Point {
        int x;
        int y;
    } p = {10, 20};
    
    struct Point* ptr = &p;  // 结构体指针
    ptr->x = 30;             // 通过指针访问结构体字段
    (*ptr).y = 40;           // 通过指针解引用访问结构体字段
}

// 双重指针
void double_pointer() {
    int x = 10;
    int* ptr = &x;
    int** dptr = &ptr;  // 双重指针
    
    **dptr = 20;        // 通过双重指针修改值
}
```

## 8. 类型转换数据流

### 查询规则
```
(cast_expression
  type: (type_descriptor) @target.type
  value: (identifier) @source.variable) @data.flow.type.conversion
```

### 测试用例
```c
// 基本类型转换
void basic_type_conversion() {
    int i = 10;
    float f;
    double d;
    char c;
    
    f = (float)i;      // int到float转换
    d = (double)i;     // int到double转换
    c = (char)i;       // int到char转换
}

// 指针类型转换
void pointer_type_conversion() {
    int x = 10;
    void* ptr;
    int* int_ptr;
    
    ptr = (void*)&x;   // int指针到void指针转换
    int_ptr = (int*)ptr; // void指针到int指针转换
}

// 结构体指针转换
void struct_pointer_conversion() {
    struct Base {
        int x;
    } base;
    
    struct Derived {
        int x;
        int y;
    } derived;
    
    struct Base* base_ptr = (struct Base*)&derived; // 派生类指针到基类指针转换
}

// 函数指针转换
void function_pointer_conversion() {
    void (*generic_ptr)();
    int (*int_ptr)(int);
    
    generic_ptr = (void (*)())int_ptr; // 函数指针类型转换
}

// 复杂类型转换
void complex_type_conversion() {
    int** dptr;
    void* vptr;
    
    vptr = (void*)dptr;      // 双重指针到void指针转换
    dptr = (int**)vptr;      // void指针到双重指针转换
}
```

## 9. 条件表达式数据流

### 查询规则
```
[
  (assignment_expression
    left: (identifier) @target.variable
    right: (conditional_expression
      condition: (identifier) @source.condition
      consequence: (identifier) @source.consequence
      alternative: (identifier) @source.alternative)) @data.flow.conditional.assignment
  (init_declarator
    declarator: (identifier) @target.variable
    value: (conditional_expression
      condition: (identifier) @source.condition
      consequence: (identifier) @source.consequence
      alternative: (identifier) @source.alternative)) @data.flow.conditional.assignment
] @data.flow.conditional.operation
```

### 测试用例
```c
// 基本条件表达式
void basic_conditional_expression() {
    int a = 10, b = 20, c = 30, result;
    
    result = a > b ? a : b;        // 条件表达式赋值
    result = a > b ? c : a;        // 不同变量的条件表达式
}

// 条件表达式初始化
void conditional_initialization() {
    int a = 10, b = 20, c = 30;
    
    int result = a > b ? a : b;    // 条件表达式初始化
    int max = a > b ? (a > c ? a : c) : (b > c ? b : c); // 嵌套条件表达式
}

// 复杂条件表达式
void complex_conditional_expression() {
    int a = 10, b = 20, c = 30, d = 40;
    int result;
    
    result = (a > b && c > d) ? a : (b > c ? b : d);
    result = (a + b > c + d) ? (a - b) : (c - d);
}

// 条件表达式与函数调用
void conditional_with_function_call() {
    int a = 10, b = 20;
    int result;
    
    result = a > b ? calculate(a) : calculate(b);
    result = a > b ? max(a, b) : min(a, b);
}

// 条件表达式链
void conditional_chain() {
    int a = 10, b = 20, c = 30, d = 40;
    int result;
    
    result = a > b ? (b > c ? (c > d ? c : d) : (b > d ? b : d)) : a;
}
```

## 10. 内存操作数据流

### 查询规则
```
(call_expression
  function: (identifier) @memory.function
  arguments: (argument_list
    (_) @memory.argument)*) @data.flow.memory.operation
```

### 测试用例
```c
#include <stdlib.h>
#include <string.h>

// 内存分配操作
void memory_allocation() {
    int* ptr;
    int size = 100;
    
    ptr = malloc(size * sizeof(int));  // 内存分配
    ptr = calloc(10, sizeof(int));     // 内存清零分配
    ptr = realloc(ptr, 200 * sizeof(int)); // 内存重新分配
}

// 内存复制操作
void memory_copy() {
    int source[10] = {1, 2, 3, 4, 5};
    int destination[10];
    int size = 10 * sizeof(int);
    
    memcpy(destination, source, size);     // 内存复制
    memmove(destination + 2, destination, 5 * sizeof(int)); // 内存移动
}

// 内存设置操作
void memory_set() {
    char buffer[100];
    int value = 0;
    size_t size = 100;
    
    memset(buffer, value, size);  // 内存设置
    memset(buffer + 50, 1, 50);  // 部分内存设置
}

// 内存比较操作
void memory_compare() {
    char buffer1[100] = "Hello";
    char buffer2[100] = "World";
    size_t size = 5;
    
    int result = memcmp(buffer1, buffer2, size); // 内存比较
}

// 复杂内存操作
void complex_memory_operations() {
    int* array1, *array2;
    int size = 10;
    
    array1 = malloc(size * sizeof(int));
    array2 = malloc(size * sizeof(int));
    
    // 初始化内存
    memset(array1, 0, size * sizeof(int));
    
    // 复制内存
    memcpy(array2, array1, size * sizeof(int));
    
    // 重新分配内存
    array1 = realloc(array1, size * 2 * sizeof(int));
    
    // 释放内存
    free(array1);
    free(array2);
}
```

## 11. 链式访问数据流

### 查询规则
```
(assignment_expression
  left: [
    (field_expression
      argument: (field_expression
        argument: (identifier) @source.object
        field: (field_identifier) @source.field1)
      field: (field_identifier) @source.field2))
    (subscript_expression
      argument: (subscript_expression
        argument: (identifier) @source.array
        index: (identifier) @source.index1)
      index: (identifier) @source.index2))
  ]
  right: (identifier) @target.variable) @data.flow.chained.access
```

### 测试用例
```c
// 嵌套结构体字段访问
void nested_struct_access() {
    struct Address {
        char street[50];
        char city[20];
    };
    
    struct Person {
        char name[50];
        struct Address address;
    };
    
    struct Person person;
    int value = 100;
    
    person.address.street[0] = value;  // 嵌套结构体字段访问
}

// 二维数组访问
void multi_dimensional_array_access() {
    int matrix[10][10];
    int i = 5, j = 3, value = 100;
    
    matrix[i][j] = value;  // 二维数组访问
}

// 结构体数组访问
void struct_array_access() {
    struct Point {
        int x;
        int y;
    };
    
    struct Point points[10];
    int i = 5, value = 100;
    
    points[i].x = value;  // 结构体数组访问
}

// 复杂链式访问
void complex_chained_access() {
    struct Node {
        int value;
        struct Node* next;
    };
    
    struct LinkedList {
        struct Node* head;
        int count;
    };
    
    struct LinkedList list;
    int value = 100;
    
    list.head->next->value = value;  // 多级指针访问
}

// 指针数组访问
void pointer_array_access() {
    int* array[10];
    int i = 5, j = 3, value = 100;
    
    array[i][j] = value;  // 指针数组访问
}
```

## 12. 宏调用数据流

### 查询规则
```
(init_declarator
  declarator: (identifier) @target.variable
  value: (identifier) @macro.value) @data.flow.macro.assignment
```

### 测试用例
```c
// 宏定义
#define MAX_VALUE 100
#define BUFFER_SIZE 256
#define DEFAULT_PORT 8080

// 宏赋值
void macro_assignment() {
    int max_value = MAX_VALUE;        // 宏赋值
    int buffer_size = BUFFER_SIZE;    // 宏赋值
    int default_port = DEFAULT_PORT;  // 宏赋值
}

// 带参数的宏
#define SQUARE(x) ((x) * (x))
#define MAX(a, b) ((a) > (b) ? (a) : (b))
#define ABS(x) ((x) < 0 ? -(x) : (x))

void parameterized_macro_assignment() {
    int x = 10, y = 20;
    
    int square = SQUARE(x);          // 参数化宏赋值
    int max = MAX(x, y);             // 参数化宏赋值
    int abs_value = ABS(-5);         // 参数化宏赋值
}

// 复杂宏表达式
#define COMPLEX_MACRO(a, b, c) ((a) + (b) * (c))

void complex_macro_assignment() {
    int a = 10, b = 20, c = 30;
    
    int result = COMPLEX_MACRO(a, b, c);  // 复杂宏赋值
}

// 条件宏
#ifdef DEBUG
    #define LOG_LEVEL 3
#else
    #define LOG_LEVEL 1
#endif

void conditional_macro_assignment() {
    int log_level = LOG_LEVEL;  // 条件宏赋值
}
```

## 13. sizeof表达式数据流

### 查询规则
```
[
  (assignment_expression
    left: (identifier) @target.variable
    right: (sizeof_expression
      (parenthesized_expression
        (identifier) @source.variable))) @data.flow.sizeof.assignment
  (init_declarator
    declarator: (identifier) @target.variable
    value: (sizeof_expression
      (parenthesized_expression
        (identifier) @source.variable))) @data.flow.sizeof.assignment
  (sizeof_expression
    (parenthesized_expression
      (identifier) @source.variable)) @data.flow.sizeof.expression
] @data.flow.sizeof.operation
```

### 测试用例
```c
// 基本sizeof表达式
void basic_sizeof() {
    int x;
    int size;
    
    size = sizeof(x);        // 变量的sizeof
    size = sizeof(int);      // 类型的sizeof
}

// sizeof赋值
void sizeof_assignment() {
    int x, size;
    
    size = sizeof(x);        // sizeof赋值
}

// sizeof初始化
void sizeof_initialization() {
    int x;
    int size = sizeof(x);    // sizeof初始化
}

// 结构体sizeof
void struct_sizeof() {
    struct Point {
        int x;
        int y;
    } p;
    
    int size = sizeof(p);    // 结构体变量的sizeof
    size = sizeof(struct Point); // 结构体类型的sizeof
}

// 数组sizeof
void array_sizeof() {
    int array[10];
    int size;
    
    size = sizeof(array);    // 数组的sizeof
    size = sizeof(array[0]); // 数组元素的sizeof
}

// 指针sizeof
void pointer_sizeof() {
    int x;
    int* ptr = &x;
    int size;
    
    size = sizeof(ptr);      // 指针的sizeof
    size = sizeof(*ptr);     // 指针解引用的sizeof
}

// 复杂sizeof表达式
void complex_sizeof() {
    int array[10][20];
    int size;
    
    size = sizeof(array);           // 二维数组的sizeof
    size = sizeof(array[0]);        // 一维数组的sizeof
    size = sizeof(array[0][0]);     // 数组元素的sizeof
}
```

## 14. 综合测试用例

### 测试用例
```c
#include <stdlib.h>
#include <string.h>

// 综合数据流示例
void comprehensive_data_flow() {
    // 赋值数据流
    int a = 10, b = 20, c;
    c = a + b;                    // 表达式赋值
    
    // 复合赋值数据流
    a += b;                       // 复合赋值
    a *= (c - b);                 // 复合赋值与表达式
    
    // 增量/减量数据流
    a++;                          // 增量
    --b;                          // 减量
    
    // 函数调用数据流
    int result = calculate(a, b, c); // 函数参数传递
    
    // 返回值数据流
    return result * 2;            // 返回表达式结果
    
    // 初始化数据流
    struct Point {
        int x, y;
    } point = {a, b};             // 结构体初始化
    
    int numbers[3] = {a, b, c};   // 数组初始化
    
    // 指针操作数据流
    int* ptr = &a;                // 指针赋值
    *ptr = 100;                   // 指针解引用赋值
    
    // 类型转换数据流
    float f = (float)a;           // 类型转换
    
    // 条件表达式数据流
    int max = a > b ? a : b;      // 条件表达式
    
    // 内存操作数据流
    int* dynamic = malloc(sizeof(int) * 10); // 内存分配
    memset(dynamic, 0, 10 * sizeof(int));     // 内存设置
    memcpy(dynamic, &a, sizeof(int));         // 内存复制
    free(dynamic);                              // 内存释放
    
    // 链式访问数据流
    struct Rectangle {
        struct Point top_left;
        struct Point bottom_right;
    } rect;
    
    rect.top_left.x = a;          // 链式访问
    
    // sizeof表达式数据流
    int size = sizeof(rect);      // sizeof表达式
}

// 复杂数据流示例
void complex_data_flow() {
    // 嵌套结构体与指针
    struct Node {
        int value;
        struct Node* next;
    };
    
    struct LinkedList {
        struct Node* head;
        int count;
    } list;
    
    // 创建链表节点
    list.head = malloc(sizeof(struct Node));
    list.head->value = 10;
    list.head->next = malloc(sizeof(struct Node));
    list.head->next->value = 20;
    list.head->next->next = NULL;
    list.count = 2;
    
    // 遍历链表
    struct Node* current = list.head;
    int sum = 0;
    
    while (current != NULL) {
        sum += current->value;    // 指针解引用与字段访问
        current = current->next;  // 指针赋值
    }
    
    // 条件处理
    int max_value = (sum > 100) ? sum : 100; // 条件表达式
    
    // 类型转换与内存操作
    char* buffer = malloc(max_value);
    memset(buffer, 0, max_value);
    memcpy(buffer, &sum, sizeof(int));
    
    // 清理
    free(list.head->next);
    free(list.head);
    free(buffer);
}