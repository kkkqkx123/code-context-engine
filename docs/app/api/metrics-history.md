# 指标历史管理 API

## GET /api/metrics/history

查询历史聚合指标数据，支持时间范围、指标名称和项目过滤。

### 请求

**方法**: `GET`

**查询参数**:

| 参数 | 类型 | 必填 | 描述 |
|-----|------|------|------|
| `from` | string | 是 | 开始时间（ISO 8601格式） |
| `to` | string | 是 | 结束时间（ISO 8601格式） |
| `metric` | string | 否 | 指标名称过滤 |
| `project_id` | number | 否 | 项目ID过滤 |
| `project_path` | string | 否 | 项目根目录路径过滤（与 project_id 二选一） |
| `operation_type` | string | 否 | 操作类型过滤（如 "index", "query", "embed"） |

### 响应

**Content-Type**: `application/json`

```json
[
  {
    "timestamp": "2024-01-15T10:30:00Z",
    "metric_name": "request_count",
    "operation_type": "query",
    "project_id": 1,
    "count": 100,
    "avg_value": 25.5,
    "median_value": 20.0,
    "max_value": 150.0,
    "p90_value": 45.0,
    "p99_value": 80.0
  }
]
```

**响应字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `timestamp` | string | 时间戳（ISO 8601格式） |
| `metric_name` | string | 指标名称 |
| `operation_type` | string | 操作类型 |
| `project_id` | number | 项目ID |
| `count` | number | 观测次数 |
| `avg_value` | number | 平均值 |
| `median_value` | number | 中位数（P50） |
| `max_value` | number | 最大值 |
| `p90_value` | number | P90值 |
| `p99_value` | number | P99值 |

### 示例

```bash
# 查询过去1小时的查询延迟指标
curl "http://localhost:3000/api/metrics/history?from=2024-01-15T09:00:00Z&to=2024-01-15T10:00:00Z&metric=query_execution_latency_ms"

# 查询特定项目的索引操作（使用 project_id）
curl "http://localhost:3000/api/metrics/history?from=2024-01-15T00:00:00Z&to=2024-01-15T23:59:59Z&project_id=1&operation_type=index"

# 查询特定项目的索引操作（使用 project_path）
curl "http://localhost:3000/api/metrics/history?from=2024-01-15T00:00:00Z&to=2024-01-15T23:59:59Z&project_path=/path/to/my/project&operation_type=index"
```

**注意**: `project_id` 和 `project_path` 可以同时不提供，也可以提供其中一个。如果同时提供，系统会优先使用 `project_id`。

### 使用场景

1. **性能趋势分析**: 查看指标随时间的变化趋势
2. **问题诊断**: 定位特定时间段的问题
3. **容量规划**: 基于历史数据进行资源规划
4. **SLA监控**: 验证服务级别协议达成情况

---

## DELETE /api/metrics/cleanup

清理历史聚合指标数据，支持全量清理或按时间清理。

### 请求

**方法**: `DELETE`

**查询参数**:

| 参数 | 类型 | 必填 | 描述 |
|-----|------|------|------|
| `all` | boolean | 条件必填 | 删除所有记录（与before二选一） |
| `before` | string | 条件必填 | 删除此时间之前的记录（ISO 8601格式） |

### 响应

**Content-Type**: `application/json`

```json
{
  "success": true,
  "deleted_count": 1500
}
```

**响应字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `success` | boolean | 操作是否成功 |
| `deleted_count` | number | 删除的记录数 |

### 示例

```bash
# 清理7天前的数据
curl -X DELETE "http://localhost:3000/api/metrics/cleanup?before=2024-01-08T00:00:00Z"

# 清理所有历史数据
curl -X DELETE "http://localhost:3000/api/metrics/cleanup?all=true"
```

### 使用场景

1. **存储空间管理**: 定期清理旧数据释放存储空间
2. **数据保留策略**: 实现自动化的数据保留策略
3. **测试环境清理**: 测试后清理测试数据
4. **合规性要求**: 满足数据保留期限的合规要求
