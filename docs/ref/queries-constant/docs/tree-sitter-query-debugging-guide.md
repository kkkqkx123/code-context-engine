# Tree-sitter 查询调试指南

## 概述

本文档提供调试和修复 Tree-sitter 查询模式的详细指南，特别是处理 C++ 代码解析时的常见问题。

## 常见问题及解决方案

### 1. 语法错误（Bad syntax）

**问题表现：**
- `QueryError: Bad syntax at offset XXX`
- 括号不匹配
- 捕获名称格式错误

**解决方案：**
- 确保所有括号正确匹配 (最常见的问题)
- 捕获名称不能包含点号（`.`），应使用下划线（`_`）
- 检查谓词语法是否正确

### 2. 语法结构错误（Bad pattern structure）

**问题表现：**
- `QueryError: Bad pattern structure at offset XXX`
- 节点类型不存在或拼写错误

**解决方案：**
- 验证节点类型名称是否正确
- 使用实际的语法树结构进行匹配

### 3. 空匹配（No matches）

**问题表现：**
- 查询语法正确但没有匹配到任何内容

**解决方案：**
- 获取实际的语法树结构进行对比
- 调整查询模式以匹配实际的语法节点

## 调试步骤

### 步骤 1: 获取实际语法树

对目标代码运行语法分析以获取实际的语法树结构：

```
// 示例代码
std::lock_guard<std::mutex> lock(mtx);
```

对应语法树：
```
declaration [行, 列] - [行, 列]
type: qualified_identifier [行, 列] - [行, 列]
scope: namespace_identifier [行, 列] - [行, 列]
name: template_type [行, 列] - [行, 列]
name: type_identifier [行, 列] - [行, 列]
arguments: template_argument_list [行, 列] - [行, 列]
declarator: function_declarator [行, 列] - [行, 列]
declarator: identifier [行, 列] - [行, 列]
parameters: parameter_list [行, 列] - [行, 列]
parameter_declaration [行, 列] - [行, 列]
type: type_identifier [行, 列] - [行, 列]
```

### 步骤 2: 分析语法树结构

从语法树中提取关键信息：
- 顶层节点类型（如 `declaration`）
- 各字段的节点类型（如 `type:`, `scope:`, `name:`, `declarator:` 等）
- 捕获点的具体节点类型

### 步骤 3: 构建查询模式

根据语法树结构构建查询：

```scheme
; 基于语法树结构的查询
(declaration
  type: (qualified_identifier
    scope: (namespace_identifier) @std.scope
    name: (template_type
      name: (type_identifier) @target.type))
  declarator: (function_declarator
    declarator: (identifier) @target.variable
    parameters: (parameter_list
      (parameter_declaration
        (type_identifier) @parameter.name)))
  (#match? @target.type "^(target_type1|target_type2)$")) @capture_name
```

### 步骤 4: 处理多种语法变体

某些代码可能有多种语法表示，使用 alternation（`[]`）来处理：

```scheme
; 处理变量声明和函数声明两种情况
(declaration
  type: (qualified_identifier
    scope: (namespace_identifier) @std.scope
    name: (template_type
      name: (type_identifier) @lock_guard.type))
  declarator: [
    (init_declarator
      declarator: (identifier) @lock_guard.variable
      value: (call_expression
        function: (identifier) @locked.mutex)))
    (function_declarator
      declarator: (identifier) @lock_guard.variable
      parameters: (parameter_list
        (parameter_declaration
          (type_identifier) @locked.mutex)))
  ]
  (#match? @lock_guard.type "^(lock_guard|unique_lock|shared_lock)$")) @concurrency_relationship_lock_guard
```

## C++ 特定问题

### 1. 模板类型解析

C++ 模板类型的语法树结构：
- `qualified_identifier` 作为类型
- `scope` 通常是 `namespace_identifier`，而非 `identifier`
- `name` 通常是 `template_type`，内部包含 `type_identifier`

### 2. 函数声明 vs 变量声明

C++ 中的"最令人烦恼的解析"问题：
- `std::lock_guard<std::mutex> lock(mtx);` 可能被解析为函数声明
- 语法树显示为 `function_declarator` 而非 `init_declarator`
- 参数被解析为 `parameter_declaration` 而非 `call_expression`

### 3. 命名空间标识符

- 在当前 Tree-sitter 版本中，使用 `namespace_identifier`
- 在其他语言(例如C)中，可能使用 `identifier`

## 验证查询

### 1. 语法验证
使用 `QueryLoader.validateQuerySyntax()` 方法验证查询语法。

### 2. 功能测试
使用对应的 `.tsqnb` 测试文件验证查询是否能正确匹配目标代码。

### 3. 边界情况
测试查询在不同代码变体下的表现，确保鲁棒性。

## 最佳实践

1. **始终基于实际语法树构建查询** - 不要猜测节点类型
2. **使用捕获名称保持一致性** - 便于后续处理
3. **添加谓词验证** - 使用 `#match?`、`#eq?` 等谓词进行精确匹配
4. **考虑多种语法变体** - 使用 alternation 处理不同的代码形式
5. **文档化查询意图** - 添加注释说明查询的目的
6. **编写测试用例** - 为每个查询编写对应的测试文件

## 常用谓词

- `(#match? @capture "regex")` - 正则匹配
- `(#eq? @capture "string")` - 精确匹配
- `(#not-match? @capture "regex")` - 反向正则匹配
- `(#not-eq? @capture "string")` - 反向精确匹配

## 调试工具

使用 Tree-sitter CLI 工具查看语法树：
```
tree-sitter parse your_file.cpp
tree-sitter query --test your_query.scm your_file.cpp