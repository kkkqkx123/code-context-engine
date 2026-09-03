# Qdrant 向量存储使用分析

## 概述

本文档分析 Qdrant 向量数据库在项目中的使用方式、调用路径和核心功能，为后续扩展提供基础。

## 存储架构现状

### 三种存储后端

项目使用三种不同的存储后端，各司其职：

| 存储后端 | 职责 | 数据类型 | 访问模式 |
|---------|------|---------|---------|
| **Qdrant** | 向量存储与语义搜索 | 高维向量 + Payload | 相似性搜索、最近邻查询 |
| **BM25** | 全文搜索 | 文档（标题、内容、关键词） | 关键词搜索、相关性评分 |
| **SQLite** | 元数据存储 | 实体、关系、缓存、映射 | 精确查询、关系遍历、事务 |

**关键点**：三种后端职责完全不同，不应强行统一抽象。

### 模块结构

```
src/storage/
├── mod.rs              # 模块入口，重导出类型
├── metrics.rs          # 性能指标收集
│
├── qdrant/             # Qdrant 向量存储
│   ├── mod.rs
│   ├── client.rs       # QdrantClient 主客户端
│   ├── config.rs       # 配置类型
│   ├── error.rs        # 错误类型
│   ├── types.rs        # 数据类型
│   ├── estimator.rs    # 集合大小估算
│   ├── scheduler.rs    # 配置升级调度
│   ├── upgrade.rs      # 配置升级服务
│   └── operations/     # 操作模块
│       ├── collection.rs
│       ├── points.rs
│       ├── search.rs
│       └── summary.rs
│
├── bm25/               # BM25 全文搜索
│   ├── mod.rs
│   ├── client.rs
│   ├── config.rs
│   └── ...
│
└── sqlite/             # SQLite 元数据存储
    ├── mod.rs
    ├── client.rs
    ├── helpers.rs
    ├── types.rs
    ├── utils.rs
    └── repo/           # 数据仓库
        ├── cache_repo.rs
        ├── chunk_repo.rs
        ├── entity_repo.rs
        └── ...
```

## QdrantClient 核心分析

### 客户端结构

```rust
// src/storage/qdrant/client.rs:42-57
pub struct QdrantClient {
    config: QdrantConfig,              // 配置
    http_client: Client,               // HTTP 客户端
    collection_name: String,           // 主集合名称
    summary_collection_name: String,   // 摘要集合名称
    base_url: String,                  // API 基础 URL
    
    // 操作处理器（门面模式）
    collection_ops: CollectionOperations,
    point_ops: PointOperations,
    search_ops: SearchOperations,
    summary_ops: SummaryOperations,
    
    metrics: StorageMetrics,           // 性能指标
}
```

### 核心方法

#### 1. 初始化方法

```rust
// 创建客户端
pub fn new(config: QdrantConfig, workspace_path: &str) -> Result<Self, QdrantError>

// 初始化集合
pub async fn initialize(&self) -> Result<bool, QdrantError>

// 检查集合是否存在
pub async fn collection_exists(&self) -> Result<bool, QdrantError>

// 获取集合信息
pub async fn get_collection_info(&self) -> Result<CollectionInfo, QdrantError>
```

#### 2. 数据操作方法

```rust
// Upsert 向量点
pub async fn upsert_points(&self, points: &[VectorPoint]) -> Result<(), QdrantError>

// 按文件路径删除
pub async fn delete_by_file_path(&self, file_path: &str) -> Result<(), QdrantError>

// 批量删除
pub async fn delete_by_file_paths(&self, file_paths: &[&str]) -> Result<(), QdrantError>

// 清空集合
pub async fn clear_collection(&self) -> Result<(), QdrantError>

// 删除集合
pub async fn delete_collection(&self) -> Result<(), QdrantError>
```

#### 3. 查询方法

```rust
// 向量搜索
pub async fn search(&self, query: SearchQuery) -> Result<Vec<SearchResult>, QdrantError>

// 检查是否有索引数据
pub async fn has_indexed_data(&self) -> Result<bool, QdrantError>
```

#### 4. 摘要方法

```rust
// 确保摘要集合存在
pub async fn ensure_summary_collection(&self) -> Result<(), QdrantError>

// Upsert 摘要点
pub async fn upsert_summary_points(&self, points: &[VectorPoint]) -> Result<(), QdrantError>

// 搜索摘要
pub async fn search_summaries(
    &self,
    query_vector: Vec<f32>,
    top_k: usize,
    min_score: f32,
) -> Result<Vec<SummarySearchResult>, QdrantError>

// 删除摘要
pub async fn delete_summary_by_file_path(&self, file_path: &str) -> Result<(), QdrantError>
```

#### 5. 索引状态方法

```rust
// 标记索引完成
pub async fn mark_indexing_complete(&self) -> Result<(), QdrantError>

// 标记索引进行中
pub async fn mark_indexing_in_progress(&self) -> Result<(), QdrantError>
```

### 配置系统

```rust
// src/storage/qdrant/config.rs
pub struct QdrantConfig {
    pub url: String,                    // Qdrant 服务器 URL
    pub api_key: Option<String>,        // API Key
    pub vector_size: usize,             // 向量维度
    pub distance_metric: DistanceMetric,// 距离度量
    pub timeout_ms: u64,                // 超时时间
    pub max_retries: u32,               // 最大重试次数
    pub retry_delay_ms: u64,            // 重试延迟
    pub enabled: bool,                  // 是否启用
    pub preset: CollectionPreset,       // 集合预设
}

// 集合预设
pub enum CollectionPreset {
    Tiny,    // ≤ 2000 向量，无 HNSW
    Small,   // 2000 - 10000 向量
    Medium,  // 10000 - 100000 向量（默认）
    Large,   // > 100000 向量
}
```

### 数据类型

```rust
// 向量点
pub struct VectorPoint {
    pub id: String,          // 点 ID
    pub vector: Vec<f32>,    // 向量数据
    pub payload: Payload,    // 元数据
}

// Payload 元数据
pub struct Payload {
    pub file_path: String,              // 文件路径
    pub code_chunk: String,             // 代码片段
    pub start_line: u32,                // 起始行
    pub end_line: u32,                  // 结束行
    pub entity_type: Option<String>,    // 实体类型
    pub entity_id: Option<u32>,         // 实体 ID
    pub file_extension: Option<String>, // 文件扩展名
    pub language: Option<String>,       // 编程语言
    pub content_type: Option<String>,   // 内容类型
    pub file_name: Option<String>,      // 文件名
    pub extra: HashMap<String, Value>,  // 额外元数据
}

// 搜索查询
pub struct SearchQuery {
    pub vector: Vec<f32>,               // 查询向量
    pub limit: usize,                   // 结果数量
    pub min_score: Option<f32>,         // 最小分数
    pub directory_prefix: Option<String>,// 目录前缀过滤
    pub hnsw_ef: Option<u32>,           // HNSW ef 参数
}

// 搜索结果
pub struct SearchResult {
    pub id: String,         // 点 ID
    pub score: f32,         // 相似度分数
    pub payload: Payload,   // 元数据
}
```

## 使用场景分析

### 场景 1：索引流程

**调用路径**：
```
IndexOrchestrator::index_directory
  → StorageCoordinator::store_vectors_batched
    → QdrantClient::upsert_points
```

**核心代码** (storage_coordinator.rs:90-178)：
```rust
pub async fn store_vectors_batched(
    &self,
    chunks: &[ChunkedResult],
    batch_size: usize,
    batch_delay_ms: u64,
) -> Result<usize, OrchestratorError> {
    let qdrant = self.qdrant.as_ref().ok_or(...)?;
    let embedder = self.embedder.as_ref().ok_or(...)?;
    
    for batch in chunks.chunks(batch_size) {
        // 1. 生成嵌入
        let embeddings = embedder.embed(&texts).await?;
        
        // 2. 构建 VectorPoint
        let points = self.build_storage_data(batch, &embeddings);
        
        // 3. 存储到 Qdrant
        qdrant.upsert_points(&points).await?;
        
        // 4. 存储到 SQLite（chunk records、entity mappings）
        self.store_chunk_records(&chunk_records)?;
        self.store_entity_mappings(&entity_mappings)?;
    }
}
```

**数据流**：
```
ChunkedResult → Embedding → VectorPoint → Qdrant
                          ↓
                      ChunkRecord → SQLite
                          ↓
                    EntityMapping → SQLite
```

### 场景 2：查询流程

**调用路径**：
```
QueryCoordinator::search
  → Searcher::search
    → VectorRetrieval::retrieve
      → QdrantClient::search
```

**核心代码** (searcher.rs:95-112)：
```rust
pub async fn search(&self, options: &QueryOptions) -> Result<QueryResult> {
    // 1. 确定执行策略
    let strategy = options.execution_strategy();
    
    // 2. 执行搜索
    let results = self.execute_with_strategy(options, &strategy).await?;
    
    // 3. 返回结果
    Ok(QueryResult {
        total: results.len(),
        items: results,
        elapsed_ms,
        sources: vec![options.sources.to_string()],
    })
}
```

**搜索策略**：
- `VectorOnly`: 纯向量搜索
- `VectorEnhanced`: 向量 + BM25 增强
- `Bm25Only`: 纯 BM25 搜索
- `SummaryOnly`: 文件摘要搜索
- `WithRelationExpansion`: 带关系扩展
- `SummaryPreFilter`: 摘要预过滤

### 场景 3：热更新流程

**调用路径**：
```
HotUpdateOrchestrator::handle_file_change
  → StorageCoordinator::hot_update_file
    → QdrantClient::delete_by_file_path
    → QdrantClient::upsert_points
```

**核心代码** (storage_coordinator.rs:571-618)：
```rust
pub async fn hot_update_file(
    &self,
    file_path: &std::path::Path,
    chunks: &[ChunkedResult],
) -> Result<(), OrchestratorError> {
    // 1. 删除旧数据
    qdrant.delete_by_file_path(&file_path_str).await?;
    bm25.delete("default", &file_path_str).await?;
    
    // 2. 存储新数据
    self.store_vectors_batched(chunks, 32, 0).await?;
    self.store_bm25(chunks).await?;
}
```

### 场景 4：摘要存储

**调用路径**：
```
IndexOrchestrator::index_summaries
  → StorageCoordinator::store_summaries
    → QdrantClient::upsert_summary_points
```

**核心代码** (storage_coordinator.rs:461-549)：
```rust
pub async fn store_summaries(
    &self,
    summaries: &[FileSummary],
) -> Result<usize, OrchestratorError> {
    // 1. 生成摘要嵌入
    let embeddings = embedder.embed(&texts).await?;
    
    // 2. 构建 VectorPoint
    let points = summaries.iter().zip(embeddings.iter())
        .map(|(summary, vector)| {
            VectorPoint::new(
                format!("summary:{}", summary.file_path),
                vector.clone(),
                payload,
            )
        }).collect();
    
    // 3. 存储到摘要集合
    qdrant.upsert_summary_points(&points).await?;
}
```

## StorageCoordinator 协调器

### 结构

```rust
// src/orchestrator/index/storage_coordinator.rs:26-31
pub struct StorageCoordinator {
    qdrant: Option<Arc<QdrantClient>>,              // Qdrant 客户端
    bm25: Option<Arc<tokio::sync::Mutex<Bm25Client>>>, // BM25 客户端
    embedder: Option<Arc<Embedder>>,                // 嵌入器
    metadata_store: Option<Arc<SqliteDatabase>>,    // SQLite 元数据存储
}
```

### 职责

1. **协调多后端存储**：统一管理 Qdrant、BM25、SQLite
2. **批量处理**：控制内存使用和 API 速率限制
3. **数据一致性**：维护实体映射关系
4. **热更新**：处理文件变更的增量更新

### 关键方法

| 方法 | 职责 | 涉及后端 |
|------|------|---------|
| `store_vectors_batched` | 批量存储向量 | Qdrant + SQLite |
| `store_bm25_batched` | 批量存储 BM25 文档 | BM25 + SQLite |
| `store_summaries` | 存储文件摘要 | Qdrant |
| `remove_file` | 删除文件数据 | Qdrant + BM25 + SQLite |
| `hot_update_file` | 热更新文件 | Qdrant + BM25 + SQLite |

## 配置示例

```toml
# config.toml
[database.qdrant]
url = "http://localhost:6333"
api_key = ""
vector_size = 768
distance_metric = "Cosine"
timeout_ms = 30000
max_retries = 3
retry_delay_ms = 1000
enabled = true
preset = "Medium"
```

## 性能指标

QdrantClient 内置性能指标收集：

```rust
// src/storage/metrics.rs
pub struct StorageMetrics {
    operations: HashMap<StorageOperation, OperationStats>,
    collection_size: AtomicU64,
}

pub enum StorageOperation {
    Initialize,
    GetInfo,
    Upsert,
    Delete,
    Search,
    Clear,
}
```

每次操作都会记录：
- 操作类型
- 成功/失败
- 延迟时间
- 集合大小

## 错误处理

```rust
// src/storage/qdrant/error.rs
pub enum QdrantError {
    Connection(String),
    ConnectionRefused { url: String, message: String },
    ConnectionTimeout(String),
    CollectionNotFound(NotFoundError),
    CollectionAlreadyExists(String),
    DimensionMismatch { expected: usize, actual: usize },
    InvalidUrl(String),
    Api(String),
    Request(String),
    ResponseParse(String),
    Config(ConfigError),
    InvalidConfig { field: String, reason: String },
    OperationTimeout(TimeoutError),
    NotConnected,
    Disabled,
    // ...
}
```

关键方法：
- `is_retryable()`: 判断是否可重试
- `is_connection_error()`: 判断是否连接错误
- `is_not_found()`: 判断是否未找到错误

## 总结

### Qdrant 的核心职责

1. **向量存储**：存储代码片段的向量表示
2. **语义搜索**：基于向量相似度的搜索
3. **摘要索引**：文件级别的摘要向量
4. **索引状态**：标记索引完成状态

### 与其他后端的协作

- **与 BM25**：互补搜索（语义 vs 关键词）
- **与 SQLite**：存储映射关系和元数据
- **与 Embedder**：生成向量嵌入

### 设计特点

1. **门面模式**：通过操作处理器分离关注点
2. **配置驱动**：支持预设配置，自动优化
3. **指标收集**：内置性能监控
4. **错误分类**：区分可重试和不可重试错误

### 扩展考虑

如果需要支持其他向量数据库（如 Milvus、Weaviate），应该：

1. **保持独立**：每个后端独立实现，不强行统一抽象
2. **配置切换**：通过配置选择使用的后端
3. **接口一致**：保持核心方法签名一致（但不强制 trait）
4. **类型转换**：在协调层处理不同后端的数据格式差异
