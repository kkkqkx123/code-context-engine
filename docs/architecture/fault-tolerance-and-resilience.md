# 容错与韧性设计

## 核心原则

> **永不静默降级（Never silently degrade）** — 召回路径失败时必须传播错误，不允许用另一路径的结果替代。

### 设计约束

1. **BM25 和向量检索是互补的召回通路，不是彼此的备份**
   - BM25 提供关键词精确匹配
   - 向量检索提供语义相似度匹配
   - 两个信号通过融合算法（fusion）合并，不能互相替代

2. **仅处理服务断联，不处理存储损坏**
   - 假设 Qdrant、Embedding API、BM25 索引数据本身完好
   - 仅处理网络闪断、服务重启、瞬时过载等可恢复场景

3. **重试是唯一的容错手段**
   - 不允许降级（degrade）、不允许回退（fallback）
   - 重试耗尽后暂停（断路器打开），但保留查询进度

---

## 三层容错模型

```
┌──────────────────────────────────────────────────────────┐
│ Layer 3: Retry Queue（跨请求持久化）                       │
│  职责：当重试全部耗尽时，保存查询意图，服务恢复后自动重放      │
│  触发条件：断路器 half-open → 重放队列中的查询               │
├──────────────────────────────────────────────────────────┤
│ Layer 2: Circuit Breaker（跨请求保护）                     │
│  职责：防止级联故障，快速拒绝，给上游服务恢复时间              │
│  状态机：closed → open → half-open → closed               │
│  触发条件：连续 N 次失败 → open；超时 T → half-open         │
├──────────────────────────────────────────────────────────┤
│ Layer 1: Retry with Backoff（请求内重试）                  │
│  职责：对瞬时故障进行多次尝试                               │
│  策略：指数退避 + 抖动（100ms, 200ms, 400ms, ...）          │
│  判定：仅重试 transient 错误（connection refused,          │
│        timeout, 5xx, rate limit）                         │
└──────────────────────────────────────────────────────────┘
```

### 请求生命周期

```
Client 发起查询
    │
    ▼
Coordinator.search()
    │
    ├── [Layer 1] Searcher 内部使用 with_retry() 重试
    │   └── 每次重试间隔指数退避
    │
    ├── 成功 → 返回结果
    │
    └── 失败（所有重试耗尽）
        │
        ├── [Layer 2] CircuitBreaker.record_failure()
        │   └── 如果达到阈值 → 断路器 open
        │
        ├── [Layer 3] RetryQueue.push(query_options)
        │   └── 保存查询参数，等待服务恢复
        │
        └── 向上传播 retryable error
```

### 恢复流程

```
服务恢复（Qdrant/Embedding 重新可用）
    │
    ├── [Layer 2] 断路器超时 → half-open
    │   └── 允许一个探测请求通过
    │       ├── 成功 → closed
    │       └── 失败 → open（继续等待）
    │
    ├── [Layer 3] 断路器 closed → 触发 RetryQueue 重放
    │   └── drain 队列中所有查询，重新提交到 Coordinator
    │
    └── 正常查询继续执行
```

---

## 当前实现的问题

### 问题 1：searcher.rs — BM25 作为回退

```rust
// 错误模式：HybridRecall 中的静默降级
if !vector_ok {
    // 向量路径失败 → 返回纯 BM25 结果（信号已变质）
    return self.post_process_results(bm25_filtered, options).await;
}
if !bm25_ok {
    // BM25 路径失败 → 返回纯向量结果（信号已变质）
    return self.post_process_results(vector_filtered, options).await;
}

// 错误模式：DenseRecall 中降级到 BM25
Err(e) => {
    // 向量检索失败 → 尝试 BM25 替代
    let bm25_strategy = Bm25Strategy::new(self.bm25.clone());
    let bm25_results = bm25_strategy.retrieve(options).await?;
}
```

**问题**：Hybrid 融合的前提是两个独立信号都存在。用一个信号替代另一个，得到的已经不是"融合结果"，而是静默降级的劣质结果。调用方无法感知。

### 问题 2：coordinator.rs — 健康检查主动降级

```rust
// 错误模式：根据健康检查结果禁用召回通路
match self.searcher.qdrant.health().await {
    Ok(false) | Err(_) => {
        degraded.sources.vector = false;
        degraded.sources.summary = false;
    }
}
```

**问题**：

- 健康检查是瞬时快照，不能用来决策禁用整个召回通路
- 和断路器功能重叠但语义错误（断路器有状态机，健康检查没有）
- 健康检查本身也可能因网络抖动失败，导致误禁用

### 问题 3：错误被吞掉

当前所有 BM25 回退路径都通过 `unwrap_or_else` 或 `if !ok` 静默处理了失败，调用方拿到的结果看似正常，但实际上少了关键语义信息。运维无法感知服务异常，故障排查困难。

---

## 修改方案

### 1. searcher.rs — 移除所有 BM25 回退

**HybridRecall 策略**：

```rust
// 并行执行两个召回路径（使用 with_retry）
let (vector_result, bm25_result) = tokio::join!(
    self.with_retry(|| vector_retrieval.retrieve(options)),
    self.with_retry(|| bm25_retrieval.retrieve(options)),
);

// 两个路径都必须成功
let vector_results = vector_result.map_err(|e| {
    QueryError::retryable(format!("Vector path: {}", e)).with_source("qdrant")
})?;
let bm25_results = bm25_result.map_err(|e| {
    QueryError::retryable(format!("BM25 path: {}", e)).with_source("bm25")
})?;

// 两个都成功才能融合
let fused = fuse_hybrid_results(vector_results, bm25_results, &config);
```

**DenseRecall 策略**：

```rust
let results = self.with_retry(|| retrieval.retrieve(options)).await?;
```

### 2. coordinator.rs — 移除健康检查降级

移除 `check_runtime_health` 方法。查询正常执行，如果服务不可用，searcher 返回 retryable error，coordinator 捕获后推入重试队列。

```rust
pub async fn search(&self, options: &QueryOptions) -> Result<QueryResult> {
    match self.searcher.search(options).await {
        Ok(result) => Ok(result),
        Err(e) if e.is_retryable() => {
            // 记录失败，保留进度
            self.retry_queue.push(options.clone()).await;
            // 仍然向上传播错误
            Err(e)
        }
        Err(e) => Err(e),
    }
}
```

### 3. 新增 RetryQueue

```rust
pub struct RetryQueue {
    queue: Vec<QueryOptions>,
    circuit_breaker: Arc<CircuitBreaker>,
}

impl RetryQueue {
    /// 创建重试队列
    pub fn new(circuit_breaker: Arc<CircuitBreaker>) -> Self;

    /// 将失败的查询加入队列
    pub async fn push(&mut self, options: QueryOptions);

    /// 检查断路器状态，如果已 closed 则取出所有待重试的查询
    pub async fn drain_ready(&mut self) -> Vec<QueryOptions>;

    /// 队列长度
    pub fn len(&self) -> usize;
}
```

### 4. 新增通用 with_retry

在 orchestrator 中提供通用重试函数，依赖已有 `RetryPolicy`（cce_infrastructure）。

```rust
/// 通用指数退避重试
pub async fn with_retry<F, Fut, T, E>(
    operation: F,
    max_retries: u32,
    initial_delay: Duration,
) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: Error + IsRetryable,
{
    // 指数退避：delay * 2，最大 30s
    // 仅对 is_retryable() == true 的错误重试
}
```

---

## 需要补充的 API

| API                                           | 所属模块                                  | 说明                        |
| --------------------------------------------- | ----------------------------------------- | --------------------------- |
| `QueryError::retryable(msg)`                  | `cce_orchestrator::query::error`          | 创建 retryable 错误变体     |
| `QueryError::with_source(self, source: &str)` | `cce_orchestrator::query::error`          | 标记错误来源（qdrant/bm25） |
| `RetryQueue` 完整结构                         | `cce_orchestrator::query::retry_queue`    | 重试队列：push/drain/len    |
| `Coordinator::with_retry_queue()`             | `cce_orchestrator::query::coordinator`    | 注入重试队列                |
| `Coordinator::process_retry_queue()`          | `cce_orchestrator::query::coordinator`    | 处理待重试查询（外部触发）  |
| `Searcher::with_retry()`                      | `cce_orchestrator::query::searcher`       | 通用指数退避重试            |
| `IsRetryable` trait                           | `cce_infrastructure` 或 `cce_core::types` | 统一 retryable 判定接口     |
| Qdrant 健康检查端点                           | Qdrant REST API                           | `GET /health`（已有）       |
| Qdrant 就绪检查端点                           | Qdrant REST API                           | `GET /healthz`（已有）      |

**对外暴露的 HTTP API**（详见 [server-api-supplement.md](server-api-supplement.md)）：

| 端点                       | 方法   | 说明                                              |
| -------------------------- | ------ | ------------------------------------------------- |
| `/api/health`              | GET    | 统一健康检查（聚合 Qdrant、BM25、Embedding 状态） |
| `/api/health/qdrant`       | GET    | Qdrant 详细诊断（含断路器状态、集合信息）         |
| `/api/health/embedding`    | GET    | Embedding 服务健康（多 provider 状态）            |
| `/api/health/bm25`         | GET    | BM25 索引健康                                     |
| `/api/retry-queue`         | GET    | 重试队列状态（待处理数量）                        |
| `/api/retry-queue/process` | POST   | 手动触发重试队列处理                              |
| `/api/retry-queue`         | DELETE | 清空重试队列                                      |

---

## 错误分类

| 错误类型             | 是否重试 | 是否影响断路器 | 处理方式                   |
| -------------------- | -------- | -------------- | -------------------------- |
| Connection refused   | 是       | 是             | 指数退避重试，耗尽后 open  |
| Timeout              | 是       | 是             | 同上                       |
| 5xx Server Error     | 是       | 是             | 同上                       |
| Rate Limit (429)     | 是       | 否             | 指数退避重试，不计入断路器 |
| 4xx Client Error     | 否       | 否             | 立即返回，不重试           |
| Auth Error (401/403) | 否       | 否             | 立即返回，不重试           |
| CircuitBreaker Open  | 否       | —              | 直接推入重试队列           |
