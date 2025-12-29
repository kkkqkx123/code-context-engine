# C++ Types Tree-Sitter查询规则测试用例

本文档为C++ Types的Tree-Sitter查询规则提供测试用例，每个查询规则后都附有相应的测试代码示例，方便在TreeSitter Playground中进行验证。

## 1. 类型定义

### 查询规则
```
(type_definition
  type: (_)
  declarator: (type_identifier) @name.definition.type) @definition.type
```

### 测试用例
```cpp
// typedef 定义
typedef unsigned int uint;
typedef char* string_ptr;
typedef struct Point {
    int x, y;
} Point;

// 使用类型定义
uint counter = 0;
Point origin = {0, 0};
```

## 2. 类型别名使用 'using'

### 查询规则
```
(type_alias_declaration
  name: (identifier) @name.definition.type_alias) @definition.type_alias
```

### 测试用例
```cpp
// 使用 using 声明类型别名
using Integer = int;
using String = std::string;
using IntArray = int[10];

// 模板类型别名
template<typename T>
using Vec = std::vector<T>;

// 函数指针类型别名
using FuncPtr = void(*)(int, double);

// 使用类型别名
Integer value = 42;
Vec<double> numbers = {1.0, 2.0, 3.0};
IntArray arr;
```

## 3. 枚举声明

### 查询规则
```
(enum_specifier
  name: (type_identifier) @name.definition.enum) @definition.enum
```

### 测试用例
```cpp
// 基本枚举
enum Color {
    RED,
    GREEN,
    BLUE
};

// 指定基础类型的枚举
enum class Priority : int {
    LOW = 1,
    MEDIUM = 2,
    HIGH = 3
};

// 传统枚举
enum Status {
    SUCCESS = 0,
    ERROR = -1,
    PENDING = 1
};

// 使用枚举
Color myColor = RED;
Priority level = Priority::HIGH;
Status result = SUCCESS;
```

## 4. 概念定义 (C++20)

### 查询规则
```
(concept_definition
  name: (identifier) @name.definition.concept) @definition.concept
```

### 测试用例
```cpp
#include <concepts>

// 概念定义
template<typename T>
concept Integral = std::is_integral_v<T>;

template<typename T>
concept Numeric = std::is_arithmetic_v<T>;

// 复杂概念
template<typename Container>
concept RandomAccessContainer = requires(Container c) {
    c[0];
    c.size();
    std::ranges::random_access_iterator(std::begin(c));
};

// 使用概念
template<Integral T>
void processInteger(T value) {
    // 处理整数类型
}

template<Numeric T>
T multiply(T a, T b) {
    return a * b;
}
```

## 5. Auto 类型推导

### 查询规则
```
(declaration
  type: (auto)
  declarator: (init_declarator
    declarator: (identifier) @name.definition.auto_var)) @definition.auto_var
```

### 测试用例
```cpp
#include <vector>
#include <map>

// 基本 auto 变量
auto x = 42;              // int
auto y = 3.14;            // double
auto z = 'c';             // char

// auto 与复杂类型
std::vector<int> vec = {1, 2, 3, 4, 5};
auto it = vec.begin();    // std::vector<int>::iterator

std::map<std::string, int> myMap;
auto mapIt = myMap.find("key");  // std::map<std::string, int>::iterator

// auto 与函数返回值
auto lambda = [](int a, int b) { return a + b; };
auto result = lambda(5, 10);

// 带修饰符的 auto
const auto size = vec.size();
auto&& ref = x;  // 万能引用
```

## 6. 类类型定义

### 查询规则
```
[
  (class_specifier
    name: (type_identifier) @name.definition.type) @definition.class
  (struct_specifier
    name: (type_identifier) @name.definition.type) @definition.struct
  (union_specifier
    name: (type_identifier) @name.definition.type) @definition.union
] @definition.type
```

### 测试用例
```cpp
// 类定义
class MyClass {
public:
    int value;
    void method();
};

// 结构体定义
struct MyStruct {
    int x, y;
    double data;
};

// 联合体定义
union MyUnion {
    int intValue;
    float floatValue;
    char charValue[4];
};

// 模板类
template<typename T>
class MyTemplate {
    T data;
public:
    MyTemplate(T val) : data(val) {}
};