# 向量存储后端扩展设计

## 概述

本文档描述如何扩展支持其他向量数据库后端（如 Milvus、Weaviate），同时保持现有架构的清晰性和独立性。

## 设计原则

### 核心原则

1. **职责独立**：每个存储后端独立实现，不强行统一抽象
2. **配置驱动**：通过配置选择使用的向量数据库后端
3. **最小侵入**：不修改现有 Qdrant 实现
4. **接口一致**：保持核心方法签名一致（但不强制 trait）
5. **协调层适配**：在 StorageCoordinator 处理不同后端的差异

### 为什么不使用 Trait 抽象？

1. **职责差异**：Qdrant、BM25、SQLite 职责完全不同，统一抽象是负担
2. **配置差异**：不同向量数据库配置项差异大，统一配置复杂
3. **特性差异**：每个后端有独特功能，trait 会限制灵活性
4. **性能考虑**：避免动态分发的性能损失

## 架构设计

### 模块结构

```
src/storage/
├── mod.rs              # 模块入口
├── metrics.rs          # 性能指标（共享）
│
├── qdrant/             # Qdrant 实现（现有）
│   ├── mod.rs
│   ├── client.rs
│   ├── config.rs
│   ├── error.rs
│   ├── types.rs
│   └── operations/
│
├── milvus/             # Milvus 实现（新增）
│   ├── mod.rs
│   ├── client.rs
│   ├── config.rs
│   ├── error.rs
│   ├── types.rs
│   └── operations/
│
├── weaviate/           # Weaviate 实现（新增）
│   ├── mod.rs
│   ├── client.rs
│   ├── config.rs
│   ├── error.rs
│   ├── types.rs
│   └── operations/
│
├── bm25/               # BM25 实现（现有）
│   └── ...
│
└── sqlite/             # SQLite 实现（现有）
    └── ...
```

### 协调器设计

```rust
// src/orchestrator/index/storage_coordinator.rs

/// 向量存储后端枚举
pub enum VectorBackend {
    Qdrant(Arc<QdrantClient>),
    Milvus(Arc<MilvusClient>),
    Weaviate(Arc<WeaviateClient>),
}

/// 存储协调器
pub struct StorageCoordinator {
    vector_backend: Option<VectorBackend>,         // 向量存储后端
    bm25: Option<Arc<tokio::sync::Mutex<Bm25Client>>>, // BM25 客户端
    embedder: Option<Arc<Embedder>>,                // 嵌入器
    metadata_store: Option<Arc<SqliteDatabase>>,    // SQLite 元数据存储
}
```

## 实现规范

### 1. 客户端结构

每个向量数据库客户端应包含：

```rust
pub struct <Backend>Client {
    config: <Backend>Config,        // 配置
    http_client: reqwest::Client,   // HTTP 客户端
    collection_name: String,        // 集合名称
    base_url: String,               // API 基础 URL
    metrics: StorageMetrics,        // 性能指标（共享）
    
    // 后端特有字段
    // ...
}
```

### 2. 核心方法签名

虽然不强制 trait，但应保持一致的核心方法签名：

```rust
impl <Backend>Client {
    // ==================== 生命周期 ====================
    
    /// 创建客户端
    pub fn new(config: <Backend>Config, workspace_path: &str) -> Result<Self, <Backend>Error>;
    
    /// 初始化集合
    pub async fn initialize(&self) -> Result<bool, <Backend>Error>;
    
    /// 检查集合是否存在
    pub async fn collection_exists(&self) -> Result<bool, <Backend>Error>;
    
    /// 获取集合信息
    pub async fn get_collection_info(&self) -> Result<CollectionInfo, <Backend>Error>;
    
    // ==================== 数据操作 ====================
    
    /// Upsert 向量点
    pub async fn upsert_points(&self, points: &[VectorPoint]) -> Result<(), <Backend>Error>;
    
    /// 按文件路径删除
    pub async fn delete_by_file_path(&self, file_path: &str) -> Result<(), <Backend>Error>;
    
    /// 批量删除
    pub async fn delete_by_file_paths(&self, file_paths: &[&str]) -> Result<(), <Backend>Error>;
    
    /// 清空集合
    pub async fn clear_collection(&self) -> Result<(), <Backend>Error>;
    
    /// 删除集合
    pub async fn delete_collection(&self) -> Result<(), <Backend>Error>;
    
    // ==================== 查询操作 ====================
    
    /// 向量搜索
    pub async fn search(&self, query: SearchQuery) -> Result<Vec<SearchResult>, <Backend>Error>;
    
    // ==================== 索引状态 ====================
    
    /// 检查是否有索引数据
    pub async fn has_indexed_data(&self) -> Result<bool, <Backend>Error>;
    
    /// 标记索引完成
    pub async fn mark_indexing_complete(&self) -> Result<(), <Backend>Error>;
    
    // ==================== 元信息 ====================
    
    /// 获取集合名称
    pub fn collection_name(&self) -> &str;
    
    /// 检查是否启用
    pub fn is_enabled(&self) -> bool;
}
```

### 3. 配置结构

```rust
/// 后端配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct <Backend>Config {
    /// 服务器 URL
    pub url: String,
    
    /// API Key（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    
    /// 向量维度
    pub vector_size: usize,
    
    /// 距离度量
    pub distance_metric: DistanceMetric,
    
    /// 超时时间（毫秒）
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    
    /// 最大重试次数
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    
    /// 是否启用
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    
    // 后端特有配置
    // ...
}

impl <Backend>Config {
    /// 验证配置
    pub fn validate(&self) -> Result<(), String>;
    
    /// 规范化 URL
    pub fn normalized_url(&self) -> String;
}
```

### 4. 错误类型

```rust
/// 后端错误类型
#[derive(Error, Debug)]
pub enum <Backend>Error {
    #[error("Connection error: {0}")]
    Connection(String),
    
    #[error("Collection not found: {0}")]
    CollectionNotFound(String),
    
    #[error("Invalid configuration: {field} - {reason}")]
    InvalidConfig { field: String, reason: String },
    
    #[error("API error: {0}")]
    Api(String),
    
    // ... 其他错误变体
}

impl <Backend>Error {
    /// 判断是否可重试
    pub fn is_retryable(&self) -> bool;
    
    /// 判断是否连接错误
    pub fn is_connection_error(&self) -> bool;
}
```

### 5. 数据类型转换

```rust
// src/storage/<backend>/types.rs

/// 后端特有的点表示
pub struct <Backend>Point {
    pub id: String,
    pub vector: Vec<f32>,
    pub metadata: <Backend>Metadata,
}

impl <Backend>Point {
    /// 从通用 VectorPoint 转换
    pub fn from_vector_point(point: &VectorPoint) -> Self {
        Self {
            id: point.id.clone(),
            vector: point.vector.clone(),
            metadata: <Backend>Metadata::from_payload(&point.payload),
        }
    }
    
    /// 转换为通用 VectorPoint
    pub fn to_vector_point(&self) -> VectorPoint {
        VectorPoint::new(
            &self.id,
            self.vector.clone(),
            self.metadata.to_payload(),
        )
    }
}
```

## Milvus 实现示例

### 配置

```rust
// src/storage/milvus/config.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilvusConfig {
    pub url: String,
    pub api_key: Option<String>,
    pub vector_size: usize,
    pub distance_metric: DistanceMetric,
    
    /// 索引类型：IVF_FLAT, IVF_SQ8, HNSW, etc.
    pub index_type: MilvusIndexType,
    
    /// 索引参数
    pub index_params: IndexParams,
    
    pub timeout_ms: u64,
    pub max_retries: u32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MilvusIndexType {
    IvfFlat,
    IvfSq8,
    Hnsw,
    Annoy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexParams {
    // HNSW 参数
    pub m: Option<u32>,
    pub ef_construction: Option<u32>,
    
    // IVF 参数
    pub nlist: Option<u32>,
}
```

### 客户端

```rust
// src/storage/milvus/client.rs

pub struct MilvusClient {
    config: MilvusConfig,
    http_client: reqwest::Client,
    collection_name: String,
    base_url: String,
    metrics: StorageMetrics,
}

impl MilvusClient {
    pub fn new(config: MilvusConfig, workspace_path: &str) -> Result<Self, MilvusError> {
        config.validate()?;
        
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()?;
        
        let collection_name = Self::generate_collection_name(workspace_path);
        let base_url = config.normalized_url();
        
        Ok(Self {
            config,
            http_client,
            collection_name,
            base_url,
            metrics: StorageMetrics::new(),
        })
    }
    
    pub async fn upsert_points(&self, points: &[VectorPoint]) -> Result<(), MilvusError> {
        // 转换为 Milvus 格式
        let milvus_points: Vec<MilvusPoint> = points
            .iter()
            .map(MilvusPoint::from_vector_point)
            .collect();
        
        // 批量插入
        self.insert_batch(&milvus_points).await?;
        Ok(())
    }
    
    pub async fn search(&self, query: SearchQuery) -> Result<Vec<SearchResult>, MilvusError> {
        // 构建 Milvus 搜索请求
        let milvus_query = MilvusSearchQuery::from(query);
        
        // 执行搜索
        let results = self.execute_search(&milvus_query).await?;
        
        // 转换结果
        Ok(results.into_iter().map(SearchResult::from).collect())
    }
    
    // ... 其他方法
}
```

## StorageCoordinator 适配

### 枚举方式

```rust
// src/orchestrator/index/storage_coordinator.rs

pub enum VectorBackend {
    Qdrant(Arc<QdrantClient>),
    Milvus(Arc<MilvusClient>),
    Weaviate(Arc<WeaviateClient>),
}

impl StorageCoordinator {
    /// 设置向量存储后端
    pub fn with_vector_backend(mut self, backend: VectorBackend) -> Self {
        self.vector_backend = Some(backend);
        self
    }
    
    /// 批量存储向量
    pub async fn store_vectors_batched(
        &self,
        chunks: &[ChunkedResult],
        batch_size: usize,
        batch_delay_ms: u64,
    ) -> Result<usize, OrchestratorError> {
        let backend = match &self.vector_backend {
            Some(b) => b,
            None => return Ok(0),
        };
        
        // 根据后端类型调用相应方法
        match backend {
            VectorBackend::Qdrant(client) => {
                self.store_to_qdrant(client, chunks, batch_size, batch_delay_ms).await
            }
            VectorBackend::Milvus(client) => {
                self.store_to_milvus(client, chunks, batch_size, batch_delay_ms).await
            }
            VectorBackend::Weaviate(client) => {
                self.store_to_weaviate(client, chunks, batch_size, batch_delay_ms).await
            }
        }
    }
    
    async fn store_to_qdrant(
        &self,
        client: &QdrantClient,
        chunks: &[ChunkedResult],
        batch_size: usize,
        batch_delay_ms: u64,
    ) -> Result<usize, OrchestratorError> {
        // Qdrant 特定的存储逻辑
    }
    
    async fn store_to_milvus(
        &self,
        client: &MilvusClient,
        chunks: &[ChunkedResult],
        batch_size: usize,
        batch_delay_ms: u64,
    ) -> Result<usize, OrchestratorError> {
        // Milvus 特定的存储逻辑
    }
}
```

### 配置驱动

```rust
// src/config/database.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// 向量存储后端类型
    pub vector_backend: VectorBackendType,
    
    /// Qdrant 配置
    pub qdrant: QdrantConfig,
    
    /// Milvus 配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milvus: Option<MilvusConfig>,
    
    /// Weaviate 配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weaviate: Option<WeaviateConfig>,
    
    /// BM25 配置
    pub bm25: Bm25Config,
    
    /// SQLite 配置
    pub sqlite: SqliteConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VectorBackendType {
    Qdrant,
    Milvus,
    Weaviate,
}
```

### 初始化

```rust
// src/main.rs 或 src/api/handlers/mod.rs

async fn create_vector_backend(
    config: &DatabaseConfig,
    workspace_path: &str,
) -> Result<VectorBackend, Error> {
    match config.vector_backend {
        VectorBackendType::Qdrant => {
            let client = QdrantClient::new(config.qdrant.clone(), workspace_path)?;
            Ok(VectorBackend::Qdrant(Arc::new(client)))
        }
        
        VectorBackendType::Milvus => {
            let milvus_config = config.milvus.as_ref()
                .ok_or_else(|| Error::Config("Milvus config not provided".into()))?;
            let client = MilvusClient::new(milvus_config.clone(), workspace_path)?;
            Ok(VectorBackend::Milvus(Arc::new(client)))
        }
        
        VectorBackendType::Weaviate => {
            let weaviate_config = config.weaviate.as_ref()
                .ok_or_else(|| Error::Config("Weaviate config not provided".into()))?;
            let client = WeaviateClient::new(weaviate_config.clone(), workspace_path)?;
            Ok(VectorBackend::Weaviate(Arc::new(client)))
        }
    }
}
```

## 配置示例

```toml
# config.toml
[database]
vector_backend = "qdrant"

[database.qdrant]
url = "http://localhost:6333"
vector_size = 768
distance_metric = "Cosine"
timeout_ms = 30000
enabled = true
preset = "Medium"

[database.milvus]
url = "http://localhost:19530"
vector_size = 768
distance_metric = "Cosine"
index_type = "Hnsw"
timeout_ms = 30000
enabled = false

[database.milvus.index_params]
m = 16
ef_construction = 256

[database.weaviate]
url = "http://localhost:8080"
vector_size = 768
timeout_ms = 30000
enabled = false
```

## 迁移路径

### 阶段 1：准备 Milvus 实现

1. 创建 `src/storage/milvus/` 目录
2. 实现配置、错误、类型
3. 实现 MilvusClient 核心方法
4. 编写单元测试

**影响范围**：仅新增模块，不影响现有代码

### 阶段 2：更新协调器

1. 添加 `VectorBackend` 枚举
2. 更新 `StorageCoordinator` 支持枚举
3. 实现后端分发逻辑
4. 编写集成测试

**影响范围**：修改 `StorageCoordinator`，需要充分测试

### 阶段 3：配置支持

1. 添加 `VectorBackendType` 枚举
2. 更新配置结构
3. 实现初始化逻辑
4. 更新文档

**影响范围**：配置系统，需要向后兼容

### 阶段 4：文档与工具

1. 更新配置文档
2. 编写迁移指南
3. 提供性能对比
4. 更新架构图

## 总结

### 设计优势

1. **职责清晰**：每个后端独立实现，职责明确
2. **易于扩展**：添加新后端只需实现客户端和配置
3. **最小侵入**：不修改现有 Qdrant 实现
4. **配置灵活**：通过配置切换后端
5. **性能无损**：避免动态分发的性能损失

### 实现要点

1. **保持一致**：核心方法签名保持一致
2. **类型转换**：在客户端内部处理数据格式转换
3. **错误处理**：统一的错误分类方法
4. **指标共享**：使用共享的 StorageMetrics
5. **充分测试**：单元测试 + 集成测试

### 后续工作

1. 实现 Milvus 后端
2. 实现 Weaviate 后端
3. 性能对比测试
4. 迁移工具开发
5. 文档完善
