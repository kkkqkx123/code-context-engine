# Dart Tree-sitter 支持问题记录

## 概述

在为项目添加 Dart 语言的 tree-sitter 支持时，遇到了查询模式语法验证问题。

## 已完成的工作

1. **Language 枚举扩展** - 在 `src/types/language.rs` 中添加了 `Dart` 变体
2. **依赖添加** - 在 `Cargo.toml` 中添加了 `tree-sitter-dart = "0.1"` 依赖
3. **Tree-sitter 初始化** - 在 `src/utils/tree_sitter_init.rs` 中添加了 Dart 语言支持
4. **查询模式文件创建** - 创建了 `src/tree_sitter_query/scheme/dart.rs`
5. **模块注册** - 在 `src/tree_sitter_query/scheme/mod.rs` 中注册了 dart 模块
6. **查询加载** - 在 `src/tree_sitter_query/loader.rs` 中添加了 Dart 查询加载逻辑
7. **验证测试** - 在 `scheme/mod.rs` 中添加了 Dart 验证测试
8. **dependency_query 修复** - 修复了所有依赖查询模式的字段名问题

## 问题修复记录

### dependency_query 语法验证失败（已修复）

**原始错误信息：**
```
Query 'dependency_query' syntax error: QueryError { row: 50, column: 2, offset: 1086, message: "  uri: (uri\n  ^", kind: Structure }
```

**问题分析：**

通过 `tools/parse_dart.rs` 工具分析 AST 结构，发现了以下关键信息：

1. **import_specification** 和 **library_export** 的 AST 结构：
   - 父节点有 `uri` 字段，其值是 `configurable_uri`
   - `configurable_uri` 内部的 `uri` 节点**没有字段名**

2. **part_directive** 的 AST 结构：
   - 有 `uri` 字段，其值是 `uri` 节点

3. **part_of_directive** 的 AST 结构：
   - `uri` 节点**没有字段名**

**修复方案：**

根据 AST 结构分析，修正了查询模式：

1. **import_specification** 和 **library_export**：
   - 使用 `uri:` 字段名指定 `configurable_uri`
   - `configurable_uri` 内部的 `uri` 节点不使用字段名

2. **part_directive**：
   - 使用 `uri:` 字段名指定 `uri` 节点

3. **part_of_directive**：
   - `uri` 节点不使用字段名

**修复后的查询模式：**

```scheme
; Import directive
(import_specification
  uri: (configurable_uri
    (uri
      (string_literal) @dependency.import.path
    )
  )
) @dependency.import

; Export directive
(library_export
  uri: (configurable_uri
    (uri
      (string_literal) @dependency.export.path
    )
  )
) @dependency.export

; Part directive
(part_directive
  uri: (uri
    (string_literal) @dependency.part.path
  )
) @dependency.part

; Part of directive
(part_of_directive
  (uri
    (string_literal) @dependency.part_of.path
  )
) @dependency.part_of
```

## 测试结果

所有查询模式测试均已通过：

- `entity_query` - ✅ 通过
- `call_query` - ✅ 通过
- `comment_query` - ✅ 通过
- `dependency_query` - ✅ 通过

## 相关文件

- `src/tree_sitter_query/scheme/dart.rs` - Dart 查询模式定义
- `tools/parse_dart.rs` - AST 结构分析工具
- `src/types/language.rs` - 语言枚举定义
- `src/utils/tree_sitter_init.rs` - Tree-sitter 初始化
- `src/tree_sitter_query/loader.rs` - 查询加载器
