# 多语言签名捕获分离设计方案

## 1. 背景与动机

### 1.1 问题描述

通过对所有主要语言的 tree-sitter 查询文件进行分析，发现它们都存在类似的问题：**实体捕获（如 `@entity.struct`、`@entity.function`、`@entity.method`）捕获了整个节点，包括代码体（字段定义、方法实现、函数体等），导致签名包含了大量不必要的代码片段**。

### 1.2 影响范围

以下语言都存在类似问题：

| 语言 | 受影响的实体类型 | 问题描述 |
|------|------------------|----------|
| **C/C++** | struct, class, function, method | 捕获整个节点，包括字段定义和函数体 |
| **Go** | struct, interface, function, method | 捕获整个节点，包括字段定义和函数体 |
| **Python** | class, function | 捕获整个节点，包括类体和函数体 |
| **Java** | class, method | 捕获整个节点，包括类体和方法体 |
| **TypeScript/JavaScript** | interface, method, function | 捕获整个节点，包括方法体和函数体 |
| **Kotlin** | class, function, method | 捕获整个节点，包括类体、函数体和方法体 |
| **Scala** | class, trait, object, function | 捕获整个节点，包括类体和函数体 |
| **Ruby** | class, module, method | 捕获整个节点，包括类体和方法体 |
| **PHP** | class, interface, trait, function | 捕获整个节点，包括类体和函数体 |
| **Dart** | class, method | 捕获整个节点，包括类体和方法体 |
| **C#** | class, interface, struct, method | 捕获整个节点，包括类体和方法体 |
| **Bash** | function | 捕获整个节点，包括函数体 |
| **Lua** | function, method | 捕获整个节点，包括函数体和方法体 |
| **Svelte/Vue/HTML** | element | 捕获整个节点，包括元素内容 |
| **CSS** | style_rule | 捕获整个节点，包括规则块 |

### 1.3 目标

设计一个统一的方案，为所有语言添加签名特定的捕获，解决签名包含大量代码片段的问题。

## 2. 现状分析

### 2.1 问题根源

所有语言的 tree-sitter 查询都存在类似的问题：

```tree-sitter
; 示例：C 语言结构体查询
(struct_specifier
  name: (type_identifier) @entity.struct.name
  body: (field_declaration_list) @entity.struct.body
) @entity.struct  ; ← 捕获了整个 struct_specifier 节点
```

### 2.2 签名提取逻辑

当前的签名提取逻辑（`extract_signature` 函数）使用 `find_main_capture` 选择了最大的捕获，然后提取整个捕获的文本：

```rust
pub fn extract_signature(mat: &QueryMatch, source: &str) -> String {
    if let Some(main) = find_main_capture(mat) {
        return utils::extract_text_from_source(source, main.start_byte, main.end_byte);
    }
    String::new()
}
```

### 2.3 实体需求分析

通过对比 `structured` 和 `chunks` 目录的输出，可以发现：

| 目录 | 内容 | 用途 |
|------|------|------|
| `structured/` | 原始的实体信息，包括签名 | 调试、分析、关系提取 |
| `chunks/emb/` | 经过 NL 转换后的内容 | 嵌入、搜索 |

**关键发现**：
- Chunks 目录中的内容已经经过 NL 转换，只包含必要的信息
- Structured 目录中的签名包含了整个代码片段，这是不必要的
- **当前实体也不需要这些数据**：签名应该只包含签名部分，而不是整个代码片段

## 3. 核心决策

### 3.1 方案选择

| 方案 | 说明 | 优点 | 缺点 |
|------|------|------|------|
| A. 后处理分离 | 保持现有捕获不变，在提取签名时进行后处理 | 改动最小，向后兼容 | 无法完全解决根本问题 |
| B. 捕获分离 | 为每个语言添加签名特定的捕获 | 根本解决问题，职责清晰 | 改动较大，需要迁移 |
| C. 混合方案 | 保持现有捕获，为签名添加特定捕获 | 平衡改动和效果 | 可能增加复杂性 |

**决策**：采用方案 B（捕获分离），因为：
1. 根本解决问题，职责清晰
2. 长期维护成本低
3. 支持未来扩展

### 3.2 捕获命名规范

为所有语言定义统一的签名捕获命名规范：

```
@entity.<type>.signature          # 主签名捕获
@entity.<type>.signature.name     # 名称捕获
@entity.<type>.signature.params   # 参数捕获（可选）
@entity.<type>.signature.return_type  # 返回类型捕获（可选）
```

示例：
- `@entity.struct.signature`
- `@entity.function.signature`
- `@entity.method.signature`
- `@entity.class.signature`
- `@entity.interface.signature`

## 4. 架构设计

### 4.1 新架构

```
Tree-sitter 查询（包含签名特定捕获）
    ↓
QueryExecutor.execute_query()
    ↓
QueryMatch (包含所有捕获)
    ↓
EntityExtractor.process_match()
    ↓
┌─────────────────────────────────────────┐
│  签名提取分支                           │
│  ↓                                      │
│  SignatureExtractor.extract_signature() │
│  ↓                                      │
│  Entity.signature (只包含签名部分)      │
└─────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────┐
│  NL 转换分支                            │
│  ↓                                      │
│  NlConverter.convert()                  │
│  ↓                                      │
│  自然语言描述                           │
└─────────────────────────────────────────┘
```

### 4.2 签名提取策略

对于每种实体类型，定义签名提取策略：

| 实体类型 | 签名部分 | 排除部分 |
|----------|----------|----------|
| struct/class | 名称 + 泛型参数 | 字段定义、注释 |
| interface/trait | 名称 + 泛型参数 | 方法声明、注释 |
| function/method | 名称 + 参数 + 返回类型 | 函数体、注释 |
| enum | 名称 + 枚举值 | 枚举体、注释 |
| impl | 类型 + trait | 方法实现、注释 |

### 4.3 查询改造模板

为每种语言定义查询改造模板：

```tree-sitter
; 原始查询（保留，用于 NL 转换）
(<node_type>
  name: (<name_type>) @entity.<type>.name
  body: (_) @entity.<type>.body
) @entity.<type>

; 签名查询（新增，用于签名提取）
(<node_type>
  name: (<name_type>) @entity.<type>.signature.name
  type_parameters: (_)? @entity.<type>.signature.type_params
) @entity.<type>.signature
```

## 5. 各语言改造方案

### 5.1 C/C++

**结构体/类**：
```tree-sitter
; 原始查询
(struct_specifier
  name: (type_identifier) @entity.struct.name
  body: (field_declaration_list) @entity.struct.body
) @entity.struct

; 签名查询
(struct_specifier
  name: (type_identifier) @entity.struct.signature.name
) @entity.struct.signature

; 原始查询
(class_specifier
  name: (type_identifier) @entity.class.name
  body: (field_declaration_list) @entity.class.body
) @entity.class

; 签名查询
(class_specifier
  name: (type_identifier) @entity.class.signature.name
) @entity.class.signature
```

**函数/方法**：
```tree-sitter
; 原始查询
(function_definition
  type: (_) @entity.function.return_type
  declarator: (function_declarator
    declarator: (identifier) @entity.function.name
    parameters: (parameter_list) @entity.function.params
  )
  body: (compound_statement) @entity.function.body
) @entity.function

; 签名查询
(function_definition
  type: (_) @entity.function.signature.return_type
  declarator: (function_declarator
    declarator: (identifier) @entity.function.signature.name
    parameters: (parameter_list) @entity.function.signature.params
  )
) @entity.function.signature
```

### 5.2 Go

**结构体/接口**：
```tree-sitter
; 原始查询
(type_declaration
  (type_spec
    name: (type_identifier) @entity.struct.name
    type: (struct_type) @entity.struct.body
  ) @entity.struct
)

; 签名查询
(type_declaration
  (type_spec
    name: (type_identifier) @entity.struct.signature.name
    type: (struct_type)
  ) @entity.struct.signature
)

; 原始查询
(type_declaration
  (type_spec
    name: (type_identifier) @entity.interface.name
    type: (interface_type) @entity.interface.body
  ) @entity.interface
)

; 签名查询
(type_declaration
  (type_spec
    name: (type_identifier) @entity.interface.signature.name
    type: (interface_type)
  ) @entity.interface.signature
)
```

**函数/方法**：
```tree-sitter
; 原始查询
(function_declaration
  name: (identifier) @entity.function.name
  parameters: (parameter_list) @entity.function.params
  result: (_)? @entity.function.return_type
  body: (block) @entity.function.body
) @entity.function

; 签名查询
(function_declaration
  name: (identifier) @entity.function.signature.name
  parameters: (parameter_list) @entity.function.signature.params
  result: (_)? @entity.function.signature.return_type
) @entity.function.signature
```

### 5.3 Python

**类**：
```tree-sitter
; 原始查询
(class_definition
  name: (identifier) @entity.class.name
  superclasses: (argument_list)? @entity.class.base
  body: (block) @entity.class.body
) @entity.class

; 签名查询
(class_definition
  name: (identifier) @entity.class.signature.name
  superclasses: (argument_list)? @entity.class.signature.base
) @entity.class.signature
```

**函数**：
```tree-sitter
; 原始查询
(function_definition
  name: (identifier) @entity.function.name
  parameters: (parameters) @entity.function.params
  return_type: (type)? @entity.function.return_type
  body: (block) @entity.function.body
) @entity.function

; 签名查询
(function_definition
  name: (identifier) @entity.function.signature.name
  parameters: (parameters) @entity.function.signature.params
  return_type: (type)? @entity.function.signature.return_type
) @entity.function.signature
```

### 5.4 Java

**类**：
```tree-sitter
; 原始查询
(class_declaration
  name: (identifier) @entity.class.name
  body: (class_body) @entity.class.body
  superclass: (type_identifier)? @entity.class.base
) @entity.class

; 签名查询
(class_declaration
  name: (identifier) @entity.class.signature.name
  superclass: (type_identifier)? @entity.class.signature.base
) @entity.class.signature
```

**方法**：
```tree-sitter
; 原始查询
(method_declaration
  type: (_) @entity.method.return_type
  name: (identifier) @entity.method.name
  parameters: (formal_parameters) @entity.method.params
  body: (block) @entity.method.body
) @entity.method

; 签名查询
(method_declaration
  type: (_) @entity.method.signature.return_type
  name: (identifier) @entity.method.signature.name
  parameters: (formal_parameters) @entity.method.signature.params
) @entity.method.signature
```

### 5.5 TypeScript/JavaScript

**接口**：
```tree-sitter
; 原始查询
(interface_declaration
  name: (type_identifier) @entity.interface.name
  body: (interface_body) @entity.interface.body
) @entity.interface

; 签名查询
(interface_declaration
  name: (type_identifier) @entity.interface.signature.name
) @entity.interface.signature
```

**方法/函数**：
```tree-sitter
; 原始查询
(method_definition
  name: (property_identifier) @entity.method.name
  parameters: (formal_parameters) @entity.method.params
  body: (statement_block) @entity.method.body
) @entity.method

; 签名查询
(method_definition
  name: (property_identifier) @entity.method.signature.name
  parameters: (formal_parameters) @entity.method.signature.params
) @entity.method.signature

; 原始查询
(function_declaration
  name: (identifier) @entity.function.name
  parameters: (formal_parameters) @entity.function.params
  body: (statement_block) @entity.function.body
) @entity.function

; 签名查询
(function_declaration
  name: (identifier) @entity.function.signature.name
  parameters: (formal_parameters) @entity.function.signature.params
) @entity.function.signature
```

### 5.6 Kotlin

**类**：
```tree-sitter
; 原始查询
(class_declaration
  name: (identifier) @entity.class.name
  (class_body)? @entity.class.body
) @entity.class

; 签名查询
(class_declaration
  name: (identifier) @entity.class.signature.name
) @entity.class.signature
```

**函数/方法**：
```tree-sitter
; 原始查询
(function_declaration
  name: (identifier) @entity.function.name
  (function_value_parameters) @entity.function.params
  (function_body)? @entity.function.body
) @entity.function

; 签名查询
(function_declaration
  name: (identifier) @entity.function.signature.name
  (function_value_parameters) @entity.function.signature.params
) @entity.function.signature
```

### 5.7 Scala

**类/特质/对象**：
```tree-sitter
; 原始查询
(class_definition
  name: (identifier) @entity.class.name
  body: (template_body)? @entity.class.body
) @entity.class

; 签名查询
(class_definition
  name: (identifier) @entity.class.signature.name
) @entity.class.signature

; 原始查询
(trait_definition
  name: (identifier) @entity.trait.name
  body: (template_body)? @entity.trait.body
) @entity.trait

; 签名查询
(trait_definition
  name: (identifier) @entity.trait.signature.name
) @entity.trait.signature

; 原始查询
(object_definition
  name: (identifier) @entity.object.name
  body: (template_body)? @entity.object.body
) @entity.object

; 签名查询
(object_definition
  name: (identifier) @entity.object.signature.name
) @entity.object.signature
```

**函数**：
```tree-sitter
; 原始查询
(function_definition
  name: (identifier) @entity.function.name
  parameters: (parameters)? @entity.function.params
  body: (_)? @entity.function.body
) @entity.function

; 签名查询
(function_definition
  name: (identifier) @entity.function.signature.name
  parameters: (parameters)? @entity.function.signature.params
) @entity.function.signature
```

### 5.8 Ruby

**类/模块**：
```tree-sitter
; 原始查询
(class
  name: (constant) @entity.class.name
  superclass: (constant)? @entity.class.superclass
  body: (body_statement) @entity.class.body
) @entity.class

; 签名查询
(class
  name: (constant) @entity.class.signature.name
  superclass: (constant)? @entity.class.signature.superclass
) @entity.class.signature

; 原始查询
(module
  name: (constant) @entity.module.name
  body: (body_statement) @entity.module.body
) @entity.module

; 签名查询
(module
  name: (constant) @entity.module.signature.name
) @entity.module.signature
```

**方法**：
```tree-sitter
; 原始查询
(method
  name: (identifier) @entity.method.instance.name
  parameters: (method_parameters)? @entity.method.instance.params
  body: (body_statement)? @entity.method.instance.body
) @entity.method.instance

; 签名查询
(method
  name: (identifier) @entity.method.instance.signature.name
  parameters: (method_parameters)? @entity.method.instance.signature.params
) @entity.method.instance.signature
```

### 5.9 PHP

**类/接口/特质**：
```tree-sitter
; 原始查询
(class_declaration
  name: (name) @entity.class.name
  body: (declaration_list) @entity.class.body
) @entity.class

; 签名查询
(class_declaration
  name: (name) @entity.class.signature.name
) @entity.class.signature

; 原始查询
(interface_declaration
  name: (name) @entity.interface.name
  body: (declaration_list) @entity.interface.body
) @entity.interface

; 签名查询
(interface_declaration
  name: (name) @entity.interface.signature.name
) @entity.interface.signature

; 原始查询
(trait_declaration
  name: (name) @entity.trait.name
  body: (declaration_list) @entity.trait.body
) @entity.trait

; 签名查询
(trait_declaration
  name: (name) @entity.trait.signature.name
) @entity.trait.signature
```

**函数**：
```tree-sitter
; 原始查询
(function_definition
  name: (name) @entity.function.name
  parameters: (formal_parameters) @entity.function.params
  body: (compound_statement) @entity.function.body
) @entity.function

; 签名查询
(function_definition
  name: (name) @entity.function.signature.name
  parameters: (formal_parameters) @entity.function.signature.params
) @entity.function.signature
```

### 5.10 Dart

**类**：
```tree-sitter
; 原始查询
(class_declaration
  name: (identifier) @entity.class.name
  body: (class_body) @entity.class.body
) @entity.class

; 签名查询
(class_declaration
  name: (identifier) @entity.class.signature.name
) @entity.class.signature
```

**方法**：
```tree-sitter
; 原始查询
(method_signature
  (function_signature
    name: (identifier) @entity.method.name
  )
) @entity.method

; 签名查询
(method_signature
  (function_signature
    name: (identifier) @entity.method.signature.name
  )
) @entity.method.signature
```

### 5.11 C#

**类/接口/结构体**：
```tree-sitter
; 原始查询
(class_declaration
  name: (identifier) @entity.class.name
) @entity.class

; 签名查询
(class_declaration
  name: (identifier) @entity.class.signature.name
) @entity.class.signature

; 原始查询
(interface_declaration
  name: (identifier) @entity.interface.name
) @entity.interface

; 签名查询
(interface_declaration
  name: (identifier) @entity.interface.signature.name
) @entity.interface.signature

; 原始查询
(struct_declaration
  name: (identifier) @entity.struct.name
) @entity.struct

; 签名查询
(struct_declaration
  name: (identifier) @entity.struct.signature.name
) @entity.struct.signature
```

**方法**：
```tree-sitter
; 原始查询
(method_declaration
  name: (identifier) @entity.method.name
  parameters: (parameter_list) @entity.method.params
  body: (_) @entity.method.body
) @entity.method

; 签名查询
(method_declaration
  name: (identifier) @entity.method.signature.name
  parameters: (parameter_list) @entity.method.signature.params
) @entity.method.signature
```

### 5.12 Bash

**函数**：
```tree-sitter
; 原始查询
(function_definition
  name: (word) @entity.function.name
  body: (compound_statement) @entity.function.body
) @entity.function

; 签名查询
(function_definition
  name: (word) @entity.function.signature.name
) @entity.function.signature
```

### 5.13 Lua

**函数/方法**：
```tree-sitter
; 原始查询
(function_declaration
  name: [
    (identifier) @entity.function.name
    (dot_index_expression
      field: (identifier) @entity.function.name)
  ]
  parameters: (parameters)? @entity.function.params
  body: (block)? @entity.function.body
) @entity.function

; 签名查询
(function_declaration
  name: [
    (identifier) @entity.function.signature.name
    (dot_index_expression
      field: (identifier) @entity.function.signature.name)
  ]
  parameters: (parameters)? @entity.function.signature.params
) @entity.function.signature

; 原始查询
(function_declaration
  name: (method_index_expression
    method: (identifier) @entity.method.name)
  parameters: (parameters)? @entity.method.params
  body: (block)? @entity.method.body
) @entity.method

; 签名查询
(function_declaration
  name: (method_index_expression
    method: (identifier) @entity.method.signature.name)
  parameters: (parameters)? @entity.method.signature.params
) @entity.method.signature
```

### 5.14 Svelte/Vue/HTML

**元素**：
```tree-sitter
; 原始查询
(element
  (start_tag
    (tag_name) @entity.element.name
  ) @entity.element.start_tag
  (end_tag)? @entity.element.end_tag
) @entity.element

; 签名查询
(element
  (start_tag
    (tag_name) @entity.element.signature.name
  ) @entity.element.signature.start_tag
) @entity.element.signature
```

### 5.15 CSS

**样式规则**：
```tree-sitter
; 原始查询
(rule_set
  (selectors) @entity.style_rule.selectors
  (block) @entity.style_rule.block
) @entity.style_rule

; 签名查询
(rule_set
  (selectors) @entity.style_rule.signature.selectors
) @entity.style_rule.signature
```

## 6. 实施计划

### 6.1 阶段划分

| 阶段 | 内容 | 时间 | 依赖 |
|------|------|------|------|
| 1. Rust 改造 | 完成 Rust 语言的签名捕获改造 | 1 周 | 无 |
| 2. C/C++ 改造 | 完成 C/C++ 语言的签名捕获改造 | 1 周 | 无 |
| 3. Go/Python 改造 | 完成 Go 和 Python 语言的签名捕获改造 | 1 周 | 无 |
| 4. Java/TypeScript 改造 | 完成 Java 和 TypeScript 语言的签名捕获改造 | 1 周 | 无 |
| 5. 其他语言改造 | 完成 Kotlin/Scala/Ruby/PHP/Dart/C#/Bash/Lua 的改造 | 2 周 | 无 |
| 6. 前端语言改造 | 完成 Svelte/Vue/HTML/CSS 的改造 | 1 周 | 无 |
| 7. 集成测试 | 验证所有语言的签名提取正确性 | 1 周 | 阶段 1-6 |
| 8. 文档更新 | 更新相关文档和示例 | 0.5 周 | 阶段 7 |

**总时间**：8.5 周

### 6.2 详细任务

#### 阶段 1：Rust 改造（1 周）

1. **修改 Rust 查询**
   - 为结构体添加 `@entity.struct.signature` 捕获
   - 为 impl 块添加 `@entity.impl.signature` 捕获
   - 为函数添加 `@entity.function.signature` 捕获

2. **单元测试**
   - 验证签名捕获的存在
   - 验证签名提取的正确性

#### 阶段 2：C/C++ 改造（1 周）

1. **修改 C 查询**
   - 为结构体添加 `@entity.struct.signature` 捕获
   - 为函数添加 `@entity.function.signature` 捕获

2. **修改 C++ 查询**
   - 为类添加 `@entity.class.signature` 捕获
   - 为方法添加 `@entity.method.signature` 捕获

3. **单元测试**
   - 验证签名捕获的存在
   - 验证签名提取的正确性

#### 阶段 3：Go/Python 改造（1 周）

1. **修改 Go 查询**
   - 为结构体添加 `@entity.struct.signature` 捕获
   - 为接口添加 `@entity.interface.signature` 捕获
   - 为函数添加 `@entity.function.signature` 捕获

2. **修改 Python 查询**
   - 为类添加 `@entity.class.signature` 捕获
   - 为函数添加 `@entity.function.signature` 捕获

3. **单元测试**
   - 验证签名捕获的存在
   - 验证签名提取的正确性

#### 阶段 4：Java/TypeScript 改造（1 周）

1. **修改 Java 查询**
   - 为类添加 `@entity.class.signature` 捕获
   - 为方法添加 `@entity.method.signature` 捕获

2. **修改 TypeScript 查询**
   - 为接口添加 `@entity.interface.signature` 捕获
   - 为方法添加 `@entity.method.signature` 捕获
   - 为函数添加 `@entity.function.signature` 捕获

3. **单元测试**
   - 验证签名捕获的存在
   - 验证签名提取的正确性

#### 阶段 5：其他语言改造（2 周）

1. **修改 Kotlin 查询**
   - 为类添加 `@entity.class.signature` 捕获
   - 为函数添加 `@entity.function.signature` 捕获

2. **修改 Scala 查询**
   - 为类添加 `@entity.class.signature` 捕获
   - 为特质添加 `@entity.trait.signature` 捕获
   - 为对象添加 `@entity.object.signature` 捕获
   - 为函数添加 `@entity.function.signature` 捕获

3. **修改 Ruby 查询**
   - 为类添加 `@entity.class.signature` 捕获
   - 为模块添加 `@entity.module.signature` 捕获
   - 为方法添加 `@entity.method.instance.signature` 捕获

4. **修改 PHP 查询**
   - 为类添加 `@entity.class.signature` 捕获
   - 为接口添加 `@entity.interface.signature` 捕获
   - 为特质添加 `@entity.trait.signature` 捕获
   - 为函数添加 `@entity.function.signature` 捕获

5. **修改 Dart 查询**
   - 为类添加 `@entity.class.signature` 捕获
   - 为方法添加 `@entity.method.signature` 捕获

6. **修改 C# 查询**
   - 为类添加 `@entity.class.signature` 捕获
   - 为接口添加 `@entity.interface.signature` 捕获
   - 为结构体添加 `@entity.struct.signature` 捕获
   - 为方法添加 `@entity.method.signature` 捕获

7. **修改 Bash 查询**
   - 为函数添加 `@entity.function.signature` 捕获

8. **修改 Lua 查询**
   - 为函数添加 `@entity.function.signature` 捕获
   - 为方法添加 `@entity.method.signature` 捕获

9. **单元测试**
   - 验证所有语言的签名捕获存在
   - 验证所有语言的签名提取正确性

#### 阶段 6：前端语言改造（1 周）

1. **修改 Svelte 查询**
   - 为元素添加 `@entity.element.signature` 捕获

2. **修改 Vue 查询**
   - 为元素添加 `@entity.element.signature` 捕获

3. **修改 HTML 查询**
   - 为元素添加 `@entity.element.signature` 捕获

4. **修改 CSS 查询**
   - 为样式规则添加 `@entity.style_rule.signature` 捕获

5. **单元测试**
   - 验证所有前端语言的签名捕获存在
   - 验证所有前端语言的签名提取正确性

#### 阶段 7：集成测试（1 周）

1. **功能测试**
   - 验证所有语言的签名提取正确性
   - 验证与现有功能的一致性

2. **性能测试**
   - 对比新旧方案的解析时间
   - 验证内存使用情况

3. **回归测试**
   - 运行现有测试套件
   - 确保没有引入回归问题

#### 阶段 8：文档更新（0.5 周）

1. **更新 API 文档**
   - 为新捕获添加文档
   - 更新现有文档

2. **更新用户指南**
   - 添加多语言签名提取的说明
   - 添加迁移指南

## 7. 验证方法

### 7.1 功能验证

1. **签名提取正确性**
   - 验证所有语言的签名只包含签名部分，不包含代码体
   - 验证签名捕获的存在和正确性

2. **NL 转换正确性**
   - 验证自然语言转换结果正确
   - 验证与现有转换的一致性

3. **关系提取正确性**
   - 验证调用关系提取正确
   - 验证依赖关系提取正确

### 7.2 性能验证

1. **解析时间**
   - 对比新旧方案的解析时间
   - 确保性能下降不超过 10%

2. **内存使用**
   - 对比新旧方案的内存使用
   - 确保内存增加不超过 15%

### 7.3 兼容性验证

1. **向后兼容**
   - 验证现有代码不需要修改
   - 验证现有测试全部通过

2. **API 兼容**
   - 验证现有 API 不变
   - 验证新 API 与现有 API 一致

## 8. 风险与缓解

### 8.1 风险识别

| 风险 | 影响 | 概率 | 缓解措施 |
|------|------|------|----------|
| 性能下降 | 高 | 中 | 优化实现，添加缓存机制 |
| 向后兼容问题 | 高 | 低 | 充分测试，渐进式迁移 |
| 实现复杂度高 | 中 | 中 | 分阶段实施，逐步优化 |
| 测试覆盖不足 | 中 | 低 | 增加测试用例，提高覆盖率 |

### 8.2 缓解措施

1. **性能优化**
   - 使用缓存机制避免重复计算
   - 优化正则表达式和字符串处理
   - 添加性能监控和调优

2. **向后兼容**
   - 保持现有 API 不变
   - 添加兼容性层
   - 提供迁移指南

3. **复杂度管理**
   - 分阶段实施，逐步优化
   - 代码审查，确保质量
   - 文档化设计决策

## 9. 总结

本方案通过为所有语言添加签名特定的捕获，解决了签名包含大量代码片段的问题。主要改进包括：

1. **统一规范**：为所有语言定义统一的签名捕获命名规范
2. **职责分离**：实体捕获和签名提取使用不同的捕获逻辑
3. **全面覆盖**：覆盖所有主要编程语言和前端框架
4. **可扩展性**：支持未来添加更多语言

实施本方案后，签名将只包含签名部分，不包含代码实现，从而提高可读性和可维护性。同时，关系提取和 NL 转换的职责将更加清晰，便于独立优化和扩展。