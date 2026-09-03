# Qdrant 稀疏向量支持更新说明

## 更新日期
2026-05-12

## 概述

本次更新为项目添加了完整的 Qdrant 稀疏向量支持，使 BGE M3 等双模式模型能够充分利用稠密+稀疏的混合检索能力。

## 主要变更

### 1. 数据类型扩展 (`src/storage/qdrant/types.rs`)

#### 新增 `SparseVector` 结构
```rust
pub struct SparseVector {
    pub indices: Vec<u32>,  // Token IDs (sorted)
    pub values: Vec<f32>,   // Weights
}
```

**特性：**
- 自动排序索引（Qdrant 要求）
- 提供 `from_lexical_weights` 构造函数
- 支持空值检查

#### 扩展 `VectorPoint` 结构
```rust
pub struct VectorPoint {
    pub id: String,
    pub vector: Vec<f32>,              // Dense vector
    pub sparse_vector: Option<SparseVector>,  // NEW: Optional sparse vector
    pub payload: Payload,
}
```

**新增方法：**
- `VectorPoint::with_sparse()` - 创建包含稀疏向量的 Point

### 2. 集合配置更新 (`src/storage/qdrant/operations/collection.rs`)

在创建集合时自动添加稀疏向量配置：

```json
{
  "sparse_vectors": {
    "sparse": {
      "index": {
        "on_disk": false
      },
      "modifier": "idf"
    }
  }
}
```

**配置说明：**
- **on_disk: false** - 使用内存索引以获得更好性能
- **modifier: idf** - 启用逆文档频率加权，提升稀有词权重

### 3. Upsert 操作增强 (`src/storage/qdrant/operations/points.rs`)

支持命名向量格式，同时存储稠密和稀疏向量：

```json
{
  "vector": {
    "dense": [0.1, 0.2, 0.3],
    "sparse": {
      "indices": [1, 42],
      "values": [0.22, 0.8]
    }
  }
}
```

**向后兼容：** 如果 `sparse_vector` 为 `None`，则使用传统的单向量格式。

### 4. 搜索功能升级 (`src/storage/qdrant/operations/search.rs`)

#### 扩展 `SearchQuery` 结构
```rust
pub struct SearchQuery {
    pub vector: Vec<f32>,
    pub sparse_vector: Option<SparseVector>,  // NEW
    pub use_hybrid: bool,                      // NEW
    // ... other fields
}
```

**新增方法：**
- `SearchQuery::new_hybrid()` - 创建混合查询

#### 实现混合搜索 (`hybrid_search`)

使用 Qdrant 的 Prefetch + RRF Fusion 机制：

```rust
{
  "prefetch": [
    {
      "query": {"indices": [...], "values": [...]},
      "using": "sparse",
      "limit": 20
    },
    {
      "query": [0.1, 0.2, 0.3],
      "using": "dense",
      "limit": 20
    }
  ],
  "query": {"fusion": "rrf"},
  "limit": 10
}
```

**融合策略：**
- 使用 **RRF (Reciprocal Rank Fusion)** 对多路召回结果进行排序融合
- 每路召回数量 = `max(limit * 2, 20)`

### 5. API 导出更新

- `src/storage/qdrant/mod.rs` - 导出 `SparseVector`
- `src/storage/mod.rs` - 导出 `SparseVector` 到公共 API

## 使用示例

### 创建包含稀疏向量的 Point

```rust
use code_context_engine::storage::{SparseVector, VectorPoint, Payload};

let sparse = SparseVector::from_lexical_weights(
    vec![101, 2056, 3415],  // token IDs
    vec![0.5, 0.3, 0.8],     // weights
);

let point = VectorPoint::with_sparse(
    "doc_1",
    vec![0.1, 0.2, 0.3],  // dense vector
    sparse,
    Payload::new("src/main.rs"),
);
```

### 执行混合搜索

```rust
use code_context_engine::storage::{SparseVector, SearchQuery};

let query = SearchQuery::new_hybrid(
    vec![0.1, 0.2, 0.3],           // dense query
    SparseVector::from_lexical_weights(
        vec![101, 2056],
        vec![0.5, 0.8],
    ),
    10,  // top_k
);

let results = client.search(query).await?;
```

## 兼容性说明

### ✅ 向后兼容

- **现有代码无需修改**：如果不使用稀疏向量，行为与之前完全一致
- **集合自动升级**：新创建的集合会自动包含稀疏向量配置
- **查询自动降级**：如果未提供稀疏向量，自动使用传统稠密搜索

### ⚠️ 注意事项

1. **Token ID 转换**：BGE M3 返回的是 `HashMap<String, f32>`（token → weight），需要先通过 tokenizer 转换为 `HashMap<u32, f32>`（token_id → weight）。

2. **已有集合**：已存在的集合不会自动添加稀疏向量配置。如需支持，需要：
   - 删除旧集合并重新创建，或
   - 手动通过 Qdrant API 添加稀疏向量配置

3. **性能影响**：
   - 启用 IDF modifier 会略微增加索引构建时间
   - 混合搜索比单一搜索稍慢，但召回质量更高

## 待完成工作

### 🔧 需要后续实现

1. **Tokenizer 集成**
   - 集成 `tokenizers` crate
   - 实现 BGE M3 tokenizer 加载
   - 提供 `lexical_weights` → `SparseVector` 的自动转换工具

2. **StorageCoordinator 更新**
   - 在 `store_vectors_batched` 中支持稀疏向量
   - 从 `embed_advanced()` 提取稀疏向量并存储

3. **Embedder 增强**
   - `OpenAICompatibleProvider::embed_advanced()` 完整实现
   - 自动调用 tokenizer 转换

4. **测试覆盖**
   - 单元测试：`SparseVector` 排序逻辑
   - 集成测试：混合搜索端到端流程
   - 性能基准测试：稠密 vs 稀疏 vs 混合

## 文档

- **详细指南**: `docs/storage/vector/qdrant/sparse-vector-support.md`
- **使用示例**: `docs/storage/vector/qdrant/sparse-vector-example.rs`

## 参考资料

- [Qdrant Sparse Vectors Documentation](https://qdrant.tech/documentation/concepts/sparse-vectors/)
- [Qdrant Hybrid Search](https://qdrant.tech/documentation/concepts/hybrid-search/)
- [BGE M3 Official Repository](https://github.com/FlagOpen/FlagEmbedding)

---

*更新作者: AI Assistant*
*审核状态: Pending Review*
