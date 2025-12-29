# C语言依赖关系Tree-Sitter查询规则测试用例

本文档为C语言依赖关系的Tree-Sitter查询规则提供测试用例，每个查询规则后都附有相应的测试代码示例，方便在Tree-Sitter Playground中进行验证。

## 1. 头文件包含依赖关系

### 查询规则
```
(preproc_include
  path: (string_literal) @dependency.include.path) @dependency.relationship.include
```

### 测试用例
```c
// 基本头文件包含
#include "stdio.h"
#include "stdlib.h"
#include "string.h"

// 相对路径头文件包含
#include "../include/common.h"
#include "utils/helper.h"
#include "./local_header.h"

// 绝对路径头文件包含
#include "/usr/include/stdio.h"
#include "/home/user/project/include/config.h"

// 带空格的头文件包含
#include "my header.h"
#include "complex path/header.h"

// 嵌套目录头文件包含
#include "deep/nested/directory/structure/header.h"
#include "module1/submodule1/header.h"
#include "module2/submodule2/submodule3/header.h"
```

## 2. 系统库包含依赖关系

### 查询规则
```
(preproc_include
  path: (system_lib_string) @dependency.system.path) @dependency.relationship.system
```

### 测试用例
```c
// 标准系统库包含
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <time.h>

// POSIX系统库包含
#include <unistd.h>
#include <fcntl.h>
#include <sys/types.h>
#include <sys/stat.h>
#include <pthread.h>

// 网络系统库包含
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <netdb.h>

// 图形系统库包含
#include <X11/Xlib.h>
#include <GL/gl.h>
#include <gtk/gtk.h>

// 数据库系统库包含
#include <mysql/mysql.h>
#include <sqlite3.h>

// 复杂系统库路径
#include <sys/time.h>
#include <linux/socket.h>
#include <asm/errno.h>
```

## 3. 宏定义依赖关系

### 查询规则
```
(preproc_def
  name: (identifier) @dependency.macro.name
  value: (identifier)? @dependency.macro.value) @dependency.relationship.macro
```

### 测试用例
```c
// 基本宏定义
#define MAX_SIZE 100
#define BUFFER_SIZE 256
#define DEFAULT_PORT 8080

// 带变量值的宏定义
#define GLOBAL_CONFIG config
#define MAIN_WINDOW main_window
#define ERROR_HANDLER error_handler

// 带表达式的宏定义
#define ARRAY_SIZE (sizeof(array) / sizeof(array[0]))
#define PI 3.14159265359

// 条件宏定义
#ifdef DEBUG
    #define LOG_LEVEL 3
#else
    #define LOG_LEVEL 1
#endif

// 带参数的宏定义
#define SQUARE(x) ((x) * (x))
#define MAX(a, b) ((a) > (b) ? (a) : (b))
#define ABS(x) ((x) < 0 ? -(x) : (x))

// 复杂宏定义
#define COMPLEX_MACRO(a, b, c) ((a) + (b) * (c))
#define SAFE_FREE(ptr) do { if(ptr) { free(ptr); ptr = NULL; } } while(0)
```

## 4. 宏函数定义依赖关系

### 查询规则
```
(preproc_function_def
  name: (identifier) @dependency.macro.function.name
  parameters: (parameter_list
    (identifier) @dependency.macro.parameter)*) @dependency.relationship.macro.function
```

### 测试用例
```c
// 基本宏函数定义
#define MIN(x, y) ((x) < (y) ? (x) : (y))
#define MAX(x, y) ((x) > (y) ? (x) : (y))
#define ABS(x) ((x) < 0 ? -(x) : (x))

// 多参数宏函数定义
#define CALCULATE(a, b, c) ((a) + (b) * (c))
#define PROCESS_DATA(input, output, size) process_function(input, output, size)

// 带类型参数的宏函数定义
#define DECLARE_ARRAY(type, name, size) type name[size]
#define MALLOC_POINTER(type, name) type* name = malloc(sizeof(type))

// 复杂宏函数定义
#define SAFE_ALLOC(ptr, type, count) \
    do { \
        if (ptr) { \
            free(ptr); \
        } \
        ptr = malloc(sizeof(type) * (count)); \
    } while(0)

#define LOG_ERROR(format, ...) \
    do { \
        fprintf(stderr, "[ERROR] %s:%d: " format "\n", __FILE__, __LINE__, ##__VA_ARGS__); \
    } while(0)

// 条件宏函数定义
#ifdef DEBUG
    #define DEBUG_PRINT(msg) printf("[DEBUG] %s\n", msg)
#else
    #define DEBUG_PRINT(msg)
#endif
```

## 5. 条件编译依赖关系

### 查询规则
```
(preproc_if
  condition: (identifier) @dependency.condition.symbol) @dependency.relationship.conditional
```

### 测试用例
```c
// 基本条件编译
#if DEBUG
    printf("Debug mode enabled\n");
#endif

#if WINDOWS
    #include <windows.h>
#elif LINUX
    #include <unistd.h>
#endif

// 复杂条件编译
#if defined(DEBUG) && defined(VERBOSE)
    printf("Debug and verbose mode enabled\n");
#endif

#if VERSION >= 2
    #include "new_features.h"
#endif

// 嵌套条件编译
#if PLATFORM == WINDOWS
    #if COMPILER == MSVC
        #include <windows.h>
    #elif COMPILER == GCC
        #include <windows.h>
    #endif
#elif PLATFORM == LINUX
    #include <unistd.h>
#endif

// 条件编译与表达式
#if sizeof(void*) == 8
    #define ARCH_64BIT
#else
    #define ARCH_32BIT
#endif
```

## 6. 条件编译ifdef依赖关系

### 查询规则
```
(preproc_ifdef
  name: (identifier) @dependency.ifdef.symbol) @dependency.relationship.ifdef
```

### 测试用例
```c
// 基本ifdef
#ifdef DEBUG
    printf("Debug mode\n");
#endif

// 基本ifndef
#ifndef VERSION
    #define VERSION "1.0"
#endif

// 嵌套ifdef
#ifdef DEBUG
    #ifdef VERBOSE
        printf("Debug and verbose mode\n");
    #endif
#endif

// 多重ifdef
#ifdef WINDOWS
    #include <windows.h>
    #ifdef UNICODE
        #define UNICODE_ENABLED
    #endif
#endif

// ifdef与else
#ifdef TESTING
    #define RUN_TESTS
#else
    #define RUN_PRODUCTION
#endif

// 复杂ifdef结构
#ifdef FEATURE_A
    #include "feature_a.h"
    #ifdef FEATURE_B
        #include "feature_b.h"
    #endif
#else
    #include "basic_features.h"
#endif
```

## 7. 条件编译elif依赖关系

### 查询规则
```
(preproc_elif
  condition: (identifier) @dependency.elif.symbol) @dependency.relationship.elif
```

### 测试用例
```c
// 基本elif
#ifdef WINDOWS
    #include <windows.h>
#elif LINUX
    #include <unistd.h>
#elif MACOS
    #include <mach/mach.h>
#endif

// 多重elif
#if PLATFORM == WINDOWS
    #define OS_NAME "Windows"
#elif PLATFORM == LINUX
    #define OS_NAME "Linux"
#elif PLATFORM == MACOS
    #define OS_NAME "macOS"
#else
    #define OS_NAME "Unknown"
#endif

// elif与表达式
#if ARCH == X86
    #define ARCH_NAME "x86"
#elif ARCH == X64
    #define ARCH_NAME "x64"
#elif ARCH == ARM
    #define ARCH_NAME "ARM"
#endif

// 复杂elif结构
#ifdef DEBUG
    #define LOG_LEVEL 3
#elif TESTING
    #define LOG_LEVEL 2
#elif PRODUCTION
    #define LOG_LEVEL 1
#endif

// elif与条件组合
#if defined(WINDOWS) && defined(UNICODE)
    #define PLATFORM_NAME "Windows Unicode"
#elif defined(WINDOWS)
    #define PLATFORM_NAME "Windows"
#elif defined(LINUX)
    #define PLATFORM_NAME "Linux"
#endif
```

## 8. 类型引用依赖关系

### 查询规则
```
(declaration
  type: (type_identifier) @dependency.type.reference
  declarator: (identifier) @dependency.variable.name) @dependency.relationship.type
```

### 测试用例
```c
// 基本类型引用
typedef int Integer;
typedef char Character;
typedef float FloatingPoint;

Integer number;
Character letter;
FloatingPoint decimal;

// 自定义类型引用
typedef struct Point Point;
typedef enum Color Color;
typedef union Data Data;

Point position;
Color shade;
Data information;

// 复杂类型引用
typedef struct LinkedList LinkedList;
typedef struct BinaryTree BinaryTree;
typedef struct HashTable HashTable;

LinkedList* list;
BinaryTree* tree;
HashTable* table;

// 指针类型引用
typedef int* IntPtr;
typedef char* String;
typedef void* Pointer;

IntPtr int_ptr;
String text;
Pointer generic_ptr;

// 函数指针类型引用
typedef int (*Operation)(int, int);
typedef void (*Callback)(void*);
typedef char* (*StringFunction)(const char*);

Operation add_function;
Callback event_handler;
StringFunction string_processor;
```

## 9. 结构体引用依赖关系

### 查询规则
```
(field_declaration
  type: (type_identifier) @dependency.struct.reference
  declarator: (field_identifier) @dependency.field.name) @dependency.relationship.struct
```

### 测试用例
```c
// 基本结构体字段引用
struct Point {
    int x;
    int y;
};

struct Rectangle {
    Point top_left;      // 结构体类型引用
    Point bottom_right;  // 结构体类型引用
    int width;
    int height;
};

// 嵌套结构体引用
struct Address {
    char street[50];
    char city[20];
    char country[20];
};

struct Person {
    char name[50];
    int age;
    Address home_address;    // 结构体类型引用
    Address work_address;    // 结构体类型引用
};

// 复杂结构体引用
struct Node {
    int data;
    Node* next;              // 自引用结构体
};

struct LinkedList {
    Node* head;              // 结构体类型引用
    int count;
};

struct TreeNode {
    int value;
    TreeNode* left;          // 自引用结构体
    TreeNode* right;         // 自引用结构体
    TreeNode* parent;        // 自引用结构体
};

// 结构体数组引用
struct Polygon {
    Point vertices[10];      // 结构体数组引用
    int vertex_count;
};
```

## 10. 函数声明依赖关系

### 查询规则
```
(declaration
  type: (_)
  declarator: (function_declarator
    declarator: (identifier) @dependency.function.name
    parameters: (parameter_list
      (parameter_declaration
        type: (type_identifier) @dependency.parameter.type)*)?)) @dependency.relationship.function
```

### 测试用例
```c
// 基本函数声明
int calculate(int x, int y);
void process_data(Data* data);
char* get_string(String input);

// 带自定义类型参数的函数声明
Point create_point(int x, int y);
Rectangle create_rectangle(Point top_left, Point bottom_right);
LinkedList* create_linked_list();

// 复杂函数声明
BinaryTree* insert_node(BinaryTree* tree, int value);
HashTable* create_hash_table(int size);
TreeNode* build_tree(int* array, int size);

// 带多个自定义类型参数的函数声明
int compare_points(Point p1, Point p2);
void merge_lists(LinkedList* dest, LinkedList* src);
Data process_data(InputData input, OutputData output);

// 函数指针参数声明
void set_callback(Callback callback);
void register_handler(EventHandler handler);
void set_operation(Operation operation);

// 返回自定义类型的函数声明
Point get_center(Rectangle rect);
LinkedList* reverse_list(LinkedList* list);
Data transform_data(Data original, Transformer transformer);

// 复杂参数和返回类型声明
TreeNode* balance_tree(TreeNode* unbalanced_tree);
LinkedList* filter_list(LinkedList* list, FilterFunction filter);
HashTable* merge_tables(HashTable* table1, HashTable* table2);
```

## 11. 函数调用依赖关系

### 查询规则
```
(call_expression
  function: (identifier) @dependency.call.function
  arguments: (argument_list
    (identifier) @dependency.call.argument)*) @dependency.relationship.call
```

### 测试用例
```c
// 基本函数调用
void basic_function_calls() {
    int a = 10, b = 20;
    int result = calculate(a, b);  // 函数调用依赖
    
    Data data;
    process_data(&data);           // 函数调用依赖
    
    String text = get_string("input");  // 函数调用依赖
}

// 结构体相关函数调用
void struct_function_calls() {
    Point p1 = create_point(10, 20);     // 函数调用依赖
    Point p2 = create_point(30, 40);     // 函数调用依赖
    
    Rectangle rect = create_rectangle(p1, p2);  // 函数调用依赖
    Point center = get_center(rect);             // 函数调用依赖
}

// 复杂函数调用
void complex_function_calls() {
    int array[10] = {1, 2, 3, 4, 5};
    BinaryTree* tree = build_tree(array, 10);    // 函数调用依赖
    TreeNode* balanced = balance_tree(tree);     // 函数调用依赖
    
    LinkedList* list = create_linked_list();     // 函数调用依赖
    LinkedList* filtered = filter_list(list, filter_function);  // 函数调用依赖
    
    HashTable* table1 = create_hash_table(100);  // 函数调用依赖
    HashTable* table2 = create_hash_table(200);  // 函数调用依赖
    HashTable* merged = merge_tables(table1, table2);  // 函数调用依赖
}

// 函数指针调用
void function_pointer_calls() {
    Callback callback = event_handler;           // 变量依赖
    set_callback(callback);                      // 函数调用依赖
    
    Operation operation = add_function;          // 变量依赖
    set_operation(operation);                    // 函数调用依赖
}

// 嵌套函数调用
void nested_function_calls() {
    int result = calculate(add(5, 3), multiply(4, 6));  // 嵌套函数调用依赖
    
    Point p = create_point(10, 20);                       // 函数调用依赖
    Rectangle rect = create_rectangle(p, create_point(30, 40));  // 嵌套函数调用依赖
}
```

## 12. 枚举引用依赖关系

### 查询规则
```
(declaration
  type: (enum_specifier
    name: (type_identifier) @dependency.enum.name)
  declarator: (identifier) @dependency.variable.name) @dependency.relationship.enum
```

### 测试用例
```c
// 基本枚举类型引用
enum Color {
    RED,
    GREEN,
    BLUE
};

Color primary_color;     // 枚举类型引用
Color secondary_color;   // 枚举类型引用

// 复杂枚举类型引用
enum Status {
    STATUS_PENDING,
    STATUS_PROCESSING,
    STATUS_COMPLETED,
    STATUS_FAILED
};

enum Status current_status;    // 枚举类型引用
enum Status previous_status;   // 枚举类型引用

// 带显式值的枚举
enum Priority {
    PRIORITY_LOW = 1,
    PRIORITY_MEDIUM = 5,
    PRIORITY_HIGH = 10,
    PRIORITY_CRITICAL = 20
};

enum Priority task_priority;    // 枚举类型引用

// 枚举数组
enum Color color_palette[10];   // 枚举数组类型引用
enum Status status_history[100]; // 枚举数组类型引用

// 结构体中的枚举引用
struct Task {
    char name[50];
    enum Priority priority;     // 枚举类型引用
    enum Status status;         // 枚举类型引用
};

struct Task current_task;       // 结构体变量声明
```

## 13. 联合体引用依赖关系

### 查询规则
```
(declaration
  type: (union_specifier
    name: (type_identifier) @dependency.union.name)
  declarator: (identifier) @dependency.variable.name) @dependency.relationship.union
```

### 测试用例
```c
// 基本联合体类型引用
union Data {
    int integer;
    float floating;
    char* string;
};

union Data value;        // 联合体类型引用
union Data buffer;       // 联合体类型引用

// 复杂联合体类型引用
union Variant {
    int int_value;
    float float_value;
    char char_value;
    double double_value;
    void* pointer_value;
};

union Variant variant;   // 联合体类型引用

// 联合体数组
union Data data_array[10];    // 联合体数组类型引用
union Variant variants[5];    // 联合体数组类型引用

// 结构体中的联合体引用
struct Container {
    int type;
    union Data data;       // 联合体类型引用
    union Variant variant; // 联合体类型引用
};

struct Container container;   // 结构体变量声明

// 联合体指针
union Data* data_ptr;      // 联合体指针类型引用
union Variant* variant_ptr; // 联合体指针类型引用

// 函数参数中的联合体引用
void process_data(union Data data);     // 函数参数联合体引用
void handle_variant(union Variant var); // 函数参数联合体引用
```

## 14. 全局变量依赖关系

### 查询规则
```
(declaration
  type: (_)
  declarator: (identifier) @dependency.global.variable
  (#match? @dependency.global.variable "^[gG][a-zA-Z0-9_]*$")) @dependency.relationship.global
```

### 测试用例
```c
// 基本全局变量
int globalCounter;
char globalBuffer[256];
float globalTemperature;

// 带g前缀的全局变量
int g_config_value;
char g_error_message[100];
float g_scale_factor;

// 带G前缀的全局变量
int G_system_state;
char G_log_file[50];
double G_pi_value;

// 复杂全局变量
struct Point g_origin;
LinkedList* g_global_list;
HashTable* g_config_table;

// 全局变量数组
int g_scores[100];
char g_names[50][50];
Data g_data_items[10];

// 全局指针变量
int* g_global_pointer;
void* g_memory_pool;
Callback g_global_callback;

// 常量全局变量
const int g_max_connections = 100;
const char* g_version = "1.0.0";
const float g_gravity = 9.81;

// 全局枚举变量
enum Status g_current_status;
enum Priority g_default_priority;
```

## 15. 外部变量依赖关系

### 查询规则
```
(declaration
  (storage_class_specifier) @dependency.extern.specifier
  type: (_)
  declarator: (identifier) @dependency.extern.variable) @dependency.relationship.extern
```

### 测试用例
```c
// 基本外部变量声明
extern int external_counter;
extern char external_buffer[256];
extern float external_value;

// 外部结构体声明
extern struct Config global_config;
extern LinkedList* main_list;
extern HashTable* settings_table;

// 外部函数指针声明
extern Callback global_callback;
extern Operation default_operation;
extern EventHandler event_handler;

// 外部枚举声明
extern enum Status current_status;
extern enum Priority system_priority;

// 外部联合体声明
extern union Data shared_data;
extern union Variant system_variant;

// 复杂外部变量声明
extern struct {
    int version;
    char name[50];
    void* context;
} global_context;

extern int* external_array;
extern void** pointer_array;
extern char** string_array;

// 带const的外部变量声明
extern const int max_connections;
extern const char* version_string;
extern const float pi_value;

// 外部函数声明（虽然不是变量，但使用extern关键字）
extern void initialize_system(void);
extern int process_data(int input);
extern char* get_error_message(int error_code);
```

## 16. 静态变量依赖关系

### 查询规则
```
(declaration
  (storage_class_specifier) @dependency.static.specifier
  type: (_)
  declarator: (identifier) @dependency.static.variable) @dependency.relationship.static
```

### 测试用例
```c
// 基本静态变量
static int static_counter;
static char static_buffer[256];
static float static_value;

// 函数内静态变量
void function_with_static() {
    static int call_count = 0;    // 静态局部变量
    static char message[100];     // 静态局部变量
    static float accumulator = 0.0f; // 静态局部变量
    
    call_count++;
    sprintf(message, "Call count: %d", call_count);
    accumulator += 1.5f;
}

// 静态结构体变量
static struct Point static_origin = {0, 0};
static LinkedList* static_list = NULL;
static HashTable* static_cache = NULL;

// 静态数组变量
static int static_scores[100];
static char static_names[50][50];
static Data static_data_items[10];

// 静态指针变量
static int* static_pointer;
static void* static_memory_pool;
static Callback static_callback;

// 静态常量变量
static const int static_max_size = 100;
static const char* static_version = "1.0.0";
static const float static_pi = 3.14159f;

// 静态枚举变量
static enum Status static_status;
static enum Priority static_priority;

// 静态联合体变量
static union Data static_data;
static union Variant static_variant;

// 复杂静态变量
static struct {
    int initialized;
    char error_message[100];
    void* context;
} static_context;

// 文件作用域静态变量
static int file_static_variable = 0;
static char file_static_buffer[512];
```

## 17. 类型别名依赖关系

### 查询规则
```
(type_definition
  type: (type_identifier) @dependency.alias.original
  declarator: (type_identifier) @dependency.alias.new) @dependency.relationship.alias
```

### 测试用例
```c
// 基本类型别名
typedef int Integer;
typedef char Character;
typedef float FloatingPoint;
typedef double DoublePrecision;

// 结构体类型别名
typedef struct Point Point;
typedef struct Rectangle Rectangle;
typedef struct LinkedList LinkedList;
typedef struct HashTable HashTable;

// 枚举类型别名
typedef enum Color Color;
typedef enum Status Status;
typedef enum Priority Priority;

// 联合体类型别名
typedef union Data Data;
typedef union Variant Variant;

// 指针类型别名
typedef int* IntPtr;
typedef char* String;
typedef void* Pointer;
typedef void* Handle;

// 函数指针类型别名
typedef int (*Operation)(int, int);
typedef void (*Callback)(void*);
typedef char* (*StringFunction)(const char*);
typedef int (*CompareFunction)(const void*, const void*);

// 复杂类型别名
typedef struct TreeNode TreeNode;
typedef struct BinaryTree BinaryTree;
typedef struct GraphNode GraphNode;

// 数组类型别名
typedef int IntArray[10];
typedef char StringArray[50][50];
typedef float FloatMatrix[3][3];

// 多级指针类型别名
typedef int** IntPtrPtr;
typedef char*** StringPtrPtr;
typedef void** HandleArray;

// 带const的类型别名
typedef const int ConstInt;
typedef const char* ConstString;
typedef const void* ConstPointer;
```

## 18. 综合测试用例

### 测试用例
```c
// 综合依赖关系示例
#include "common.h"           // 头文件包含依赖
#include <stdlib.h>           // 系统库包含依赖

#define MAX_SIZE 100         // 宏定义依赖
#define GLOBAL_CONFIG config  // 带变量值的宏定义

// 类型别名依赖
typedef struct Point Point;
typedef enum Color Color;
typedef int (*Operation)(int, int);

// 全局变量依赖
int g_global_counter;
Point g_origin;

// 外部变量依赖
extern LinkedList* main_list;
extern Callback global_callback;

// 静态变量依赖
static int static_initialized = 0;
static HashTable* static_cache = NULL;

// 枚举类型定义
enum Color {
    RED,
    GREEN,
    BLUE
};

// 结构体定义
struct Point {
    int x;
    int y;
};

struct Rectangle {
    Point top_left;      // 结构体引用依赖
    Point bottom_right;  // 结构体引用依赖
};

struct LinkedList {
    int data;
    LinkedList* next;    // 自引用结构体
};

// 函数声明依赖
Point create_point(int x, int y);
Rectangle create_rectangle(Point top_left, Point bottom_right);
LinkedList* create_linked_list();

// 函数实现
Point create_point(int x, int y) {
    Point p = {x, y};    // 结构体初始化
    return p;
}

Rectangle create_rectangle(Point top_left, Point bottom_right) {
    Rectangle rect;      // 结构体变量声明
    rect.top_left = top_left;      // 结构体字段赋值
    rect.bottom_right = bottom_right; // 结构体字段赋值
    return rect;
}

LinkedList* create_linked_list() {
    static_initialized++;  // 静态变量依赖
    LinkedList* list = malloc(sizeof(LinkedList)); // 函数调用依赖
    list->data = 0;
    list->next = NULL;
    return list;
}

// 条件编译依赖
#ifdef DEBUG
    static int debug_level = 3;
    void debug_print(const char* message) {
        printf("[DEBUG] %s\n", message);
    }
#else
    static int debug_level = 1;
    void debug_print(const char* message) {
        // 空实现
    }
#endif

// 复杂函数调用依赖
void complex_operations() {
    Point p1 = create_point(10, 20);    // 函数调用依赖
    Point p2 = create_point(30, 40);    // 函数调用依赖
    
    Rectangle rect = create_rectangle(p1, p2); // 函数调用依赖
    
    LinkedList* list = create_linked_list();   // 函数调用依赖
    main_list = list;                    // 外部变量依赖
    
    g_global_counter++;                  // 全局变量依赖
    
    if (global_callback) {               // 外部变量依赖
        global_callback();               // 函数调用依赖
    }
    
    debug_print("Operation completed");  // 条件编译函数调用依赖
}