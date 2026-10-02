# 无状态 vs 有状态工具设计

## 概述

`orchestrator/src/tools/` 提供两类工具：**无状态（Stateless）** 和 **有状态（Stateful）**。本文档说明两者的区别、使用场景和设计原则，作为后续开发的参考。

---

## 分类总览

| 类别 | 工具 | 依赖项目索引 | 是否需要 `project_id` | 数据来源 |
|------|------|-------------|---------------------|----------|
| **无状态** | `compress` (单文件压缩) | ❌ | ✅ 不需要 | 本地文件实时解析 |
| **无状态** | `batchCompress` (批量压缩) | ❌ | ✅ 不需要 | 本地文件实时解析 |
| **无状态** | `diagnose` (AST 诊断) | ❌ | ✅ 不需要 | 源代码文本 |
| **无状态** | `fold` (文件折叠) | ❌ | ✅ 不需要 | 源代码文本 |
| **无状态** | `foldBatch` (批量文件折叠) | ❌ | ✅ 不需要 | 源代码文本数组，与单条语义等价 |
| **有状态** | `getSymbols` (符号查找) | ✅ | ✅ 必填 | SQLite Symbol Table |
| **有状态** | `findReferences` (引用查找) | ✅ | ✅ 必填 | SQLite Relation Graph |
| **有状态** | `gotoDefinition` (跳转定义) | ✅ | ✅ 必填 | SQLite Symbol Table + Relations |
| **有状态** | `keywordSearch` (关键词搜索) | ✅ | ✅ 必填 | BM25 Index |

---

## 无状态工具（Stateless Tools）

### 设计原则

1. **On-Demand Processing**: 输入 → 处理 → 输出，无副作用
2. **No Side Effects**: 不执行嵌入、不缓存、不存储
3. **Independent Execution**: 每个请求独立执行，互不影响
4. **Real-Time Parsing**: 从文件/源码实时解析，不使用项目索引中的中间产物

### 核心特征

#### 数据流

```
Input: 本地文件路径 / 源代码文本
  │
  ├─→ 1. Validate (文件存在性、大小、语言检测)
  │
  ├─→ 2. Parse (tree-sitter AST)
  │
  ├─→ 3. Process (Grouping / Diagnosis / Folding)
  │
  └─→ 4. Output (临时结果，不持久化)
```

#### 典型实现模式

```rust
pub struct StatelessTool {
    /// 复用 parse coordinator 减少启动开销
    parse_coordinator: Arc<Mutex<ParseCoordinator>>,
    /// 预处理器配置
    preprocessing_config: NestProcessorConfig,
    /// 转换器
    converter: PresentationConverter,
}

impl StatelessTool {
    pub fn new() -> Self {
        Self {
            parse_coordinator: Arc::new(Mutex::new(ParseCoordinator::new())),
            preprocessing_config: NestProcessorConfig::default(),
            converter: PresentationConverter::new(),
        }
    }

    pub async fn process(&self, request: InputRequest) -> Result<OutputResponse> {
        // Step 1: Validate file
        let source = self.validate_file(&request.file_path)?;

        // Step 2: Parse with coordinator
        let parsed_file = self.parse_file(&request.file_path, &source).await?;

        // Step 3: Process (Group/Diagnose/Fold)
        let result = self.convert_to_semantic_text(&parsed_file);

        // Step 4: Return response
        Ok(OutputResponse {
            from_cache: false,  // 始终为 false
            result,
        })
    }
}
```

### 代表工具

#### 1. Semantic Compression (`compress` / `batchCompress`)

**功能**: 将代码文件转换为自然语言描述

**流程**:
```
file_path → Read source
          → Parse (AST)
          → Group (EntityGroup)
          → Convert (Semantic Text via PresentationConverter)
          → Return {semantic_text, entities?, groups?}
```

**关键细节**:
- ❌ **不能复用项目中已解析的 `ParsedFile`**：缺少 `EntityGroup` 和 `Semantic Text`
- ✅ **可以复用 `ParseCoordinator`**：减少 parser 启动开销
- ✅ **支持批处理**：通过 `max_concurrency` 控制并发

#### 2. AST Diagnosis (`diagnose`)

**功能**: 分析源代码的语法正确性和结构

**流程**:
```
code_text [option: language, file_name]
       → Detect language
       → Parse with error collector
       → Collect diagnostics
       → Return {is_valid, diagnostics, ast?}
```

**特点**:
- 纯内存操作，无需访问文件系统
- 支持多种语言的错误收集策略
- 可选返回完整 AST 树

#### 3. File Folding (`fold`)

**功能**: 提取代码骨架结构，压缩 token 数

**流程**:
```
text [option: language, max_tokens, mode]
   → Detect language
   → Parse and extract structure
   → Apply folding strategy
   → Return {folded_text, kept_sections, dropped_sections}
```

**用途**:
- 大文件的概览视图
- Token 预算管理
- LLM 上下文优化

#### 4. Batch File Folding (`foldBatch`)

**功能**: 一次请求折叠多条文本，语义与单条 `fold` 等价，用于压缩窗口内一次往返完成快照内全部命中条目。

**流程**:
```
items[id, text, option: language, file_name, max_tokens, mode] + 全局默认值
   → 逐条继承默认值并复用单条 fold
   → 按请求顺序回显 id 并汇总 token 统计
```

**特点**:
- 顺序执行，`max_concurrency` 仅作前向兼容保留
- 条目级问题降级返回，请求级超限直接拒绝并提示拆分

---

## 有状态工具（Stateful Tools）

### 设计原则

1. **Project Context Required**: 必须指定 `project_id`
2. **Database Backed**: 查询 SQLite 存储的关系图、符号表
3. **Cross-File Analysis**: 需要跨文件解析能力
4. **Index Dependent**: 依赖项目索引状态

### 核心特征

#### 数据流

```
Input: project_id + path/line/query
  │
  ├─→ 1. Validate project existence
  │
  ├─→ 2. Query SQLite (Symbol Table / Relation Graph / BM25 Index)
  │
  ├─→ 3. Resolve cross-file references
  │
  └─→ 4. Output (关系结果 / 检索结果)
```

#### 典型实现模式

```rust
pub struct StatefulTool {
    /// SQLite 客户端（用于查询项目数据）
    sqlite_client: Arc<SqliteClient>,
    /// 项目 ID（由请求传入）
    project_id: i64,
}

impl StatefulTool {
    pub async fn resolve(
        &self,
        request: ProjectScopedRequest,
    ) -> Result<Response> {
        // Validate project exists
        if !self.project_exists(request.project_id).await {
            return Err(Error::ProjectNotFound);
        }

        // Query SQLite
        let symbols = SymbolRepository::get_by_project(
            &self.sqlite_client,
            request.project_id,
            request.paths,
        ).await?;

        // Resolve cross-file relations if needed
        if let Some(relations) = self.resolve_relations(&symbols).await {
            // ...
        }

        Ok(Response {
            success: true,
            result: Some(symbols),
        })
    }
}
```

### 代表工具

#### 1. Symbol Lookup (`getSymbols`)

**功能**: 获取项目中指定文件的所有符号信息

**依赖**:
- SQLite `symbol_table` 表
- SQLite `entity` 表

**流程**:
```
project_id + paths[]
  → Load symbol table from SQLite
  → Filter by paths
  → Return {path, symbols[], symbol_count}
```

**关键点**:
- 符号表在索引过程中构建
- 需要处理名称冲突（overloading）
- 可选返回父子层级结构

#### 2. Find References (`findReferences`)

**功能**: 查找符号的调用者/被调用者

**依赖**:
- SQLite `relation_graph` 表
- Entity metadata

**流程**:
```
project_id + path + line + symbol
  → Load relation edges from SQLite
  → Filter by entity context
  → Resolve cross-file targets
  → Return {references[], callee_info?, caller_info?}
```

**关键点**:
- 区分 callee（被调用者）和 caller（调用者）
- 可选返回上下文行号和代码片段
- 支持 entity 元数据增强

#### 3. Goto Definition (`gotoDefinition`)

**功能**: 跳转到符号的定义位置

**依赖**:
- SQLite `symbol_table` 表
- Relation graph for implementations

**流程**:
```
project_id + path + line + symbol
  → Find definition in SQLite
  → Resolve interface implementations
  → Return {definitions[], code, signature}
```

**关键点**:
- 接口实现可能有多个定义点
- 可选返回完整函数体（vs 仅签名）
- 需要计算精确的行号范围

#### 4. Keyword Search (`keywordSearch`)

**功能**: BM25 全文检索

**依赖**:
- Tantivy BM25 index（每个项目独立）

**流程**:
```
project_id + query + top_n
  → Query Tantivy index
  → Score matches
  → Read raw source snippets
  → Return {results[], total_count, snippets[]}
```

**关键点**:
- BM25 index per-project
- 支持 OR/AND term operator
- 返回原始源码片段与行号，供调用方直接检索或阅读

---

## 设计对比

| 维度 | 无状态工具 | 有状态工具 |
|------|-----------|-----------|
| **输入要求** | 文件路径 / 源代码 | `project_id` + 路径/行号/查询 |
| **数据来源** | 实时读取本地文件 | SQLite / Tantivy |
| **计算复杂度** | O(n) 解析 + 转换 | O(m) 数据库查询 + 关系解析 |
| **缓存策略** | ❌ 从不缓存 | ✅ 可缓存（SQL 查询结果） |
| **跨文件能力** | ❌ 仅限单文件 | ✅ 原生支持 |
| **性能瓶颈** | Parser / Converter | DB queries / Index access |
| **扩展方向** | 添加语言后端 | 添加新索引类型 |

---

## 常见误区

### ❌ 误区 1：无状态工具应该尝试复用项目索引

**错误做法**:
```rust
// 错误：试图复用项目中已解析的 ParsedFile
let cached_parsed_file = indexed_project.get_parsed_file(file_path)?;
let semantic_text = convert_from_parsed_file(cached_parsed_file)?;
```

**问题**:
- 项目中未存储 `EntityGroup`（只存 `ParsedFile`）
- `semantic_text` 需通过 `PresentationConverter` 计算（依赖 Grouper 输出）
- Parser/Grouper 配置变更会导致结果不一致

**正确做法**:
```rust
// 正确：完全重新解析 + 转换
let source = read_file(file_path)?;
let parsed_file = parse_with_coordinator(file_path, &source).await?;
let groups = group_entities(parsed_file);  // 实时分组
let semantic_text = convert_groups(groups);  // 实时转换
```

### ❌ 误区 2：无状态工具可以接受可选的 `project_id`

**错误设计**:
```rust
pub struct BatchCompressionRequest {
    pub file_paths: Vec<String>,
    pub project_id: Option<i64>,  // ❌ 误导：实际从未使用
}
```

**问题**:
- 用户可能误以为传入 `project_id` 会触发缓存复用
- 实际上 `from_cache` 始终为 `false`
- 增加 API 认知负担，无实际收益

**正确设计**:
```rust
pub struct BatchCompressionRequest {
    pub file_paths: Vec<String>,
    // ✅ 明确：无项目依赖字段
}
```

### ❌ 误区 3：混合两类工具的职责边界

**错误实现**:
```rust
pub async fn process_tool(
    &self,
    request: MixedRequest,  // ❌ 既有 file_path 又有 project_id
) -> Result<Response> {
    if let Some(project_id) = request.project_id {
        // 查询项目索引...
    } else {
        // 读取本地文件...
    }
}
```

**问题**:
- 职责模糊：工具到底是 stateless 还是 stateful？
- 测试困难：行为依赖运行时条件
- 维护成本高：分支逻辑难以覆盖

**正确做法**:
- 明确分类，保持单一职责
- Stateless 工具专注于"on-demand processing"
- Stateful 工具专注于"cross-file analysis"

---

## 扩展指南

### 新增无状态工具

当需要实现以下场景时，选择无状态设计：

✅ **适用场景**:
- 单文件分析（语法检查、代码质量）
- 即时的格式转换（AST → NL）
- 轻量级处理（token 预算内完成）
- 不依赖项目上下文的操作

**实现步骤**:
1. 定义 Request/Response 模型（不含 `project_id`）
2. 实现 Processor 逻辑（Parse → Process → Output）
3. 复用 `ParseCoordinator` 减少开销
4. 设置 `from_cache: false`

### 新增有状态工具

当需要实现以下场景时，选择有状态设计：

✅ **适用场景**:
- 跨文件关系查询
- 全局符号检索
- 全文索引搜索
- 需要项目上下文的操作

**实现步骤**:
1. 定义 Request/Response 模型（含 `project_id`）
2. 添加 SQLite/Tantivy 查询逻辑
3. 处理跨文件引用解析
4. 考虑缓存策略（Redis/Memcache）

---

## 最佳实践总结

### 命名约定

- **Stateless**: `compress`, `fold`, `diagnose`, `validate`
- **Stateful**: `find_symbols`, `get_references`, `search_code`, `lookup_definition`

### 错误处理

```rust
// Stateless: 文件/解析相关错误
#[derive(Error, Debug)]
pub enum CompressionError {
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("Language detection failed: {0}")]
    LanguageDetectionError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
}

// Stateful: 项目/数据库相关错误
#[derive(Error, Debug)]
pub enum SymbolLookupError {
    #[error("Project not found: {0}")]
    ProjectNotFound(i64),
    #[error("SQLite error: {0}")]
    DatabaseError(#[from] sqlite::Error),
    #[error("Relation graph unavailable")]
    RelationGraphUnavailable,
}
```

### 测试策略

```rust
// Stateless: 单元测试为主
#[test]
fn test_compress_single_function() {
    let input = "fn add(a: i32, b: i32) -> i32 { a + b }";
    let result = fold(input).unwrap();
    assert!(result.folded_text.contains("add"));
    assert_eq!(result.structure_known, true);
}

// Stateful: 集成测试 + Mock 数据库
#[tokio::test]
async fn test_find_references_cross_file() {
    let mock_db = MockSqliteClient::new();
    let tool = ReferenceFinder::new(mock_db);
    let result = tool.find(...).await.unwrap();
    assert_eq!(result.references.len(), 3);
}
```

---

## 参考文献

- [`docs/app/orchestrator/tools/compression.md`](./compression.md) - 语义压缩详细设计
- [`docs/app/orchestrator/tools/symbol-lookup-design.md`](./symbol-lookup-design.md) - 符号查找设计
- [`crates/app/cce-orchestrator/src/tools/`](`../../../../../crates/app/cce-orchestrator/src/tools/`) - 工具模块源码
