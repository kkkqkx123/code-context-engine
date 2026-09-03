# Entity 数据流图

## 1. 整体架构图

```
源代码文件
    │
    ▼
┌─────────────────────────────────────────────────────────────────────┐
│                      FileScanner                                     │
│  - WalkDir 遍历项目目录                                               │
│  - 支持 .gitignore 规则                                               │
│  - 按文件扩展名/排除目录过滤                                          │
│  - 返回 FileEntry（含路径、LanguageInfo）                              │
└─────────────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    ParseCoordinator (cce_parser)                      │
│  ParsePipeline:                                                      │
│    Stage 1: LanguageDetectionStage  — 语言检测                        │
│    Stage 2: AstParsingStage         — tree-sitter 解析 AST            │
│    Stage 3: ExtractionStage         — 实体+关系提取                   │
│    Stage 4: PostProcessingStage     — 后处理+合并结果                 │
└─────────────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     ParsedFile (cce_core::types::entity::file)        │
│  ├── language: Language                                              │
│  ├── path: String                                                    │
│  ├── source: Arc<str>                                                │
│  ├── entities: Vec<Entity>                                           │
│  ├── local_symbols: HashMap<String, Vec<EntityId>>                   │
│  ├── raw_relations: Vec<RawRelationData>                             │
│  ├── embedded_blocks: Vec<EmbeddedBlock> (Vue/Svelte SFC)           │
│  ├── block_relations: Vec<BlockRelation>                             │
│  └── file_doc_comment: Option<String>                                │
└─────────────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    FileProcessor (cce_orchestrator)                    │
│  ├── 路由: 代码文件 / 文档文件 / 文本文件                             │
│  ├── PreProcessor: 实体分组 (NestEntityProcessor)                    │
│  ├── AstToNlConverter: 实体组 → NL 文本                              │
│  ├── Chunker: NL 文本 → ChunkedResult 分块                            │
│  └── 增强: 关联 entity_kind 元数据                                   │
└─────────────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────────────┐
│                  StorageCoordinator (存储协调)                         │
│  ├── Qdrant (向量存储) — embedding → 向量点                           │
│  ├── BM25 (全文搜索) — Tantivy 索引                                   │
│  ├── SQLite (元数据) — entities, files, relations, chunks            │
│  └── FileSummary (文件摘要) — LLM 生成                                │
└─────────────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────────────┐
│                   IndexBuilder (关系索引)                              │
│  ├── 解析 imports/exports/dependencies                               │
│  ├── 标准化导入/导出信息                                              │
│  ├── 构建函数调用索引                                                 │
│  ├── 构建继承/实现等结构关系                                          │
│  └── 输出 RelationIndex (DashMap 线程安全)                            │
└─────────────────────────────────────────────────────────────────────┘
```

## 2. 详细数据流

### 2.1 解析阶段 (Parser)

```
源代码文件
    │
    ▼
LanguageDetector (cce_parser::parser::language_detector)
  - 根据文件路径检测编程语言
  - 支持 18+ 种语言 (Rust/Python/JS/TS/Java/Go/C++/C#/Ruby 等)
  - 返回 LanguageInfo { language, file_type, extensions }
    │
    ▼
AstParser (cce_parser::parser::ast_parser)
  - 使用 tree-sitter 解析源代码
  - 生成树状 AST (Tree 对象)
  - 检测语法错误
    │
    ▼
EntityExtractor (cce_parser::parser::extractor::entity_extractor)
  - 从 AST 中提取语义实体
  - 执行 tree-sitter 查询匹配
  - 处理上下文信息
  - 生成 Vec<Entity>
  - 构建本地符号表 (local_symbols)
    │
    ▼
RelationExtractor (cce_parser::parser::extractor::relation_extractor)
  - 从 AST 中提取实体间关系
  - 识别调用、继承、引用等关系类型
  - 生成 Vec<Relation>
    │
    ▼
PostProcessingStage
  - 合并主实体与内嵌块实体
  - 转换为 ParsedFile 结构
  - 解析跨块关系 (SFC 文件)
```

### 2.2 预处理阶段 (PreProcessor)

```
ParsedFile
    │
    ▼
NestEntityProcessor (cce_orchestrator 中的 pre_processor)
  - 根据配置对实体进行分组
  - 处理嵌套关系 (类包含方法)
  - 识别样板代码和设计模式
  - 标记工具函数
  - 返回 ProcessingResult { groups, stats }
    │
    ▼
EntityGroup 列表
  ├── group_id: String
  ├── group_type: GroupType
  ├── header: Option<GroupedEntity>       (主实体, 如类头)
  ├── members: Vec<GroupedEntity>          (成员实体, 如方法)
  ├── kind: EntityKind
  ├── span: Span
  └── source: Arc<str>
```

### 2.3 转换阶段 (AstToNlConverter)

```
EntityGroup 列表
    │
    ▼
AstToNlConverter (cce_parser::ast_to_nl)
  - 核心路径: 实体 → NL 文本 (BM25 专用)
  - 内联路径: 实体 → NL 文本 (Embedding 专用)
  - 使用模板系统生成自然语言描述
    │
    ├── BM25 路径 ────────────────── 生成关键词丰富的摘要文本
    │     ├── keyword_extractor (提取关键词)
    │     ├── mixed_tokenizer (混合分词)
    │     └── text_cleaner (文本清洗)
    │
    └── Embedding 路径 ────────────── 生成语义浓缩的描述文本
          ├── 标准库模板 (stdlib)
          ├── 设计模式模板 (design_patterns)
          ├── 样板代码模板 (boilerplate_patterns)
          └── 类型注解清洗 (type_annotation_cleaner)
    │
    ▼
ConversionResult
  ├── bm25_text: String
  ├── embedding_text: String
  ├── entity_names: Vec<String>
  └── entity_kinds: Vec<String>
```

### 2.4 分块阶段 (Chunker)

```
ConversionResult 列表
    │
    ▼
Chunker (cce_parser::ast_to_nl::chunker)
  ├── boundary.rs — 边界检测 (函数/类/结构体边界)
  ├── splitter.rs — 分割策略 (成员级/句子级/段落级/行级)
  ├── overlap.rs — 重叠控制
  ├── tracker.rs — 分块跟踪
  └── config.rs — 分块配置 (max_chunk_size, overlap_size)
    │
    ▼
ChunkedResult
  ├── chunk_id: String
  ├── file_path: String
  ├── content: String (NL 文本)
  ├── source_group_id: String
  ├── metadata: ChunkMetadata
  │     └── CodeMetadata { entity_kind, entity_names, ... }
  └── token_count: Option<usize>
```

### 2.5 存储阶段 (Storage)

#### 2.5.1 SQLite 元数据存储

```
ChunkedResult + ParsedFile
    │
    ▼
SqliteClient (cce_infrastructure::storage::sqlite)
  ├── FileRepository — 存储文件元数据
  ├── EntityRepository — 存储实体记录 (含 FTS5 全文搜索)
  ├── ChunkRepository — 存储分块记录
  ├── RelationRepository — 存储关系记录
  ├── EntityDetailMappingRepository — 实体↔分块映射
  ├── FileSummaryMappingRepository — 文件↔摘要映射
  └── CacheRepository — 缓存管理
```

#### 2.5.2 BM25 全文搜索存储

```
ChunkedResult
    │
    ▼
Bm25Client (cce_infrastructure::storage::bm25)
  ├── 使用 Tantivy (定制分支) 建立倒排索引
  ├── Schema: title, content, keywords, file_path, chunk_id
  ├── 支持中文分词 (jieba-rs)
  └── 权重配置: title=4.0, keywords=2.0, content=1.0
```

#### 2.5.3 Qdrant 向量存储

```
ChunkedResult
    │
    ▼
OpenAICompatibleProvider (cce_infrastructure::llm::services::embedding)
  ├── 对 ChunkedResult.content 生成 embedding 向量
  ├── 支持多种 embedding 模型 (通过 preprocessor 处理前缀)
  └── 批量处理 + 速率限制
    │
    ▼
QdrantClient (cce_infrastructure::storage::qdrant)
  ├── 存储向量点 (含 payload: file_path, chunk_id, entity_kind 等)
  ├── 支持过滤搜索 (目录前缀、文件类型、语言)
  └── 支持定时自动升级
```

### 2.6 查询阶段 (Query)

```
查询请求 (QueryOptions)
    │
    ▼
QueryCoordinator (cce_orchestrator::query::coordinator)
  ├── 构建查询选项
  ├── 检查索引能力 (IndexCapabilities)
  └── 路由到 Searcher
    │
    ▼
Searcher (cce_orchestrator::query::searcher)
  ├── 确定执行策略 (ExecutionStrategy)
  │     ├── DenseRecall — 纯向量检索
  │     ├── Bm25Recall — 纯 BM25 检索
  │     └── HybridRecall — 混合检索 (向量 + BM25)
  │
  ├── 检索阶段:
  │     ├── VectorRetrieval — Qdrant 向量搜索
  │     └── Bm25Retrieval — Tantivy BM25 搜索
  │
  ├── 融合阶段:
  │     └── HybridFusion — 加权归一化融合
  │           ├── MinMax 归一化
  │           └── 向量权重 + BM25 权重
  │
  ├── 增强阶段:
  │     ├── RelationBoost — 调用关系权重提升
  │     └── SummaryBoost — 摘要相关性提升
  │
  ├── 排序阶段:
  │     ├── ScoreSorter — 得分排序
  │     ├── ThresholdFilter — 阈值过滤
  │     └── GlobFilter — 通配符过滤
  │
  ├── 重排序阶段 (可选):
  │     └── LlmReranker — LLM 重排序
  │
  └── 组装阶段 (可选):
        └── SPSRGraphAssembler — 调用链组装
              ├── 前向传播 (caller → callee)
              ├── 后向传播 (callee → caller)
              └── 路径发现
```

## 3. 数据格式转换

### 3.1 Entity → GroupedEntity

```rust
// file: cce_core/src/types/entity/grouped.rs
pub fn from_entity(entity: &Entity) -> GroupedEntity {
    // 丢弃: parent, children, span, depth, modifiers, attributes
    // 保留: id, name, kind, signature, parameters, return_type,
    //       doc_comment, is_stdlib, stdlib_category, metadata
    // 优化: parameters → SmallVec<[(CompactString, Option<CompactString>); 4]>
}
```

### 3.2 EntityGroup → ConversionResult

```rust
// file: cce_parser/src/ast_to_nl/*/generator.rs
// BM25 路径: 生成关键词丰富的摘要
// Embedding 路径: 生成语义浓缩的描述
pub fn convert(&self, group: &EntityGroup) -> ConversionResult {
    let bm25_text = self.generate_bm25_text(group);      // 关键词丰富
    let embedding_text = self.generate_embedding_text(group); // 语义浓缩
    ConversionResult { bm25_text, embedding_text, ... }
}
```

### 3.3 ConversionResult → ChunkedResult

```rust
// file: cce_parser/src/ast_to_nl/chunker/chunker.rs
pub fn chunk_groups(&mut self, conversions: &[ConversionResult], path: &str) -> Vec<ChunkedResult> {
    // 将转换结果分割成 token 限制内的块
    // 维护语义边界 (函数/类/段落)
    // 添加重叠以确保上下文连续性
}
```

## 4. 存储后端的可选性

项目支持灵活的存储后端组合，通过 `IndexOptions` 控制：

```rust
// file: cce_orchestrator/src/index/options.rs
pub struct IndexOptions {
    pub store_vectors: bool,   // Qdrant 向量存储
    pub store_bm25: bool,      // Tantivy BM25 全文搜索
    pub store_summaries: bool, // 文件摘要存储
    pub build_relations: bool, // 关系索引构建
}
```

- 可以只启用部分后端
- 查询时会根据可用后端自动调整检索策略
- 存储协调器 (StorageCoordinator) 负责处理可选后端的差异

## 5. 热更新数据流

```
文件系统事件 (notify)
    │
    ▼
HotUpdateCoordinator (cce_orchestrator::hot_update)
  ├── 模式: FileWatch / PeriodicScan / Manual
  ├── 去抖: 防抖窗口合并快速连续事件
  ├── 风暴检测: 高事件率时切换到周期性扫描
    │
    ▼
ChangeDetector → 计算文件变更 (新增/修改/删除)
    │
    ▼
ChangeProcessors:
  ├── ContextProcessor — 重新解析+分组+转换+分块
  ├── EmbeddingProcessor — 更新 Qdrant 向量
  ├── Bm25Processor — 更新 Tantivy BM25
  ├── RelationProcessor — 增量更新关系索引
  └── SummaryProcessor — 重新生成文件摘要
    │
    ▼
存储更新:
  ├── Qdrant: upsert/delete 向量点
  ├── BM25: upsert/delete 文档
  ├── SQLite: 更新元数据
  └── RelationIndex: 增量更新
```

## 6. 错误处理流程

### 6.1 解析错误

```
ParseError
  ├── LanguageDetection — 无法检测语言
  ├── AstParsing — tree-sitter 解析失败
  ├── EntityExtraction — 实体提取失败 (SQLite 文件跳过)
  ├── RelationExtraction — 关系提取失败
  └── EncodingDetection — 编码检测/转换失败
```

### 6.2 存储错误

```
StorageError
  ├── Qdrant:
  │     ├── Connection — 连接失败 (自动重试)
  │     ├── Upsert — 向量写入失败 (批量回退)
  │     └── Search — 搜索超时 (降级到 BM25)
  ├── BM25:
  │     ├── IndexWrite — 索引写入失败
  │     └── Search — 搜索失败
  └── SQLite:
        ├── Connection — 数据库连接失败
        ├── Transaction — 事务冲突 (重试)
        └── ConstraintViolation — 约束违反
```

### 6.3 查询错误

```
QueryError
  ├── Config — 数据库未配置
  ├── InvalidQuery — 查询语法错误
  ├── Retrival — 检索后端失败 (自动降级)
  └── Rerank — 重排服务不可用 (跳过重排)
```

## 7. 性能监控点

### 7.1 关键指标

| 指标 | 位置 | 说明 |
|------|------|------|
| `parse_time` | ParseCoordinator | 单文件解析耗时 |
| `extract_entity_count` | EntityExtractor | 每个文件的实体数 |
| `group_count` | NestEntityProcessor | 分组后的组数 |
| `chunk_count` | Chunker | 分块数 |
| `embedding_latency` | EmbeddingHandler | Embedding API 延迟 |
| `storage_write_latency` | StorageCoordinator | 存储写入延迟 |
| `search_latency` | Searcher | 搜索整体延迟 |
| `fusion_entity_count` | HybridFusion | 融合后的唯一实体数 |

### 7.2 监控位置

```rust
// cce_orchestrator/src/metrics.rs — IndexMetrics
// cce_infrastructure/src/metrics — StorageMetrics, QueryMetrics
// cce_core/src/metrics — ParserMetrics
```

## 8. 扩展点

### 8.1 自定义实体提取器

通过 `EntityExtractor` 的查询系统添加新语言的实体提取规则，详见 `cce_parser/src/parser/extractor/`。

### 8.2 自定义存储后端

通过 `StorageCoordinator` 的 `with_*` 方法链式注册新的存储后端。目前支持 Qdrant、BM25 (Tantivy)、SQLite。

### 8.3 自定义查询策略

通过 `SearcherBuilder` 组合不同的检索策略、增强器、排序器和重排序器。`ExecutionStrategy` 枚举控制搜索流程。

## 9. 总结

Entity 数据流从文件扫描开始，经过解析、分组、转换、分块、存储和索引构建等多个阶段，最终支持高效的语义搜索和关系查询。系统设计强调：

- **模块化**：每个阶段职责单一，可独立替换
- **可配置**：存储后端、分块参数、查询策略均可按需配置
- **可扩展**：支持插件系统 (Lua) 扩展框架检测和实体处理
- **错误隔离**：单一文件/批次的失败不影响整体处理
