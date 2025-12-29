# C++ Functions Tree-Sitter查询规则测试用例

本文档为C++ Functions的Tree-Sitter查询规则提供测试用例，每个查询规则后都附有相应的测试代码示例，方便在TreeSitter Playground中进行验证。

## 1. 统一的函数查询

### 查询规则
```
[
  (function_definition
    type: (_)
    declarator: (function_declarator
      declarator: (identifier) @function.name)
    body: (compound_statement) @function.body) @definition.function
  (declaration
    type: (_)
    declarator: (function_declarator
      declarator: (identifier) @function.name
      parameters: (parameter_list))) @definition.function.prototype
] @definition.function
```

### 测试用例
```cpp
// 函数定义
int add(int a, int b) {
    return a + b;
}

// 函数声明
void printMessage(const char* msg);

// 函数定义
double multiply(double x, double y) {
    return x * y;
}
```

## 2. 方法查询

### 查询规则
```
(function_definition
  declarator: (function_declarator
    declarator: (field_identifier) @method.name)
  body: (compound_statement) @method.body) @definition.method
```

### 测试用例
```cpp
class Calculator {
public:
    int add(int a, int b) {
        return a + b;
    }
    
    int subtract(int a, int b) {
        return a - b;
    }
    
    void displayResult() {
        // display result
    }
};
```

## 3. 构造函数和析构函数查询

### 查询规则
```
[
  (function_definition
    declarator: (function_declarator
      declarator: (identifier) @constructor.name)
    body: (compound_statement) @constructor.body) @definition.constructor
  (function_definition
    declarator: (function_declarator
      declarator: (destructor_name) @destructor.name)
    body: (compound_statement) @destructor.body) @definition.destructor
] @definition.constructor_or_destructor
```

### 测试用例
```cpp
class MyClass {
private:
    int* data;
    size_t size;
    
public:
    // 构造函数
    MyClass(size_t s) : size(s) {
        data = new int[size];
    }
    
    // 析构函数
    ~MyClass() {
        delete[] data;
    }
    
    // 拷贝构造函数
    MyClass(const MyClass& other) {
        size = other.size;
        data = new int[size];
        for(size_t i = 0; i < size; ++i) {
            data[i] = other.data[i];
        }
    }
};
```

## 4. 带参数的函数查询

### 查询规则
```
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @function.name
    parameters: (parameter_list
      (parameter_declaration
        type: (_)
        declarator: (identifier) @param.name)*))
  body: (compound_statement) @function.body) @definition.function.with_params
```

### 测试用例
```cpp
// 单参数函数
void printInt(int value) {
    printf("%d\n", value);
}

// 多参数函数
int calculate(int a, int b, int c) {
    return a * b + c;
}

// 无参数函数（不会被此查询匹配）
void doSomething() {
    // do something
}
```

## 5. 模板函数查询

### 查询规则
```
(template_declaration
  parameters: (template_parameter_list)
  (function_definition
    declarator: (function_declarator
      declarator: (identifier) @template.function.name)
    body: (compound_statement) @template.function.body)) @definition.template.function
```

### 测试用例
```cpp
// 模板函数
template<typename T>
T max(T a, T b) {
    return (a > b) ? a : b;
}

// 多模板参数函数
template<typename T, typename U>
T convert(U value) {
    return static_cast<T>(value);
}

// 模板函数特化
template<>
int max<int>(int a, int b) {
    return (a > b) ? a : b;
}
```

## 6. 运算符重载查询

### 查询规则
```
[
  (function_definition
    declarator: (function_declarator
      declarator: (operator_name) @operator.name)) @definition.operator.overload
  (function_definition
    declarator: (function_declarator
      declarator: (operator_name) @operator.new.name))
  (#match? @operator.new.name "^(new|delete)$") @definition.operator.new.delete
] @definition.operator
```

### 测试用例
```cpp
class Complex {
private:
    double real, imag;
    
public:
    Complex(double r = 0, double i = 0) : real(r), imag(i) {}
    
    // 重载加法运算符
    Complex operator+(const Complex& other) {
        return Complex(real + other.real, imag + other.imag);
    }
    
    // 重载赋值运算符
    Complex& operator=(const Complex& other) {
        real = other.real;
        imag = other.imag;
        return *this;
    }
    
    // 重载下标运算符
    double& operator[](int index) {
        return (index == 0) ? real : imag;
    }
};

// 重载 new 和 delete 运算符
void* operator new(size_t size) {
    return malloc(size);
}

void operator delete(void* ptr) {
    free(ptr);
}
```

## 7. 特殊函数修饰符查询

### 查询规则
```
[
  (function_definition
    (storage_class_specifier) @constexpr.specifier
    declarator: (function_declarator
      declarator: (identifier) @constexpr.function)
    (#match? @constexpr.specifier "^(constexpr|consteval)$")) @definition.constexpr.function
  (function_definition
    (explicit_specifier) @explicit.specifier
    declarator: (function_declarator
      declarator: (identifier) @explicit.function)) @definition.explicit.function
  (function_definition
    (virtual_specifier) @virtual.specifier
    declarator: (function_declarator
      declarator: (field_identifier) @virtual.method)) @definition.virtual.method
] @definition.special.function
```

### 测试用例
```cpp
class Example {
public:
    // constexpr 函数
    constexpr int square(int x) {
        return x * x;
    }
    
    // consteval 函数 (C++20)
    consteval int cube(int x) {
        return x * x * x;
    }
    
    // explicit 构造函数
    explicit Example(int value) {
        // constructor
    }
    
    // virtual 方法
    virtual void virtualMethod() {
        // virtual method
    }
    
    // 纯虚函数
    virtual void pureVirtual() = 0;
};
```

## 8. 虚函数重写查询

### 查询规则
```
(function_definition
  declarator: (function_declarator
    declarator: (field_identifier) @override.method)
  (virtual_specifier) @virtual.specifier
  (override_specifier) @override.specifier) @definition.virtual.override
```

### 测试用例
```cpp
class Base {
public:
    virtual void method() {
        // base method
    }
    virtual void pureVirtual() = 0;
};

class Derived : public Base {
public:
    // 重写虚函数
    void method() override {
        // derived method
    }
    
    void pureVirtual() override {
        // implementation
    }
};