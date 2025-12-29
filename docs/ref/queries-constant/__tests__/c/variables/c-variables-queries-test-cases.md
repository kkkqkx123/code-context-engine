# C Variables Tree-Sitter查询规则测试用例

本文档为C Variables的Tree-Sitter查询规则提供测试用例，每个查询规则后都附有相应的测试代码示例，方便在TreeSitter Playground中进行验证。

## 1. 基本变量声明查询

### 查询规则
```
(declaration
  type: (_)
  declarator: (identifier) @name.definition.variable) @definition.variable
```

### 测试用例
```c
// 基本类型变量声明
int count;
float price;
char ch;
double distance;

// 自定义类型变量声明
struct Point p;
enum Color c;
typedef unsigned int uint;
uint number;
```

## 2. 带初始化的变量声明查询

### 查询规则
```
(declaration
  type: (_)
  declarator: (init_declarator
    declarator: (identifier) @name.definition.variable)) @definition.variable
```

### 测试用例
```c
// 基本初始化
int x = 10;
float pi = 3.14159f;
char letter = 'A';

// 数组初始化
int numbers[5] = {1, 2, 3, 4, 5};
char name[] = "John";

// 结构体初始化
struct Point p = {10, 20};

// 指针初始化
int* ptr = NULL;
char* str = "Hello";

// 复杂表达式初始化
int result = calculate(5, 3);
int* array_ptr = malloc(sizeof(int) * 10);
```

## 3. 变量赋值查询

### 查询规则
```
(assignment_expression
  left: (identifier) @name.definition.assignment) @definition.assignment
```

### 测试用例
```c
// 基本赋值
x = 20;
price = 99.99f;
ch = 'B';

// 复合赋值
count += 5;
total *= 2;
value /= 3;

// 指针赋值
ptr = &x;
str = "World";

// 数组元素赋值
numbers[0] = 100;
name[0] = 'J';

// 结构体成员赋值
p.x = 30;
p.y = 40;

// 函数返回值赋值
result = calculate(10, 20);
```
