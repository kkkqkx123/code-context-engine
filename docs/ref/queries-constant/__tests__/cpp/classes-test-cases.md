# C++ Classes Query Test Cases

## 测试用例 1: 基本类定义
**代码文件 (code.cpp):**
```cpp
class MyClass {
public:
    int x;
    int y;
    
    MyClass(int x, int y) : x(x), y(y) {}
    
    int getX() const {
        return x;
    }
    
    int getY() const {
        return y;
    }
};
```

**查询规则:**
```
(class_specifier
  name: (type_identifier) @type.name
  body: (field_declaration_list) @type.body) @definition.class
```

## 测试用例 2: 结构体定义
**代码文件 (code.cpp):**
```cpp
struct Point {
    int x;
    int y;
    
    Point(int x, int y) : x(x), y(y) {}
};
```

**查询规则:**
```
(struct_specifier
  name: (type_identifier) @type.name
  body: (field_declaration_list) @type.body) @definition.struct
```

## 测试用例 3: 联合体定义
**代码文件 (code.cpp):**
```cpp
union Data {
    int i;
    float f;
    char str[20];
};
```

**查询规则:**
```
(union_specifier
  name: (type_identifier) @type.name
 body: (field_declaration_list) @type.body) @definition.union
```

## 测试用例 4: 模板类定义
**代码文件 (code.cpp):**
```cpp
template<typename T>
class Vector {
private:
    T* data;
    size_t size;
public:
    Vector(size_t s) : size(s) {
        data = new T[size];
    }
    
    ~Vector() {
        delete[] data;
    }
};
```

**查询规则:**
```
(template_declaration
  parameters: (template_parameter_list)
  (class_specifier
    name: (type_identifier) @template.class.name
    body: (field_declaration_list) @template.class.body)) @definition.template.type
```

## 测试用例 5: 带继承的类
**代码文件 (code.cpp):**
```cpp
class Animal {
public:
    virtual void speak() = 0;
};

class Dog : public Animal {
public:
    void speak() override {
        // Dog barks
    }
};
```

**查询规则:**
```
(class_specifier
  name: (type_identifier) @class.name
  base_class_clause: .
    (base_class_clause
      (type_identifier) @base.class)
  body: (field_declaration_list) @class.body) @definition.class.with_inheritance
```

## 测试用例 6: 访问说明符
**代码文件 (code.cpp):**
```cpp
class AccessExample {
private:
    int privateVar;
protected:
    int protectedVar;
public:
    int publicVar;
};
```

**查询规则:**
```
(access_specifier) @definition.access.specifier
```

## 测试用例 7: 构造函数和析构函数
**代码文件 (code.cpp):**
```cpp
class Lifecycle {
private:
    int* data;
public:
    // 构造函数
    Lifecycle(int size) {
        data = new int[size];
    }
    
    // 析构函数
    ~Lifecycle() {
        delete[] data;
    }
};
```

**查询规则:**
```
[
  (function_definition
    declarator: (function_declarator
      declarator: (identifier) @constructor.name)
    body: (compound_statement) @constructor.body)
  (function_definition
    declarator: (function_declarator
      declarator: (destructor_name) @destructor.name)
    body: (compound_statement) @destructor.body)
] @definition.constructor_or_destructor
```

## 测试用例 8: 构造函数初始化列表
**代码文件 (code.cpp):**
```cpp
class InitList {
private:
    int x, y;
public:
    InitList(int a, int b) : x(a), y(b) {}
};
```

**查询规则:**
```
(constructor_initializer
    name: (field_identifier) @member.name
    value: (_) @member.value) @definition.constructor_initializer
```

## 测试用例 9: 成员字段
**代码文件 (code.cpp):**
```cpp
class Fields {
public:
    int publicField;
    static int staticField;
private:
    int privateField;
};
```

**查询规则:**
```
(field_declaration
  type: (_) @field.type
  declarator: (field_declarator
    declarator: (field_identifier) @field.name)) @definition.field
```

## 测试用例 10: 成员方法
**代码文件 (code.cpp):**
```cpp
class Methods {
public:
    void method1() {
        // method body
    }
    
    int method2(int param) {
        return param * 2;
    }
};
```

**查询规则:**
```
(function_definition
  declarator: (function_declarator
    declarator: (field_identifier) @method.name)
  body: (compound_statement) @method.body) @definition.method
```

## 测试用例 11: 静态成员
**代码文件 (code.cpp):**
```cpp
class StaticMembers {
public:
    static int staticVar;
    static void staticMethod() {
        // static method
    }
};
```

**查询规则:**
```
[
  (field_declaration
    (storage_class_specifier) @static.specifier
    type: (_) @static.field.type
    declarator: (field_declarator
      declarator: (field_identifier) @static.field.name)
    (#match? @static.specifier "static")) @definition.static.field
  (function_definition
    (storage_class_specifier) @static.specifier
    declarator: (function_declarator
      declarator: (field_identifier) @static.method.name)
    (#match? @static.specifier "static")) @definition.static.method
] @definition.static.member
```

## 测试用例 12: 虚函数
**代码文件 (code.cpp):**
```cpp
class Base {
public:
    virtual void virtualMethod() {
        // virtual method
    }
    
    virtual void pureVirtualMethod() = 0;
};
```

**查询规则:**
```
(function_definition
  (virtual_specifier) @virtual.specifier
  declarator: (function_declarator
    declarator: (field_identifier) @virtual.method.name)) @definition.virtual.method
```

## 测试用例 13: 纯虚函数
**代码文件 (code.cpp):**
```cpp
class Abstract {
public:
    virtual void pureVirtual() = 0;
};
```

**查询规则:**
```
(function_definition
  (virtual_specifier) @virtual.specifier
  declarator: (function_declarator
    declarator: (field_identifier) @pure.virtual.method)
  body: (pure_virtual_clause)) @definition.pure.virtual.method
```

## 测试用例 14: 模板成员函数
**代码文件 (code.cpp):**
```cpp
template<typename T>
class TemplateMember {
public:
    template<typename U>
    void templateMethod(U value) {
        // template method
    }
};
```

**查询规则:**
```
(template_declaration
  parameters: (template_parameter_list)
  (function_definition
    declarator: (function_declarator
      declarator: (field_identifier) @template.method.name))) @definition.template.method
```

## 测试用例 15: 友元函数
**代码文件 (code.cpp):**
```cpp
class FriendClass {
private:
    int value;
public:
    FriendClass(int v) : value(v) {}
    
    friend void friendFunction(FriendClass& obj);
};

void friendFunction(FriendClass& obj) {
    // can access private members
    obj.value = 0;
}
```

**查询规则:**
```
(friend_declaration
  (function_definition
    declarator: (function_declarator
      declarator: (identifier) @friend.function))) @definition.friend.function
```

## 测试用例 16: 友元类
**代码文件 (code.cpp):**
```cpp
class MyClass {
private:
    int value;
public:
    MyClass(int v) : value(v) {}
    friend class FriendClass;
};

class FriendClass {
public:
    void accessPrivate(MyClass& obj) {
        // can access private members of MyClass
        obj.value = 0;
    }
};
```

**查询规则:**
```
(friend_declaration
  (class_specifier
    name: (type_identifier) @friend.class)) @definition.friend.class
```

## 测试用例 17: 交替查询 - 统一类型声明
**代码文件 (code.cpp):**
```cpp
class MyClass {
public:
    int x;
};

struct MyStruct {
    int y;
};

union MyUnion {
    int i;
    float f;
};
```

**查询规则:**
```
[
  (class_specifier
    name: (type_identifier) @type.name
    body: (field_declaration_list) @type.body) @definition.class
  (struct_specifier
    name: (type_identifier) @type.name
    body: (field_declaration_list) @type.body) @definition.struct
  (union_specifier
    name: (type_identifier) @type.name
    body: (field_declaration_list) @type.body) @definition.union
] @definition.type
```

## 测试用例 18: 友元交替查询
**代码文件 (code.cpp):**
```cpp
class FriendExample {
private:
    int value;
public:
    FriendExample(int v) : value(v) {}
    
    friend void friendFunction(FriendExample& obj);
    friend class FriendClass;
};
```

**查询规则:**
```
[
  (friend_declaration
    (function_definition
      declarator: (function_declarator
        declarator: (identifier) @friend.function))) @definition.friend.function
  (friend_declaration
    (class_specifier
      name: (type_identifier) @friend.class)) @definition.friend.class
] @definition.friend
```

## 测试用例 19: 静态成员交替查询
**代码文件 (code.cpp):**
```cpp
class StaticExample {
public:
    static int staticField;
    static void staticMethod() {}
};
int StaticExample::staticField = 0;
```

**查询规则:**
```
[
  (field_declaration
    (storage_class_specifier) @static.specifier
    type: (_) @static.field.type
    declarator: (field_declarator
      declarator: (field_identifier) @static.field.name)
    (#match? @static.specifier "static")) @definition.static.field
  (function_definition
    (storage_class_specifier) @static.specifier
    declarator: (function_declarator
      declarator: (field_identifier) @static.method.name)
    (#match? @static.specifier "static")) @definition.static.method
] @definition.static.member
```

## 测试用例 20: 构造函数和析构函数交替查询
**代码文件 (code.cpp):**
```cpp
class LifecycleExample {
private:
    int* data;
public:
    LifecycleExample() {
        data = new int[10];
    }
    
    ~LifecycleExample() {
        delete[] data;
    }
};
```

**查询规则:**
```
[
  (function_definition
    declarator: (function_declarator
      declarator: (identifier) @constructor.name)
    body: (compound_statement) @constructor.body)
  (function_definition
    declarator: (function_declarator
      declarator: (destructor_name) @destructor.name)
    body: (compound_statement) @destructor.body)
] @definition.constructor_or_destructor