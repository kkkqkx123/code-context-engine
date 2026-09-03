# 监控功能扩展设计方案

## 1. 概述

### 1.1 背景

当前项目已实现基础的进度追踪和缓存监控功能，但核心业务模块（如 Embedding、存储、关系构建等）缺乏细粒度的性能和质量指标。本方案旨在扩展监控体系，提升系统可观测性，帮助快速定位性能瓶颈和故障点。

### 1.2 设计原则

- **轻量级**：避免引入重型监控框架，保持现有简洁架构
- **低开销**：使用原子操作和异步记录，最小化对业务流程的影响
- **可扩展**：预留接口支持未来集成 Prometheus 等专业监控系统
- **聚焦核心**：不监控系统资源（CPU/内存），重点关注 LLM 交互和业务逻辑性能

### 1.3 目标

1. 补充核心业务模块的性能指标（延迟、成功率、吞吐量）
2. 统一指标采集接口，便于后续扩展
3. 提供标准化的 API 端点暴露监控数据
4. 为故障诊断和性能优化提供数据支撑

---

## 2. 现状分析

### 2.1 已集成的监控模块

| 模块 | 监控内容 | 实现方式 |
|------|---------|---------|
| ProjectRegistry | 缓存命中率 | MetricsRegistry (Counter) |
| IndexOrchestrator | 索引进度 | ProgressTracker (AtomicUsize) |
| API 层 | 子系统状态聚合 | `/api/metrics` 端点 |
| Qdrant Client | 预留字段 | `_metrics: Arc<()>` (占位符) |
| 全局日志 | 运行状态 | tracing (info/debug/warn/error) |

### 2.2 缺失的监控模块

| 模块 | 缺失指标类型 | 优先级 |
|------|------------|--------|
| Embedding Provider | 延迟、成功率、吞吐量 | P0 |
| BM25 Storage | 查询延迟、索引大小 | P0 |
| Relation Builder | 关系提取数量、构建耗时 | P1 |
| Parser | 解析成功率、错误率 | P1 |
| Summary Generator | 生成延迟、成功率 | P1 |
| Hot Update | 更新延迟、失败率 | P2 |
| Query Engine | 查询延迟、召回率 | P2 |
| 其他缓存模块 | 命中率统计 | P2 |

---

## 3. 架构设计

### 3.1 整体架构

```
┌─────────────────────────────────────────────────┐
│              Business Modules                    │
│  (Embedding, Storage, Parser, Relation, etc.)   │
└──────────────┬──────────────────┬───────────────┘
               │                  │
               ▼                  ▼
    ┌──────────────────┐  ┌──────────────────┐
    │  MetricCollector  │  │ ProgressTracker  │
    │  (新增)           │  │  (现有)          │
    └────────┬─────────┘  └────────┬─────────┘
             │                     │
             ▼                     ▼
    ┌──────────────────────────────────────┐
    │       MetricsRegistry (扩展)         │
    │  - Counters (计数器)                 │
    │  - Gauges (仪表)                     │
    │  - Histograms (直方图) [新增]        │
    └──────────────┬───────────────────────┘
                   │
                   ▼
    ┌──────────────────────────────────────┐
    │      /api/metrics Endpoint           │
    │  (聚合所有子系统指标并返回 JSON)      │
    └──────────────────────────────────────┘
```

### 3.2 核心组件

#### 3.2.1 MetricCollector（新增）

统一的指标采集器，提供类型安全的指标注册和记录接口。

**职责**：
- 管理不同类型的指标（Counter、Gauge、Histogram）
- 提供线程安全的指标更新接口
- 支持标签（Labels）以实现多维度统计

**关键特性**：
- 基于 `Arc<AtomicU64>` 实现高性能计数器
- 使用滑动窗口算法实现直方图（用于延迟统计）
- 支持可选的标签系统（预留扩展）

#### 3.2.2 MetricsRegistry（扩展）

在现有基础上增加 Histogram 支持和批量导出能力。

**新增功能**：
```rust
pub struct MetricsRegistry {
    counters: Arc<RwLock<HashMap<String, Counter>>>,
    gauges: Arc<RwLock<HashMap<String, Gauge>>>,
    histograms: Arc<RwLock<HashMap<String, Histogram>>>, // 新增
}

impl MetricsRegistry {
    // 现有方法
    pub fn counter(&self, name: &str) -> Counter;
    pub fn gauge(&self, name: &str) -> Gauge;
    
    // 新增方法
    pub fn histogram(&self, name: &str, buckets: Vec<f64>) -> Histogram;
    pub fn export_all(&self) -> MetricsSnapshot;
}
```

#### 3.2.3 Histogram（新增）

用于统计延迟分布的直方图指标。

**设计要点**：
- 预定义桶边界（如：1ms, 5ms, 10ms, 50ms, 100ms, 500ms, 1s, 5s）
- 使用原子操作保证线程安全
- 支持百分位数计算（P50, P90, P95, P99）

---

## 4. 模块级监控设计

### 4.1 Embedding Provider 监控

#### 4.1.1 监控指标

| 指标名称 | 类型 | 标签 | 说明 |
|---------|------|------|------|
| `embedding_requests_total` | Counter | `provider`, `status` | 向量化请求总数 |
| `embedding_latency_ms` | Histogram | `provider`, `batch_size` | 向量化延迟分布 |
| `embedding_tokens_total` | Counter | `provider` | 处理的 Token 总数 |
| `embedding_errors_total` | Counter | `provider`, `error_type` | 错误计数 |
| `embedding_batch_size` | Histogram | `provider` | 批处理大小分布 |

#### 4.1.2 实现位置

在 `src/embedding/base.rs` 中扩展 `EmbeddingProvider` trait：

```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError>;
    
    // 新增：获取监控指标
    fn get_metrics(&self) -> Option<Arc<EmbeddingMetrics>>;
}

pub struct EmbeddingMetrics {
    requests: Counter,
    latency: Histogram,
    tokens: Counter,
    errors: Counter,
}
```

#### 4.1.3 集成示例

在 `OpenAICompatibleProvider::embed()` 中：

```rust
async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
    let start = Instant::now();
    
    // 执行向量化
    let result = self.do_embed(texts).await;
    
    let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
    
    // 记录指标
    if let Some(metrics) = &self.metrics {
        metrics.requests.inc();
        metrics.latency.observe(latency_ms);
        
        let token_count = estimate_tokens(texts);
        metrics.tokens.add(token_count as u64);
        
        if result.is_err() {
            metrics.errors.inc();
        }
    }
    
    result
}
```

---

### 4.2 BM25 Storage 监控

#### 4.2.1 监控指标

| 指标名称 | 类型 | 标签 | 说明 |
|---------|------|------|------|
| `bm25_index_size` | Gauge | - | 索引文档数量 |
| `bm25_query_latency_ms` | Histogram | - | 查询延迟分布 |
| `bm25_queries_total` | Counter | `status` | 查询总数 |
| `bm25_index_build_time_ms` | Histogram | - | 索引构建时间 |

#### 4.2.2 实现位置

在 `src/storage/bm25/client.rs` 中添加：

```rust
pub struct Bm25Metrics {
    index_size: Gauge,
    query_latency: Histogram,
    queries: Counter,
    build_time: Histogram,
}

impl Bm25Client {
    pub async fn search(&self, query: &str, top_k: usize) -> Result<Vec<SearchResult>, Bm25Error> {
        let start = Instant::now();
        let result = self.do_search(query, top_k).await;
        
        if let Some(metrics) = &self.metrics {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            metrics.query_latency.observe(latency);
            metrics.queries.inc();
        }
        
        result
    }
}
```

---

### 4.3 Relation Builder 监控

#### 4.3.1 监控指标

| 指标名称 | 类型 | 标签 | 说明 |
|---------|------|------|------|
| `relations_extracted_total` | Counter | `relation_type` | 提取的关系总数 |
| `relation_build_latency_ms` | Histogram | - | 关系构建延迟 |
| `relation_resolution_rate` | Gauge | - | 关系解析成功率 |
| `files_processed_for_relations_total` | Counter | `status` | 处理文件数 |

#### 4.3.2 实现位置

在 `src/relation/index/builder.rs` 中添加：

```rust
pub struct RelationMetrics {
    extracted: Counter,
    build_latency: Histogram,
    resolution_rate: Gauge,
    files_processed: Counter,
}

impl IndexBuilder {
    pub fn build_group_relations(&self, parsed_files: &[ParsedFile], results: &[ProcessingResult]) {
        let start = Instant::now();
        
        // 构建关系逻辑...
        
        if let Some(metrics) = &self.metrics {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            metrics.build_latency.observe(latency);
            metrics.extracted.add(self.index.resolved_relation_count() as u64);
        }
    }
}
```

---

### 4.4 Parser 监控

#### 4.4.1 监控指标

| 指标名称 | 类型 | 标签 | 说明 |
|---------|------|------|------|
| `parse_attempts_total` | Counter | `language`, `status` | 解析尝试次数 |
| `parse_latency_ms` | Histogram | `language` | 解析延迟分布 |
| `parse_errors_total` | Counter | `language`, `error_type` | 解析错误数 |

#### 4.4.2 实现位置

在 `src/parser/mod.rs` 或相关解析器中添加：

```rust
pub struct ParserMetrics {
    attempts: Counter,
    latency: Histogram,
    errors: Counter,
}

impl CodeParser {
    pub fn parse(&self, content: &str, language: &str) -> Result<ParsedFile, ParseError> {
        let start = Instant::now();
        let result = self.do_parse(content, language);
        
        if let Some(metrics) = &self.metrics {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            metrics.attempts.inc();
            metrics.latency_with_label("language", language).observe(latency);
            
            if result.is_err() {
                metrics.errors.inc();
            }
        }
        
        result
    }
}
```

---

### 4.5 Summary Generator 监控

#### 4.5.1 监控指标

| 指标名称 | 类型 | 标签 | 说明 |
|---------|------|------|------|
| `summary_generated_total` | Counter | `status` | 摘要生成总数 |
| `summary_generation_latency_ms` | Histogram | - | 生成延迟分布 |
| `summary_avg_length` | Gauge | - | 平均摘要长度 |

---

### 4.6 Hot Update 监控

#### 4.6.1 监控指标

| 指标名称 | 类型 | 标签 | 说明 |
|---------|------|------|------|
| `hot_update_triggered_total` | Counter | `trigger_type` | 触发次数 |
| `hot_update_latency_ms` | Histogram | - | 更新延迟分布 |
| `hot_update_failures_total` | Counter | `module`, `error_type` | 失败次数 |

---

### 4.7 Query Engine 监控

#### 4.7.1 监控指标

| 指标名称 | 类型 | 标签 | 说明 |
|---------|------|------|------|
| `query_total` | Counter | `query_type`, `status` | 查询总数 |
| `query_latency_ms` | Histogram | `query_type` | 查询延迟分布 |
| `query_results_count` | Histogram | `query_type` | 结果数量分布 |

---

## 5. API 层扩展

### 5.1 增强 `/api/metrics` 端点

在现有基础上添加更多子系统指标：

```json
{
  "success": true,
  "timestamp": "2026-05-13T10:30:00Z",
  "uptime_seconds": 3600,
  "subsystems": {
    "project_registry": {
      "cache_hits": 150,
      "cache_misses": 20,
      "hit_rate_percent": "88.24",
      "status": "healthy"
    },
    "index_orchestrator": {
      "total_files": 1000,
      "processed_files": 850,
      "process_percentage": "85.0",
      "status": "processing"
    },
    "embedding": {
      "requests_total": 5000,
      "avg_latency_ms": 120.5,
      "p95_latency_ms": 250.0,
      "error_rate_percent": "0.5",
      "status": "healthy"
    },
    "bm25": {
      "index_size": 8500,
      "queries_total": 1200,
      "avg_query_latency_ms": 15.3,
      "status": "healthy"
    },
    "relation": {
      "relations_extracted": 3500,
      "build_latency_ms": 45.2,
      "resolution_rate": "95.0",
      "status": "healthy"
    },
    "storage": {
      "qdrant": {
        "status": "connected",
        "points_count": 8500
      },
      "bm25": {
        "status": "connected",
        "documents_count": 8500
      }
    }
  }
}
```

### 5.2 新增专用端点（可选）

```
GET /api/metrics/embedding    # Embedding 详细指标
GET /api/metrics/storage      # 存储层详细指标
GET /api/metrics/query        # 查询性能指标
```

---

## 6. 实施计划

### 6.1 Phase 1: 基础设施扩展（P0）

**目标**：扩展 MetricsRegistry，添加 Histogram 支持

**任务**：
1. 实现 `Histogram` 结构体（滑动窗口算法）
2. 扩展 `MetricsRegistry` 添加 `histogram()` 方法
3. 实现 `MetricsSnapshot` 用于批量导出
4. 编写单元测试

**预计工作量**：2-3 天

---

### 6.2 Phase 2: 核心模块集成（P0）

**目标**：为 Embedding 和 BM25 添加监控

**任务**：
1. 在 `EmbeddingProvider` trait 中添加指标接口
2. 为 `OpenAICompatibleProvider` 实现指标记录
3. 为 `Bm25Client` 添加查询和索引监控
4. 更新 `/api/metrics` 端点展示新指标

**预计工作量**：3-4 天

---

### 6.3 Phase 3: 次要模块集成（P1）

**目标**：为 Relation、Parser、Summary 添加监控

**任务**：
1. 为 `IndexBuilder` 添加关系构建监控
2. 为解析器添加解析成功率和延迟监控
3. 为摘要生成器添加性能监控
4. 完善 API 端点

**预计工作量**：3-4 天

---

### 6.4 Phase 4: 高级功能（P2）

**目标**：优化和扩展

**任务**：
1. 为 Hot Update 和 Query Engine 添加监控
2. 实现指标持久化（可选，写入 SQLite）
3. 添加简单的告警机制（阈值检测）
4. 性能优化和文档完善

**预计工作量**：4-5 天

---

## 7. 技术细节

### 7.1 Histogram 实现

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct Histogram {
    buckets: Vec<f64>,
    counts: Vec<Arc<AtomicU64>>,
    sum: Arc<AtomicU64>,  // 存储微秒级的总和
    count: Arc<AtomicU64>,
}

impl Histogram {
    pub fn new(buckets: Vec<f64>) -> Self {
        let counts = buckets.iter().map(|_| Arc::new(AtomicU64::new(0))).collect();
        Self {
            buckets,
            counts,
            sum: Arc::new(AtomicU64::new(0)),
            count: Arc::new(AtomicU64::new(0)),
        }
    }
    
    pub fn observe(&self, value_ms: f64) {
        let value_us = (value_ms * 1000.0) as u64;
        self.sum.fetch_add(value_us, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
        
        for (i, &bucket) in self.buckets.iter().enumerate() {
            if value_ms <= bucket {
                self.counts[i].fetch_add(1, Ordering::Relaxed);
                break;
            }
        }
    }
    
    pub fn percentile(&self, p: f64) -> f64 {
        // 计算百分位数
        let total = self.count.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        
        let target = (total as f64 * p / 100.0) as u64;
        let mut cumulative = 0u64;
        
        for (i, count) in self.counts.iter().enumerate() {
            cumulative += count.load(Ordering::Relaxed);
            if cumulative >= target {
                return self.buckets[i];
            }
        }
        
        self.buckets.last().copied().unwrap_or(0.0)
    }
}
```

### 7.2 标签系统（简化版）

为避免复杂度，第一版暂不实现完整的标签系统，而是通过命名约定实现简单分类：

```rust
// 使用下划线分隔标签
metrics.counter("embedding_requests_openai_success").inc();
metrics.counter("embedding_requests_openai_error").inc();
metrics.histogram("embedding_latency_llamacpp_batch_32").observe(latency);
```

未来如需完整标签支持，可参考 Prometheus 的数据模型进行扩展。

---

## 8. 测试策略

### 8.1 单元测试

- 测试 `Histogram` 的准确性和线程安全性
- 测试并发场景下的指标记录
- 验证百分位数计算的正确性

### 8.2 集成测试

- 模拟 Embedding 请求，验证指标记录
- 执行 BM25 查询，检查延迟统计
- 验证 `/api/metrics` 端点返回数据的完整性

### 8.3 性能测试

- 基准测试：评估指标记录对业务流程的影响（目标：<1% 额外开销）
- 压力测试：高并发场景下的指标采集稳定性

---

## 9. 风险与缓解

### 9.1 性能影响

**风险**：频繁的指标记录可能影响业务性能

**缓解措施**：
- 使用原子操作而非锁
- 直方图采用预分配桶，避免动态内存分配
- 提供配置开关，可在生产环境中禁用详细监控

### 9.2 内存增长

**风险**：长期运行可能导致内存泄漏

**缓解措施**：
- Histogram 使用固定大小的桶数组
- 定期清理不再使用的指标（可选）
- 监控指标本身的内存占用

### 9.3 复杂性增加

**风险**：代码复杂度上升，维护成本增加

**缓解措施**：
- 提供清晰的文档和使用示例
- 封装通用逻辑，减少重复代码
- 保持 API 简洁，避免过度设计

---

## 10. 未来扩展方向

### 10.1 Prometheus 集成

导出指标到 Prometheus，利用其强大的查询和告警能力：

```rust
pub fn export_prometheus_format(&self) -> String {
    // 生成 Prometheus exposition format
    let mut output = String::new();
    
    for (name, counter) in &self.counters {
        output.push_str(&format!("{} {}\n", name, counter.get()));
    }
    
    // ... 其他指标类型
    
    output
}
```

### 10.2 分布式追踪

集成 OpenTelemetry，实现端到端的请求追踪：

- 追踪单个查询从 API 到 Embedding 再到 Storage 的完整链路
- 识别跨模块的性能瓶颈

### 10.3 智能告警

基于历史数据建立基线，自动检测异常：

- 延迟突增告警
- 错误率异常告警
- 容量预警

---

## 11. 总结

本方案在不改变现有架构的前提下，逐步扩展监控能力，重点关注 LLM 交互和核心业务逻辑的性能指标。通过分阶段实施，可以在控制风险的同时持续提升系统的可观测性，为性能优化和故障诊断提供有力支撑。

**核心价值**：
- ✅ 快速定位性能瓶颈（特别是 Embedding 延迟）
- ✅ 量化系统健康状况（成功率、错误率）
- ✅ 支持容量规划（索引大小、Token 用量）
- ✅ 为后续集成专业监控系统奠定基础
