# 指标导出 API

## GET /api/metrics

导出 Prometheus 文本格式的指标。

### 请求

**方法**: `GET`

### 响应

**Content-Type**: `text/plain; version=0.0.4`

```
# HELP embedding_requests_total Total embedding API requests
# TYPE embedding_requests_total counter
embedding_requests_total{provider="openai"} 42

# HELP http_request_duration_ms HTTP request latency distribution in milliseconds
# TYPE http_request_duration_ms histogram
http_request_duration_ms_bucket{method="GET",path="/api/search",le="1"} 120
...
http_request_duration_ms_bucket{method="GET",path="/api/search",le="+Inf"} 400
http_request_duration_ms_sum{method="GET",path="/api/search"} 12.345
http_request_duration_ms_count{method="GET",path="/api/search"} 400

# HELP system_cpu_usage_percent System CPU usage percentage
# TYPE system_cpu_usage_percent gauge
system_cpu_usage_percent 12.5
```

### 指标命名约定

- 计数器以 `_total` 结尾。
- 直方图记录毫秒级延迟，名称以 `_ms` 结尾；`_bucket`/`_sum`/`_count` 与 `_bucket` 边界单位一致（毫秒），可直接用于 `histogram_quantile()`。
- 浮点仪表（百分比/比率/均值）名称包含 `_rate`/`_ratio`/`avg_`。
- 每个指标的 `# HELP` 描述由集中描述表生成（`crates/cce_core/src/metrics/descriptions.rs`），新增指标必须补充描述，否则导出时回落为 `No description available`。

### 指标分组

| 分组 | 示例指标 |
|------|---------|
| HTTP | `http_requests_total`、`http_request_duration_ms`、`http_errors_total`、`http_active_connections` |
| Embedding | `embedding_requests_total`、`embedding_latency_ms`、`embedding_tokens_total`、`embedding_errors_total` |
| 解析/流水线 | `parse_attempts_total`、`pipeline_stage_latency_ms`、`file_processing_total_latency_ms` |
| 摘要 | `summaries_generated_total`、`summary_generation_latency_ms`、`summary_avg_length` |
| 关系 | `relations_extracted_total`、`relation_build_latency_ms` |
| 插件 | `plugin_loads_total`、`plugin_executions_total`、`plugin_execution_latency_ms` |
| 搜索/查询 | `search_queries_total`、`search_query_latency_ms`、`queries_executed_total`、`query_cache_hit_rate` |
| 存储 | `bm25_*`、`qdrant_*`（含 `qdrant_circuit_breaker_state`）、`sqlite_*` |
| 热更新/监视 | `hot_update_*`、`watch_*` |
| 队列 | `operation_queue_depth`、`retry_queue_depth` |
| 运行时/系统（瞬时） | `tokio_*`、`system_*` |
| 后台任务 | `bg_aggregation_cycles_total`、`bg_last_aggregation_timestamp` |
| 重排 | `rerank_requests_total`、`rerank_latency_ms` |

### 示例

```bash
curl "http://localhost:9000/api/metrics"
```

---

## GET /api/metrics/json

导出 JSON 格式的指标快照。

### 请求

**方法**: `GET`

### 响应

**Content-Type**: `application/json`

```json
{
  "timestamp": "2026-01-15T10:30:00Z",
  "summary": {
    "total_counters": 5,
    "total_gauges": 2,
    "total_float_gauges": 1,
    "total_histograms": 3
  },
  "metrics": [
    {
      "name": "embedding_requests_total",
      "labels": { "provider": "openai" },
      "value": { "Counter": 42 }
    },
    {
      "name": "embedding_latency_ms",
      "value": {
        "Histogram": {
          "count": 42,
          "average": 120.5,
          "sum_microseconds": 5061000,
          "max_ms": 800.0,
          "p50": 100.0,
          "p90": 500.0,
          "p95": 500.0,
          "p99": 1000.0,
          "buckets": [1, 5, 10, 50, 100, 500, 1000, 5000],
          "bucket_counts": [0, 0, 0, 5, 20, 15, 2, 0],
          "overflow_count": 0
        }
      }
    }
  ]
}
```

### 响应字段

| 字段 | 类型 | 描述 |
|-----|------|------|
| `timestamp` | string | 快照采集时间（RFC3339） |
| `summary` | object | 各类指标数量汇总 |
| `metrics` | array | 指标列表（按 `(名称, 标签)` 去重，同键多类型时仪表优先于计数器） |

`value` 为带标签枚举：`Counter(u64)`、`Gauge(u64)`、`FloatGauge(f64)`、`Histogram(HistogramStats)`。直方图统计中的延迟单位均为毫秒（`sum_microseconds` 例外，为微秒，供导出器换算）。

### 示例

```bash
curl "http://localhost:9000/api/metrics/json"
```

---

## GET /api/metrics/history

查询 SQLite 中聚合的历史指标。

### 请求

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `from` | string | 是 | 起始时间（RFC3339） |
| `to` | string | 是 | 结束时间（RFC3339） |
| `metric` | string | 否 | 按指标名过滤 |
| `project_id` | integer | 否 | 按项目过滤 |
| `operation_type` | string | 否 | 按操作类型过滤（如 `indexing`、`querying`） |

### 响应

**Content-Type**: `application/json`

```json
[
  {
    "timestamp": "2026-01-15T10:30:00Z",
    "metric_name": "embedding_latency_ms",
    "metric_type": "histogram",
    "labels_json": "{\"provider\":\"openai\"}",
    "count": 42,
    "avg": 120.5,
    "median": 100.0,
    "max": 800.0,
    "p90": 500.0,
    "p99": 1000.0,
    "project_id": null,
    "operation_type": null
  }
]
```

### metric_type 与列语义

`metric_type` 表明记录种类，各统计列的解读不同：

| metric_type | count | avg | median/max/p90/p99 |
|------------|-------|-----|--------------------|
| `counter` | 窗口内增量 | NULL | NULL |
| `gauge` | 1 | 采样值 | NULL（max 为采样值） |
| `histogram` | 窗口内观测数 | 窗口均值 | 窗口统计（直方图聚合按窗口增量计算，反映窗口内活动而非累计值） |

### 示例

```bash
curl "http://localhost:9000/api/metrics/history?from=2026-01-15T00:00:00Z&to=2026-01-15T23:59:59Z&metric=embedding_latency_ms"
```

---

## DELETE /api/metrics/cleanup

清理历史聚合指标。

### 请求

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `all` | boolean | 否 | 清空全部记录（与 `before` 二选一） |
| `before` | string | 否 | 删除该时间点（RFC3339）之前的记录 |

### 响应

```json
{
  "success": true,
  "deleted_count": 1234
}
```

### 示例

```bash
curl -X DELETE "http://localhost:9000/api/metrics/cleanup?before=2026-01-01T00:00:00Z"
```

---

## 使用场景

### Prometheus 集成

```yaml
scrape_configs:
  - job_name: 'code-context-engine'
    static_configs:
      - targets: ['localhost:9000']
    metrics_path: '/api/metrics'
```

### 历史趋势

历史指标默认保留 7 天，可在 `config.toml` 的 `[metrics.aggregation]` 中调整 `retention_seconds` 与 `cleanup_interval_secs`；`tokio_*` 瞬时指标不参与历史聚合。
