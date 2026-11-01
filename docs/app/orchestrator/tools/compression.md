# 语义压缩模块

## 概述

语义压缩模块（`crates/app/cce-orchestrator/src/tools/compression.rs` + `compression/types.rs`，单文件实现，无 `compression/` 目录）提供代码文件的按需语义压缩功能,面向外部 API,为人和 LLM 提供可读的语义化文本。

## 核心特性

- **纯语义输出**: 仅生成纯自然语言描述,移除所有代码符号
- **按需处理**: 单文件处理,不执行向量嵌入、不缓存、不存储
- **无缓存查询**: 不做任何缓存命中判断,`from_cache` 恒为 `false`
- **批处理支持**: `compress_batch` 顺序逐文件处理（见下文说明）
- **文件大小限制**: 最大支持 10MB 文件

## 模块结构

```
crates/app/cce-orchestrator/src/tools/
├── compression.rs      # 核心逻辑和 CompressionRetrieval 实现
└── compression/
    └── types.rs        # 类型定义 (请求、响应、错误)
```

## 核心 API

### CompressionRetrieval

语义压缩处理器,提供单文件和批处理两种模式。

```rust
// 创建实例
let retrieval = CompressionRetrieval::new()
    .with_state_tracker(state_tracker)
    .with_preprocessing_config(nest_processor_config);

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

注：`BatchCompressionRequest` 有 `max_concurrency` 字段（默认 4，`with_max_concurrency` 可设置），但当前实现**未消费该字段**——`compress_batch` 是顺序 for 循环，逐文件调用 `compress`。文档与调用方应按"顺序执行"理解；并发化是未来的方向。

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
  ├─→ 1. 文件验证 (validate_file)
  │     - 路径 canonicalize + is_file 检查
  │     - 检查文件大小 (≤ 10MB)
  │     - FileProcessor 处理（文本检测 + 语言检测，Unknown 拒绝）
  │     - 读取源码
  │
  ├─→ 2. 完整解析
  │     ├─→ AST 解析 (ParseCoordinator::parse_with_language_info，实例在批处理中复用)
  │     ├─→ 实体分组 (PreprocessingPipeline::with_config)
  │     └─→ 语义转换 (PresentationConverter::convert_entity_groups，
  │                  展平各 group 的 header/member 转换，取 embedding_text 拼接)
  │
  └─→ 3. 构建响应
        - file_hash = SHA-256，from_cache = false（恒定）
        - 按请求返回 entities / groups
        - 返回纯自然语言语义文本
```

注：没有缓存检查步骤——`compress` 每次都完整解析，`from_cache` 恒为 `false`，不存在 `CacheRepository`。

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
- **无缓存**: 每次请求完整解析（from_cache 恒为 false）
- **批处理**: 顺序执行；`max_concurrency` 字段存在但未被消费
- **实例复用**: ParseCoordinator 在批处理中通过 `Arc<Mutex<...>>` 复用

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
