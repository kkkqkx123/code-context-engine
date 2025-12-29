# C Structs Tree-Sitter查询规则测试用例

本文档为C Structs的Tree-Sitter查询规则提供测试用例，每个查询规则后都附有相应的测试代码示例，方便在TreeSitter Playground中进行验证。

## 1. 结构体、联合体和枚举定义查询

### 查询规则
```
[
  (struct_specifier
    name: (type_identifier) @type.name
    body: (field_declaration_list
      (field_declaration
        type: (_) @field.type
        declarator: (field_identifier) @field.name)*)) @definition.struct
  (union_specifier
    name: (type_identifier) @type.name
    body: (field_declaration_list
      (field_declaration
        type: (_) @field.type
        declarator: (field_identifier) @field.name)*)) @definition.union
  (enum_specifier
    name: (type_identifier) @type.name
    body: (enumerator_list
      (enumerator
        name: (identifier) @enum.constant)*)) @definition.enum
] @definition.type
```

### 测试用例
```c
// 结构体定义
struct Point {
    int x;
    int y;
    double z;
};

struct Person {
    char name[50];
    int age;
    float height;
};

// 联合体定义
union Data {
    int i;
    float f;
    char str[20];
};

union Variant {
    char byte;
    short word;
    int dword;
};

// 枚举定义
enum Color {
    RED,
    GREEN,
    BLUE
};

enum Status {
    PENDING = 0,
    RUNNING = 1,
    COMPLETED = 2,
    FAILED = 3
};
```

## 2. 类型别名查询

### 查询规则
```
(type_definition
  type: (_)
  declarator: (type_identifier) @alias.name) @definition.type.alias
```

### 测试用例
```c
// 基本类型别名
typedef unsigned int uint32;
typedef unsigned char byte;
typedef long long int64;

// 结构体类型别名
typedef struct {
    int x;
    int y;
} Point2D;

typedef struct Node {
    int data;
    struct Node* next;
} ListNode;

// 函数指针类型别名
typedef int (*CompareFunc)(const void*, const void*);
typedef void (*Callback)(void* data);
```

## 3. 数组和指针声明查询

### 查询规则
```
[
  (declaration
    type: (_)
    declarator: (array_declarator
      declarator: (identifier) @array.name
      size: (_)? @array.size)) @definition.array
  (declaration
    type: (_)
    declarator: (pointer_declarator
      declarator: (identifier) @pointer.name)) @definition.pointer
] @definition.variable
```

### 测试用例
```c
// 数组声明
int numbers[10];
char buffer[256];
float matrix[3][3];
int dynamic_array[];

// 指针声明
int* ptr;
char* str;
void* data;
struct Point* point_ptr;
```

## 4. 成员访问查询

### 查询规则
```
[
  (field_expression
    argument: (identifier) @object.name
    field: (field_identifier) @field.name) @definition.member.access
  (field_expression
    argument: (identifier) @pointer.name
    field: (field_identifier) @field.name) @definition.pointer.member.access
 (field_expression
    argument: (parenthesized_expression
      (pointer_expression
        argument: (identifier) @pointer.name))
    field: (field_identifier) @field.name) @definition.pointer.member.access
] @definition.access
```

### 测试用例
```c
struct Point p;
struct Point* ptr_p;

// 普通成员访问
int x = p.x;
int y = p.y;

// 指针成员访问
int px = ptr_p->x;
int py = ptr_p->y;

// 解引用指针成员访问
int dx = (*ptr_p).x;
int dy = (*ptr_p).y;

struct Rectangle {
    struct Point top_left;
    struct Point bottom_right;
} rect;

// 嵌套成员访问
int left_x = rect.top_left.x;
int right_x = rect.bottom_right.x;
```

## 5. 数组访问查询

### 查询规则
```
[
  ; 一维数组访问: arr[i]
  (subscript_expression
    argument: (identifier) @array.name
    indices: (subscript_argument_list
      (_) @index)) @definition.array.access

  ; 二维数组访问: matrix[i][j]
  (subscript_expression
    argument: (subscript_expression
      argument: (identifier) @array.name
      indices: (subscript_argument_list _))
    indices: (subscript_argument_list
      (_) @index)) @definition.array.access

  ; 支持表达式作为索引的情况，比如 matrix[i+1][j-1]
  ; 上面两条已经能捕获，只要 @index 能匹配任意表达式即可
] @definition.array.access
```

### 测试用例
```c
int arr[10];
int matrix[3][3];

// 一维数组访问
int value = arr[5];
arr[0] = 100;

// 二维数组访问
int element = matrix[1][2];
matrix[0][0] = 1;

// 使用变量作为索引
for (int i = 0; i < 10; i++) {
    arr[i] = i * 2;
}

// 使用表达式作为索引
int result = matrix[i + 1][j - 1];
```

## 6. 嵌套结构体查询

### 查询规则
```
(struct_specifier
  body: (field_declaration_list
    (field_declaration
      type: (struct_specifier)
      declarator: (field_identifier) @nested.field.name))) @definition.nested.struct
```

### 测试用例
```c
// 嵌套结构体定义
struct Outer {
    int outer_field;
    struct {
        int inner_field1;
        float inner_field2;
    } nested;
};

struct Address {
    char street[50];
    char city[30];
    char country[30];
};

struct Person {
    char name[50];
    int age;
    struct Address home_address;
    struct Address work_address;
};

// 匿名结构体
struct Container {
    int id;
    struct {
        int x;
        int y;
    };  // 匿名结构体
};
```
