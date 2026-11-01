# Symbol Lookup 设计

## 概述

本项目用内部索引替代 LSP，提供 `findReferences`、`getSymbols`、`gotoDefinition` 三个代码导航工具。实现位于 `crates/app/cce-orchestrator/src/tools/`（`find_references.rs`、`get_symbols.rs`、`goto_definition.rs`，公共类型在 `symbol_lookup_types.rs`）。

## 数据源架构

- **主数据源**：`LayeredSnapshotIndex`（`cce_codegraph::index`）——已发布的内存快照索引，提供实体、调用关系（resolved relations）、按文件取实体、按实体取文件路径等查询。所有引用解析与符号查询都走快照，不做实时解析回退。
- **SQLite（RelationStore）**：仅用于代码正文读取——findReferences 的 snippet 提取和 gotoDefinition 的函数体读取。chunk 行不持久化原始代码，正文经 `source_reader` 从源文件惰性读取。
- **QueryCache（LRU）**：findReferences 内部缓存 callee → callers 映射（容量 128）。
- 两阶段 epoch 解析：SQLite 读取先查本代 epoch，miss 回退父代，父代命中中属于被覆盖文件的结果被剔除。

## find_references

**请求** `FindReferencesRequest`：`path`、`line`（1-based）、`column`（1-based，可选）、`symbol`（可选）、`context_lines`（可选）、`include_snippet`（可选）、`include_entity_info`（可选）。

**流程**：

1. 符号解析：找到包含光标位置的文件实体；若光标落在某条 resolved relation 的调用 span 上，取该 relation 的 callee 作为目标；否则目标是实体本身（调用点 vs 定义点区分）。
2. 反向引用：`index.get_callers_by_callee_entity(target_id)`（LRU 缓存优先）。
3. 位置展开：遍历各 caller 的 resolved relations，筛选 `callee_id == target_id`，生成 `ReferenceLocation`。
4. 按文件分组输出。

**响应** `FindReferencesResponse`：`symbol`、`total_count`、`file_count`、`references: Vec<GroupedReferences>`（`path` / `count` / `references`）。

**引用位置** `ReferenceLocation`：`path`、`line`/`column`/`end_line`/`end_column`（1-based），以及可选的 `snippet`、`caller_entity`（`CallerEntityInfo { name, kind, entity_id }`）和 callee 定义位置（`callee_file` / `callee_line` / `callee_end_line`，来自 relation 的 callee_symbol 快照）。

**配置** `FindReferencesConfig`：`default_context_lines = 2`、`include_definition = false`（未启用定义本身作为引用）。

## get_symbols

**请求** `GetSymbolsRequest`：`paths: Vec<String>`。

**流程**：逐文件从索引取实体，按 `parent` 字段构建父子映射，从根实体递归构建 `SymbolInfo` 树。索引无数据则该文件返回失败结果（无实时解析回退）。

**响应** `GetSymbolsResponse`：`results: Vec<FileSymbolResult>`（`path`、`success`、`symbol_count`、`symbols`、`error`）、`success_count`、`fail_count`。

**符号类型**：`SymbolKind` 定义在 `symbol_lookup_types.rs`（对齐 LSP SymbolKind 的字段集合，无显式数字编号），由 `From<EntityKind>` 映射：`Trait`/`TraitImpl` → `Interface`、`InherentImpl` → `Struct`、`TypeAlias` → `Class`、`EnumVariant` → `EnumMember`、`Annotation` → `Property`、`Macro` → `Function` 等，其余兜底为 `Variable`。

## goto_definition

**请求** `GotoDefinitionRequest`：`path`、`line`（1-based）、`column`（1-based，可选）、`symbol`（可选）、`include_body: bool`。

**流程**：

1. `resolve_symbol_at_position`：与 find_references 一致的调用点/定义点区分（行匹配 + 可选列精确匹配调用 span）。
2. 定义查找：`get_function_by_entity_id` + `get_file_path_by_entity` 生成 `DefinitionLocation`。
3. 代码获取：`include_body` 为真时从 SQLite chunk 读完整函数体（两阶段 epoch 解析），失败或为假时回退实体 `signature`。

**响应** `GotoDefinitionResponse`：`symbol`、`definitions: Vec<DefinitionCode>`；`DefinitionCode { location, name, kind, code, signature }`。

## 通用约定

- **行列号 1-based**：所有对外输出的行列号均为内部 0-based tree-sitter 坐标 +1。
- **按文件分组**：引用结果按 `path` 分组，附带每文件计数。
- **括号匹配 `find_block_end`**：当实体 span 跨度不足以覆盖完整代码块时，可通过大括号计数扩展结束行（上限 500 行，防止失控）。
- **错误类型**：`SymbolLookupError`（SymbolNotFound / FileNotFound / FileNotReadable / EntityNotFound / NoSymbolAtPosition / ParseError / UnknownLanguage / IndexNotAvailable / Internal）。

## 功能映射

| LSP 功能 | 工具 API | 数据源 |
|---------|---------|--------|
| find_references | `FindReferencesTool::find_references` | LayeredSnapshotIndex（反向 callers + resolved relations）；SQLite 仅 snippet |
| get_symbols | `GetSymbolsTool::get_symbols` | LayeredSnapshotIndex（按文件实体 + parent 树） |
| goto_definition | `GotoDefinitionTool::goto_definition` | LayeredSnapshotIndex（定位）；SQLite chunk（函数体） |
| 按名称解析（辅助） | `FindReferencesTool::resolve_symbol_by_name` | 快照名称索引优先，回退 SQLite FTS5（两阶段 epoch） |

## 未实现 / 未来方向

以下能力当前**未实现**，仅为可能的方向：

- 实时解析回退：索引无数据时现场解析文件并缓存结果（getSymbols 现在直接返回失败）。
- 符号模糊匹配与正则表达式搜索。
- 基于 ImportTable 的跨文件导入符号解析。
- 接口实现 / 继承关系的多定义点返回（当前 `find_definitions` 仅返回单一位置）。
