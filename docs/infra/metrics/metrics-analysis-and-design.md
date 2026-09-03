# Metrics 模块分析与改进设计方案

## 1. 现状分析

### 1.1 现有 API 接口
目前系统通过 HTTP 和 CLI 提供以下指标查询能力：
- **GET /api/metrics**: 获取系统综合快照（JSON 格式），包含项目注册表缓存、索引编排器进度及 Qdrant 存储状态。
- **CLI 命令**: `cce-cli metrics` 支持 Prometheus 和 JSON 两种导出格式。

### 1.2 核心组件架构
- **ProgressTracker**: 负责实时跟踪文件扫描与处理进度，采用原子操作保证线程安全。
- **Business Metrics**: 涵盖 Embedding、BM25、Relation、Parser、Summary、HotUpdate 及 Query 等子系统的性能监控。
- **Exporter System**: 支持将内存中的指标序列化为 JSON 或 Prometheus 文本格式。

### 1.3 存在的问题
1. **API 端点缺失**: 路由中未定义 `/api/metrics/json`，导致 CLI 的 JSON 导出功能无法正常工作。
2. **缺乏历史维度**: 仅能提供当前时刻的快照，无法回溯历史趋势或进行长期性能分析。
3. **数据粒度粗糙**: 缺少按项目 ID、文件类型或具体操作类型的细分统计。
4. **存储控制缺失**: 目前没有持久化机制，重启后所有指标丢失；若直接全量持久化，则面临存储空间无限增长的风险。

---

## 2. 改进设计方案

### 2.1 总体架构：独立日志 + SQLite 聚合存储

为避免引入沉重的时序数据库（如 InfluxDB 或 Prometheus TSDB），建议采用**"独立日志采集 + 定期聚合存 SQLite"**的轻量化方案。

#### 核心设计原则
1. **采集与存储分离**: 所有原始指标数据写入独立的 `metrics.log` 文件（与业务 Tracing 日志完全分离）。
2. **聚合驱动持久化**: 后台定时任务从内存缓存或日志中提取数据，计算统计量后存入 SQLite。
3. **查询面向 SQLite**: 所有历史数据查询、清理操作仅针对 SQLite 中的聚合数据，不直接操作日志文件。
4. **日志生命周期管理**: `metrics.log` 仅作为临时缓冲，基于文件大小定期轮转删除，不作为长期存储介质。

#### 数据流转架构
```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐     ┌──────────┐
│  业务代码   │────▶│ MetricsRegistry│────▶│ metrics.log │     │          │
│ (采集层)    │     │  (内存计数器)  │     │ (独立日志)  │     │          │
└─────────────┘     └──────┬───────┘     └──────┬──────┘     │          │
                           │                    │              │          │
                           │  定期读取           │  定期轮转     │          │
                           │  (每5分钟)          │  (按大小)     │          │
                           ▼                    ▼              │          │
                  ┌────────────────┐   ┌──────────────┐       │          │
                  │  聚合引擎       │──▶│  SQLite      │◀──────┼── 查询   │
                  │ (Aggregation)  │   │ (聚合数据存储)│       │  /清理   │
                  └────────────────┘   └──────────────┘       │          │
                                                               └──────────┘
```

#### 各组件职责
1. **采集层 (Collection)**: 
   - 业务逻辑调用 `MetricsRegistry` 记录指标（如 embedding 延迟、请求计数等）。
   - 指标同时写入内存计数器（用于实时查询）和异步追加到 `metrics.log` 文件。
   - `metrics.log` 使用独立的 logger 实例，与主应用的 tracing 日志物理隔离。

2. **缓冲层 (Buffering)**: 
   - 在内存中维护短期缓存（如最近 5 分钟的原始指标点）。
   - 缓存用于加速聚合计算，避免频繁读取磁盘日志文件。

3. **聚合层 (Aggregation)**: 
   - 后台定时任务（如每 5 分钟）触发聚合流程。
   - 从内存缓存或 `metrics.log` 中提取原始数据，按指标名称和标签分组。
   - 计算每个分组的统计量：**总条目数 (count)、平均值 (avg)、中位数 (median)、最大值 (max)、P90、P99**。
   - 将聚合结果批量插入 SQLite 的 `metrics_aggregated` 表。
   - 聚合完成后，清空对应的内存缓存段。

4. **存储层 (Storage)**: 
   - **SQLite**: 持久化存储聚合后的统计数据，支持结构化查询和历史回溯。
   - **metrics.log**: 仅作为临时缓冲区，当文件大小超过阈值（如 10MB）或跨天时自动截断/删除旧内容。

5. **查询层 (Query)**: 
   - **实时查询**: 直接从内存中的 `MetricsRegistry` 和 `ProgressTracker` 读取当前快照。
   - **历史查询**: 仅从 SQLite 的 `metrics_aggregated` 表中检索聚合数据。

6. **清理层 (Cleanup)**: 
   - 仅对 SQLite 中的聚合数据执行清理操作（全量清空或按时间截断）。
   - `metrics.log` 的清理由日志轮转机制自动管理，不对外提供 API。

### 2.2 存储占用控制策略
为了确保存储占用可控，采取以下措施：
- **原始日志轮转**: `metrics.log` 文件大小超过阈值（如 10MB）或跨天时自动归档/删除。
- **聚合数据留存**: SQLite 中仅保留聚合后的统计数据。例如，将 n 条 Embedding 记录聚合成一条“5分钟区间统计”。
- **自动清理机制**: 提供 API 支持按时间范围清理过期的聚合数据。

---

## 3. API 设计与实现细节

### 3.1 HTTP API 接口定义

| 方法 | 路径 | 数据来源 | 描述 | 参数示例 |
| :--- | :--- | :--- | :--- | :--- |
| GET | `/api/metrics` | **内存** (ProgressTracker + MetricsRegistry) | 获取实时综合快照，包含索引进度、缓存命中率、Qdrant 状态等 | - |
| GET | `/api/metrics/json` | **内存** (MetricsRegistry) | **(补全)** 导出完整的 JSON 格式指标（含所有 Counter/Gauge/Histogram） | `type=embedding` (可选过滤) |
| GET | `/api/metrics/history` | **SQLite** (metrics_aggregated 表) | **(新增)** 查询历史聚合数据，返回指定时间范围内的统计趋势 | `from=2024-01-01&to=2024-01-02&metric=embedding_latency&agg_interval=5m` |
| DELETE | `/api/metrics/cleanup` | **SQLite** (metrics_aggregated 表) | **(新增)** 清理历史聚合数据 | `before=2024-01-01` 或 `all=true` |

#### 接口详细说明

##### GET /api/metrics (实时快照)
- **数据来源**: 内存中的 `AppState.progress_tracker` 和 `MetricsRegistry`。
- **返回内容**: 
  - `index_orchestrator`: 索引进度（总文件数、已扫描、已处理、错误数、完成百分比）。
  - `project_registry`: 缓存统计（命中数、未命中数、命中率）。
  - `storage`: Qdrant 连接状态和点数。
- **特点**: 零延迟，反映系统当前瞬时状态，重启后数据丢失。

##### GET /api/metrics/json (完整指标导出)
- **数据来源**: 内存中的 `MetricsRegistry`（包含所有注册的 Counter、Gauge、Histogram）。
- **返回内容**: 标准化的 JSON 结构，包含指标名称、标签、当前值、直方图分布等。
- **用途**: 供 CLI 工具导出或外部监控系统抓取。
- **特点**: 实时数据，不包含历史信息。

##### GET /api/metrics/history (历史聚合查询)
- **数据来源**: SQLite 数据库中的 `metrics_aggregated` 表。
- **返回内容**: 时间序列化的聚合统计数据数组，每条记录包含：
  - `timestamp`: 聚合时间窗口的起始时间。
  - `metric_name`: 指标名称（如 `embedding_latency_ms`）。
  - `labels`: 标签键值对（如 `{provider: "openai"}`）。
  - `count`: 该窗口内的样本总数。
  - `avg`, `median`, `max`, `p90`, `p99`: 各项统计量。
- **查询参数**:
  - `from` / `to`: ISO 8601 格式的时间范围（必填）。
  - `metric`: 指标名称过滤（可选，如 `embedding_latency_ms`）。
  - `agg_interval`: 聚合粒度（如 `5m`, `1h`, `1d`，默认为存储时的粒度）。
- **特点**: 支持长期趋势分析，数据经过聚合压缩，查询效率高。

##### DELETE /api/metrics/cleanup (历史数据清理)
- **操作对象**: SQLite 数据库中的 `metrics_aggregated` 表。
- **清理模式**:
  1. **完全清空**: `DELETE /api/metrics/cleanup?all=true`
     - 执行 `DELETE FROM metrics_aggregated`。
     - 可选：同时重置内存中的 Counter 计数器（需通过配置开关控制）。
  2. **时间截断**: `DELETE /api/metrics/cleanup?before=2024-01-01T00:00:00Z`
     - 执行 `DELETE FROM metrics_aggregated WHERE timestamp < ?`。
     - 仅清理指定时间点之前的所有聚合记录。
- **限制**: 
  - **不支持**按标签（Label）过滤清理，以保持 SQL 简单性和执行效率。
  - **不影响** `metrics.log` 文件，日志轮转由独立的文件系统任务管理。

### 3.2 日志文件管理策略

#### metrics.log 生命周期
- **写入方式**: 异步追加模式，每条指标记录为单行 JSON 或键值对格式。
- **轮转触发条件**（满足任一即触发）:
  1. **文件大小**: 超过阈值（如 10MB）时，截断文件头部或删除整个文件后重建。
  2. **时间周期**: 跨天零点时，归档当前日志并创建新文件（可选，取决于配置）。
- **清理责任**: 由文件系统层面的日志管理器自动执行，**不通过 HTTP API 暴露**。
- **与 SQLite 的关系**: 
  - 日志文件是聚合任务的**数据源之一**，但不是查询目标。
  - 聚合任务完成后，对应的日志段可安全删除（因为数据已持久化到 SQLite）。
  - 若聚合任务失败，日志文件可作为数据恢复的后备源（保留最近 N 个周期的日志）。

### 3.3 查询与清理的实现逻辑

#### 实时查询逻辑 (GET /api/metrics, /api/metrics/json)
```rust
// 伪代码示例
async fn handle_get_metrics(State(state): State<AppState>) -> Json<MetricsResponse> {
    // 1. 从内存读取 ProgressTracker
    let progress = state.progress_tracker.get_progress();
    
    // 2. 从内存读取 MetricsRegistry 的当前值
    let registry_snapshot = state.metrics_registry.snapshot();
    
    // 3. 组装响应（不涉及 SQLite 或日志文件）
    Json(MetricsResponse { progress, registry: registry_snapshot })
}
```

#### 历史查询逻辑 (GET /api/metrics/history)
```rust
// 伪代码示例
async fn handle_get_history(
    State(state): State<AppState>,
    Query(params): Query<HistoryQueryParams>
) -> Json<Vec<AggregatedMetric>> {
    // 1. 解析时间范围和过滤条件
    let from = params.from.parse::<DateTime<Utc>>()?;
    let to = params.to.parse::<DateTime<Utc>>()?;
    
    // 2. 构建 SQL 查询（仅针对 SQLite）
    let sql = "SELECT timestamp, metric_name, labels, count, avg, median, max, p90, p99 
               FROM metrics_aggregated 
               WHERE timestamp BETWEEN ? AND ? 
               AND metric_name = COALESCE(?, metric_name)
               ORDER BY timestamp ASC";
    
    // 3. 执行查询并返回结果
    let rows = state.sqlite_db.query_all(sql, &[from, to, params.metric]).await?;
    Json(rows)
}
```

#### 清理逻辑 (DELETE /api/metrics/cleanup)
```rust
// 伪代码示例
async fn handle_cleanup(
    State(state): State<AppState>,
    Query(params): Query<CleanupParams>
) -> StatusCode {
    if params.all {
        // 模式1: 完全清空 SQLite 表
        state.sqlite_db.execute("DELETE FROM metrics_aggregated").await?;
    } else if let Some(before) = params.before {
        // 模式2: 按时间截断
        let cutoff = before.parse::<DateTime<Utc>>()?;
        state.sqlite_db.execute(
            "DELETE FROM metrics_aggregated WHERE timestamp < ?", 
            &[cutoff]
        ).await?;
    }
    
    // 注意：此处不操作 metrics.log 文件
    StatusCode::NO_CONTENT
}
```

---

## 4. 实施路线图

### 第一阶段：基础补全与日志分离
1. **补全路由**: 在 `router.rs` 中添加 `/api/metrics/json` 端点，绑定到现有的 `ExporterManager`。
2. **实现独立日志器**: 
   - 创建 `MetricsLogger` 模块，使用独立的 `tracing_subscriber` 配置，输出到 `logs/metrics.log`。
   - 确保与主应用的 `tracing` 日志（如 `logs/app.log`）物理隔离。
3. **定义 SQLite 表结构**: 
   ```sql
   CREATE TABLE metrics_aggregated (
       id INTEGER PRIMARY KEY AUTOINCREMENT,
       timestamp DATETIME NOT NULL,           -- 聚合窗口起始时间
       metric_name TEXT NOT NULL,             -- 指标名称（如 embedding_latency_ms）
       labels_json TEXT,                      -- 标签的 JSON 序列化（如 {"provider":"openai"}）
       count INTEGER NOT NULL,                -- 样本总数
       avg REAL,                              -- 平均值
       median REAL,                           -- 中位数
       max REAL,                              -- 最大值
       p90 REAL,                              -- P90 分位数
       p99 REAL,                              -- P99 分位数
       created_at DATETIME DEFAULT CURRENT_TIMESTAMP
   );
   CREATE INDEX idx_metrics_time ON metrics_aggregated(timestamp);
   CREATE INDEX idx_metrics_name_time ON metrics_aggregated(metric_name, timestamp);
   ```

### 第二阶段：聚合引擎与清理 API
1. 实现后台聚合任务，定期将内存/日志数据写入 SQLite。
2. 开发 `/api/metrics/history` 查询接口。
3. 实现 `/api/metrics/cleanup` 接口，支持全量和按时间截断清理。

### 第三阶段：多维度优化
1. 在聚合数据中增加 `project_id` 和 `operation_type` 字段。
2. 优化查询性能，为时间戳和项目 ID 建立联合索引。

---

## 6. 实现现状与演进记录

### 6.1 与原始设计的差异

1. **未采用 `metrics.log` 独立日志方案**：实际实现为纯内存 Registry + 定时聚合到 SQLite，无日志缓冲层。
2. **`/api/metrics` 已改为 Prometheus 文本导出**（`text/plain; version=0.0.4`），`/api/metrics/json` 为完整快照；实时快照与历史查询均已落地。
3. **`metrics_aggregated` 表已扩展**：包含 `project_id`、`operation_type`、`metric_type` 列及对应索引。

### 6.2 2026-08 重构要点（见 `docs/plan/metrics_system_refactor_design.md`）

1. **聚合窗口语义**：直方图聚合改为按窗口增量计算（count/sum/bucket_counts 相对上一周期求差，窗口 max 独立追踪），历史记录反映窗口内活动而非累计值。
2. **`metric_type` 列**：`counter`/`gauge`/`histogram` 三种记录语义互不相同，由该列区分；存量数据由迁移脚本尽力回填。
3. **`operation_type` 数据来源修正**：聚合器改从 `operation` 标签提取（此前读取的 `operation_type` 标签键不在白名单内，恒为空）。
4. **单位统一**：Prometheus 直方图 `_sum` 与 `_bucket` 边界统一为毫秒，`histogram_quantile()` 可用。
5. **HELP 描述集中化**：`crates/cce_core/src/metrics/descriptions.rs` 为唯一描述来源，并有覆盖测试防漂移。
6. **接线与清理**：`BackgroundTaskMetrics`、`RerankMetrics` 接入生产链路；删除 `RegistryConfig`、未使用的静态标签枚举与 `ChatMetrics` 死代码。
7. **重启基线**：聚合启动时先记录 counter 基线，避免重启后首个窗口的假峰值。
8. **配置补全**：`retention_seconds`、`cleanup_interval_secs` 可在 `[metrics.aggregation]` 中配置。
9. **直方图真实 max**：`HistogramStats.max_ms` 记录真实观测最大值（累计），取代原先的 bucket 边界估算。
