# 向量存储抽象层设计（已弃用）

> **已弃用（Deprecated）**：本文档描述的 `VectorStorage` trait + 工厂多后端抽象**未落地**。
> 依据《存储模块重构方案》§3.1（S1，方案 A：退场 + 轻抽象）决策：当前只有 Qdrant 一个向量后端，
> 明确放弃未落地的多后端 trait 抽象，检索接口收敛为确定性的具体类型层
> （`storage/vector_retrieval.rs`，Qdrant 专用：`Payload`/`ScoredPoint`/`DenseSearchQuery`/`SearchFilter`）。
> 若未来确实引入第二后端，需重新立项，从现有类型层平滑演进，本设计文档仅供历史参考。

## 概述

本文档描述向量存储抽象层的设计方案，旨在支持多种向量数据库后端（Qdrant、Milvus、Weaviate 等），同时保持现有 Qdrant 实现的稳定性。

## 设计目标

### 主要目标

1. **可扩展性**：支持轻松添加新的向量存储后端
2. **向后兼容**：不破坏现有 Qdrant 实现
3. **统一接口**：提供一致的 API 供上层使用
4. **配置驱动**：通过配置切换不同后端
5. **性能透明**：保持各后端的性能优势

### 非目标

- 不实现跨后端数据迁移
- 不支持多后端同时写入
- 不改变现有数据模型

## 架构设计

### 整体架构

```
┌─────────────────────────────────────────────────────────────┐
│                    应用层 (Application)                      │
├─────────────────────────────────────────────────────────────┤
│                协调层 (Orchestrator Layer)                   │
│  ┌─────────────────────────────────────────────────────┐   │
│  │           StorageCoordinator                         │   │
│  └─────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────┤
│                   抽象层 (Abstraction Layer)                 │
│  ┌─────────────────────────────────────────────────────┐   │
│  │           VectorStorage Trait                        │   │
│  └─────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────┤
│                   实现层 (Implementation Layer)              │
│  ┌─────────────┐  ┌─────────────┐  ┌───────────────────┐  │
│  │  Qdrant     │  │   Milvus    │  │    Weaviate       │  │
│  │  Client     │  │   Client    │  │     Client        │  │
│  └─────────────┘  └─────────────┘  └───────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### 模块划分

```
src/storage/
├── mod.rs                    # 模块入口
├── traits.rs                 # VectorStorage trait 定义
├── error.rs                  # 统一错误类型
├── factory.rs                # 后端工厂
├── metrics.rs                # 指标收集（现有）
│
├── qdrant/                   # Qdrant 实现（现有）
│   ├── mod.rs
│   ├── client.rs
│   ├── config.rs
│   ├── error.rs
│   ├── types.rs
│   ├── estimator.rs
│   ├── scheduler.rs
│   ├── upgrade.rs
│   └── operations/
│
├── milvus/                   # Milvus 实现（新增）
│   ├── mod.rs
│   ├── client.rs
│   ├── config.rs
│   ├── error.rs
│   ├── types.rs
│   └── operations/
│
└── weaviate/                 # Weaviate 实现（新增）
    ├── mod.rs
    ├── client.rs
    ├── config.rs
    ├── error.rs
    ├── types.rs
    └── operations/
```

## 核心接口设计

### VectorStorage Trait

```rust
use async_trait::async_trait;
use crate::storage::{VectorPoint, SearchQuery, SearchResult, CollectionInfo};

/// Vector storage backend trait
///
/// This trait defines the unified interface for all vector storage backends.
/// Each backend must implement all methods to ensure consistent behavior.
#[async_trait]
pub trait VectorStorage: Send + Sync {
    // ==================== 生命周期管理 ====================
    
    /// Initialize the storage backend
    ///
    /// Returns `true` if a new collection was created, `false` if it already existed.
    async fn initialize(&self) -> Result<bool, VectorStorageError>;
    
    /// Check if storage is configured and ready
    fn is_configured(&self) -> bool;
    
    /// Check if collection exists
    async fn collection_exists(&self) -> Result<bool, VectorStorageError>;
    
    /// Delete the collection
    async fn delete_collection(&self) -> Result<(), VectorStorageError>;
    
    /// Clear all points from collection
    async fn clear_collection(&self) -> Result<(), VectorStorageError>;
    
    // ==================== 数据操作 ====================
    
    /// Upsert vector points
    ///
    /// If a point with the same ID exists, it will be updated.
    /// Otherwise, a new point will be created.
    async fn upsert_points(&self, points: &[VectorPoint]) -> Result<(), VectorStorageError>;
    
    /// Delete points by file path
    async fn delete_by_file_path(&self, file_path: &str) -> Result<(), VectorStorageError>;
    
    /// Delete points by multiple file paths
    async fn delete_by_file_paths(&self, file_paths: &[&str]) -> Result<(), VectorStorageError>;
    
    // ==================== 查询操作 ====================
    
    /// Search for similar vectors
    async fn search(&self, query: SearchQuery) -> Result<Vec<SearchResult>, VectorStorageError>;
    
    /// Get collection information
    async fn get_collection_info(&self) -> Result<CollectionInfo, VectorStorageError>;
    
    // ==================== 索引状态 ====================
    
    /// Check if has indexed data
    async fn has_indexed_data(&self) -> Result<bool, VectorStorageError>;
    
    /// Mark indexing as complete
    async fn mark_indexing_complete(&self) -> Result<(), VectorStorageError>;
    
    /// Mark indexing as in progress
    async fn mark_indexing_in_progress(&self) -> Result<(), VectorStorageError>;
    
    // ==================== 元信息 ====================
    
    /// Get collection name
    fn collection_name(&self) -> &str;
    
    /// Get backend type name
    fn backend_type(&self) -> &'static str;
}

/// Optional operations for backends that support advanced features
#[async_trait]
pub trait AdvancedVectorStorage: VectorStorage {
    /// Search with custom scoring function
    async fn search_with_scoring(
        &self,
        query: SearchQuery,
        scoring_fn: ScoringFunction,
    ) -> Result<Vec<SearchResult>, VectorStorageError>;
    
    /// Batch search for multiple queries
    async fn batch_search(
        &self,
        queries: &[SearchQuery],
    ) -> Result<Vec<Vec<SearchResult>>, VectorStorageError>;
    
    /// Get points by IDs
    async fn get_points(&self, ids: &[&str]) -> Result<Vec<VectorPoint>, VectorStorageError>;
}
```

### 统一错误类型

```rust
/// Unified error type for vector storage operations
#[derive(Error, Debug)]
pub enum VectorStorageError {
    // 后端特定错误
    #[error("Qdrant error: {0}")]
    Qdrant(#[from] QdrantError),
    
    #[error("Milvus error: {0}")]
    Milvus(#[from] MilvusError),
    
    #[error("Weaviate error: {0}")]
    Weaviate(#[from] WeaviateError),
    
    // 通用错误
    #[error("Connection error: {0}")]
    Connection(String),
    
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Operation timeout: {0}")]
    Timeout(String),
    
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    
    #[error("Backend not supported: {0}")]
    UnsupportedBackend(String),
}

impl VectorStorageError {
    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Connection(_) | Self::Timeout(_) => true,
            Self::Qdrant(e) => e.is_retryable(),
            // 其他后端的判断逻辑
            _ => false,
        }
    }
    
    /// Check if this is a connection error
    pub fn is_connection_error(&self) -> bool {
        matches!(self, Self::Connection(_))
    }
}
```

## 后端实现规范

### Qdrant 实现

Qdrant 作为现有实现，需要适配到新的 trait 接口：

```rust
// src/storage/qdrant/client.rs

#[async_trait]
impl VectorStorage for QdrantClient {
    async fn initialize(&self) -> Result<bool, VectorStorageError> {
        self.initialize().await.map_err(Into::into)
    }
    
    fn is_configured(&self) -> bool {
        self.is_enabled()
    }
    
    async fn upsert_points(&self, points: &[VectorPoint]) -> Result<(), VectorStorageError> {
        self.upsert_points(points).await.map_err(Into::into)
    }
    
    async fn search(&self, query: SearchQuery) -> Result<Vec<SearchResult>, VectorStorageError> {
        self.search(query).await.map_err(Into::into)
    }
    
    fn collection_name(&self) -> &str {
        self.collection_name()
    }
    
    fn backend_type(&self) -> &'static str {
        "qdrant"
    }
    
    // ... 其他方法实现
}
```

### Milvus 实现示例

```rust
// src/storage/milvus/client.rs

pub struct MilvusClient {
    config: MilvusConfig,
    http_client: reqwest::Client,
    collection_name: String,
    metrics: StorageMetrics,
}

impl MilvusClient {
    pub fn new(config: MilvusConfig, workspace_path: &str) -> Result<Self, MilvusError> {
        config.validate()?;
        
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()?;
        
        let collection_name = Self::generate_collection_name(workspace_path);
        
        Ok(Self {
            config,
            http_client,
            collection_name,
            metrics: StorageMetrics::new(),
        })
    }
}

#[async_trait]
impl VectorStorage for MilvusClient {
    async fn initialize(&self) -> Result<bool, VectorStorageError> {
        // 检查集合是否存在
        if self.collection_exists().await? {
            return Ok(false);
        }
        
        // 创建集合
        self.create_collection().await?;
        Ok(true)
    }
    
    async fn upsert_points(&self, points: &[VectorPoint]) -> Result<(), VectorStorageError> {
        // 转换为 Milvus 格式
        let milvus_points: Vec<MilvusPoint> = points
            .iter()
            .map(|p| self.convert_point(p))
            .collect();
        
        // 批量插入
        self.insert_points(&milvus_points).await?;
        Ok(())
    }
    
    async fn search(&self, query: SearchQuery) -> Result<Vec<SearchResult>, VectorStorageError> {
        // 构建 Milvus 搜索请求
        let milvus_query = self.build_search_request(&query);
        
        // 执行搜索
        let results = self.execute_search(&milvus_query).await?;
        
        // 转换结果格式
        Ok(results.into_iter().map(|r| r.into()).collect())
    }
    
    fn backend_type(&self) -> &'static str {
        "milvus"
    }
    
    // ... 其他方法
}
```

## 工厂模式

### 后端枚举

```rust
/// Supported vector storage backends
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VectorStorageBackend {
    Qdrant,
    Milvus,
    Weaviate,
}

impl VectorStorageBackend {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Qdrant => "qdrant",
            Self::Milvus => "milvus",
            Self::Weaviate => "weaviate",
        }
    }
    
    pub fn from_str(s: &str) -> Result<Self, VectorStorageError> {
        match s.to_lowercase().as_str() {
            "qdrant" => Ok(Self::Qdrant),
            "milvus" => Ok(Self::Milvus),
            "weaviate" => Ok(Self::Weaviate),
            _ => Err(VectorStorageError::UnsupportedBackend(s.to_string())),
        }
    }
}
```

### 工厂函数

```rust
// src/storage/factory.rs

/// Create vector storage backend
pub async fn create_vector_storage(
    backend: VectorStorageBackend,
    config: &VectorStorageConfig,
    workspace_path: &str,
) -> Result<Arc<dyn VectorStorage>, VectorStorageError> {
    match backend {
        VectorStorageBackend::Qdrant => {
            let qdrant_config = config.qdrant.clone()
                .ok_or_else(|| VectorStorageError::Config("Qdrant config not provided".into()))?;
            
            let client = QdrantClient::new(qdrant_config, workspace_path)?;
            Ok(Arc::new(client))
        }
        
        VectorStorageBackend::Milvus => {
            let milvus_config = config.milvus.clone()
                .ok_or_else(|| VectorStorageError::Config("Milvus config not provided".into()))?;
            
            let client = MilvusClient::new(milvus_config, workspace_path)?;
            Ok(Arc::new(client))
        }
        
        VectorStorageBackend::Weaviate => {
            let weaviate_config = config.weaviate.clone()
                .ok_or_else(|| VectorStorageError::Config("Weaviate config not provided".into()))?;
            
            let client = WeaviateClient::new(weaviate_config, workspace_path)?;
            Ok(Arc::new(client))
        }
    }
}
```

## 配置设计

### 配置结构

```rust
/// Vector storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorStorageConfig {
    /// Active backend
    pub backend: VectorStorageBackend,
    
    /// Qdrant configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qdrant: Option<QdrantConfig>,
    
    /// Milvus configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milvus: Option<MilvusConfig>,
    
    /// Weaviate configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weaviate: Option<WeaviateConfig>,
}

/// Milvus configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilvusConfig {
    /// Milvus server URL
    pub url: String,
    
    /// API key for authentication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    
    /// Vector dimension
    pub vector_size: usize,
    
    /// Distance metric
    pub distance_metric: DistanceMetric,
    
    /// Index type: IVF_FLAT, IVF_SQ8, HNSW, etc.
    pub index_type: MilvusIndexType,
    
    /// Index parameters
    pub index_params: IndexParams,
    
    /// Request timeout in milliseconds
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    
    /// Whether the client is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}
```

### 配置文件示例

```toml
# Vector storage configuration
[database.vector_storage]
# Backend type: qdrant, milvus, weaviate
backend = "qdrant"

# Qdrant configuration
[database.vector_storage.qdrant]
url = "http://localhost:6333"
api_key = ""
vector_size = 768
distance_metric = "Cosine"
timeout_ms = 30000
enabled = true
preset = "Medium"

# Milvus configuration (when backend = "milvus")
[database.vector_storage.milvus]
url = "http://localhost:19530"
api_key = ""
vector_size = 768
distance_metric = "Cosine"
index_type = "HNSW"
timeout_ms = 30000
enabled = true

[database.vector_storage.milvus.index_params]
# HNSW parameters
m = 16
ef_construction = 256

# Weaviate configuration (when backend = "weaviate")
[database.vector_storage.weaviate]
url = "http://localhost:8080"
api_key = ""
vector_size = 768
timeout_ms = 30000
enabled = true
```

## 集成方案

### StorageCoordinator 更新

```rust
// src/orchestrator/index/storage_coordinator.rs

use crate::storage::traits::VectorStorage;

pub struct StorageCoordinator {
    /// Vector storage backend (trait object)
    vector_storage: Option<Arc<dyn VectorStorage>>,
    
    /// BM25 full-text search client
    bm25: Option<Arc<tokio::sync::Mutex<Bm25Client>>>,
    
    /// Embedder for vector generation
    embedder: Option<Arc<Embedder>>,
    
    /// SQLite metadata store
    metadata_store: Option<Arc<SqliteDatabase>>,
}

impl StorageCoordinator {
    /// Create a new storage coordinator
    pub fn new() -> Self {
        Self {
            vector_storage: None,
            bm25: None,
            embedder: None,
            metadata_store: None,
        }
    }
    
    /// Set vector storage backend
    pub fn with_vector_storage(mut self, storage: Arc<dyn VectorStorage>) -> Self {
        self.vector_storage = Some(storage);
        self
    }
    
    /// Check if storage is configured
    pub fn is_configured(&self) -> bool {
        self.vector_storage.as_ref().map(|s| s.is_configured()).unwrap_or(false)
            || self.bm25.is_some()
    }
    
    /// Store vectors from chunked results
    pub async fn store_vectors_batched(
        &self,
        chunks: &[ChunkedResult],
        batch_size: usize,
        batch_delay_ms: u64,
    ) -> Result<usize, OrchestratorError> {
        let vector_storage = match &self.vector_storage {
            Some(vs) => vs,
            None => {
                tracing::warn!("Vector storage not configured, skipping");
                return Ok(0);
            }
        };
        
        // ... 使用 vector_storage trait 方法
    }
}
```

### 初始化流程

```rust
// src/main.rs 或 src/api/handlers/mod.rs

async fn initialize_storage(config: &AppConfig) -> Result<StorageCoordinator, Error> {
    let mut coordinator = StorageCoordinator::new();
    
    // 创建向量存储后端
    if config.database.vector_storage.enabled {
        let vector_storage = create_vector_storage(
            config.database.vector_storage.backend,
            &config.database.vector_storage,
            &config.workspace_path,
        ).await?;
        
        coordinator = coordinator.with_vector_storage(vector_storage);
    }
    
    // 创建 BM25 客户端
    if config.database.bm25.enabled {
        let bm25 = Bm25Client::new(config.database.bm25.clone())?;
        coordinator = coordinator.with_bm25(Arc::new(tokio::sync::Mutex::new(bm25)));
    }
    
    // 创建 SQLite 元数据存储
    let sqlite = SqliteDatabase::new(&config.database.sqlite)?;
    coordinator = coordinator.with_metadata_store(Arc::new(sqlite));
    
    Ok(coordinator)
}
```

## 性能考虑

### 批量操作优化

所有后端都应支持批量操作：

```rust
// 批量 upsert
async fn upsert_points(&self, points: &[VectorPoint]) -> Result<(), VectorStorageError> {
    // 分批处理，避免单次请求过大
    const MAX_BATCH_SIZE: usize = 1000;
    
    for batch in points.chunks(MAX_BATCH_SIZE) {
        self.upsert_batch(batch).await?;
    }
    
    Ok(())
}
```

### 连接池管理

```rust
pub struct VectorStoragePool {
    clients: Vec<Arc<dyn VectorStorage>>,
    current: AtomicUsize,
}

impl VectorStoragePool {
    pub fn get_client(&self) -> Arc<dyn VectorStorage> {
        let idx = self.current.fetch_add(1, Ordering::Relaxed) % self.clients.len();
        self.clients[idx].clone()
    }
}
```

### 指标收集

```rust
// 在 trait 实现中记录指标
async fn search(&self, query: SearchQuery) -> Result<Vec<SearchResult>, VectorStorageError> {
    let start = Instant::now();
    let result = self.search_internal(query).await;
    let latency_ms = start.elapsed().as_millis() as u64;
    
    self.metrics.record_operation(
        StorageOperation::Search,
        result.is_ok(),
        latency_ms,
    );
    
    result
}
```

## 测试策略

### Trait 测试

```rust
// tests/vector_storage_trait_test.rs

/// Generic test for VectorStorage trait
pub async fn test_vector_storage_trait<S: VectorStorage>(storage: &S) {
    // 测试初始化
    let created = storage.initialize().await.expect("Failed to initialize");
    assert!(storage.is_configured());
    
    // 测试 upsert
    let points = vec![create_test_point()];
    storage.upsert_points(&points).await.expect("Failed to upsert");
    
    // 测试 search
    let query = SearchQuery::new(vec![0.1; 768], 10);
    let results = storage.search(query).await.expect("Failed to search");
    assert!(!results.is_empty());
    
    // 测试删除
    storage.delete_by_file_path("test.rs").await.expect("Failed to delete");
}
```

### 后端特定测试

```rust
// tests/qdrant_integration_test.rs
#[tokio::test]
async fn test_qdrant_backend() {
    let config = QdrantConfig::default();
    let client = QdrantClient::new(config, "/test").expect("Failed to create client");
    test_vector_storage_trait(&client).await;
}

// tests/milvus_integration_test.rs
#[tokio::test]
async fn test_milvus_backend() {
    let config = MilvusConfig::default();
    let client = MilvusClient::new(config, "/test").expect("Failed to create client");
    test_vector_storage_trait(&client).await;
}
```

## 迁移路径

### 阶段 1：抽象层引入（低风险）

1. 创建 `traits.rs` 定义 `VectorStorage` trait
2. 创建 `error.rs` 定义统一错误类型
3. 为 `QdrantClient` 实现 trait
4. 编写 trait 测试

**影响范围**：仅新增文件，不修改现有代码

### 阶段 2：工厂模式（中风险）

1. 创建 `factory.rs` 实现工厂函数
2. 更新配置结构支持多后端
3. 更新 `StorageCoordinator` 使用 trait
4. 更新初始化流程

**影响范围**：修改 `StorageCoordinator`，需要充分测试

### 阶段 3：新后端实现（低风险）

1. 实现 Milvus 后端
2. 实现 Weaviate 后端
3. 编写集成测试
4. 更新文档

**影响范围**：仅新增模块，不影响现有功能

### 阶段 4：文档与工具（低风险）

1. 更新配置文档
2. 编写迁移指南
3. 提供性能对比工具
4. 更新架构图

## 风险与缓解

### 风险 1：性能下降

**风险**：trait object 动态分发可能带来性能损失

**缓解**：
- 使用 `Arc<dyn VectorStorage>` 而非 `Box<dyn VectorStorage>`，减少分配
- 在热路径使用泛型而非 trait object
- 性能基准测试验证

### 风险 2：功能差异

**风险**：不同后端功能不完全一致

**缓解**：
- 定义核心 trait 包含所有后端共同功能
- 使用 `AdvancedVectorStorage` trait 扩展可选功能
- 文档明确标注各后端支持的功能

### 风险 3：配置复杂度

**风险**：多后端配置增加复杂度

**缓解**：
- 提供配置验证和默认值
- 提供配置迁移工具
- 详细的配置文档和示例

## 总结

本设计方案通过引入 `VectorStorage` trait 抽象层，实现了向量存储后端的可扩展性，同时保持了现有 Qdrant 实现的稳定性。主要优势：

1. **最小侵入**：不修改现有 Qdrant 实现
2. **渐进迁移**：分阶段实施，降低风险
3. **统一接口**：简化上层使用
4. **易于扩展**：添加新后端只需实现 trait

通过工厂模式和配置驱动，用户可以轻松切换不同的向量存储后端，满足不同场景的需求。
