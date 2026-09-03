# 健康监控与重试队列 API

## 健康检查

### GET /api/health

统一健康检查，聚合所有外部服务的健康状态。

#### 请求

**方法**: `GET`

#### 响应

**Content-Type**: `application/json`

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

#### 字段说明

| 字段 | 类型 | 说明 |
|------|------|------|
| `healthy` | bool | 所有关键服务是否都可用 |
| `qdrant.reachable` | bool | Qdrant 向量数据库是否可访问 |
| `qdrant.message` | string | 详细描述 |
| `bm25.reachable` | bool | BM25 全文搜索是否可用 |
| `bm25.message` | string | 详细描述 |
| `embedding.reachable` | bool | Embedding 服务是否可用 |
| `embedding.message` | string | 详细描述 |

#### 示例

```bash
curl "http://localhost:9000/api/health"
```

---

### GET /api/health/qdrant

Qdrant 详细诊断，包含断路器状态、集合信息等。

#### 请求

**方法**: `GET`

#### 响应

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

#### 字段说明

| 字段 | 类型 | 说明 |
|------|------|------|
| `healthy` | bool | Qdrant 是否可用 |
| `circuit_breaker` | string | 断路器状态: `closed`(正常), `open`(拒绝请求), `half-open`(测试恢复) |
| `diagnostic.reachable` | bool | Qdrant 服务是否可连接 |
| `diagnostic.version` | string\|null | Qdrant 版本号 |
| `diagnostic.collection_exists` | bool | 目标集合是否存在 |
| `diagnostic.points_count` | u64 | 集合中的向量数量 |
| `diagnostic.error` | string\|null | 错误信息（如果有） |

#### 示例

```bash
curl "http://localhost:9000/api/health/qdrant"
```

---

### GET /api/health/embedding

Embedding 服务健康状态，查看各 provider 的健康情况。

#### 请求

**方法**: `GET`

#### 响应

```json
{
  "healthy": true,
  "provider_count": 2,
  "providers": [
    {
      "provider_id": "openai-embed",
      "healthy": true
    },
    {
      "provider_id": "siliconflow-embed",
      "healthy": false
    }
  ]
}
```

#### 字段说明

| 字段 | 类型 | 说明 |
|------|------|------|
| `healthy` | bool | 至少一个 provider 是否健康 |
| `provider_count` | number | 配置的 provider 总数（主+备） |
| `providers[].provider_id` | string | Provider 标识 |
| `providers[].healthy` | bool | 该 provider 是否健康 |

#### 示例

```bash
curl "http://localhost:9000/api/health/embedding"
```

---

### GET /api/health/bm25

BM25 全文搜索索引健康状态。

#### 请求

**方法**: `GET`

#### 响应

```json
{
  "enabled": true,
  "connected": true,
  "index_path": null
}
```

#### 字段说明

| 字段 | 类型 | 说明 |
|------|------|------|
| `enabled` | bool | BM25 是否启用 |
| `connected` | bool | 索引是否已连接 |
| `index_path` | string\|null | 索引存储路径 |

#### 示例

```bash
curl "http://localhost:9000/api/health/bm25"
```

---

## 重试队列管理

### GET /api/retry-queue

查看重试队列状态。

#### 请求

**方法**: `GET`

#### 响应

```json
{
  "pending_count": 3,
  "is_empty": false
}
```

#### 字段说明

| 字段 | 类型 | 说明 |
|------|------|------|
| `pending_count` | number | 等待重试的查询数量 |
| `is_empty` | bool | 队列是否为空 |

#### 示例

```bash
curl "http://localhost:9000/api/retry-queue"
```

---

### POST /api/retry-queue/process

手动触发重试队列处理。将所有冷却期已过的查询重新执行。

#### 请求

**方法**: `POST`

#### 响应

```json
{
  "processed": 2,
  "message": "Retry queue processing complete, 2 queries re-attempted"
}
```

#### 字段说明

| 字段 | 类型 | 说明 |
|------|------|------|
| `processed` | number | 本次处理的查询数量 |
| `message` | string | 处理结果描述 |

#### 示例

```bash
curl -X POST "http://localhost:9000/api/retry-queue/process"
```

---

### DELETE /api/retry-queue

清空重试队列，丢弃所有待处理的查询。

#### 请求

**方法**: `DELETE`

#### 响应

```json
{
  "cleared": 3,
  "message": "Retry queue cleared, 3 queries discarded"
}
```

#### 字段说明

| 字段 | 类型 | 说明 |
|------|------|------|
| `cleared` | number | 被清空的查询数量 |
| `message` | string | 操作结果描述 |

#### 示例

```bash
curl -X DELETE "http://localhost:9000/api/retry-queue"
```

---

## 典型运维流程

### 服务中断排查

```bash
# 1. 快速概览
curl http://localhost:9000/api/health

# 2. Qdrant 深度检查
curl http://localhost:9000/api/health/qdrant

# 3. Embedding 多 provider 检查
curl http://localhost:9000/api/health/embedding

# 4. BM25 索引检查
curl http://localhost:9000/api/health/bm25

# 5. 查看是否有查询被阻塞
curl http://localhost:9000/api/retry-queue

# 6. 服务恢复后触发重放
curl -X POST http://localhost:9000/api/retry-queue/process

# 7. 确认队列清空
curl http://localhost:9000/api/retry-queue
```