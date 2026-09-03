# 语义压缩模块

## 概述

语义压缩模块 (`src/orchestrator/query/tools/compression`) 提供代码文件的按需语义压缩功能,面向外部 API,为人和 LLM 提供可读的语义化文本。

## 核心特性

- **纯语义输出**: 仅生成纯自然语言描述,移除所有代码符号
- **按需处理**: 单文件处理,不执行向量嵌入、不缓存、不存储
- **缓存复用**: 优先使用已索引项目的缓存数据
- **批处理支持**: 支持并发处理多个文件
- **文件大小限制**: 最大支持 10MB 文件

## 模块结构

```
src/orchestrator/query/tools/compression/
├── mod.rs              # 核心逻辑和 CompressionRetrieval 实现
└── types.rs            # 类型定义 (请求、响应、错误)
```

## 核心 API

### CompressionRetrieval

语义压缩处理器,提供单文件和批处理两种模式。

```rust
// 创建实例
let retrieval = CompressionRetrieval::new()
    .with_sqlite_client(sqlite_client)
    .with_state_tracker(state_tracker);

// 单文件处理
let request = CompressionRequest::new("src/main.rs")
    .with_entities(true)
    .with_groups(true);

let response = retrieval.compress(request).await?;

// 批处理
let batch_request = BatchCompressionRequest::new(vec![
    "src/main.rs".to_string(),
    "src/lib.rs".to_string(),
])
.with_entities(true)
.with_max_concurrency(4);

let batch_response = retrieval.compress_batch(batch_request).await;
```

### CompressionRequest

```rust
pub struct CompressionRequest {
    /// 文件路径 (绝对或相对)
    pub file_path: String,
    
    /// 是否包含实体信息
    pub include_entities: bool,
    
    /// 是否包含实体分组
    pub include_groups: bool,
}
```

### CompressionResponse

```rust
pub struct CompressionResponse {
    /// 文件路径
    pub file_path: String,
    
    /// 编程语言
    pub language: String,
    
    /// 文件哈希 (SHA-256)
    pub file_hash: String,
    
    /// 是否来自缓存
    pub from_cache: bool,
    
    /// 实体列表 (可选)
    pub entities: Option<Vec<Entity>>,
    
    /// 实体分组 (可选)
    pub groups: Option<Vec<EntityGroup>>,
    
    /// 语义文本 (纯自然语言)
    pub semantic_text: String,
}
```

## 处理流程

```text
输入: 文件路径
  │
  ├─→ 1. 文件验证
  │     - 检查文件存在性
  │     - 检查文件大小 (≤ 10MB)
  │     - 检查文件类型
  │     - 检测编程语言
  │
  ├─→ 2. 检查已索引项目
  │     - 查询 UpdateStateTracker
  │     - 如果已索引,进入步骤 3
  │     - 否则,跳到步骤 4
  │
  ├─→ 3. 检查缓存
  │     - 计算文件哈希
  │     - 查询 CacheRepository
  │     - 如果命中,跳到步骤 5
  │
  ├─→ 4. 完整解析
  │     ├─→ AST 解析 (ParseCoordinator)
  │     ├─→ 实体分组 (PreprocessingPipeline)
  │     └─→ 语义转换 (AstToNlConverter, Embedding 模式)
  │
  └─→ 5. 构建响应
        - 返回纯自然语言语义文本
```

## 设计决策

### 为什么仅保留 Embedding 文本?

1. **定位清晰**: tools 模块面向外部 API,服务于人和 LLM
2. **可读性**: Embedding 文本是纯自然语言,无代码符号干扰
3. **语义化**: 聚焦于功能和意图描述,而非技术细节
4. **避免混淆**: BM25 用于内部索引构建,不应暴露给外部 API

### 为什么移除关键词提取?

1. **重复处理**: Embedding 文本已包含语义信息
2. **质量较低**: 从自然语言提取的词不如代码实体关键词精确
3. **未被使用**: 关键词字段没有被任何模块消费
4. **性能开销**: 每次响应都要进行文本分词和停用词过滤

## 使用场景

- **代码审查**: 快速理解大型文件的功能和结构
- **文档生成**: 为代码文件生成自然语言描述
- **LLM 辅助**: 为 LLM 提供代码语义上下文
- **代码搜索**: 基于语义相似度搜索代码

## 性能考虑

- **文件大小限制**: 10MB,防止内存溢出
- **缓存复用**: 优先使用已索引项目的缓存
- **批处理并发**: 可配置最大并发数
- **实例复用**: ParseCoordinator 在批处理中复用

## 错误处理

```rust
pub enum CompressionError {
    FileNotFound(String),      // 文件不存在
    FileNotReadable(String),   // 文件不可读
    FileTooLarge(String),      // 文件过大
    UnsupportedFileType(String), // 不支持的文件类型
    ParseError(String),        // 解析错误
    LanguageDetectionError(String), // 语言检测失败
    CacheError(String),        // 缓存错误
}
```

## 示例输出

输入代码:
```rust
fn calculate_total(items: &[Item]) -> f64 {
    items.iter().map(|item| item.price * item.quantity).sum()
}
```

输出语义文本:
```
This function calculates the total cost of a collection of items. 
It iterates through each item, multiplies its price by its quantity, 
and sums all the results to produce the final total amount.
```
