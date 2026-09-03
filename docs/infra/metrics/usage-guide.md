# 指标系统使用指南

本文档介绍如何使用增强后的指标系统，包括标签系统、序列化和导出功能。

## 快速开始

### 基本用法

```rust
use code_context_engine::metrics::{MetricsRegistry, Labels};

let registry = MetricsRegistry::new();

// 创建带标签的 counter
registry
    .counter("http_requests", &[("method", "GET"), ("status", "200")])
    .increment();

// 创建带标签的 histogram
registry
    .histogram_default("embedding_latency", &[("provider", "openai")])
    .observe(125.5);
```

### 标签系统

标签系统允许你按多个维度跟踪指标：

```rust
use code_context_engine::metrics::Labels;

// 方式1：从键值对创建
let labels = Labels::from_pairs(&[("service", "api"), ("version", "1.0")]);

// 方式2：链式调用
let labels = Labels::new()
    .add("method", "POST")
    .add("status", "created");

// 方式3：合并标签
let base_labels = Labels::from_pairs(&[("service", "api")]);
let final_labels = base_labels.add("endpoint", "/users");
```

**重要特性**：
- 标签会自动排序，确保相同标签的不同顺序产生相同的存储键
- 标签集合是不可变的，每次添加都会返回新的实例

### 指标类型

#### Counter（计数器）

```rust
// 简单计数器
let counter = registry.counter_simple("total_requests");
counter.increment();
counter.add(5);

// 带标签的计数器
let counter = registry.counter("requests", &[("method", "GET")]);
counter.increment();
```

#### Gauge（仪表）

```rust
// 简单仪表
let gauge = registry.gauge_simple("active_connections");
gauge.set(42);

// 带标签的仪表
let gauge = registry.gauge("connections", &[("protocol", "http")]);
gauge.set(100);
```

#### Histogram（直方图）

```rust
// 自定义桶
let hist = registry.histogram(
    "request_latency",
    vec![1.0, 5.0, 10.0, 50.0, 100.0],
    &[("service", "api")]
);
hist.observe(25.5);

// 默认桶 [1, 5, 10, 50, 100, 500, 1000, 5000] ms
let hist = registry.histogram_default("embedding_latency", &[("provider", "openai")]);
hist.observe(125.5);

// 获取统计信息
println!("Count: {}", hist.get_count());
println!("Average: {:.2} ms", hist.get_average());
println!("P50: {:.2} ms", hist.p50());
println!("P95: {:.2} ms", hist.p95());
println!("P99: {:.2} ms", hist.p99());
```

## 序列化

### JSON 格式

```rust
use code_context_engine::metrics::serialization::MetricsSnapshot;

let registry = MetricsRegistry::new();
registry.counter("test", &[("label", "value")]).increment();

// 创建快照
let snapshot = MetricsSnapshot::from_registry(&registry);

// 序列化为 JSON
let json = serde_json::to_string_pretty(&snapshot)?;
println!("{}", json);
```

输出示例：

```json
{
  "timestamp": "2026-05-13T10:30:00Z",
  "metrics": [
    {
      "name": "test",
      "labels": {
        "label": "value"
      },
      "value": {
        "type": "Counter",
        "value": 1
      }
    }
  ],
  "summary": {
    "total_counters": 1,
    "total_gauges": 0,
    "total_histograms": 0
  }
}
```

### Prometheus 格式

```rust
use code_context_engine::metrics::exporter::PrometheusExporter;

let registry = MetricsRegistry::new();
registry.counter("requests", &[("method", "GET")]).increment();

let exporter = PrometheusExporter;
let prometheus_text = exporter.export(&registry).await?;
println!("{}", prometheus_text);
```

输出示例：

```
requests{method="GET"} 1
```

## 导出器

### 使用 ExporterManager

```rust
use code_context_engine::metrics::ExporterManager;

let registry = MetricsRegistry::new();
registry.counter("test", &[]).increment();

let manager = ExporterManager::new();

// 导出为 JSON
let json = manager.export("json", &registry).await?;

// 导出为 Prometheus 格式
let prometheus = manager.export("prometheus", &registry).await?;

// 查看支持的格式
let formats = manager.supported_formats();
println!("Supported formats: {:?}", formats);
```

### 自定义导出器

你可以实现自己的导出器：

```rust
use code_context_engine::metrics::{MetricExporter, ExportError};
use async_trait::async_trait;

struct CustomExporter;

#[async_trait]
impl MetricExporter for CustomExporter {
    async fn export(
        &self,
        registry: &MetricsRegistry,
    ) -> Result<String, ExportError> {
        // 自定义导出逻辑
        Ok("custom format".to_string())
    }

    fn name(&self) -> &str {
        "custom"
    }
}
```

## 实际应用场景

### Embedding Provider 监控

```rust
use std::time::Instant;

let registry = MetricsRegistry::new();

// 记录请求次数
registry
    .counter("embedding_requests", &[("provider", "openai"), ("status", "success")])
    .increment();

// 记录延迟
let start = Instant::now();
// ... embedding 操作 ...
let latency = start.elapsed().as_secs_f64() * 1000.0;

registry
    .histogram_default("embedding_latency", &[("provider", "openai")])
    .observe(latency);
```

### Parser 监控

```rust
// 解析成功率
registry
    .counter("parse_attempts", &[("language", "typescript"), ("status", "success")])
    .increment();

// 解析延迟
let start = Instant::now();
// ... parsing 操作 ...
let latency = start.elapsed().as_secs_f64() * 1000.0;

registry
    .histogram_default("parse_latency", &[("language", "typescript")])
    .observe(latency);
```

### BM25 Storage 监控

```rust
// 查询延迟
let start = Instant::now();
// ... query 操作 ...
let latency = start.elapsed().as_secs_f64() * 1000.0;

registry
    .histogram_default("bm25_query_latency", &[("index", "code_chunks")])
    .observe(latency);

// 索引大小
registry
    .gauge("bm25_index_size", &[("index", "code_chunks")])
    .set(document_count as u64);
```

## 最佳实践

### 1. 标签命名规范

- 使用小写字母和下划线：`service_name` 而非 `serviceName`
- 避免使用特殊字符
- 保持标签键的一致性

### 2. 标签数量控制

- 每个指标的标签数建议不超过 5-10 个
- 避免高基数标签（如用户 ID、时间戳）
- 优先使用低基数的分类标签

### 3. 指标命名规范

- 使用名词描述指标：`http_requests`、`embedding_latency`
- 包含单位信息：`latency_ms`、`size_bytes`
- 保持一致的前缀：`embedding_*`、`parser_*`

### 4. 性能考虑

- 标签主要在指标创建时有开销，运行时操作无额外成本
- 常用标签组合可以复用 Labels 实例
- 定期导出指标，避免频繁序列化

## API 参考

### MetricsRegistry

- `counter(name, labels)` - 创建带标签的 counter
- `counter_simple(name)` - 创建不带标签的 counter
- `gauge(name, labels)` - 创建带标签的 gauge
- `gauge_simple(name)` - 创建不带标签的 gauge
- `histogram(name, buckets, labels)` - 创建带标签的 histogram
- `histogram_simple(name, buckets)` - 创建不带标签的 histogram
- `histogram_default(name, labels)` - 使用默认桶创建 histogram
- `histogram_default_simple(name)` - 使用默认桶创建不带标签的 histogram
- `export_all()` - 导出所有指标为 MetricsSnapshot
- `get_all_counters_with_keys()` - 获取所有 counters 及其 MetricKey
- `get_all_gauges_with_keys()` - 获取所有 gauges 及其 MetricKey
- `get_all_histograms_with_keys()` - 获取所有 histograms 及其 MetricKey

### Labels

- `new()` - 创建空标签集合
- `from_pairs(pairs)` - 从键值对创建
- `add(key, value)` - 添加标签
- `merge(other)` - 合并标签集合
- `len()` - 获取标签数量
- `is_empty()` - 检查是否为空
- `iter()` - 迭代标签
- `to_hashmap()` - 转换为 HashMap

### MetricExporter

- `export(registry)` - 导出指标
- `name()` - 获取导出器名称

## 迁移指南

如果你之前使用了旧的 API，需要进行以下更改：

### Counter

```rust
// 旧 API
registry.counter("name").inc();

// 新 API
registry.counter_simple("name").increment();
// 或
registry.counter("name", &[]).increment();
```

### Gauge

```rust
// 旧 API
registry.gauge("name").set(42);

// 新 API
registry.gauge_simple("name").set(42);
// 或
registry.gauge("name", &[]).set(42);
```

### Histogram

```rust
// 旧 API
registry.histogram_default("name").observe(100.0);

// 新 API
registry.histogram_default_simple("name").observe(100.0);
// 或
registry.histogram_default("name", &[]).observe(100.0);
```

## 常见问题

### Q: 标签的顺序会影响指标吗？

A: 不会。标签会自动排序，所以 `[("a", "1"), ("b", "2")]` 和 `[("b", "2"), ("a", "1")]` 会产生相同的存储键。

### Q: 如何删除指标？

A: 当前实现不支持动态删除指标。如果需要重置，可以创建新的 MetricsRegistry 实例。

### Q: 标签的值可以是数字吗？

A: 可以，但会被转换为字符串。例如：`.add("count", "42")`。

### Q: 支持哪些序列化格式？

A: 目前支持 JSON 和 Prometheus exposition format。可以通过实现 MetricExporter trait 添加自定义格式。

## 下一步

- 查看 `extension-design-guide.md` 了解设计细节
- 查看源代码中的测试用例获取更多示例
- 根据实际需求自定义导出器
