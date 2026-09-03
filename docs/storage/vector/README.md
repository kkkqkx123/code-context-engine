# 向量存储模块文档

## 概述

本目录包含向量存储模块的完整文档，涵盖 Qdrant 使用分析和后端扩展设计。

## 文档列表

### 1. Qdrant 使用分析

**文件**: [qdrant-usage-analysis.md](./qdrant-usage-analysis.md)

**内容**:
- 存储架构现状（Qdrant、BM25、SQLite）
- QdrantClient 核心分析
- 使用场景分析（索引、查询、热更新、摘要）
- StorageCoordinator 协调器
- 配置与性能指标
- 错误处理

**适用读者**: 所有开发者

### 2. 后端扩展设计

**文件**: [vector-backend-extension-design.md](./vector-backend-extension-design.md)

**内容**:
- 设计原则（为什么不使用 Trait 抽象）
- 架构设计（枚举方式）
- 实现规范（客户端、配置、错误、类型转换）
- Milvus 实现示例
- StorageCoordinator 适配
- 配置示例
- 迁移路径

**适用读者**: 架构师、后端开发者

### 3. Qdrant 配置指南

**文件**: [qdrant-configuration-guide.md](./qdrant-configuration-guide.md)

**内容**:
- Qdrant 配置详解
- 预设配置说明
- 性能调优
- 监控指标

**适用读者**: 运维工程师、所有用户

### 4. Qdrant 实现分析

**文件**: [qdrant-implementation-analysis.md](./qdrant-implementation-analysis.md)

**内容**:
- Qdrant 实现细节
- 操作模块分析
- 性能优化
- 错误处理

**适用读者**: 开发者

### 5. 单集合数据隔离实施计划

**文件**: [single-collection-data-isolation-plan.md](./single-collection-data-isolation-plan.md)

**内容**:
- 单集合重构的确认方向
- 需要修改的模块和文件
- 未完全确定的关键点
- 分阶段实施顺序
- 验收标准与风险

**适用读者**: 架构师、后端开发者

## 快速导航

### 我想了解...

| 需求 | 推荐文档 | 章节 |
|------|---------|------|
| Qdrant 如何使用 | qdrant-usage-analysis.md | 使用场景分析 |
| 如何添加新后端 | vector-backend-extension-design.md | 实现规范 |
| 为什么不用 Trait | vector-backend-extension-design.md | 设计原则 |
| Qdrant 配置 | qdrant-configuration-guide.md | 配置详解 |
| Qdrant 实现细节 | qdrant-implementation-analysis.md | 实现分析 |

### 我是...

| 角色 | 推荐阅读顺序 |
|------|-------------|
| 架构师 | 1. Qdrant 使用分析 → 2. 后端扩展设计 |
| 开发者 | 1. Qdrant 使用分析 → 2. Qdrant 实现分析 → 3. 后端扩展设计 |
| 运维工程师 | 1. Qdrant 配置指南 → 2. Qdrant 使用分析 |
| 新用户 | 1. Qdrant 配置指南 → 2. Qdrant 使用分析 |

## 核心概念

### 三种存储后端

| 后端 | 职责 | 数据类型 | 访问模式 |
|------|------|---------|---------|
| **Qdrant** | 向量存储与语义搜索 | 高维向量 + Payload | 相似性搜索 |
| **BM25** | 全文搜索 | 文档（标题、内容、关键词） | 关键词搜索 |
| **SQLite** | 元数据存储 | 实体、关系、缓存、映射 | 精确查询、事务 |

**关键点**：三种后端职责完全不同，不应强行统一抽象。Qdrant 只负责向量存储与最小路径隔离字段，元数据查询由 SQLite 承担。

### QdrantClient 核心方法

```rust
// 生命周期
pub fn new(config: QdrantConfig, workspace_path: &str) -> Result<Self, QdrantError>;
pub async fn initialize(&self) -> Result<bool, QdrantError>;

// 数据操作
pub async fn upsert_points(&self, points: &[VectorPoint]) -> Result<(), QdrantError>;
pub async fn delete_by_file_path(&self, file_path: &str) -> Result<(), QdrantError>;

// 查询操作
pub async fn search(&self, query: SearchQuery) -> Result<Vec<SearchResult>, QdrantError>;
```

### StorageCoordinator 协调器

```rust
pub struct StorageCoordinator {
    qdrant: Option<Arc<QdrantClient>>,              // Qdrant 客户端
    bm25: Option<Arc<tokio::sync::Mutex<Bm25Client>>>, // BM25 客户端
    embedder: Option<Arc<Embedder>>,                // 嵌入器
    metadata_store: Option<Arc<SqliteDatabase>>,    // SQLite 元数据存储
}
```

**职责**：
- 协调多后端存储
- 批量处理控制
- 数据一致性维护
- 热更新处理

## 使用场景

### 场景 1：索引流程

```
IndexOrchestrator::index_directory
  → StorageCoordinator::store_vectors_batched
    → QdrantClient::upsert_points
    → SQLite::store_chunk_records
    → SQLite::store_entity_mappings
```

### 场景 2：查询流程

```
QueryCoordinator::search
  → Searcher::search
    → VectorRetrieval::retrieve
      → QdrantClient::search
```

### 场景 3：热更新流程

```
HotUpdateOrchestrator::handle_file_change
  → StorageCoordinator::hot_update_file
    → QdrantClient::delete_by_file_path
    → QdrantClient::upsert_points
```

## 配置示例

```toml
# config.toml
[database.qdrant]
url = "http://localhost:6333"
vector_size = 768
distance_metric = "Cosine"
timeout_ms = 30000
enabled = true
preset = "Medium"
```

## 扩展设计

### 为什么不使用 Trait 抽象？

1. **职责差异**：Qdrant、BM25、SQLite 职责完全不同
2. **配置差异**：不同向量数据库配置项差异大
3. **特性差异**：每个后端有独特功能
4. **性能考虑**：避免动态分发的性能损失

### 扩展方案

使用枚举方式支持多后端：

```rust
pub enum VectorBackend {
    Qdrant(Arc<QdrantClient>),
    Milvus(Arc<MilvusClient>),
    Weaviate(Arc<WeaviateClient>),
}

pub struct StorageCoordinator {
    vector_backend: Option<VectorBackend>,
    // ...
}
```

### 实现规范

新后端需要实现：

1. **客户端结构**：包含配置、HTTP 客户端、集合名称、指标
2. **核心方法**：保持与 QdrantClient 一致的签名
3. **配置结构**：后端特有配置 + 通用配置
4. **错误类型**：统一的错误分类方法
5. **类型转换**：在客户端内部处理数据格式转换

## 性能指标

### Qdrant 性能（默认配置）

| 操作 | 延迟 | 吞吐量 |
|------|------|--------|
| 插入 | 0.12ms/point | 8,000 points/s |
| 搜索 | 5ms | 200 QPS |
| 删除 | 0.05ms/point | 20,000 points/s |

### 优化建议

1. **批量操作**：使用批量接口减少网络开销
2. **预设配置**：根据数据规模选择合适的预设
3. **索引参数**：调整 HNSW 参数优化搜索性能
4. **硬件配置**：使用 SSD 提升磁盘 IO 性能

## 测试

### 运行测试

```bash
# 单元测试
cargo test --lib storage::qdrant

# 集成测试（需要实际 Qdrant 服务）
cargo test --test qdrant_integration -- --ignored

# 性能基准测试
cargo bench --bench vector_storage_benchmark
```

### 测试覆盖

- ✅ 配置验证
- ✅ 客户端创建
- ✅ 集合管理
- ✅ 点操作
- ✅ 搜索功能
- ✅ 错误处理
- ✅ 性能指标

## 监控

### 关键指标

- `storage_operation_duration_ms`: 操作延迟
- `storage_operation_total`: 操作计数
- `storage_collection_size`: 集合大小
- `storage_error_total`: 错误计数

### 监控示例

```rust
// 获取指标
let metrics = storage.get_metrics();
println!("Search latency: {}ms", metrics.search_latency_ms);
println!("Total points: {}", metrics.collection_size);
```

## 故障排查

### 常见问题

1. **连接失败**
   - 检查 Qdrant 服务是否运行
   - 检查 URL 配置是否正确
   - 检查网络连接

2. **性能下降**
   - 检查索引配置
   - 检查向量维度
   - 检查硬件资源

3. **搜索结果不准确**
   - 检查距离度量配置
   - 检查向量生成质量
   - 检查索引参数

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

### 扩展建议

1. **保持独立**：每个后端独立实现
2. **配置切换**：通过配置选择后端
3. **接口一致**：保持核心方法签名一致
4. **充分测试**：单元测试 + 集成测试
