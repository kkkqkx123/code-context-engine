# C语言注解关系Tree-Sitter查询规则测试用例

本文档为C语言注解关系的Tree-Sitter查询规则提供测试用例，每个查询规则后都附有相应的测试代码示例，方便在Tree-Sitter Playground中进行验证。

## 1. C11属性说明符

### 查询规则
```
(attribute_declaration
  (attribute_specifier
    name: (identifier) @annotation.name
    arguments: (argument_list
      (_) @annotation.argument)*)) @annotation.relationship
```

### 测试用例
```c
// 基本属性说明符
[[deprecated]] void old_function();

// 带参数的属性说明符
[[deprecated("Use new_function instead")]] void old_function();

// 多个属性说明符
[[nodiscard, always_inline]] int calculate(int x);

// 带多个参数的属性说明符
[[format_args(1, 2)]] void printf_like(const char* format, ...);

// 自定义属性
[[custom_attribute("param1", 42)]] void custom_function();

// GCC风格的属性
__attribute__((deprecated)) void gcc_old_function();
__attribute__((deprecated("Use new_function instead"))) void gcc_old_function();
__attribute__((always_inline, hot)) int inline_function(int x);
```

## 2. 类型注解

### 查询规则
```
(type_definition
  (attribute_specifier
    name: (identifier) @annotation.name
    arguments: (argument_list
      (_) @annotation.argument)*)?) @annotation.relationship.type
```

### 测试用例
```c
// 带注解的类型定义
[[deprecated("Use NewType instead")]] typedef int OldType;

// 带多个注解的类型定义
[[nodiscard, packed]] struct PackedStruct {
    int value;
};

// 带参数的类型注解
[[aligned(16)]] typedef struct AlignedStruct {
    double data[2];
} AlignedStruct;

// 枚举类型注解
[[deprecated("Use NewEnum instead")]] typedef enum {
    OLD_VALUE1,
    OLD_VALUE2
} OldEnum;

// 复杂类型注解
[[gnu::packed, gnu::aligned(4)]] typedef struct {
    char a;
    int b;
} PackedAlignedStruct;
```

## 3. 函数注解

### 查询规则
```
(function_definition
  (attribute_specifier
    name: (identifier) @annotation.name
    arguments: (argument_list
      (_) @annotation.argument)*)?) @annotation.relationship.function
```

### 测试用例
```c
// 带注解的函数定义
[[deprecated("Use new_function instead")]] void old_function() {
    // 函数体
}

// 带多个注解的函数
[[nodiscard, always_inline]] int calculate(int x) {
    return x * 2;
}

// 带参数的函数注解
[[format_args(1, 2)]] void debug_print(const char* format, ...) {
    // 函数体
}

// 带多个参数的函数注解
[[custom_attribute("debug", 1, true)]] void complex_function() {
    // 函数体
}

// GCC风格的函数注解
__attribute__((hot, always_inline)) int hot_function(int x) {
    return x * x;
}

// 混合注解风格
[[nodiscard]] __attribute__((malloc)) void* allocate_memory(size_t size) {
    return malloc(size);
}
```

## 4. 变量注解

### 查询规则
```
(declaration
  (attribute_specifier
    name: (identifier) @annotation.name
    arguments: (argument_list
      (_) @annotation.argument)*)?) @annotation.relationship.variable
```

### 测试用例
```c
// 带注解的变量声明
[[deprecated("Use new_variable instead")]] int old_variable;

// 带多个注解的变量
[[unused, maybe_unused]] int debug_variable;

// 带参数的变量注解
[[aligned(16)]] int aligned_array[8];

// 带多个参数的变量注解
[[custom_attribute("section", ".data")]] int special_variable;

// 指针变量注解
[[nodiscard]] void* result_pointer;

// 常量变量注解
[[deprecated("Use NEW_CONSTANT instead")]] const int OLD_CONSTANT = 42;

// GCC风格的变量注解
__attribute__((section(".special_data"))) int special_data = 100;

// 复杂类型变量注解
[[gnu::aligned(8), gnu::cleanup(cleanup_function)]] char* buffer;
```

## 5. 结构体字段注解

### 查询规则
```
(field_declaration
  (attribute_specifier
    name: (identifier) @annotation.name
    arguments: (argument_list
      (_) @annotation.argument)*)?) @annotation.relationship.field
```

### 测试用例
```c
// 带注解的结构体字段
struct AnnotatedStruct {
    [[deprecated("Use new_field instead")]] int old_field;
    [[unused]] int debug_field;
    [[aligned(8)]] double aligned_field;
    [[custom_attribute("description", "Important field")]] char important_field[64];
};

// 带多个注解的字段
struct MultiAnnotatedStruct {
    [[nodiscard, maybe_unused]] int multi_field;
    [[gnu::packed, gnu::aligned(2)]] short packed_field;
};

// 位字段注解
struct BitFieldStruct {
    [[deprecated("Use new_bits instead")]] unsigned int old_bits : 4;
    [[custom_attribute("flag", true)]] unsigned int flag_bit : 1;
};

// 联合体字段注解
union AnnotatedUnion {
    [[deprecated("Use new_member instead")]] int old_member;
    [[aligned(16)]] double aligned_member;
};

// 嵌套结构体字段注解
struct NestedStruct {
    struct {
        [[deprecated("Use nested_new_field instead")]] int nested_old_field;
        [[custom_attribute("metadata", "nested")]] char nested_meta[32];
    } inner;
};

// 复杂类型字段注解
struct ComplexStruct {
    [[gnu::cleanup(string_cleanup)]] char* string_field;
    [[gnu::nonnull]] void* pointer_field;
    [[gnu::sentinel]] int* sentinel_field;
};
```

## 6. 综合测试用例

### 测试用例
```c
// 综合使用各种注解
[[deprecated("Use NewStruct instead")]] typedef struct {
    [[deprecated("Use new_id instead")]] int old_id;
    [[aligned(16)]] double matrix[4][4];
    [[custom_attribute("metadata", "version", 2)]] char version_info[32];
    
    struct {
        [[nodiscard]] int result;
        [[maybe_unused]] int reserved;
    } status;
} OldStruct;

[[nodiscard, always_inline]]
[[custom_attribute("performance", "critical")]]
int complex_calculation(
    [[maybe_unused]] int param1,
    [[deprecated("Use new_param instead")]] int old_param
) {
    [[unused]] int local_var = 0;
    [[aligned(8)]] int aligned_buffer[4];
    
    return param1 + old_param;
}

// 全局变量注解
[[deprecated("Use global_config_v2 instead")]]
[[gnu::section(".config")]]
struct {
    [[nodiscard]] int version;
    [[custom_attribute("checksum", true)]] unsigned int checksum;
} global_config;