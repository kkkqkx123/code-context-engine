# C++ Namespaces Tree-Sitter查询规则测试用例

本文档为C++ Namespaces的Tree-Sitter查询规则提供测试用例，每个查询规则后都附有相应的测试代码示例，方便在TreeSitter Playground中进行验证。

## 1. 基本命名空间定义

### 查询规则
```
(namespace_definition
  name: (namespace_identifier) @name.definition.namespace) @definition.namespace
```

### 测试用例
```cpp
// 基本命名空间
namespace Math {
    const double PI = 3.14159;
    
    int add(int a, int b) {
        return a + b;
    }
    
    class Calculator {
    public:
        int multiply(int x, int y) {
            return x * y;
        }
    };
}

// 命名空间中的函数
namespace Utilities {
    void print(const char* str) {
        printf("%s\n", str);
    }
}
```

## 2. 嵌套命名空间定义

### 查询规则
```
(namespace_definition
  body: (declaration_list
    (namespace_definition
      name: (namespace_identifier) @name.definition.namespace))) @definition.namespace
```

### 测试用例
```cpp
// 嵌套命名空间
namespace Graphics {
    namespace Rendering {
        namespace OpenGL {
            void initialize() {
                // OpenGL initialization
            }
            
            void render() {
                // OpenGL rendering
            }
        }
        
        namespace DirectX {
            void initialize() {
                // DirectX initialization
            }
        }
    }
    
    namespace UI {
        class Button {
        public:
            void draw() {
                // Draw button
            }
        };
    }
}
```

## 3. 使用声明

### 查询规则
```
(using_declaration) @definition.using
```

### 测试用例
```cpp
#include <iostream>

namespace MyNamespace {
    int value = 42;
    
    void function() {
        std::cout << "Hello from MyNamespace" << std::endl;
    }
}

// 使用声明
using MyNamespace::value;
using MyNamespace::function;

// 使用整个命名空间
using namespace std;

int main() {
    cout << value << endl;  // 使用了 using 声明
    function();             // 使用了 using 声明
    return 0;
}
```

## 4. 匿名命名空间

### 查询规则
```
(namespace_definition
  name: (identifier)? @name.definition.namespace) @definition.namespace
```

### 测试用例
```cpp
// 匿名命名空间
namespace {
    int internalValue = 100;
    
    void internalFunction() {
        // 仅在此翻译单元内可见
    }
    
    class InternalClass {
    public:
        void doWork() {
            // 内部实现
        }
    };
}

int main() {
    internalValue = 200;
    internalFunction();
    return 0;
}
```

## 5. 内联命名空间

### 查询规则
```
(inline_namespace_definition
  name: (namespace_identifier) @name.definition.namespace) @definition.namespace
```

### 测试用例
```cpp
namespace Library {
    inline namespace Version1 {
        int getVersion() {
            return 1;
        }
        
        class API {
        public:
            void initialize() {
                // Version 1 API
            }
        };
    }
    
    // 内联命名空间中的内容可以直接访问
    void useAPI() {
        API api;
        api.initialize();
    }
}
```

## 6. 命名空间别名

### 查询规则
```
(namespace_alias_definition
  name: (namespace_identifier) @name.definition.namespace
  value: (identifier) @name.reference.namespace) @definition.namespace.alias
```

### 测试用例
```cpp
namespace VeryLongNamespaceName {
    namespace NestedNamespace {
        int value = 42;
        
        void function() {
            // function implementation
        }
    }
}

// 命名空间别名
namespace VLN = VeryLongNamespaceName;
namespace VLN_Nested = VeryLongNamespaceName::NestedNamespace;

int main() {
    // 使用别名
    int val = VLN_Nested::value;
    VLN_Nested::function();
    
    return 0;
}