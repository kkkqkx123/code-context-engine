# LSP 替代工具模块设计

## 模块结构

符号查找工具族位于 `crates/app/cce-orchestrator/src/tools/`，采用扁平布局：

```
crates/app/cce-orchestrator/src/tools/
├── symbol_lookup.rs          # 聚合 re-export（无独立逻辑）
├── symbol_lookup_types.rs    # 公共请求/响应/错误类型
├── find_references.rs        # 查找引用
├── get_symbols.rs            # 获取符号
├── goto_definition.rs        # 跳转定义
├── common.rs                 # 共享辅助（按文件取实体、读连接等）
├── compression.rs            # 语义压缩（单文件 + types.rs）
├── ast_diagnosis/            # AST 诊断（含 strategies/ 子目录）
├── keyword_search.rs         # 关键词搜索
└── file_fold.rs              # 文件折叠
```

`symbol_lookup.rs` 仅做 re-export：把三个工具与其配置（`FindReferencesTool`、`FindReferencesConfig`、`GetSymbolsTool`、`GotoDefinitionTool`）以及 `symbol_lookup_types.rs` 中的全部类型统一导出，供 `tools.rs` 作为单一入口使用。

## 核心数据源

当前实现的主数据源是 **LayeredSnapshotIndex**（`cce_codegraph::index`），即已发布的内存快照索引，提供实体查询、调用关系查询、按文件取实体、按实体取文件路径等能力（通过 `snapshot_query` 的 `SnapshotEntityQueryOps` / `SnapshotRelationQueryOps` / `SnapshotFileQueryOps` trait）。

早期设计中假设的 `RelationIndex`（DashMap 内存索引）、`ProjectSymbolTable`、`FileCache` 均已被放弃，不再出现在代码中。

SQLite（通过 `RelationStore` 包装 `cce_storage_metadb_sqlite::SqliteClient`）只作为 **chunk 正文读取**的后备：用于 snippet 提取（`find_references`）和定义函数体读取（`goto_definition`），**不参与引用解析**。chunk 行不再持久化原始代码，正文通过 `source_reader` 从源文件惰性读取（先 `resolve_project_root`，再 `read_source_lines`）。

其他辅助数据源：

- `QueryCache`（`cce_codegraph::query`）：`FindReferencesTool` 内部的 LRU 结果缓存（容量 128，`parking_lot::RwLock` 包裹），缓存 callee → callers 的映射。
- 两阶段 epoch 解析：SQLite 读取（`resolve_symbol_by_name`、`goto_definition` 的 body 读取）遵循"先查本代，miss 再回退父代 epoch"的规则，父代命中中被覆盖文件的结果会被剔除。

## 工具设计

### FindReferencesTool（查找引用）

- 构造：`new(index: Arc<LayeredSnapshotIndex>, project_id: i64)`，可链式 `.with_config(...)` 和 `.with_sqlite(...)`。`project_id` 绑定用于 SQLite 读取与查询过滤视图。
- `FindReferencesConfig { default_context_lines: 2, include_definition: false }`（默认值保留不变）。
- 查询流程（`find_references`）：
  1. `resolve_symbol`：按 path 取该文件实体，找到包含光标位置的实体；再查该实体的 resolved relations，若光标落在某个调用 span 上则返回被调用者（callee）的 EntityId，否则返回实体自身 ID——即正确区分**调用点 vs 定义点**。
  2. 先查 `QueryCache`；miss 则调用 `index.get_callers_by_callee_entity(target_id)` 并写回缓存。
  3. `get_reference_locations`：遍历每个 caller 的 resolved relations，筛选 `callee_id == target_id` 的关系，生成 `ReferenceLocation`（行列号 +1 转为 1-based）。
  4. `group_by_file`：按文件路径分组，输出 `GroupedReferences { path, count, references }`。
- 可选增强（由请求字段控制，默认关闭）：
  - `include_snippet`：从源文件按调用行 ± context_lines 提取代码片段；
  - `include_entity_info`：附带 caller 实体信息（`CallerEntityInfo { name, kind, entity_id }`）；
  - relation 中的 `callee_symbol` 快照可提供 `callee_file` / `callee_line` / `callee_end_line`。
- 另有公开的 `resolve_symbol_by_name`：先走快照名称索引（O(1)），miss 时回退 SQLite FTS5（`EntityRepository::search_fts_at_epoch`，含两阶段 epoch 解析）。

### GetSymbolsTool（获取符号）

- 仅持有 `index: Arc<LayeredSnapshotIndex>`，**无实时解析回退**：若索引中没有该文件的实体，直接返回失败的 `FileSymbolResult`（error 为 "No entities found in index"）。
- `get_symbols` 逐个文件调用 `get_symbols_for_file`：从索引取实体 → 按 `parent` 字段构建父子映射 → 从根实体递归构建 `SymbolInfo` 树（name、kind、1-based 行号、detail=signature、children）。
- `SymbolKind` 定义在 `symbol_lookup_types.rs`，通过 `impl From<EntityKind> for SymbolKind` 映射（如 `Trait`/`TraitImpl` → `Interface`、`EnumVariant` → `EnumMember`、`Macro` → `Function` 等）。

### GotoDefinitionTool（跳转定义）

- 构造与 `FindReferencesTool` 相同：`new(index, project_id)` + 可选 `.with_sqlite(...)`。
- `goto_definition` 流程：
  1. `resolve_symbol_at_position`：与 find_references 相同的调用点/定义点区分逻辑（行匹配 + 可选列精确匹配）；
  2. `find_definitions`：通过 `index.get_function_by_entity_id` + `get_file_path_by_entity` 生成 `DefinitionLocation`；
  3. `get_definition_code`：`include_body` 为真时从 SQLite chunk 读取完整函数体（两阶段 epoch 解析），读取失败或为假时回退到实体 `signature`。
- 响应为 `GotoDefinitionResponse { symbol, definitions: Vec<DefinitionCode> }`，`DefinitionCode` 含 location、name、kind、code、signature。

## 错误模型

`SymbolLookupError`（`symbol_lookup_types.rs`）覆盖：`SymbolNotFound`、`FileNotFound`、`FileNotReadable`、`EntityNotFound(EntityId)`、`NoSymbolAtPosition`、`ParseError`、`UnknownLanguage`、`IndexNotAvailable`、`Internal(String)`。

## 设计要点

1. **统一查询路径**：主查询全部走内存快照索引；SQLite 仅负责 snippet / chunk body 读取（本地分支端口，等待契约层补齐后收敛）。
2. **project_id 绑定**：工具实例在构造时绑定项目，避免每次请求传递。
3. **LRU 缓存**：findReferences 对 callers 结果做进程内缓存，重复查询无索引开销。
4. **调用点 vs 定义点**：resolve 逻辑统一区分光标位于调用表达式还是定义本身，两个工具共享同一语义。
