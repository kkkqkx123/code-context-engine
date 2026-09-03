# Entity 系统总结

## 核心概念

- **Entity**：跨语言统一的语义抽象，不是 AST 节点包装器
  - Python `def` / Rust `fn` / Java `method` → 统一为 `Function`
  - Python `class` / Rust `struct` / Java `class` → 统一为 `Class`/`Struct`
  - HTML `<div>` / Vue `<template>` → 统一为 `Element`
- **EntityId**：文件局部的递增 ID (`u64`)
- **EntityKind**：跨语言统一的实体类型枚举 (50+ 种)
- **GroupedEntity**：扁平化实体表示（分组后使用，无 parent/children）

## 关键特性

### 1. 跨语言统一

所有支持的 18+ 语言都映射到相同的 `EntityKind` 枚举，确保查询和展示时语言无关。

### 2. 信息完整性

一次提取包含所有下游需要的信息：签名、参数列表、返回类型、文档注释、修饰符、属性、元数据、标准库标记等。

### 3. 自包含性

Entity 不依赖原始 AST 或 tree-sitter 树，提取后独立存在，便于序列化和跨进程传输。

### 4. 内存优化

- `GroupedEntity` 使用 `CompactString` 和 `SmallVec` 减少堆分配
- `ParsedFile.source` 使用 `Arc<str>` 支持共享
- 序列化使用 rkyv 零拷贝反序列化

## 核心数据结构

### Entity 定义

**位置**：`crates/cce_core/src/types/entity/full.rs`

```rust
pub struct Entity {
    pub id: EntityId,                                  // 文件局部 ID
    pub kind: EntityKind,                              // 跨语言统一类型
    pub name: String,                                  // 实体名称
    pub signature: String,                             // 签名 (从 AST 提取)
    pub parameters: Vec<(String, Option<String>)>,     // 参数列表 [(name, type)]
    pub return_type: Option<String>,                   // 返回类型
    pub span: Span,                                    // 源代码位置
    pub depth: usize,                                  // 语义嵌套深度
    pub parent: Option<EntityId>,                      // 语义父实体
    pub children: Vec<EntityId>,                       // 语义子实体
    pub doc_comment: Option<String>,                   // 文档注释
    pub modifiers: Vec<String>,                        // 修饰符 (pub, static, async)
    pub attributes: HashMap<String, String>,           // 元素属性 (HTML/Vue)
    pub metadata: HashMap<String, String>,             // 扩展元数据
    pub is_stdlib: bool,                               // 标准库标记
    pub stdlib_category: Option<StdlibCategory>,       // 标准库分类
}
```

### GroupedEntity 定义

**位置**：`crates/cce_core/src/types/entity/grouped.rs`

```rust
pub struct GroupedEntity {
    pub id: EntityId,
    pub name: String,
    pub kind: EntityKind,
    pub signature: String,
    pub parameters: SmallVec<[(CompactString, Option<CompactString>); 4]>,
    pub return_type: Option<String>,
    pub doc_comment: Option<String>,
    pub is_stdlib: bool,
    pub stdlib_category: Option<StdlibCategory>,
    pub metadata: HashMap<String, String>,
    // 与 Entity 的区别：无 parent, children, span, depth, modifiers, attributes
}
```

### EntityKind 枚举

**位置**：`crates/cce_core/src/types/entity/kind.rs`

主要分类：

| 分类 | EntityKind 值 |
|------|---------------|
| 类型定义 | `Class`, `Struct`, `Enum`, `Interface`, `Trait`, `TypeAlias`, `Union` |
| 函数方法 | `Function`, `Method`, `Constructor`, `Destructor`, `Operator` |
| 变量 | `Field`, `Property`, `Parameter`, `Variable`, `Constant` |
| 模块 | `Module`, `Namespace`, `Package` |
| 模板 | `Element`, `Attribute`, `Expression`, `Component`, `Template`, `Directive`, `ControlFlow`, `Binding`, `Action`, `EventHandler` |
| 样式 | `StyleRule`, `StyleSelector`, `StyleProperty`, `Keyframe`, `AtRule`, `Animation` |
| SFC | `ScriptContent`, `StyleContent`, `EmbeddedBlock` |
| 测试 | `TestSuite`, `TestCase`, `TestHook`, `Assertion`, `Mock` |

### ParsedFile 结构

**位置**：`crates/cce_core/src/types/entity/file.rs`

```rust
pub struct ParsedFile {
    pub language: Language,
    pub path: String,
    pub source: Arc<str>,
    pub entities: Vec<Entity>,
    pub local_symbols: HashMap<String, Vec<EntityId>>,
    pub raw_relations: Vec<RawRelationData>,
    pub import_table: Option<ImportTable>,
    pub embedded_blocks: Vec<EmbeddedBlock>,       // Vue/Svelte SFC
    pub block_relations: Vec<BlockRelation>,         // 跨块关系
    pub file_doc_comment: Option<String>,            // 文件级文档
}
```

**设计决策**：import_table 将导入信息缓存在 ParsedFile 中，避免在 IndexBuilder 中重新解析 AST 提取 imports/exports/dependencies；IndexBuilder 直接从 import_table 获取标准化导入数据。

## 处理流程

### 1. 核心处理流水线

```
源文件
  │
  ├─ 代码文件 (Source/Header) ──  AST 解析 → 实体提取 → 分组 → NL 转换 → 分块
  │
  ├─ 文档文件 (Documentation) ──  Markdown/HTML/JSON 流水线 → 分块
  │
  └─ 文本文件 (Config/Text)   ──  纯文本分割 → 分块
```

### 2. 实体分组流程

```rust
// cce_orchestrator 中的 pre_processor (NestEntityProcessor)
// 输入: ParsedFile.entities (Vec<Entity>)
// 输出: ProcessingResult { groups: Vec<EntityGroup>, stats: GroupStats }

fn process(&self, parsed: &ParsedFile) -> ProcessingResult {
    // 1. 按深度排序实体
    // 2. 识别嵌套关系 (类→方法, 结构体→字段)
    // 3. 分组：header = 父实体, members = 子实体
    // 4. 标记工具函数和样板代码
    // 5. 统计: input_entities, output_groups, class_method_associations, utility_functions
}
```

### 3. 实体存储流程

```rust
// cce_orchestrator/src/index/storage_coordinator.rs
async fn store_vectors_batched(&self, chunks: &[ChunkedResult], ...) -> Result<usize> {
    // 1. 构建 StorageData（含 embedding 文本）
    // 2. 调用 EmbeddingProvider 批量生成向量
    // 3. 写入 Qdrant (upsert points with payload)
    // 4. 写入 SQLite chunk_records
    // 5. 写入 SQLite entity_detail_mappings
}
```

## 三存储系统关联

```
                         ┌──────────────┐
                         │   SQLite     │
                         │  (元数据)    │
                         ├──────────────┤
                         │ EntityRecord │◄── FTS5 全文搜索
                         │ FileRecord   │
                         │ ChunkRecord  │
                         │ RelationRecord│
                         │ Mapping*     │
                         └──────┬───────┘
                                │ file_path / entity_id
           ┌────────────────────┼────────────────────┐
           ▼                    ▼                    ▼
   ┌──────────────┐    ┌──────────────┐    ┌──────────────┐
   │   Qdrant     │    │    BM25      │    │  Relation    │
   │  (向量)      │    │  (全文搜索)  │    │  (关系索引)  │
   ├──────────────┤    ├──────────────┤    ├──────────────┤
   │ vector +     │    │ title +      │    │ caller →     │
   │ payload      │    │ content +    │    │ callee       │
   │ (entity_id,  │    │ keywords     │    │ 索引         │
   │  file_path)  │    │              │    │              │
   └──────────────┘    └──────────────┘    └──────────────┘
```

## 查询流程

### 混合查询执行策略

```rust
// cce_orchestrator/src/query/searcher.rs
pub async fn search(&self, options: &QueryOptions) -> Result<QueryResult> {
    let strategy = options.execution_strategy();
    let mut results = self.execute_search_flow(options, &strategy).await?;

    // 可选组装 (SPSR-Graph)
    if let WithAssembly { depth, strategy, .. } = &strategy {
        results = self.assembly_handler.assemble_results(results, depth, strategy).await?;
    }
    Ok(results)
}

async fn execute_search_flow(&self, options: &QueryOptions, strategy: &ExecutionStrategy) -> Result<Vec<SearchResult>> {
    // 1. 检索: 向量 / BM25 / 混合
    // 2. 融合: MinMax 归一化 + 加权融合
    // 3. 增强: RelationBoost + SummaryBoost
    // 4. 排序: ScoreSorter + ThresholdFilter + GlobFilter
    // 5. 可选: LLM Reranker
}
```

## 性能优化

### 1. 批量处理

- 扫描批次: 100 文件/批
- Embedding 批次: 按 token 数动态调整
- 数据库事务: 批次级批量提交

### 2. 内存优化

- `Arc<str>` 共享源代码
- `CompactString` 减少短字符串堆分配
- `SmallVec<[T; 4]>` 小向量栈分配
- LRU Cache: 避免重复处理

### 3. 并发控制

- 文件处理: tokio::spawn 并发
- 索引访问: DashMap 无锁读写
- 存储写入: 独立连接池

## 扩展性设计

### 1. 新语言支持

在 `cce_parser/src/parser/extractor/symbol_extractor/` 下添加语言目录，实现查询规则和提取器。

### 2. 新 EntityKind

修改 `cce_core/src/types/entity/kind.rs`，在 `EntityKind` 枚举中添加新变体，同步更新所有 match。

### 3. 新存储后端

实现 `StorageCoordinator` 的 with_* 方法，通过 `IndexOrchestrator` 注册。

### 4. 插件系统 (Lua)

通过 PluginRegistry 注册自定义处理逻辑，支持框架检测和扩展实体处理。

## 相关源文件

| 文件 | 作用 |
|------|------|
| `cce_core/src/types/entity/mod.rs` | Entity 模块入口 |
| `cce_core/src/types/entity/full.rs` | Entity 结构体定义 |
| `cce_core/src/types/entity/grouped.rs` | GroupedEntity 定义 |
| `cce_core/src/types/entity/file.rs` | ParsedFile, RawRelationData |
| `cce_core/src/types/entity/kind.rs` | EntityKind 枚举 |
| `cce_core/src/types/entity/id.rs` | EntityId |
| `cce_core/src/types/entity/embedded_block.rs` | EmbeddedBlock, BlockRelation |
| `cce_parser/src/parser/extractor/entity_extractor.rs` | 实体提取器 |
| `cce_orchestrator/src/index/file_processor.rs` | 文件处理管道 |
| `cce_orchestrator/src/index/storage_coordinator.rs` | 存储协调器 |
| `cce_orchestrator/src/query/searcher.rs` | 查询搜索器 |
