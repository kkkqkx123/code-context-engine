# 单集合数据隔离架构设计

## 概述

本文档描述 Qdrant 向量存储的数据隔离架构方案：**使用单一 Collection 配合 Payload 字段进行逻辑隔离**，替代当前多 Collection 隔离模式。

## 现状与问题

### 当前架构

```
每个工作空间独立集合 + 独立 Summary 集合
  cce_<workspace>_<hash>           (chunk 向量)
  cce_<workspace>_<hash>-summary   (文件摘要向量)
```

- 集合名称基于工作空间路径哈希生成（`client.rs:235`）
- 每个项目有独立的 SQLite 元数据库，但向量全部混入同一 Qdrant 集合
- Qdrant payload 只保留最小字段，**不承担元数据查询职责**
- Summary 不再使用独立集合，而是作为同一集合中的 `type`

### 问题

1. **无显式项目隔离**：跨项目数据无法按项目维度过滤，只能靠 `file_path` 隐式区分
2. **双集合开销**：每个工作空间需要维护主集合 + summary 集合
3. **Summary 查询低效**：需先查 summary 集合获得相关文件，再用 `file_path` 后过滤候选结果
4. **SearchFilter 只保留路径隔离与排除规则**：Qdrant 只负责 `group_id`、`type`、`directory_prefix` 和内容排除，`file_extension`、`entity_type`、`language` 这类元数据不再进入向量检索链路

## 目标架构

### 核心原则

1. **逻辑隔离优于物理隔离**：单集合 + Payload 过滤，避免多集合的固定开销（空集合约 350MB）
2. **索引不会被"污染"**：HNSW 全局图是特性而非问题，过滤条件保证查询结果纯度
3. **单集合统一管理**：不再区分主集合和 Summary 集合

### 最终架构

```
单一集合: cce_vectors

Payload 结构:
{
  "source_id":    "group_9_emb_0",
  "file_path":    "src/main.rs",
  "group_id":     "proj_a1b2c3d4",    // 项目/租户标识
  "type":         "chunk"             // "chunk" | "summary"
}
```

### 集合配置

```json
PUT /collections/cce_vectors
{
  "vectors": { "size": 768, "distance": "Cosine" },
  "hnsw_config": {
    "m": 16,
    "payload_m": 16,
    "ef_construct": 256,
    "on_disk": true
  }
}
```

- `m: 16`：全局 HNSW 连接数，保证图连通性
- `payload_m: 16`：同组数据额外连接，优化过滤查询性能
- **不使用 `m: 0`**：全局图不会被"污染"，跨组连接增强图结构；过滤精度由 `group_id` filter 保障

### Payload 索引

```json
POST /collections/cce_vectors/index
{
  "field_name": "group_id",
  "field_type": "keyword",
  "is_tenant": true
}
```

`is_tenant: true` 启用 Qdrant 租户优化，使同组数据在物理存储上更紧凑。

## 数据模型变更

### Payload 结构（`types.rs`）

```rust
pub struct Payload {
    pub source_id: String,
    pub file_path: String,
    pub group_id: Option<String>,
    pub r#type: Option<String>,
}
```

### SearchFilter（`vector_retrieval.rs`）

```rust
pub struct SearchFilter {
    pub group_id: Option<String>,
    pub point_type: Option<String>,
    pub directory_prefix: Option<String>,
    pub exclude_test: bool,
    pub exclude_generated: bool,
    pub exclude_vendor: bool,
    pub raw_filter: Option<serde_json::Value>,
}
```

## 模块变更

### QdrantClient（`client.rs`）

- 移除 `summary_collection_name` 字段
- 移除 `ensure_summary_collection()`、`upsert_summary_points()`、`search_summaries()`、`search_summaries_with_paths()` 方法
- 集合名改为固定值 `"cce_vectors"`，不再基于路径生成
- `generate_collection_name()` 改为返回 `"cce_vectors"`（向后兼容可保留旧逻辑但不再使用）

### SummaryOperations（`operations.rs`）

**整体移除**，其功能合并入主集合操作：

- Summary 点存储：`type = "summary"`
- Summary 搜索：使用 `{"type": "summary"}` filter 搜索，结果通过 `group_id` filter 限制项目范围
- 摘要文本不再写入 Qdrant payload，需要时应从 SQLite 或其他存储回填

### CollectionOperations（`operations.rs`）

- `create_with_config()` 添加 `payload_m` 参数支持
- 发送到 Qdrant 时包含 `hnsw_config.payload_m`

### 检索层（`retrieval.rs`）

- `build_filter()` 添加 `group_id` 条件：`must: [{key: "group_id", match: {value: group_id}}]`
- Summary 过滤：搜索时按需添加 `{key: "type", match: {value: "summary"}}` 或 `{key: "type", match: {value: "chunk"}}`
- 只保留路径相关过滤和内容排除规则，其他元数据查询走 SQLite，不再由 Qdrant 处理

### HNSW 配置（`config.rs` / `project.rs`）

- `HnswConfig` 添加 `payload_m: Option<u32>` 字段
- 预设调整：`small()/medium()/large()` 中 `payload_m` 值与 `m` 保持一致
- `HnswConfigOverride` 添加 `payload_m: Option<u32>`

### StorageCoordinator（`storage_coordinator.rs`）

- `initialize_qdrant()` 移除 `ensure_summary_collection()` 调用
- `store_summaries()` 改为向主集合写入 `type: "summary"` 的点
- `remove_file_from_summary()` 改为通过 `file_path + type: "summary"` filter 删除
- Qdrant payload 不再携带实体类型、文件扩展名、语言、摘要文本等字段，后续如需元数据查询从 SQLite 回填

### SummaryBoost（`summary.rs`）

- 改为向主集合搜索 `type: "summary"` 的点
- 搜索时带 `group_id` filter + `type: "summary"` filter
- 仍按 `file_path` 后过滤候选结果

## 搜索流程对比

### 当前

```
Query → embed → search chunks → candidate chunks
  + Query → embed → search summary collection → summary candidates
    → filter by candidate file_paths → boost contributions
```

### 改造后

```
Query → embed → search with {group_id, type: "chunk"} → candidate chunks
  + Query → embed → search with {group_id, type: "summary"} → summary candidates
    → filter by candidate file_paths → boost contributions
```

## 数据迁移

### 策略

全局单集合迁移，不再为每个项目独立迁移。

```
1. 导出 cce_<workspace>_<hash> 集合所有点 (scroll_all)
2. 导出 cce_<workspace>_<hash>-summary 集合所有点
3. 创建新集合 cce_vectors (payload_m 配置 + payload 索引)
4. 导入 chunk 点 →  补全 group_id、type="chunk"
5. 导入 summary 点 →  补全 group_id、type="summary"
6. 删除旧集合
7. 重建 HNSW 索引
```

### group_id 生成

```rust
fn generate_group_id(workspace_path: &str) -> String {
    let hash = calculate_hash(workspace_path.as_bytes());
    format!("proj_{}", &hash[..12])
}
```

## 验证结果

基于 Qdrant 1.16.2 实测：

| 测试项 | 结果 |
|---|---|
| `m: 16` 作为 HNSW 参数 | ✅ 接受 |
| `payload_m: 16`（整数语法） | ✅ 接受 |
| `payload_m: {"group_id": 16}`（对象语法） | ❌ 仅支持 `usize` 整数 |
| 集合配置写入后确认 | `m: 16`, `payload_m: 16`, `ef_construct: 100` |
| 带 `group_id` filter 的搜索 | ✅ 正常工作 |

## 注意事项

1. **`payload_m` 只接受整数**：Qdrant 1.16 不支持对象语法 (`{"field": value}`)，当前 `payload_m` 值会应用于所有 payload 字段的 HNSW 子图构建
2. **`full_scan_threshold` 默认 10000**：集合点数不足此值时使用全扫描，`payload_m` 子图索引在超过阈值后生效
3. **数据量较大时建议 `m=4` 配合 `payload_m=16`**：若需严格限制跨组连接，可将全局 `m` 降至最小值 `4`，组内 `payload_m` 保持 `16`
4. **索引重建**：`payload_m` 变更需要重建索引才能生效
