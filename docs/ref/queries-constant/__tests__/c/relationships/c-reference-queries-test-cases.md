# C语言引用关系Tree-Sitter查询规则测试用例

本文档为C语言引用关系的Tree-Sitter查询规则提供测试用例，每个查询规则后都附有相应的测试代码示例，方便在Tree-Sitter Playground中进行验证。

## 1. 变量引用关系

### 查询规则
```
(identifier) @reference.variable
(#not-match? @reference.variable "^(if|for|while|do|switch|break|continue|return|goto|sizeof|typeof|alignof)$")
```

### 测试用例
```c
// 基本变量引用
void basic_variable_reference() {
    int a = 10, b = 20, c;
    c = a + b;        // a, b, c 变量引用
    printf("%d\n", c); // c 变量引用
}

// 复杂变量引用
void complex_variable_reference() {
    int x = 5, y = 10, z = 15;
    int result;
    
    result = x * y + z;    // x, y, z, result 变量引用
    result = result / x;   // result, x 变量引用
    y = result + z;        // y, result, z 变量引用
}

// 嵌套变量引用
void nested_variable_reference() {
    int a = 1, b = 2, c = 3, d = 4, e = 5;
    int result;
    
    result = a + b * c - d / e;  // a, b, c, d, e, result 变量引用
    result = (a + b) * (c - d) + e; // a, b, c, d, e, result 变量引用
}

// 函数参数变量引用
void function_parameter_reference(int x, int y) {
    int z = x + y;      // x, y, z 变量引用
    printf("%d\n", z);  // z 变量引用
}

// 循环中的变量引用
void loop_variable_reference() {
    int i, sum = 0;
    
    for (i = 0; i < 10; i++) {  // i, sum 变量引用
        sum += i;               // sum, i 变量引用
    }
    
    while (sum > 0) {          // sum 变量引用
        sum--;                  // sum 变量引用
    }
}
```

## 2. 函数引用关系

### 查询规则
```
(call_expression
  function: (identifier) @reference.function) @reference.relationship.function
```

### 测试用例
```c
// 基本函数引用
void basic_function_reference() {
    int result = calculate(10, 20);  // calculate 函数引用
    printf("Result: %d\n", result);  // printf 函数引用
}

// 嵌套函数引用
void nested_function_reference() {
    int result = add(multiply(2, 3), divide(10, 2));  // add, multiply, divide 函数引用
    printf("Result: %d\n", result);                   // printf 函数引用
}

// 函数指针引用
void function_pointer_reference() {
    int (*operation)(int, int) = add;  // add 函数引用
    int result = operation(5, 3);      // operation 函数指针引用
    printf("Result: %d\n", result);    // printf 函数引用
}

// 递归函数引用
int recursive_function_reference(int n) {
    if (n <= 1) {
        return n;
    }
    return recursive_function_reference(n - 1) + recursive_function_reference(n - 2);  // 递归函数引用
}

// 结构体方法引用
void struct_method_reference() {
    struct Calculator {
        int (*add)(int, int);
        int (*multiply)(int, int);
    } calc;
    
    calc.add = add_function;        // add_function 函数引用
    calc.multiply = multiply_function; // multiply_function 函数引用
    
    int result = calc.add(10, 20);  // calc.add 函数引用
    printf("Result: %d\n", result); // printf 函数引用
}
```

## 3. 结构体字段引用关系

### 查询规则
```
(field_expression
  argument: (identifier) @reference.object
  field: (field_identifier) @reference.field) @reference.relationship.field
```

### 测试用例
```c
// 基本结构体字段引用
void basic_field_reference() {
    struct Point {
        int x;
        int y;
    } p;
    
    p.x = 10;        // p 对象引用，x 字段引用
    p.y = 20;        // p 对象引用，y 字段引用
    printf("(%d, %d)\n", p.x, p.y); // p 对象引用，x, y 字段引用
}

// 嵌套结构体字段引用
void nested_field_reference() {
    struct Address {
        char street[50];
        char city[20];
    };
    
    struct Person {
        char name[50];
        struct Address address;
    } person;
    
    person.address.street[0] = '1';  // person 对象引用，address 字段引用，street 字段引用
    person.address.city[0] = 'N';     // person 对象引用，address 字段引用，city 字段引用
}

// 结构体指针字段引用
void pointer_field_reference() {
    struct Point {
        int x;
        int y;
    } *p = malloc(sizeof(struct Point));
    
    p->x = 10;       // p 指针引用，x 字段引用
    p->y = 20;       // p 指针引用，y 字段引用
    printf("(%d, %d)\n", p->x, p->y); // p 指针引用，x, y 字段引用
}

// 复杂结构体字段引用
void complex_field_reference() {
    struct Rectangle {
        struct Point {
            int x;
            int y;
        } top_left;
        struct Point bottom_right;
    } rect;
    
    rect.top_left.x = 0;        // rect 对象引用，top_left 字段引用，x 字段引用
    rect.top_left.y = 0;        // rect 对象引用，top_left 字段引用，y 字段引用
    rect.bottom_right.x = 100;  // rect 对象引用，bottom_right 字段引用，x 字段引用
    rect.bottom_right.y = 100;  // rect 对象引用，bottom_right 字段引用，y 字段引用
}
```

## 4. 数组元素引用关系

### 查询规则
```
(subscript_expression
  argument: (identifier) @reference.array
  index: (_) @reference.index) @reference.relationship.array
```

### 测试用例
```c
// 基本数组元素引用
void basic_array_reference() {
    int array[10] = {0, 1, 2, 3, 4, 5, 6, 7, 8, 9};
    int i = 5;
    
    array[0] = 10;    // array 数组引用，0 索引引用
    array[i] = 20;    // array 数组引用，i 索引引用
    printf("%d\n", array[i]); // array 数组引用，i 索引引用
}

// 多维数组元素引用
void multi_dimensional_array_reference() {
    int matrix[3][3] = {{1, 2, 3}, {4, 5, 6}, {7, 8, 9}};
    int i = 1, j = 2;
    
    matrix[0][0] = 10;     // matrix 数组引用，0, 0 索引引用
    matrix[i][j] = 20;     // matrix 数组引用，i, j 索引引用
    printf("%d\n", matrix[i][j]); // matrix 数组引用，i, j 索引引用
}

// 字符数组引用
void char_array_reference() {
    char str[50] = "Hello";
    int i = 0;
    
    str[0] = 'h';       // str 数组引用，0 索引引用
    str[i] = 'H';       // str 数组引用，i 索引引用
    printf("%c\n", str[i]); // str 数组引用，i 索引引用
}

// 结构体数组引用
void struct_array_reference() {
    struct Point {
        int x;
        int y;
    } points[10];
    int i = 5;
    
    points[0].x = 10;       // points 数组引用，0 索引引用，x 字段引用
    points[i].y = 20;       // points 数组引用，i 索引引用，y 字段引用
    printf("(%d, %d)\n", points[i].x, points[i].y); // points 数组引用，i 索引引用，x, y 字段引用
}

// 指针数组引用
void pointer_array_reference() {
    int* array[10];
    int values[10] = {0, 1, 2, 3, 4, 5, 6, 7, 8, 9};
    int i = 5;
    
    array[0] = &values[0];  // array 数组引用，0 索引引用
    array[i] = &values[i];  // array 数组引用，i 索引引用
    printf("%d\n", *array[i]); // array 数组引用，i 索引引用
}
```

## 5. 指针解引用关系

### 查询规则
```
(pointer_expression
  argument: (identifier) @reference.pointer) @reference.relationship.pointer
```

### 测试用例
```c
// 基本指针解引用
void basic_pointer_dereference() {
    int x = 10;
    int* ptr = &x;
    
    *ptr = 20;        // ptr 指针引用
    printf("%d\n", *ptr); // ptr 指针引用
}

// 双重指针解引用
void double_pointer_dereference() {
    int x = 10;
    int* ptr = &x;
    int** dptr = &ptr;
    
    **dptr = 20;      // dptr 双重指针引用
    printf("%d\n", **dptr); // dptr 双重指针引用
}

// 结构体指针解引用
void struct_pointer_dereference() {
    struct Point {
        int x;
        int y;
    } p = {10, 20};
    struct Point* ptr = &p;
    
    (*ptr).x = 30;    // ptr 结构体指针引用
    (*ptr).y = 40;    // ptr 结构体指针引用
    printf("(%d, %d)\n", (*ptr).x, (*ptr).y); // ptr 结构体指针引用
}

// 函数指针解引用
void function_pointer_dereference() {
    int (*operation)(int, int) = add;
    int result;
    
    result = (*operation)(10, 20); // operation 函数指针引用
    printf("%d\n", result);
}

// 复杂指针解引用
void complex_pointer_dereference() {
    int array[10] = {0, 1, 2, 3, 4, 5, 6, 7, 8, 9};
    int* ptr = array;
    int** dptr = &ptr;
    
    *(*dptr + 1) = 100; // dptr 双重指针引用
    printf("%d\n", *(*dptr + 1)); // dptr 双重指针引用
}
```

## 6. 类型引用关系

### 查询规则
```
(type_identifier) @reference.type
```

### 测试用例
```c
// 基本类型引用
void basic_type_reference() {
    Integer number;      // Integer 类型引用
    Character ch;        // Character 类型引用
    FloatingPoint fp;    // FloatingPoint 类型引用
}

// 结构体类型引用
void struct_type_reference() {
    Point position;      // Point 类型引用
    Rectangle rect;      // Rectangle 类型引用
    LinkedList* list;    // LinkedList 类型引用
}

// 函数指针类型引用
void function_pointer_type_reference() {
    Operation op;        // Operation 类型引用
    Callback cb;         // Callback 类型引用
    StringFunction sf;   // StringFunction 类型引用
}

// 复杂类型引用
void complex_type_reference() {
    BinaryTree* tree;    // BinaryTree 类型引用
    HashTable* table;    // HashTable 类型引用
    GraphNode* node;     // GraphNode 类型引用
}

// 类型别名引用
void typedef_reference() {
    IntPtr int_ptr;      // IntPtr 类型引用
    String text;         // String 类型引用
    Pointer generic_ptr; // Pointer 类型引用
}

// 枚举类型引用
void enum_type_reference() {
    Color shade;         // Color 类型引用
    Status state;        // Status 类型引用
    Priority level;      // Priority 类型引用
}

// 联合体类型引用
void union_type_reference() {
    Data value;          // Data 类型引用
    Variant var;         // Variant 类型引用
}
```

## 7. 枚举常量引用关系

### 查询规则
```
(identifier) @reference.enum.constant
(#match? @reference.enum.constant "^[A-Z][A-Z0-9_]*$")
```

### 测试用例
```c
// 基本枚举常量引用
void basic_enum_reference() {
    Color color = RED;           // RED 枚举常量引用
    Status status = SUCCESS;     // SUCCESS 枚举常量引用
    Priority priority = HIGH;    // HIGH 枚举常量引用
}

// 枚举常量比较
void enum_comparison() {
    Color color = RED;
    
    if (color == RED) {          // RED 枚举常量引用
        printf("Red color\n");
    } else if (color == GREEN) { // GREEN 枚举常量引用
        printf("Green color\n");
    } else if (color == BLUE) {  // BLUE 枚举常量引用
        printf("Blue color\n");
    }
}

// 枚举常量在表达式中
void enum_in_expression() {
    Status status = PENDING;
    
    switch (status) {
        case PENDING:            // PENDING 枚举常量引用
            printf("Pending\n");
            break;
        case PROCESSING:         // PROCESSING 枚举常量引用
            printf("Processing\n");
            break;
        case COMPLETED:          // COMPLETED 枚举常量引用
            printf("Completed\n");
            break;
        case FAILED:             // FAILED 枚举常量引用
            printf("Failed\n");
            break;
    }
}

// 位标志枚举常量引用
void bit_flags_enum_reference() {
    int permissions = READ | WRITE; // READ, WRITE 枚举常量引用
    
    if (permissions & READ) {      // READ 枚举常量引用
        printf("Read permission\n");
    }
    
    if (permissions & WRITE) {     // WRITE 枚举常量引用
        printf("Write permission\n");
    }
    
    if (permissions & EXECUTE) {   // EXECUTE 枚举常量引用
        printf("Execute permission\n");
    }
}

// 复杂枚举常量引用
void complex_enum_reference() {
    LogLevel level = DEBUG;
    
    if (level >= ERROR) {          // ERROR 枚举常量引用
        printf("Error or higher\n");
    } else if (level >= WARNING) { // WARNING 枚举常量引用
        printf("Warning or higher\n");
    } else if (level >= INFO) {    // INFO 枚举常量引用
        printf("Info or higher\n");
    } else {                       // DEBUG 枚举常量引用
        printf("Debug level\n");
    }
}
```

## 8. 函数参数引用关系

### 查询规则
```
(parameter_declaration
  type: (_)
  declarator: (identifier) @reference.parameter) @reference.relationship.parameter
```

### 测试用例
```c
// 基本函数参数引用
void basic_parameter_reference(int x, int y) {
    int sum = x + y;      // x, y 参数引用
    printf("%d\n", sum);
}

// 结构体参数引用
void struct_parameter_reference(Point p, Rectangle rect) {
    printf("Point: (%d, %d)\n", p.x, p.y); // p 参数引用
    printf("Rectangle: (%d, %d) to (%d, %d)\n", 
           rect.top_left.x, rect.top_left.y, 
           rect.bottom_right.x, rect.bottom_right.y); // rect 参数引用
}

// 指针参数引用
void pointer_parameter_reference(int* ptr, char* str) {
    *ptr = 100;           // ptr 参数引用
    str[0] = 'H';         // str 参数引用
    printf("%d, %c\n", *ptr, str[0]);
}

// 数组参数引用
void array_parameter_reference(int array[], int size) {
    for (int i = 0; i < size; i++) { // size 参数引用
        printf("%d ", array[i]);      // array 参数引用
    }
    printf("\n");
}

// 函数指针参数引用
void function_pointer_parameter_reference(Operation op, Callback cb) {
    int result = op(10, 20);  // op 参数引用
    cb();                     // cb 参数引用
    printf("%d\n", result);
}

// 复杂参数引用
void complex_parameter_reference(LinkedList* list, HashTable* table, 
                                BinaryTree* tree, GraphNode* node) {
    list->data = 10;          // list 参数引用
    table->size = 100;        // table 参数引用
    tree->value = 50;         // tree 参数引用
    node->data = 75;          // node 参数引用
}
```

## 9. 局部变量引用关系

### 查询规则
```
(declaration
  type: (_)
  declarator: (identifier) @reference.local.variable) @reference.relationship.local
```

### 测试用例
```c
// 基本局部变量引用
void basic_local_variable_reference() {
    int a = 10;          // a 局部变量引用
    int b = 20;          // b 局部变量引用
    int c = a + b;       // a, b, c 局部变量引用
    printf("%d\n", c);   // c 局部变量引用
}

// 复杂局部变量引用
void complex_local_variable_reference() {
    int x = 5, y = 10, z = 15; // x, y, z 局部变量引用
    int result;                 // result 局部变量引用
    
    result = x * y + z;         // x, y, z, result 局部变量引用
    result = result / x;        // result, x 局部变量引用
    y = result + z;             // y, result, z 局部变量引用
}

// 结构体局部变量引用
void struct_local_variable_reference() {
    struct Point p = {10, 20};  // p 局部变量引用
    printf("(%d, %d)\n", p.x, p.y); // p 局部变量引用
}

// 数组局部变量引用
void array_local_variable_reference() {
    int array[10] = {0, 1, 2, 3, 4, 5, 6, 7, 8, 9}; // array 局部变量引用
    for (int i = 0; i < 10; i++) { // i 局部变量引用
        printf("%d ", array[i]);  // array, i 局部变量引用
    }
    printf("\n");
}

// 指针局部变量引用
void pointer_local_variable_reference() {
    int x = 10;          // x 局部变量引用
    int* ptr = &x;       // ptr 局部变量引用
    *ptr = 20;           // ptr 局部变量引用
    printf("%d\n", *ptr); // ptr 局部变量引用
}

// 嵌套作用域局部变量引用
void nested_scope_local_variable_reference() {
    int x = 10;          // x 局部变量引用
    
    {
        int y = 20;      // y 局部变量引用
        int z = x + y;   // x, y, z 局部变量引用
        printf("%d\n", z); // z 局部变量引用
    }
    
    printf("%d\n", x);   // x 局部变量引用
}
```

## 10. 全局变量引用关系

### 查询规则
```
(identifier) @reference.global.variable
(#match? @reference.global.variable "^[gG][a-zA-Z0-9_]*$")
```

### 测试用例
```c
// 全局变量定义
int g_global_counter = 0;
char g_global_buffer[256];
float g_global_temperature = 0.0f;

// 基本全局变量引用
void basic_global_variable_reference() {
    g_global_counter++;        // g_global_counter 全局变量引用
    g_global_temperature = 25.5f; // g_global_temperature 全局变量引用
    printf("Counter: %d\n", g_global_counter); // g_global_counter 全局变量引用
}

// 全局变量在函数中引用
void global_variable_in_function() {
    static int local_static = 0;
    
    g_global_counter += local_static; // g_global_counter 全局变量引用
    local_static++;
    
    sprintf(g_global_buffer, "Count: %d", g_global_counter); // g_global_buffer 全局变量引用
    printf("%s\n", g_global_buffer); // g_global_buffer 全局变量引用
}

// 全局变量在多个函数中引用
void function1() {
    g_global_counter = 10;    // g_global_counter 全局变量引用
}

void function2() {
    g_global_counter += 5;    // g_global_counter 全局变量引用
    printf("Counter: %d\n", g_global_counter); // g_global_counter 全局变量引用
}

// 复杂全局变量引用
struct Point g_global_origin = {0, 0};
LinkedList* g_global_list = NULL;
HashTable* g_global_table = NULL;

void complex_global_variable_reference() {
    g_global_origin.x = 10;   // g_global_origin 全局变量引用
    g_global_origin.y = 20;   // g_global_origin 全局变量引用
    
    if (!g_global_list) {     // g_global_list 全局变量引用
        g_global_list = create_linked_list(); // g_global_list 全局变量引用
    }
    
    if (!g_global_table) {    // g_global_table 全局变量引用
        g_global_table = create_hash_table(100); // g_global_table 全局变量引用
    }
}

// 全局变量数组引用
int g_global_scores[100];
char g_global_names[50][50];

void global_array_reference() {
    for (int i = 0; i < 100; i++) {
        g_global_scores[i] = i; // g_global_scores 全局变量引用
    }
    
    strcpy(g_global_names[0], "John"); // g_global_names 全局变量引用
    strcpy(g_global_names[1], "Jane"); // g_global_names 全局变量引用
}
```

## 11. 静态变量引用关系

### 查询规则
```
(identifier) @reference.static.variable
(#match? @reference.static.variable "^[sS][a-zA-Z0-9_]*$")
```

### 测试用例
```c
// 静态变量定义
static int s_static_counter = 0;
static char s_static_buffer[256];
static float s_static_value = 0.0f;

// 基本静态变量引用
void basic_static_variable_reference() {
    s_static_counter++;        // s_static_counter 静态变量引用
    s_static_value = 25.5f;    // s_static_value 静态变量引用
    printf("Counter: %d\n", s_static_counter); // s_static_counter 静态变量引用
}

// 函数内静态变量引用
void function_static_variable_reference() {
    static int s_local_static = 0; // s_local_static 静态变量引用
    
    s_local_static++;             // s_local_static 静态变量引用
    s_static_counter += s_local_static; // s_static_counter, s_local_static 静态变量引用
    
    sprintf(s_static_buffer, "Count: %d", s_static_counter); // s_static_buffer 静态变量引用
    printf("%s\n", s_static_buffer); // s_static_buffer 静态变量引用
}

// 多个函数中的静态变量引用
void static_function1() {
    static int s_function1_counter = 0; // s_function1_counter 静态变量引用
    s_function1_counter++;              // s_function1_counter 静态变量引用
    printf("Function1 counter: %d\n", s_function1_counter); // s_function1_counter 静态变量引用
}

void static_function2() {
    static int s_function2_counter = 0; // s_function2_counter 静态变量引用
    s_function2_counter++;              // s_function2_counter 静态变量引用
    printf("Function2 counter: %d\n", s_function2_counter); // s_function2_counter 静态变量引用
}

// 复杂静态变量引用
static struct Point s_static_origin = {0, 0};
static LinkedList* s_static_list = NULL;
static HashTable* s_static_table = NULL;

void complex_static_variable_reference() {
    s_static_origin.x = 10;   // s_static_origin 静态变量引用
    s_static_origin.y = 20;   // s_static_origin 静态变量引用
    
    if (!s_static_list) {     // s_static_list 静态变量引用
        s_static_list = create_linked_list(); // s_static_list 静态变量引用
    }
    
    if (!s_static_table) {    // s_static_table 静态变量引用
        s_static_table = create_hash_table(100); // s_static_table 静态变量引用
    }
}

// 静态变量数组引用
static int s_static_scores[100];
static char s_static_names[50][50];

void static_array_reference() {
    for (int i = 0; i < 100; i++) {
        s_static_scores[i] = i; // s_static_scores 静态变量引用
    }
    
    strcpy(s_static_names[0], "Alice"); // s_static_names 静态变量引用
    strcpy(s_static_names[1], "Bob");   // s_static_names 静态变量引用
}
```

## 12. 宏引用关系

### 查询规则
```
(identifier) @reference.macro
(#match? @reference.macro "^[A-Z_][A-Z0-9_]*$")
```

### 测试用例
```c
// 宏定义
#define MAX_SIZE 100
#define BUFFER_SIZE 256
#define PI 3.14159265359

// 基本宏引用
void basic_macro_reference() {
    int array[MAX_SIZE];         // MAX_SIZE 宏引用
    char buffer[BUFFER_SIZE];    // BUFFER_SIZE 宏引用
    float area = PI * 10 * 10;   // PI 宏引用
}

// 宏函数引用
void macro_function_reference() {
    int a = 10, b = 20;
    int max_val = MAX(a, b);     // MAX 宏引用
    int min_val = MIN(a, b);     // MIN 宏引用
    int abs_val = ABS(-5);       // ABS 宏引用
}

// 条件宏引用
void conditional_macro_reference() {
#ifdef DEBUG
    printf("Debug mode\n");      // DEBUG 宏引用
    int debug_level = DEBUG_LEVEL; // DEBUG_LEVEL 宏引用
#endif

#if VERSION >= 2
    printf("Version 2 or higher\n"); // VERSION 宏引用
#endif
}

// 复杂宏引用
void complex_macro_reference() {
    int size = ARRAY_SIZE(array); // ARRAY_SIZE 宏引用
    int result = SQUARE(5);       // SQUARE 宏引用
    
    SAFE_FREE(ptr);               // SAFE_FREE 宏引用
    LOG_ERROR("Error occurred");  // LOG_ERROR 宏引用
}

// 位标志宏引用
void bit_flags_macro_reference() {
    int flags = READ | WRITE;     // READ, WRITE 宏引用
    
    if (flags & READ) {           // READ 宏引用
        printf("Read permission\n");
    }
    
    if (flags & WRITE) {          // WRITE 宏引用
        printf("Write permission\n");
    }
    
    if (flags & EXECUTE) {        // EXECUTE 宏引用
        printf("Execute permission\n");
    }
}

// 平台相关宏引用
void platform_macro_reference() {
#ifdef WINDOWS
    printf("Windows platform\n"); // WINDOWS 宏引用
    system("pause");              // system 函数调用
#elif LINUX
    printf("Linux platform\n");   // LINUX 宏引用
    system("read -p 'Press Enter to continue...' var"); // system 函数调用
#endif
}
```

## 13. 标签引用关系

### 查询规则
```
(goto_statement
  label: (statement_identifier) @reference.label) @reference.relationship.goto
```

### 测试用例
```c
// 基本标签引用
void basic_label_reference() {
    int i = 0;
    
start:                      // 标签定义
    printf("%d\n", i);
    i++;
    
    if (i < 10) {
        goto start;          // start 标签引用
    }
}

// 多个标签引用
void multiple_label_reference() {
    int i = 0;
    
start:                      // 标签定义
    printf("Start: %d\n", i);
    
    if (i < 5) {
        i++;
        goto start;          // start 标签引用
    }
    
middle:                     // 标签定义
    printf("Middle: %d\n", i);
    
    if (i < 8) {
        i++;
        goto middle;         // middle 标签引用
    }
    
end:                        // 标签定义
    printf("End: %d\n", i);
}

// 嵌套标签引用
void nested_label_reference() {
    int i = 0, j = 0;
    
outer_start:                // 标签定义
    printf("Outer start: %d, %d\n", i, j);
    
inner_start:                // 标签定义
    printf("Inner start: %d, %d\n", i, j);
    j++;
    
    if (j < 3) {
        goto inner_start;    // inner_start 标签引用
    }
    
    i++;
    j = 0;
    
    if (i < 3) {
        goto outer_start;    // outer_start 标签引用
    }
}

// 错误处理标签引用
void error_handling_label_reference() {
    FILE* file = fopen("test.txt", "r");
    
    if (file == NULL) {
        goto error;          // error 标签引用
    }
    
    // 文件操作
    if (fread(buffer, 1, sizeof(buffer), file) == 0) {
        goto close_file;     // close_file 标签引用
    }
    
    // 处理数据
    process_data(buffer);
    
close_file:                 // 标签定义
    fclose(file);
    return;
    
error:                      // 标签定义
    printf("Error occurred\n");
}

// 复杂标签引用
void complex_label_reference() {
    int state = 0;
    
start:                      // 标签定义
    switch (state) {
        case 0:
            printf("State 0\n");
            state = 1;
            goto start;      // start 标签引用
        case 1:
            printf("State 1\n");
            state = 2;
            goto start;      // start 标签引用
        case 2:
            printf("State 2\n");
            goto end;        // end 标签引用
    }
    
end:                        // 标签定义
    printf("End of program\n");
}
```

## 14. case标签引用关系

### 查询规则
```
(case_statement
  value: (identifier) @reference.case.label) @reference.relationship.case
```

### 测试用例
```c
// 枚举定义
enum Color {
    RED,
    GREEN,
    BLUE
};

enum Status {
    PENDING,
    PROCESSING,
    COMPLETED,
    FAILED
};

// 基本case标签引用
void basic_case_label_reference() {
    enum Color color = RED;
    
    switch (color) {
        case RED:             // RED case标签引用
            printf("Red color\n");
            break;
        case GREEN:           // GREEN case标签引用
            printf("Green color\n");
            break;
        case BLUE:            // BLUE case标签引用
            printf("Blue color\n");
            break;
    }
}

// 多个case标签引用
void multiple_case_label_reference() {
    enum Status status = PENDING;
    
    switch (status) {
        case PENDING:         // PENDING case标签引用
            printf("Pending\n");
            break;
        case PROCESSING:      // PROCESSING case标签引用
            printf("Processing\n");
            break;
        case COMPLETED:       // COMPLETED case标签引用
            printf("Completed\n");
            break;
        case FAILED:          // FAILED case标签引用
            printf("Failed\n");
            break;
    }
}

// 多个case共享代码
void shared_case_label_reference() {
    int day = 1;
    
    switch (day) {
        case 1:               // 1 case标签引用
        case 2:               // 2 case标签引用
        case 3:               // 3 case标签引用
            printf("Weekday\n");
            break;
        case 4:               // 4 case标签引用
        case 5:               // 5 case标签引用
        case 6:               // 6 case标签引用
        case 7:               // 7 case标签引用
            printf("Weekend\n");
            break;
    }
}

// 嵌套switch中的case标签引用
void nested_case_label_reference() {
    enum Color color = RED;
    enum Status status = PENDING;
    
    switch (color) {
        case RED:             // RED case标签引用
            switch (status) {
                case PENDING:     // PENDING case标签引用
                    printf("Red and pending\n");
                    break;
                case PROCESSING:  // PROCESSING case标签引用
                    printf("Red and processing\n");
                    break;
            }
            break;
        case GREEN:           // GREEN case标签引用
            printf("Green\n");
            break;
    }
}

// 复杂case标签引用
void complex_case_label_reference() {
    int option = 1;
    
    switch (option) {
        case 1:               // 1 case标签引用
            printf("Option 1\n");
            // fall through
        case 2:               // 2 case标签引用
            printf("Option 2\n");
            break;
        case 3:               // 3 case标签引用
            printf("Option 3\n");
            if (some_condition) {
                goto case_5;  // case_5 标签引用
            }
            break;
        case 4:               // 4 case标签引用
            printf("Option 4\n");
            break;
        case 5:               // 5 case标签引用
case_5:                     // 标签定义
            printf("Option 5\n");
            break;
    }
}
```

## 15. 函数指针引用关系

### 查询规则
```
(pointer_declarator
  declarator: (identifier) @reference.function.pointer) @reference.relationship.function.pointer
```

### 测试用例
```c
// 基本函数指针引用
void basic_function_pointer_reference() {
    int (*operation)(int, int);  // operation 函数指针引用
    operation = add;             // operation 函数指针引用
    int result = operation(10, 20); // operation 函数指针引用
    printf("%d\n", result);
}

// 函数指针数组引用
void function_pointer_array_reference() {
    int (*operations[4])(int, int); // operations 函数指针数组引用
    operations[0] = add;            // operations 函数指针数组引用
    operations[1] = subtract;       // operations 函数指针数组引用
    operations[2] = multiply;       // operations 函数指针数组引用
    operations[3] = divide;         // operations 函数指针数组引用
    
    for (int i = 0; i < 4; i++) {
        int result = operations[i](10, 5); // operations 函数指针数组引用
        printf("Operation %d result: %d\n", i, result);
    }
}

// 结构体中的函数指针引用
void struct_function_pointer_reference() {
    struct Calculator {
        int (*add)(int, int);      // add 函数指针引用
        int (*subtract)(int, int); // subtract 函数指针引用
        int (*multiply)(int, int); // multiply 函数指针引用
        int (*divide)(int, int);   // divide 函数指针引用
    } calculator;
    
    calculator.add = add_function;        // calculator.add 函数指针引用
    calculator.subtract = subtract_function; // calculator.subtract 函数指针引用
    calculator.multiply = multiply_function; // calculator.multiply 函数指针引用
    calculator.divide = divide_function;   // calculator.divide 函数指针引用
    
    int sum = calculator.add(10, 20);     // calculator.add 函数指针引用
    printf("Sum: %d\n", sum);
}

// 函数指针参数引用
void function_pointer_parameter_reference(int (*operation)(int, int), Callback callback) {
    int result = operation(10, 20);  // operation 函数指针引用
    printf("Result: %d\n", result);
    callback();                     // callback 函数指针引用
}

// 复杂函数指针引用
void complex_function_pointer_reference() {
    int (*operations[])(int, int) = {add, subtract, multiply, divide}; // operations 函数指针数组引用
    int (*get_operation(int))(int, int); // get_operation 函数指针引用
    
    for (int i = 0; i < 4; i++) {
        int (*op)(int, int) = get_operation(i); // op 函数指针引用
        int result = op(10, 5); // op 函数指针引用
        printf("Operation %d result: %d\n", i, result);
    }
}

// 返回函数指针的函数
int (*get_function_pointer(char op))(int, int) {
    switch (op) {
        case '+': return add;      // add 函数指针引用
        case '-': return subtract; // subtract 函数指针引用
        case '*': return multiply; // multiply 函数指针引用
        case '/': return divide;   // divide 函数指针引用
        default: return NULL;
    }
}

void return_function_pointer_reference() {
    int (*operation)(int, int) = get_function_pointer('+'); // operation 函数指针引用
    if (operation) {
        int result = operation(10, 20); // operation 函数指针引用
        printf("Result: %d\n", result);
    }
}
```

## 16. 结构体类型引用关系

### 查询规则
```
(struct_specifier
  name: (type_identifier) @reference.struct.type) @reference.relationship.struct
```

### 测试用例
```c
// 基本结构体类型引用
struct Point {              // Point 结构体类型引用
    int x;
    int y;
};

struct Rectangle {          // Rectangle 结构体类型引用
    struct Point top_left;   // Point 结构体类型引用
    struct Point bottom_right; // Point 结构体类型引用
};

// 嵌套结构体类型引用
struct Address {            // Address 结构体类型引用
    char street[50];
    char city[20];
    char country[20];
};

struct Person {             // Person 结构体类型引用
    char name[50];
    int age;
    struct Address address;  // Address 结构体类型引用
};

// 自引用结构体类型引用
struct Node {               // Node 结构体类型引用
    int data;
    struct Node* next;      // Node 结构体类型引用
};

struct LinkedList {         // LinkedList 结构体类型引用
    struct Node* head;      // Node 结构体类型引用
    int count;
};

// 复杂结构体类型引用
struct TreeNode {           // TreeNode 结构体类型引用
    int value;
    struct TreeNode* left;  // TreeNode 结构体类型引用
    struct TreeNode* right; // TreeNode 结构体类型引用
    struct TreeNode* parent; // TreeNode 结构体类型引用
};

struct BinaryTree {         // BinaryTree 结构体类型引用
    struct TreeNode* root;  // TreeNode 结构体类型引用
    int count;
};

// 函数参数中的结构体类型引用
void process_point(struct Point p) { // Point 结构体类型引用
    printf("(%d, %d)\n", p.x, p.y);
}

void process_person(struct Person person) { // Person 结构体类型引用
    printf("%s, %d years old\n", person.name, person.age);
}

// 函数返回值中的结构体类型引用
struct Point create_point(int x, int y) { // Point 结构体类型引用
    struct Point p = {x, y}; // Point 结构体类型引用
    return p;
}

struct Person create_person(char* name, int age) { // Person 结构体类型引用
    struct Person person; // Person 结构体类型引用
    strcpy(person.name, name);
    person.age = age;
    return person;
}

// 指针中的结构体类型引用
void struct_pointer_reference() {
    struct Point* ptr = malloc(sizeof(struct Point)); // Point 结构体类型引用
    struct Person* person_ptr = malloc(sizeof(struct Person)); // Person 结构体类型引用
    
    ptr->x = 10;
    ptr->y = 20;
    
    person_ptr->age = 30;
    strcpy(person_ptr->name, "John");
    
    free(ptr);
    free(person_ptr);
}
```

## 17. 联合体类型引用关系

### 查询规则
```
(union_specifier
  name: (type_identifier) @reference.union.type) @reference.relationship.union
```

### 测试用例
```c
// 基本联合体类型引用
union Data {                // Data 联合体类型引用
    int integer;
    float floating;
    char* string;
};

// 复杂联合体类型引用
union Variant {             // Variant 联合体类型引用
    int int_value;
    float float_value;
    char char_value;
    double double_value;
    void* pointer_value;
};

// 嵌套联合体类型引用
union NumericValue {         // NumericValue 联合体类型引用
    int int_value;
    float float_value;
    double double_value;
};

union ComplexData {          // ComplexData 联合体类型引用
    char type;
    union NumericValue numeric; // NumericValue 联合体类型引用
    char* text;
};

// 结构体中的联合体类型引用
struct Container {           // Container 结构体类型引用
    int type;
    union Data data;         // Data 联合体类型引用
    union Variant variant;   // Variant 联合体类型引用
};

// 函数参数中的联合体类型引用
void process_data(union Data data) { // Data 联合体类型引用
    printf("Data processed\n");
}

void process_variant(union Variant variant) { // Variant 联合体类型引用
    printf("Variant processed\n");
}

// 函数返回值中的联合体类型引用
union Data create_data(int value) { // Data 联合体类型引用
    union Data data; // Data 联合体类型引用
    data.integer = value;
    return data;
}

union Variant create_variant(float value) { // Variant 联合体类型引用
    union Variant variant; // Variant 联合体类型引用
    variant.float_value = value;
    return variant;
}

// 指针中的联合体类型引用
void union_pointer_reference() {
    union Data* data_ptr = malloc(sizeof(union Data)); // Data 联合体类型引用
    union Variant* variant_ptr = malloc(sizeof(union Variant)); // Variant 联合体类型引用
    
    data_ptr->integer = 42;
    variant_ptr->float_value = 3.14f;
    
    free(data_ptr);
    free(variant_ptr);
}

// 联合体数组引用
void union_array_reference() {
    union Data data_array[10]; // Data 联合体类型引用
    union Variant variant_array[5]; // Variant 联合体类型引用
    
    for (int i = 0; i < 10; i++) {
        data_array[i].integer = i; // data_array 联合体数组引用
    }
    
    for (int i = 0; i < 5; i++) {
        variant_array[i].float_value = i * 1.5f; // variant_array 联合体数组引用
    }
}
```

## 18. 枚举类型引用关系

### 查询规则
```
(enum_specifier
  name: (type_identifier) @reference.enum.type) @reference.relationship.enum
```

### 测试用例
```c
// 基本枚举类型引用
enum Color {                // Color 枚举类型引用
    RED,
    GREEN,
    BLUE
};

// 复杂枚举类型引用
enum Status {               // Status 枚举类型引用
    PENDING,
    PROCESSING,
    COMPLETED,
    FAILED
};

// 带显式值的枚举类型引用
enum Priority {             // Priority 枚举类型引用
    LOW = 1,
    MEDIUM = 5,
    HIGH = 10,
    CRITICAL = 20
};

// 位标志枚举类型引用
enum Permissions {          // Permissions 枚举类型引用
    READ = 0x01,
    WRITE = 0x02,
    EXECUTE = 0x04,
    DELETE = 0x08,
    CREATE = 0x10
};

// 函数参数中的枚举类型引用
void process_color(enum Color color) { // Color 枚举类型引用
    switch (color) {
        case RED:
            printf("Red color\n");
            break;
        case GREEN:
            printf("Green color\n");
            break;
        case BLUE:
            printf("Blue color\n");
            break;
    }
}

void process_status(enum Status status) { // Status 枚举类型引用
    switch (status) {
        case PENDING:
            printf("Pending\n");
            break;
        case PROCESSING:
            printf("Processing\n");
            break;
        case COMPLETED:
            printf("Completed\n");
            break;
        case FAILED:
            printf("Failed\n");
            break;
    }
}

// 函数返回值中的枚举类型引用
enum Color get_primary_color() { // Color 枚举类型引用
    return RED;
}

enum Status get_operation_status() { // Status 枚举类型引用
    return COMPLETED;
}

// 结构体中的枚举类型引用
struct Task {                 // Task 结构体类型引用
    char name[50];
    enum Priority priority;   // Priority 枚举类型引用
    enum Status status;       // Status 枚举类型引用
};

// 指针中的枚举类型引用
void enum_pointer_reference() {
    enum Color* color_ptr = malloc(sizeof(enum Color)); // Color 枚举类型引用
    enum Status* status_ptr = malloc(sizeof(enum Status)); // Status 枚举类型引用
    
    *color_ptr = BLUE;
    *status_ptr = PROCESSING;
    
    free(color_ptr);
    free(status_ptr);
}

// 枚举数组引用
void enum_array_reference() {
    enum Color colors[10]; // Color 枚举类型引用
    enum Status statuses[5]; // Status 枚举类型引用
    
    for (int i = 0; i < 10; i++) {
        colors[i] = i % 3; // colors 枚举数组引用
    }
    
    for (int i = 0; i < 5; i++) {
        statuses[i] = i % 4; // statuses 枚举数组引用
    }
}
```

## 19. 类型别名引用关系

### 查询规则
```
(type_identifier) @reference.type.alias
```

### 测试用例
```c
// 基本类型别名引用
typedef int Integer;         // Integer 类型别名引用
typedef char Character;      // Character 类型别名引用
typedef float FloatingPoint; // FloatingPoint 类型别名引用

// 结构体类型别名引用
typedef struct Point Point;  // Point 类型别名引用
typedef struct Rectangle Rectangle; // Rectangle 类型别名引用

// 指针类型别名引用
typedef int* IntPtr;         // IntPtr 类型别名引用
typedef char* String;        // String 类型别名引用
typedef void* Pointer;       // Pointer 类型别名引用

// 函数指针类型别名引用
typedef int (*Operation)(int, int); // Operation 类型别名引用
typedef void (*Callback)(void*);     // Callback 类型别名引用

// 使用类型别名
void use_type_aliases() {
    Integer number = 10;         // Integer 类型别名引用
    Character ch = 'A';          // Character 类型别名引用
    FloatingPoint fp = 3.14f;    // FloatingPoint 类型别名引用
    
    Point position;              // Point 类型别名引用
    Rectangle rect;              // Rectangle 类型别名引用
    
    IntPtr int_ptr = &number;    // IntPtr 类型别名引用
    String text = "Hello";       // String 类型别名引用
    Pointer generic_ptr = &fp;   // Pointer 类型别名引用
    
    Operation op = add;          // Operation 类型别名引用
    Callback cb = callback_func; // Callback 类型别名引用
}

// 函数参数中的类型别名引用
void process_integer(Integer value) { // Integer 类型别名引用
    printf("Value: %d\n", value);
}

void process_point(Point p) { // Point 类型别名引用
    printf("(%d, %d)\n", p.x, p.y);
}

void set_callback(Callback callback) { // Callback 类型别名引用
    callback();
}

// 函数返回值中的类型别名引用
Integer create_integer(int value) { // Integer 类型别名引用
    return value;
}

Point create_point(int x, int y) { // Point 类型别名引用
    Point p = {x, y}; // Point 类型别名引用
    return p;
}

// 复杂类型别名引用
typedef struct LinkedList LinkedList; // LinkedList 类型别名引用
typedef struct HashTable HashTable;   // HashTable 类型别名引用
typedef struct BinaryTree BinaryTree; // BinaryTree 类型别名引用

void complex_type_aliases() {
    LinkedList* list = NULL;    // LinkedList 类型别名引用
    HashTable* table = NULL;    // HashTable 类型别名引用
    BinaryTree* tree = NULL;    // BinaryTree 类型别名引用
    
    list = create_linked_list(); // LinkedList 类型别名引用
    table = create_hash_table(100); // HashTable 类型别名引用
    tree = create_binary_tree(); // BinaryTree 类型别名引用
}
```

## 20. 函数声明引用关系

### 查询规则
```
(declaration
  type: (_)
  declarator: (function_declarator
    declarator: (identifier) @reference.function.declaration)) @reference.relationship.function.declaration
```

### 测试用例
```c
// 基本函数声明引用
int calculate(int x, int y);    // calculate 函数声明引用
void process_data(void);        // process_data 函数声明引用
char* get_string(void);         // get_string 函数声明引用

// 带自定义类型参数的函数声明引用
Point create_point(int x, int y); // create_point 函数声明引用
Rectangle create_rectangle(Point top_left, Point bottom_right); // create_rectangle 函数声明引用
LinkedList* create_linked_list(void); // create_linked_list 函数声明引用

// 复杂函数声明引用
BinaryTree* insert_node(BinaryTree* tree, int value); // insert_node 函数声明引用
HashTable* create_hash_table(int size); // create_hash_table 函数声明引用
TreeNode* build_tree(int* array, int size); // build_tree 函数声明引用

// 函数指针参数声明引用
void set_callback(Callback callback); // set_callback 函数声明引用
void register_handler(EventHandler handler); // register_handler 函数声明引用
void set_operation(Operation operation); // set_operation 函数声明引用

// 返回自定义类型的函数声明引用
Point get_center(Rectangle rect); // get_center 函数声明引用
LinkedList* reverse_list(LinkedList* list); // reverse_list 函数声明引用
Data transform_data(Data original, Transformer transformer); // transform_data 函数声明引用

// 复杂参数和返回类型声明引用
TreeNode* balance_tree(TreeNode* unbalanced_tree); // balance_tree 函数声明引用
LinkedList* filter_list(LinkedList* list, FilterFunction filter); // filter_list 函数声明引用
HashTable* merge_tables(HashTable* table1, HashTable* table2); // merge_tables 函数声明引用

// 带可变参数的函数声明引用
int printf(const char* format, ...); // printf 函数声明引用
int sprintf(char* str, const char* format, ...); // sprintf 函数声明引用
int scanf(const char* format, ...); // scanf 函数声明引用

// 标准库函数声明引用
void* malloc(size_t size); // malloc 函数声明引用
void free(void* ptr); // free 函数声明引用
void* memcpy(void* dest, const void* src, size_t n); // memcpy 函数声明引用
int strcmp(const char* str1, const char* str2); // strcmp 函数声明引用
```

## 21. 返回语句中的引用关系

### 查询规则
```
(return_statement
  (identifier) @reference.return.variable) @reference.relationship.return
```

### 测试用例
```c
// 基本返回语句引用
int basic_return_reference() {
    int result = 42;
    return result;        // result 变量引用
}

int expression_return_reference() {
    int a = 10, b = 20;
    return a + b;         // a, b 变量引用
}

// 结构体返回引用
Point struct_return_reference() {
    Point p = {10, 20};
    return p;             // p 变量引用
}

// 指针返回引用
int* pointer_return_reference() {
    static int value = 100;
    return &value;        // value 变量引用
}

// 条件返回引用
int conditional_return_reference(int x) {
    int result = x * 2;
    if (result > 50) {
        return result;    // result 变量引用
    } else {
        return x;         // x 变量引用
    }
}

// 复杂返回引用
int complex_return_reference() {
    int a = 10, b = 20, c = 30;
    int sum = a + b + c;
    int product = a * b * c;
    
    if (sum > product) {
        return sum;       // sum 变量引用
    } else {
        return product;   // product 变量引用
    }
}

// 函数调用返回引用
int function_call_return_reference() {
    int x = 10, y = 20;
    int result = calculate(x, y);
    return result;        // result 变量引用
}

// 全局变量返回引用
int global_variable_return_reference() {
    extern int global_counter;
    return global_counter; // global_counter 变量引用
}

// 静态变量返回引用
int* static_variable_return_reference() {
    static int static_value = 42;
    return &static_value; // static_value 变量引用
}

// 参数返回引用
int parameter_return_reference(int x) {
    int result = x * 2;
    return result;        // result 变量引用
}
```

## 22. 赋值表达式中的引用关系

### 查询规则
```
(assignment_expression
  left: (identifier) @reference.assignment.left
  right: (identifier) @reference.assignment.right) @reference.relationship.assignment
```

### 测试用例
```c
// 基本赋值表达式引用
void basic_assignment_reference() {
    int a = 10, b = 20;
    a = b;               // a, b 变量引用
    b = a;               // a, b 变量引用
}

// 复杂赋值表达式引用
void complex_assignment_reference() {
    int a = 10, b = 20, c = 30;
    a = b;               // a, b 变量引用
    b = c;               // b, c 变量引用
    c = a;               // c, a 变量引用
}

// 结构体赋值表达式引用
void struct_assignment_reference() {
    Point p1 = {10, 20};
    Point p2;
    p2 = p1;             // p2, p1 变量引用
}

// 指针赋值表达式引用
void pointer_assignment_reference() {
    int x = 10, y = 20;
    int* ptr1 = &x;
    int* ptr2;
    ptr2 = ptr1;         // ptr2, ptr1 变量引用
    *ptr2 = y;           // ptr2 变量引用
}

// 数组元素赋值表达式引用
void array_assignment_reference() {
    int array1[10] = {0, 1, 2, 3, 4, 5, 6, 7, 8, 9};
    int array2[10];
    int i = 5;
    
    array2[i] = array1[i]; // array2, array1, i 变量引用
    array1[0] = array2[9]; // array1, array2 变量引用
}

// 全局变量赋值表达式引用
void global_assignment_reference() {
    extern int global_counter;
    int local_counter = 100;
    global_counter = local_counter; // global_counter, local_counter 变量引用
}

// 静态变量赋值表达式引用
void static_assignment_reference() {
    static int static_counter = 0;
    int local_counter = 50;
    static_counter = local_counter; // static_counter, local_counter 变量引用
}

// 函数参数赋值表达式引用
void parameter_assignment_reference(int x, int y) {
    int result;
    result = x;          // result, x 变量引用
    x = y;               // x, y 变量引用
    y = result;          // y, result 变量引用
}

// 复合赋值表达式引用
void compound_assignment_reference() {
    int a = 10, b = 20;
    a += b;              // a, b 变量引用
    b -= a;              // b, a 变量引用
    a *= b;              // a, b 变量引用
}
```

## 23. 条件表达式中的引用关系

### 查询规则
```
(if_statement
  condition: (identifier) @reference.condition.variable) @reference.relationship.condition
```

### 测试用例
```c
// 基本条件表达式引用
void basic_condition_reference() {
    int x = 10;
    if (x > 5) {          // x 变量引用
        printf("x is greater than 5\n");
    }
}

// 复杂条件表达式引用
void complex_condition_reference() {
    int a = 10, b = 20, c = 30;
    if (a > b) {          // a, b 变量引用
        printf("a is greater than b\n");
    } else if (b > c) {   // b, c 变量引用
        printf("b is greater than c\n");
    } else {
        printf("c is the greatest\n");
    }
}

// 嵌套条件表达式引用
void nested_condition_reference() {
    int a = 10, b = 20, c = 30;
    if (a > b) {          // a, b 变量引用
        if (a > c) {      // a, c 变量引用
            printf("a is the greatest\n");
        } else {
            printf("c is greater than a\n");
        }
    } else {
        printf("b is greater than or equal to a\n");
    }
}

// 逻辑条件表达式引用
void logical_condition_reference() {
    int a = 10, b = 20, c = 30;
    if (a > b && b > c) { // a, b, c 变量引用
        printf("a > b > c\n");
    } else if (a > b || b > c) { // a, b, c 变量引用
        printf("Either a > b or b > c\n");
    }
}

// 函数参数条件表达式引用
void parameter_condition_reference(int x, int y) {
    if (x > y) {          // x, y 变量引用
        printf("x is greater than y\n");
    } else {
        printf("y is greater than or equal to x\n");
    }
}

// 全局变量条件表达式引用
void global_condition_reference() {
    extern int global_counter;
    if (global_counter > 100) { // global_counter 变量引用
        printf("Global counter is greater than 100\n");
    }
}

// 静态变量条件表达式引用
void static_condition_reference() {
    static int static_counter = 0;
    static_counter++;
    
    if (static_counter > 10) { // static_counter 变量引用
        printf("Static counter is greater than 10\n");
    }
}

// 结构体字段条件表达式引用
void struct_field_condition_reference() {
    Point p = {10, 20};
    if (p.x > p.y) {      // p 变量引用，x, y 字段引用
        printf("x is greater than y\n");
    } else {
        printf("y is greater than or equal to x\n");
    }
}

// 数组元素条件表达式引用
void array_element_condition_reference() {
    int array[10] = {0, 1, 2, 3, 4, 5, 6, 7, 8, 9};
    int i = 5;
    
    if (array[i] > 5) {   // array, i 变量引用
        printf("array[%d] is greater than 5\n", i);
    }
}
```

## 24. 循环中的引用关系

### 查询规则
```
[
  (for_statement
    condition: (identifier) @reference.loop.condition)
  (while_statement
    condition: (identifier) @reference.loop.condition)
  (do_statement
    condition: (identifier) @reference.loop.condition)
] @reference.relationship.loop
```

### 测试用例
```c
// for循环中的引用
void for_loop_reference() {
    int i = 0;
    int limit = 10;
    
    for (i = 0; i < limit; i++) { // i, limit 变量引用
        printf("%d ", i);
    }
    printf("\n");
}

// while循环中的引用
void while_loop_reference() {
    int counter = 10;
    
    while (counter > 0) { // counter 变量引用
        printf("%d ", counter);
        counter--;
    }
    printf("\n");
}

// do-while循环中的引用
void do_while_loop_reference() {
    int counter = 0;
    
    do {
        printf("%d ", counter);
        counter++;
    } while (counter < 10); // counter 变量引用
    printf("\n");
}

// 复杂循环中的引用
void complex_loop_reference() {
    int i = 0, j = 0;
    int limit_i = 5, limit_j = 3;
    
    for (i = 0; i < limit_i; i++) { // i, limit_i 变量引用
        j = 0;
        while (j < limit_j) { // j, limit_j 变量引用
            printf("(%d, %d) ", i, j);
            j++;
        }
    }
    printf("\n");
}

// 全局变量循环引用
void global_loop_reference() {
    extern int global_counter;
    int local_counter = 0;
    
    while (global_counter > 0) { // global_counter 变量引用
        local_counter++;
        global_counter--;
    }
    printf("Local counter: %d\n", local_counter);
}

// 静态变量循环引用
void static_loop_reference() {
    static int static_counter = 10;
    int local_counter = 0;
    
    while (static_counter > 0) { // static_counter 变量引用
        local_counter++;
        static_counter--;
    }
    printf("Local counter: %d\n", local_counter);
}

// 函数参数循环引用
void parameter_loop_reference(int count) {
    int i = 0;
    
    for (i = 0; i < count; i++) { // i, count 变量引用
        printf("%d ", i);
    }
    printf("\n");
}

// 结构体字段循环引用
void struct_field_loop_reference() {
    struct Counter {
        int value;
        int limit;
    } counter = {0, 10};
    
    while (counter.value < counter.limit) { // counter 变量引用，value, limit 字段引用
        printf("%d ", counter.value);
        counter.value++;
    }
    printf("\n");
}

// 数组元素循环引用
void array_element_loop_reference() {
    int array[10] = {0, 1, 2, 3, 4, 5, 6, 7, 8, 9};
    int i = 0;
    int sum = 0;
    
    while (i < 10 && sum < 20) { // i, sum 变量引用
        sum += array[i]; // array, i 变量引用
        i++;
    }
    printf("Sum: %d\n", sum);
}
```

## 25. 函数调用参数中的引用关系

### 查询规则
```
(call_expression
  arguments: (argument_list
    (identifier) @reference.call.argument)) @reference.relationship.call.argument
```

### 测试用例
```c
// 基本函数调用参数引用
void basic_call_argument_reference() {
    int a = 10, b = 20;
    int result = calculate(a, b); // a, b 变量引用
    printf("Result: %d\n", result);
}

// 多参数函数调用引用
void multi_parameter_call_reference() {
    int x = 10, y = 20, z = 30;
    char* message = "Hello";
    complex_function(x, y, z, message, &x); // x, y, z, message 变量引用
}

// 结构体参数函数调用引用
void struct_parameter_call_reference() {
    Point p = {10, 20};
    Rectangle rect = {{0, 0}, {100, 100}};
    
    process_point(p); // p 变量引用
    process_rectangle(rect); // rect 变量引用
}

// 数组参数函数调用引用
void array_parameter_call_reference() {
    int array[10] = {0, 1, 2, 3, 4, 5, 6, 7, 8, 9};
    int size = 10;
    
    process_array(array, size); // array, size 变量引用
}

// 指针参数函数调用引用
void pointer_parameter_call_reference() {
    int x = 10;
    int* ptr = &x;
    
    process_pointer(ptr); // ptr 变量引用
}

// 全局变量函数调用引用
void global_call_argument_reference() {
    extern int global_counter;
    extern char global_buffer[];
    
    process_global(global_counter, global_buffer); // global_counter, global_buffer 变量引用
}

// 静态变量函数调用引用
void static_call_argument_reference() {
    static int static_counter = 0;
    static char static_buffer[256];
    
    process_static(static_counter, static_buffer); // static_counter, static_buffer 变量引用
}

// 嵌套函数调用参数引用
void nested_call_argument_reference() {
    int a = 10, b = 20, c = 30;
    
    int result = calculate(add(a, b), multiply(c, a)); // a, b, c 变量引用
    printf("Result: %d\n", result);
}

// 复杂函数调用参数引用
void complex_call_argument_reference() {
    Point points[5] = {{0, 0}, {1, 1}, {2, 2}, {3, 3}, {4, 4}};
    int count = 5;
    
    process_points(points, count, calculate_sum(points, count)); // points, count 变量引用
}

// 回调函数参数引用
void callback_call_argument_reference() {
    Callback callback = custom_callback;
    int data = 100;
    
    register_callback(callback); // callback 变量引用
    execute_callback(data); // data 变量引用
}
```

## 26. 综合测试用例

### 测试用例
```c
// 综合引用关系示例
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// 全局变量
int g_global_counter = 0;
char g_global_buffer[256];

// 静态变量
static int s_static_counter = 0;

// 类型定义
typedef struct Point Point;
typedef struct Rectangle Rectangle;

// 结构体定义
struct Point {
    int x;
    int y;
};

struct Rectangle {
    Point top_left;
    Point bottom_right;
};

// 枚举定义
enum Color {
    RED,
    GREEN,
    BLUE
};

// 函数声明
int calculate(int x, int y);
void process_point(Point p);
void process_rectangle(Rectangle rect);

// 宏定义
#define MAX_SIZE 100
#define BUFFER_SIZE 256

// 综合引用函数
void comprehensive_reference_example() {
    // 局部变量引用
    int a = 10, b = 20, c;
    Point p = {5, 10};
    Rectangle rect;
    enum Color color = RED;
    
    // 赋值表达式引用
    c = a + b;           // a, b, c 变量引用
    rect.top_left = p;   // rect, p 变量引用
    
    // 条件表达式引用
    if (a > b) {         // a, b 变量引用
        printf("a is greater than b\n");
    }
    
    // 循环中的引用
    for (int i = 0; i < MAX_SIZE; i++) { // i, MAX_SIZE 宏引用
        g_global_counter++; // g_global_counter 全局变量引用
    }
    
    // 函数调用参数引用
    int result = calculate(a, b); // a, b 变量引用
    process_point(p); // p 变量引用
    process_rectangle(rect); // rect 变量引用
    
    // 数组引用
    int array[MAX_SIZE]; // MAX_SIZE 宏引用
    for (int i = 0; i < MAX_SIZE; i++) { // i, MAX_SIZE 宏引用
        array[i] = i; // array, i 变量引用
    }
    
    // 指针引用
    int* ptr = &a;      // a 变量引用
    *ptr = 100;         // ptr 指针引用
    
    // 结构体字段引用
    p.x = 15;           // p 变量引用，x 字段引用
    p.y = 25;           // p 变量引用，y 字段引用
    
    // 枚举常量引用
    if (color == RED) { // color 变量引用，RED 枚举常量引用
        printf("Red color\n");
    }
    
    // 全局变量引用
    sprintf(g_global_buffer, "Counter: %d", g_global_counter); // g_global_buffer, g_global_counter 全局变量引用
    printf("%s\n", g_global_buffer); // g_global_buffer 全局变量引用
    
    // 静态变量引用
    s_static_counter++; // s_static_counter 静态变量引用
    printf("Static counter: %d\n", s_static_counter); // s_static_counter 静态变量引用
    
    // 宏引用
    int buffer[BUFFER_SIZE]; // BUFFER_SIZE 宏引用
    memset(buffer, 0, BUFFER_SIZE * sizeof(int)); // buffer, BUFFER_SIZE 宏引用
    
    // 返回语句引用
    return; // 无返回值引用
}

// 复杂引用函数
int complex_reference_function(int x, int y) {
    // 参数引用
    int result = x + y; // x, y 变量引用
    
    // 局部变量引用
    int local_var = result * 2; // result, local_var 变量引用
    
    // 条件表达式引用
    if (local_var > 100) { // local_var 变量引用
        result = local_var; // result, local_var 变量引用
    } else {
        result = x; // result, x 变量引用
    }
    
    // 循环中的引用
    while (result > 0) { // result 变量引用
        result--; // result 变量引用
        g_global_counter++; // g_global_counter 全局变量引用
    }
    
    // 返回语句引用
    return result; // result 变量引用
}

// 结构体引用函数
void struct_reference_function() {
    // 结构体变量引用
    Point p1 = {10, 20}; // p1 变量引用
    Point p2; // p2 变量引用
    
    // 结构体赋值引用
    p2 = p1; // p2, p1 变量引用
    
    // 结构体字段引用
    p2.x = 30; // p2 变量引用，x 字段引用
    p2.y = 40; // p2 变量引用，y 字段引用
    
    // 结构体指针引用
    Point* ptr = &p1; // p1 变量引用
    ptr->x = 50; // ptr 指针引用，x 字段引用
    ptr->y = 60; // ptr 指针引用，y 字段引用
    
    // 函数调用参数引用
    process_point(p1); // p1 变量引用
    process_point(p2); // p2 变量引用
    process_point(*ptr); // ptr 指针引用
}

// 数组引用函数
void array_reference_function() {
    // 数组变量引用
    int array[10]; // array 变量引用
    
    // 数组元素引用
    for (int i = 0; i < 10; i++) { // i 变量引用
        array[i] = i * 2; // array, i 变量引用
    }
    
    // 数组指针引用
    int* ptr = array; // array 变量引用
    for (int i = 0; i < 10; i++) { // i 变量引用
        printf("%d ", ptr[i]); // ptr, i 变量引用
    }
    printf("\n");
    
    // 全局数组引用
    for (int i = 0; i < 10; i++) { // i 变量引用
        g_global_buffer[i] = 'A' + i; // g_global_buffer, i 变量引用
    }
    g_global_buffer[10] = '\0'; // g_global_buffer 变量引用
    printf("%s\n", g_global_buffer); // g_global_buffer 全局变量引用
}