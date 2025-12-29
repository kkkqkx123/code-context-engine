# C语言关系查询模式分析报告

## 概述
本报告分析了 `src/service/parser/constants/queries/c/relationships` 目录中的查询模式，识别出哪些模式不是真正的关系（两个AST节点之间的关系）。

## 非关系查询模式

### reference.ts 文件

以下模式只是标记单个节点，而不是建立两个AST节点之间的关系：

1. **变量引用模式**
   ```tree-sitter
   (identifier) @reference.variable
   (#not-match? @reference.variable "^(if|for|while|do|switch|break|continue|return|goto|sizeof|typeof|alignof)$")
   ```
   - 该模式仅标记标识符节点，而不是建立标识符与其他节点之间的关系

2. **类型引用模式**
   ```tree-sitter
   (type_identifier) @reference.type
   ```
   - 该模式仅标记类型标识符节点，而不是建立类型与其他节点之间的关系

3. **枚举常量引用模式**
   ```tree-sitter
   (identifier) @reference.enum.constant
   (#match? @reference.enum.constant "^[A-Z][A-Z0-9_]*$")
   ```
   - 该模式仅标记枚举常量，而不是建立枚举常量与其他节点之间的关系

4. **宏引用模式**
   ```tree-sitter
   (identifier) @reference.macro
   (#match? @reference.macro "^[A-Z_][A-Z0-9_]*$")
   ```
   - 该模式仅标记宏标识符，而不是建立宏与其他节点之间的关系

5. **全局变量引用模式**
   ```tree-sitter
   (identifier) @reference.global.variable
   (#match? @reference.global.variable "^[gG][a-zA-Z0-9_]*$")
   ```
   - 该模式仅标记全局变量，而不是建立全局变量与其他节点之间的关系

6. **静态变量引用模式**
   ```tree-sitter
   (identifier) @reference.static.variable
   (#match? @reference.static.variable "^[sS][a-zA-Z0-9_]*$")
   ```
   - 该模式仅标记静态变量，而不是建立静态变量与其他节点之间的关系

### annotation.ts 文件

以下模式也仅标记单个节点：

1. **类型注解模式**
   ```tree-sitter
   (type_definition
     (attribute
       name: (identifier) @annotation.name
       arguments: (argument_list
         (_) @annotation.argument)?)) @annotation.relationship.type
   ```
   - 该模式虽然涉及多个节点，但主要是描述一个type_definition节点内部的结构，而非两个独立AST节点间的关系

### semantic.ts 文件

以下模式仅标记单个节点或描述节点内部结构：

1. **指针关系模式**
   ```tree-sitter
   (type_identifier) @pointed.type
   ```
   - 该模式仅标记类型标识符节点

## 结论

真正的关系查询模式应该识别和连接两个或多个不同的AST节点，表示它们之间的语义或结构关系。而上述列出的模式仅用于标记单个节点或描述节点内部的结构，不符合关系查询的定义。