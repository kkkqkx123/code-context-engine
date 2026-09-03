# LLM 重试与限流机制

## 1. 总览

所有 LLM 请求（embedding / chat / rerank）统一走 `HttpLlmClient` → `HttpRequestService` 的请求链路，重试与限流在该链路的三个层次上协作：

```
┌──────────────────────────────────────────────────────────────┐
│                     HttpLlmClient（每模型一个）                │
│  ┌────────────────────────────────────────────────────────┐  │
│  │ HttpRequestService（每模型一个）                        │  │
│  │  ├─ RetryPolicy        指数退避 + retry-after + jitter  │  │
│  │  ├─ CircuitBreaker     快速失败（按 base_url 共享）     │  │
│  │  ├─ ConfigurableRateLimiter（按 base_url 共享）         │  │
│  │  │   ├─ 自适应窗口 429 retry-after（被动）              │  │
│  │  │   └─ token bucket  rate_limit/分（主动）             │  │
│  │  └─ 429 解析：retry-after（秒 / HTTP-date）             │  │
│  └────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────┘
        │
        ▼
   LLM 上游 API（OpenAI 兼容 / 本地服务）
```

- **重试**：`RetryPolicy`（`crates/cce_infrastructure/src/llm/core/retry.rs`）
- **限流**：`ConfigurableRateLimiter` / `RateLimiter` / `TokenBucket`（`rate_limiter.rs`）
- **熔断**：`CircuitBreaker`（`circuit_breaker.rs`，与 Qdrant 客户端共用同一实现）
- **HTTP 与 429 处理**：`HttpRequestService`（`http_service.rs`）
- **限流器/熔断器共享**：`LlmRateLimiterRegistry`（`rate_limiter_registry.rs`）

## 2. 错误分类

`LlmError`（`crates/cce_core/src/llm/error.rs`）统一表达 API 错误，分类决定是否重试：

| 错误类型 | 触发场景 | 是否重试 |
| -------- | -------- | -------- |
| `RateLimitExceeded(retry_after_ms)` | 429，携带服务端 retry-after | 是（默认，可关闭） |
| `Http`（传输层） | 未收到 HTTP 响应：连接拒绝/重置、读响应失败 | 是（传输层故障均为瞬时） |
| `HttpStatus { status, .. }` | 5xx | 是（按状态码类型判定） |
| `HttpStatus { status, .. }` | 4xx（如 400/405/422） | 否（客户端错误，重试无意义） |
| `InvalidResponse` | 响应解析失败、维度/索引不符 | 是 |
| `Timeout` | 请求超时 | 是 |
| `Config` / `InvalidInput` / `Auth` / `ModelNotFound` / `TokenLimitExceeded` / `Internal` / `Api` | 配置、鉴权、模型不存在、其他 4xx 等 | 否（重试无意义） |

列举的是「重试」倾向。熔断失败计数（见 §6）复用同一分类，但仅将瞬时性故障（`Http` / `Timeout` / `InvalidResponse`）计入熔断失败，429、永久性错误不计入。

## 3. RetryPolicy

### 3.1 重试预算

不同错误类别使用独立预算，429 的长窗口不会消耗 5xx 的短预算：

| 参数 | 默认值 | 说明 |
| ---- | ------ | ---- |
| `max_retries` | 5（来自配置） | 普通可重试错误的预算（总尝试 = 预算 + 1） |
| `rate_limit_max_retries` | 20 | 429 独立预算 |
| `initial_delay_ms` | 1000（来自配置 `retry_delay_ms`） | 退避初值 |
| `max_delay_ms` | 30000 | 普通退避上限 |
| `rate_limit_max_delay_ms` | 60000 | 429 退避上限 |
| `backoff_multiplier` | 2.0 | 指数退避倍数 |
| `jitter_ratio` | 0.2 | 随机抖动比例 |

### 3.2 退避计算

```
普通错误：delay = min(backoff(attempt), max_delay_ms) × (1 + U(0, jitter_ratio))
429：     delay = min(max(backoff(attempt), retry_after), rate_limit_max_delay_ms)
                   × (1 + U(0, jitter_ratio))
```

要点：

- 429 的等待至少覆盖服务端 `retry-after`，不再出现「先睡退避、进限流器再等一遍」的双重等待；
- jitter 为加性随机（0~20%），破坏多任务完全同步的重试节奏；
- 429 独立预算与 jitter 有配置入口（`retry_jitter` / `rate_limit_max_retries` / `rate_limit_max_delay_ms`，见 §8），亦可经 `RetryPolicy::with_rate_limit_budget(retries, max_delay_ms)` / `with_jitter_ratio` 在代码级调整。

### 3.3 执行循环

`RetryPolicy::execute` / `execute_with_handler` 内逐次调用单次请求函数：失败 → 判 `should_retry` → 按错误类别取预算 → 超预算即返回错误，否则按 3.2 睡眠后重试。预算内到达成功即返回。

预算耗尽时 `tracing::warn!(error_code, attempts, total_wait_ms, error)` 记录累计尝试次数与总等待时长；`execute_observed` 变体在传入 `LlmRetryMetrics` 时同时累计指标（`llm_retry_total` / `llm_retry_wait_ms_total` / `llm_retry_exhausted_total` / `llm_retry_failures_total`）。

## 4. 限流器

### 4.1 双层结构

`ConfigurableRateLimiter` 组合两层限流，`wait()` 的调用顺序（重要）：

1. **自适应窗口（被动）**：429 后阻塞到 `reset_time`（429 时刻 + retry-after）；
2. **冷却（被动）**：连续 429 达阈值（默认 5 次）后额外冷却 30 秒；
3. **token bucket（主动）**：按 `rate_limit` 次/分 补充令牌，令牌不足时按补充速率等待。

顺序的设计意图：429 窗口结束时释放的并发任务会被 token bucket 以配置速率重新串行化，避免齐射；若先消费令牌再等窗口，释放瞬间会集体打向上游。

### 4.2 错峰释放

`RateLimiter::wait()` 中每个等待任务本地计算「剩余窗口 + 随机错峰偏移」（`max_stagger_ms`，默认 1000ms，可 `with_stagger` 调整），各自睡眠后重新校验窗口；窗口过期后由任意任务在写锁保护下惰性清理状态。因此：

- 429 后请求以「窗口剩余 + 随机偏移 + token 速率」三级错峰恢复；
- 等待期间窗口被新的 429 刷新（`set_rate_limit`）时，等待者会继续等新窗口；
- 清理与刷新均持 `reset_time` 写锁，不存在「清理与设置互相覆盖」的竞态。

### 4.3 限流器共享

`LlmRateLimiterRegistry` 以 **base_url** 为键缓存 `Arc<ConfigurableRateLimiter>`，同 provider 的 embedding / chat / rerank 客户端共用同一限流器（工厂 `build_client_with_endpoints` 注入）。

同一 base_url 的多个 provider（或热更新改配置）以「所有引用方配置的最小非零 `rate_limit`」为最终生效速率：`limiter_for` 已存在实例时调用 `update_rate_limit(min(当前, 新))`，新实例按传入速率创建；`rate_limit = 0`（不限速）不参与 min 计算（保留既有实例速率）。启动时 `validate_dependencies` 对同 base_url 速率不一致的配置产出警告（字段 `llm.providers`），提示最终生效速率。

## 5. 429 完整流程

```
请求发送 → 429
  ├─ 解析 retry-after（整数秒 / HTTP-date，缺失或非法默认 5s）
  ├─ limiter.on_rate_limit(retry_after)  ← 开启全局自适应窗口 + 错误计数 +1
  ├─ 返回 LlmError::RateLimitExceeded(retry_after)
  └─ RetryPolicy：
       ├─ 计入 429 独立预算（默认 5 次）
       ├─ 睡眠 max(backoff, retry_after) + jitter
       └─ 重试请求前 limiter.wait() 再次校验窗口（此时通常已过期）
成功后 limiter.on_success() 重置自适应状态与错误计数。
```

## 6. 熔断

`CircuitBreaker`（`circuit_breaker.rs`，与 Qdrant 客户端共用同一实现）在进入 `RetryPolicy` 之前快速失败，避免雪崩式重试。

### 6.1 接线位置

`HttpRequestService::post_json` / `post_raw` 在进入 retry 循环之前检查熔断：

- **Closed** → 放行；成功后 `record_success` 复位失败计数，429 不计入也需复位；
- **Open**（未到恢复超时）→ 直接返回 `LlmError::Api("Circuit breaker is open")`，不进 retry 循环；
- **HalfOpen** → 放行单次探测；成功 → Closed，失败 → 回到 Open。

### 6.2 失败计数规则

| 错误类别 | 是否计入 | 说明 |
| -------- | -------- | ---- |
| `Http` / `Timeout` / `InvalidResponse` | 是 | 反映上游不可用/瞬时可恢复故障 |
| `RateLimitExceeded` | 否 | 正常限流，不触发熔断 |
| `Auth` / `ModelNotFound` / `Config` / `TokenLimitExceeded` | 否 | 永久性错误，重试无意义 |

### 6.3 共享与配置

熔断器与限流器同粒度（**按 base_url 共享**，`LlmRateLimiterRegistry::circuit_breaker_for`，首个注册生效）：

```toml
[llm.providers.<id>.circuit_breaker]
enabled = true
failure_threshold = 5
recovery_timeout_secs = 60
```

| 参数 | 默认值 | 说明 |
| -------- | ------ | ---- |
| `enabled` | true | 是否启用熔断；禁用时行为与旧版本一致 |
| `failure_threshold` | 5 | 连续计入失败次数达到该值即 Open |
| `recovery_timeout_secs` | 60 | Open 后等待多久进入 HalfOpen 放行探测 |

同一 base_url 的多个 provider（或热更新改配置）共享熔断器：任一 provider 连续计入失败会同时熔断同上游的所有 provider。

### 6.4 指标

| 指标 | 类型 | 说明 |
| -------- | ------ | ---- |
| `llm_circuit_breaker_state` | float gauge (0 / 0.5 / 1) | 0 = Closed, 0.5 = HalfOpen, 1 = Open |
| `llm_circuit_breaker_transitions_total` | counter | 状态变化次数 |
| `llm_circuit_breaker_rejections_total` | counter | 被熔断快速拒绝的次数 |

## 7. 批处理级补重试

摘要生成（`crates/cce_parser/src/summary/generator/model_enhanced.rs`）在批处理层额外兜底：

- `generate_batch_impl` / `generate_batch_with_groups_impl` 通过内部 tracked 变体收集「模型增强因 429 失败」的文件；
- 整批并发结束后，对失败文件**顺序补重试一轮**（仍走同一客户端与限流器）；
- 补重试仍失败才降级 rule-based 摘要。

embedding 侧（`crates/cce_orchestrator/src/index/storage_coordinator/vector.rs` `store_vectors_batched`）与摘要侧对称：单批次遇到可重试错误（429 或 5xx，按 `LlmError` 类型化状态码判定）不再 `?` 中止——批次进入内存延迟列表，其余批次继续（共享限流器已全局等待，天然错峰）；全部批次结束后按最长 retry-after 等待一轮后顺序补重试；补重试成功则正常提交，仍失败则仅该批次对应 work unit 保持未提交，由既有 checkpoint / resume 机制在后续轮次重跑；非可重试错误（4xx 等）维持 `?` 中止语义不变。

## 8. 配置项

| 配置 | 默认值 | 位置 | 说明 |
| ---- | ------ | ---- | ---- |
| `max_retries` | 5 | `[llm.providers.*]` | 普通错误重试预算 |
| `retry_delay_ms` | 1000 | `[llm.providers.*]` | 退避初值 |
| `rate_limit` | 60 | `[llm.providers.*]` | 每分钟最大请求数；0 = 不限速；上限 10000（校验） |
| `embedding_batch_delay_ms` | 100 | `[orchestrator]` | embedding 批间固定节流（主动限速的另一道防线） |
| `retry_jitter` | 0.2 | `[llm.providers.*]` | 重试延迟随机抖动比例（追加 0~20%） |
| `rate_limit_max_retries` | 20 | `[llm.providers.*]` | 429 独立重试预算 |
| `rate_limit_max_delay_ms` | 60000 | `[llm.providers.*]` | 429 退避上限 |

429 独立预算（`rate_limit_max_retries` / `rate_limit_max_delay_ms`）与 jitter（`retry_jitter`）均有配置入口，亦可经 `RetryPolicy::with_rate_limit_budget` / `with_jitter_ratio` 在代码级调整。

熔断器配置（位于 `[llm.providers.<id>.circuit_breaker]`）：

```toml
[llm.providers.<id>.circuit_breaker]
enabled = true
failure_threshold = 5
recovery_timeout_secs = 60
```

| 参数 | 默认值 | 说明 |
| -------- | ------ | ---- |
| `enabled` | true | 是否启用熔断；禁用时行为与旧版本一致 |
| `failure_threshold` | 5 | 连续计入失败次数达到该值即 Open |
| `recovery_timeout_secs` | 60 | Open 后等待多久进入 HalfOpen 放行探测 |

## 9. 已知边界与限制

1. **本地服务（Ollama 等）**：建议 `rate_limit = 0` 关闭主动限速，仅保留 429 自适应窗口；
2. **429 不计入熔断失败计数**：熔断器仅将 `Http` / `Timeout` / `InvalidResponse` 计入失败，429 作为正常限流不触发熔断；但成功路径（含 429 恢复后的成功请求）会复位熔断连续失败计数；
3. **同一 base_url 的多个 provider 共享熔断器**：任一 provider 连续瞬时故障会同时熔断同上游的所有 provider（与限流器共享粒度一致）；
4. **熔断器 HalfOpen 恢复节奏**：已 Open 的熔断器恢复超时后进入 HalfOpen 仅放行单次探测；若探测期间持续有 429/失败，状态可能被刷新回 Open，整体恢复节奏受并发请求退避影响；
5. **embedding 批次补重试**：补重试阶段按最长 retry-after 全局睡眠一轮，若补重试仍失败则该批次未提交（work unit 保持未提交，由 checkpoint / resume 重跑），不影响其他批次。
