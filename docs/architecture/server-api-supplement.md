# Server 层 API 补充设计

## 1. 背景

在容错与韧性架构中，我们增加了三层容错模型（Retry with Backoff → Circuit Breaker → Retry Queue）。但用户（HTTP 客户端）无法感知这些机制的运行状态：

- 断路器是否已打开？
- 重试队列中有多少待处理查询？
- 各外部服务（Qdrant、BM25、Embedding）是否健康？
- 如何手动触发重试队列的处理？

现有 `/api/metrics` 提供 Prometheus/JSON 指标，但缺少面向运维的实时健康检查和队列管理能力。

## 2. 分析：需要补充哪些 API

### 2.1 状态监控类

| 场景 | 现有支持 | 需要补充 |
|------|----------|----------|
| 查看所有外部服务是否正常 | `/api/metrics`（间接，需解析 Prometheus 格式） | 统一健康检查端点 |
| Qdrant 详细诊断（版本、集合状态、断路器状态） | 无 | Qdrant 诊断端点 |
| Embedding 服务健康（各 provider 状态） | 无 | Embedding 健康端点 |
| BM25 索引是否可用 | 无 | BM25 健康端点 |
| 重试队列中有多少待处理查询 | 无 | 队列状态端点 |

### 2.2 手动操作类

| 场景 | 现有支持 | 需要补充 |
|------|----------|----------|
| 服务恢复后手动触发重放 | 无 | 重试队列处理端点 |
| 清空重试队列（运维操作） | 无 | 队列清空端点 |

### 2.3 API 清单

```
GET    /api/health              # 统一健康检查（聚合所有服务）
GET    /api/health/qdrant       # Qdrant 详细诊断
GET    /api/health/embedding    # Embedding 服务健康
GET    /api/health/bm25         # BM25 索引健康
GET    /api/retry-queue         # 重试队列状态
POST   /api/retry-queue/process # 手动触发重试队列处理
DELETE /api/retry-queue         # 清空重试队列
```

### 2.4 基础设施层新增方法

| 方法 | 所在模块 | 用途 |
|------|----------|------|
| `QdrantClient::circuit_breaker_state() -> String` | `cce_infrastructure::storage::qdrant` | 返回断路器状态 ("closed"/"open"/"half-open") |
| `OpenAICompatibleProvider::is_healthy() -> bool` | `cce_infrastructure::llm::embedding` | 检查是否有至少一个健康 provider |
| `OpenAICompatibleProvider::provider_count() -> usize` | `cce_infrastructure::llm::embedding` | 返回配置的 provider 数量 |
| `OpenAICompatibleProvider::client_health_states() -> Vec<(String, bool)>` | `cce_infrastructure::llm::embedding` | 返回各 provider 健康状态 |

## 3. 架构设计

```
┌─────────────────────────────────────────────┐
│                 HTTP Client                 │
└────────────────────┬────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────┐
│            cce_server::api::router          │
│                                             │
│  /api/health  →  handlers::health::*        │
│  /api/retry-queue → handlers::health::*     │
└────────────────────┬────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────┐
│          AppState (Arc<CodeContextEngine>)   │
│                                             │
│  engine.qdrant() → QdrantClient             │
│  engine.bm25()  → Bm25Client                │
│  engine.embedder() → OpenAICompatibleProvider│
│  query_coordinator.retry_queue() → RetryQueue│
└─────────────────────────────────────────────┘
```

### 3.1 响应结构设计

**统一健康检查** (`GET /api/health`)：
```json
{
  "healthy": true,
  "qdrant": {
    "reachable": true,
    "message": "Qdrant is reachable and healthy"
  },
  "bm25": {
    "reachable": true,
    "message": "BM25 client: configured"
  },
  "embedding": {
    "reachable": true,
    "message": "Embedding provider is healthy (2 provider(s))"
  }
}
```

**Qdrant 诊断** (`GET /api/health/qdrant`)：
```json
{
  "healthy": true,
  "circuit_breaker": "closed",
  "diagnostic": {
    "reachable": true,
    "version": "1.10.0",
    "collection_exists": true,
    "points_count": 15420,
    "error": null
  }
}
```

**Embedding 健康** (`GET /api/health/embedding`)：
```json
{
  "healthy": true,
  "provider_count": 2,
  "providers": [
    { "provider_id": "openai-embed", "healthy": true },
    { "provider_id": "siliconflow-embed", "healthy": false }
  ]
}
```

**重试队列状态** (`GET /api/retry-queue`)：
```json
{
  "pending_count": 3,
  "is_empty": false
}
```

**触发重试处理** (`POST /api/retry-queue/process`)：
```json
{
  "processed": 2,
  "message": "Retry queue processing complete, 2 queries re-attempted"
}
```

## 4. 修改的文件

| 文件 | 修改类型 | 说明 |
|------|----------|------|
| `crates/cce_infrastructure/src/storage/qdrant/client.rs` | 新增方法 | 添加 `circuit_breaker_state()` |
| `crates/cce_infrastructure/src/llm/services/embedding/provider.rs` | 新增方法 | 添加 `is_healthy()`, `provider_count()`, `client_health_states()` |
| `crates/cce_server/src/api/handlers/health.rs` | 新建文件 | 健康检查和重试队列管理处理器 |
| `crates/cce_server/src/api/handlers/mod.rs` | 修改 | 注册 `health` 模块 |
| `crates/cce_server/src/api/router.rs` | 修改 | 添加 7 个新路由 |

## 5. 现有监控 API 与新 API 的职责划分

| 端点 | 类型 | 用途 |
|------|------|------|
| `/api/metrics` | Prometheus 格式 | 指标采集系统（Grafana/Prometheus） |
| `/api/metrics/json` | JSON | 程序化读取当前指标快照 |
| `/api/metrics/history` | JSON | 历史指标查询 |
| `/api/health` | JSON | **新**：运维人员快速判断系统状态 |
| `/api/health/qdrant` | JSON | **新**：Qdrant 深度诊断 |
| `/api/health/embedding` | JSON | **新**：Embedding 多 provider 健康 |
| `/api/health/bm25` | JSON | **新**：BM25 索引状态 |
| `/api/retry-queue` | JSON | **新**：重试队列管理 |
| `/api/storage/status` | JSON | 存储层状态（已有） |

## 6. 运维流程

### 6.1 服务中断后恢复流程

```
1. 用户收到查询超时/错误
2. 调用 GET /api/health 确认哪个服务不可用
3. 调用 GET /api/health/qdrant 查看断路器和详细诊断
4. 问题修复后，调用 POST /api/retry-queue/process 触发重放
5. 调用 GET /api/retry-queue 确认队列已清空
```

### 6.2 主动排查流程

```
1. GET /api/health — 快速概览
2. 若 qdrant.reachable = false:
   → GET /api/health/qdrant — 查看断路器状态和错误详情
3. 若 embedding.healthy = false:
   → GET /api/health/embedding — 查看哪个 provider 异常
```