# C++ Classes Tree-Sitter查询规则测试用例

本文档为C++ Classes的Tree-Sitter查询规则提供测试用例，每个查询规则后都附有相应的测试代码示例，方便在TreeSitter Playground中进行验证。

## 1. 基本类定义

### 测试用例
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

### 查询规则
```
(class_specifier
  name: (type_identifier) @type.name
  body: (field_declaration_list) @type.body) @definition.class
```

## 2. 结构体定义

### 查询规则
```
(struct_specifier
  name: (type_identifier) @type.name
  body: (field_declaration_list) @type.body) @definition.struct
```

### 测试用例
```cpp
struct Point {
    int x;
    int y;
    
    Point(int x, int y) : x(x), y(y) {}
};
```

## 3. 联合体定义

### 查询规则
```
(union_specifier
  name: (type_identifier) @type.name
 body: (field_declaration_list) @type.body) @definition.union
```

### 测试用例
```cpp
union Data {
    int i;
    float f;
    char str[20];
};
```

## 4. 模板类定义

### 查询规则
```
(template_declaration
  parameters: (template_parameter_list)
  (class_specifier
    name: (type_identifier) @template.class.name
    body: (field_declaration_list) @template.class.body)) @definition.template.type
```

### 测试用例
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

## 5. 带继承的类

### 查询规则
```
(class_specifier
  name: (type_identifier) @class.name
  (base_class_clause
    (type_identifier) @base.class)
  body: (field_declaration_list) @class.body) @definition.class.with_inheritance
```

### 测试用例
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

## 6. 访问说明符

### 查询规则
```
(access_specifier) @definition.access.specifier
```

### 测试用例
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

## 7. 构造函数和析构函数

### 查询规则
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

### 测试用例
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

## 8. 构造函数初始化列表

### 查询规则
```
(field_initializer
  (field_identifier) @member.name
  (argument_list (_) @member.value)) @definition.member_initializer
```

### 测试用例
```cpp
class InitList {
private:
    int x, y;
public:
    InitList(int a, int b) : x(a), y(b) {}
};
```

## 9. 成员字段

### 查询规则
```
(field_declaration
  type: (_) @field.type
 declarator: (field_declarator
    declarator: (field_identifier) @field.name)) @definition.field
```

### 测试用例
```cpp
class Fields {
public:
    int publicField;
    static int staticField;
private:
    int privateField;
};
```

## 10. 成员方法

### 查询规则
```
(function_definition
 declarator: (function_declarator
    declarator: (field_identifier) @method.name)
  body: (compound_statement) @method.body) @definition.method
```

### 测试用例
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

## 11. 静态成员

### 查询规则
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

### 测试用例
```cpp
class StaticMembers {
public:
    static int staticVar;
    static void staticMethod() {
        // static method
    }
};
```

## 12. 虚函数

### 查询规则
```
(function_definition
 (virtual_specifier) @virtual.specifier
 declarator: (function_declarator
    declarator: (field_identifier) @virtual.method.name)) @definition.virtual.method
```

### 测试用例
```cpp
class Base {
public:
    virtual void virtualMethod() {
        // virtual method
    }
    
    virtual void pureVirtualMethod() = 0;
};
```

## 13. 纯虚函数

### 查询规则
```
(function_definition
  (virtual_specifier) @virtual.specifier
 declarator: (function_declarator
    declarator: (field_identifier) @pure.virtual.method)
  body: (pure_virtual_clause)) @definition.pure.virtual.method
```

### 测试用例
```cpp
class Abstract {
public:
    virtual void pureVirtual() = 0;
};
```

## 14. 模板成员函数

### 查询规则
```
(template_declaration
  parameters: (template_parameter_list)
  (function_definition
    declarator: (function_declarator
      declarator: (field_identifier) @template.method.name))) @definition.template.method
```

### 测试用例
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

## 15. 友元函数

### 查询规则
```
(friend_declaration
  (function_definition
    declarator: (function_declarator
      declarator: (identifier) @friend.function))) @definition.friend.function
```

### 测试用例
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

## 16. 友元类

### 查询规则
```
(friend_declaration
  (class_specifier
    name: (type_identifier) @friend.class)) @definition.friend.class
```

### 测试用例
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

## 17. 交替查询 - 统一类型声明

### 查询规则
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

### 测试用例
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

## 18. 友元交替查询

### 查询规则
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

### 测试用例
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

## 19. 静态成员交替查询

### 查询规则
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

### 测试用例
```cpp
class StaticExample {
public:
    static int staticField;
    static void staticMethod() {}
};
int StaticExample::staticField = 0;
```

## 20. 构造函数和析构函数交替查询

### 查询规则
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

### 测试用例
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