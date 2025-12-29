# C++ Variables Tree-Sitter查询规则测试用例

本文档为C++ Variables的Tree-Sitter查询规则提供测试用例，每个查询规则后都附有相应的测试代码示例，方便在TreeSitter Playground中进行验证。

## 1. 变量声明与初始化

### 查询规则
```
(declaration
  type: (_)
  declarator: (init_declarator
    declarator: (identifier) @name.definition.variable)) @definition.variable
```

### 测试用例
```cpp
// 基本变量声明与初始化
int counter = 0;
double pi = 3.14159;
char letter = 'A';
bool flag = true;

// 多个变量声明
int x = 10, y = 20, z = 30;
double a = 1.5, b = 2.5;

// 常量变量
const int MAX_SIZE = 100;
const char* message = "Hello, World!";

// 指针变量
int* ptr = nullptr;
char* buffer = new char[256];

// 数组变量
int array[10] = {1, 2, 3, 4, 5, 6, 7, 8, 9, 10};
char str[] = "Hello";
```

## 2. 结构化绑定 (C++17)

### 查询规则
```
(declaration
  (structured_binding_declarator
    (identifier) @name.definition.binding)) @definition.binding
```

### 测试用例
```cpp
#include <tuple>
#include <map>
#include <vector>

// 从元组进行结构化绑定
auto tuple = std::make_tuple(1, 2.5, 'c');
auto [id, value, type] = tuple;

// 从pair进行结构化绑定
std::pair<int, std::string> pair = {42, "answer"};
auto [number, text] = pair;

// 遍历时使用结构化绑定
std::map<int, std::string> myMap = {{1, "one"}, {2, "two"}, {3, "three"}};
for (const auto& [key, value] : myMap) {
    // 使用 key 和 value
}

// 结构化绑定带引用
std::vector<std::pair<int, double>> pairs = {{1, 1.1}, {2, 2.2}};
for (auto& [index, data] : pairs) {
    data *= 2;  // 修改原始数据
}

// 忽略某些元素
auto [first, , third] = std::make_tuple(1, 2, 3);  // 忽略第二个元素
```

## 3. 静态变量

### 查询规则
```
(declaration
  (storage_class_specifier) @storage.specifier
  type: (_)
  declarator: (init_declarator
    declarator: (identifier) @name.definition.static_var)
  (#match? @storage.specifier "static")) @definition.static.variable
```

### 测试用例
```cpp
// 全局静态变量
static int globalCounter = 0;

// 函数内的静态变量
void incrementCounter() {
    static int localCounter = 0;  // 只初始化一次
    localCounter++;
    globalCounter++;
}

// 类中的静态变量
class MyClass {
public:
    static int classCounter;
    static const int MAX_VALUE = 100;
    
    static void increment() {
        classCounter++;
    }
};

int MyClass::classCounter = 0;  // 静态成员变量定义
```

## 4. 线程局部变量

### 查询规则
```
(declaration
  (storage_class_specifier) @storage.specifier
 type: (_)
 declarator: (init_declarator
    declarator: (identifier) @name.definition.thread_local_var)
  (#match? @storage.specifier "thread_local")) @definition.thread_local.variable
```

### 测试用例
```cpp
#include <thread>

// 线程局部变量
thread_local int threadId = 0;
thread_local std::string threadName = "default";

void threadFunction(int id) {
    threadId = id;
    threadName = "Thread-" + std::to_string(id);
    
    // 每个线程都有自己的 threadId 和 threadName 副本
}
```

## 5. 外部变量声明

### 查询规则
```
(declaration
  (storage_class_specifier) @storage.specifier
  type: (_)
 declarator: (identifier) @name.definition.external_var)
  (#match? @storage.specifier "extern")) @definition.external.variable
```

### 测试用例
```cpp
// 外部变量声明
extern int globalVar;
extern const char* appName;

// 在另一个文件中定义
// int globalVar = 42;
// const char* appName = "MyApplication";

// 使用外部变量
void useExternalVars() {
    globalVar = 100;
    printf("App: %s, Value: %d\n", appName, globalVar);
}
```

## 6. 变量声明但未初始化

### 查询规则
```
(declaration
  type: (_)
  declarator: (identifier) @name.definition.uninitialized_var) @definition.uninitialized.variable
```

### 测试用例
```cpp
// 未初始化的变量
int uninitializedInt;
double uninitializedDouble;
char uninitializedChar;

// 数组未完全初始化
int partialArray[5] = {1, 2, 3};  // 后两个元素未初始化

// 指针未初始化
int* uninitPtr;
char* bufferPtr;

// 函数中声明但未初始化
void function() {
    int localVar;      // 未初始化
    double localDouble; // 未初始化
}
```

## 7. 引用变量

### 查询规则
```
(declaration
  type: (_)
  declarator: (reference_declarator
    declarator: (identifier) @name.definition.reference)) @definition.reference
```

### 测试用例
```cpp
// 引用变量
int original = 42;
int& ref = original;  // ref 是 original 的引用

// 函数参数中的引用
void modifyValue(int& value) {
    value = 100;  // 修改原始值
}

// 常量引用
const int& constRef = original;  // 不能通过此引用修改值

// 返回引用的函数
int& getReference(int& param) {
    return param;
}

// 引用初始化
int x = 10;
int& y = x;  // y 引用 x