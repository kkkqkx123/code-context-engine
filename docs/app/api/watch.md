# 热重载 API

## POST /api/project/{project_id}/watch/start

启动文件监视（热重载）。

### 请求

**方法**: `POST`

**Content-Type**: `application/json`

**路径参数**:

| 参数 | 类型 | 描述 |
|-----|------|------|
| `project_id` | number | 项目 ID |

**请求体**:

```json
{
  "path": "/path/to/project",
  "extensions": ["rs", "py"],
  "debounce_ms": 500
}
```

**请求字段**:

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|-----|------|------|--------|------|
| `path` | string | 是 | - | 要监视的目录路径 |
| `extensions` | string[] | 否 | `[]` | 要监视的文件扩展名 |
| `debounce_ms` | number | 否 | `500` | 防抖间隔（毫秒） |

### 响应

```json
{
  "success": true,
  "message": "Watch started",
  "watched_path": "/path/to/project"
}
```

### 示例

```bash
curl -X POST "http://localhost:3000/api/project/1/watch/start" \
  -H "Content-Type: application/json" \
  -d '{
    "path": "/path/to/project",
    "extensions": ["rs", "py"],
    "debounce_ms": 500
  }'
```

---

## POST /api/project/{project_id}/watch/stop

停止文件监视。

### 请求

**方法**: `POST`

**路径参数**:

| 参数 | 类型 | 描述 |
|-----|------|------|
| `project_id` | number | 项目 ID |

### 响应

```json
{
  "success": true,
  "message": "Watch stopped"
}
```

### 示例

```bash
curl -X POST "http://localhost:3000/api/project/1/watch/stop"
```

---

## GET /api/project/{project_id}/watch/status

获取文件监视状态。

### 请求

**方法**: `GET`

**路径参数**:

| 参数 | 类型 | 描述 |
|-----|------|------|
| `project_id` | number | 项目 ID |

### 响应

```json
{
  "success": true,
  "status": {
    "active": true,
    "watched_dirs": ["/path/to/project"],
    "events_processed": 150,
    "started_at": "2024-01-15T10:30:00Z"
  }
}
```

**响应字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `success` | boolean | 操作是否成功 |
| `status` | WatchStatus | 监视状态 |

**WatchStatus 字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `active` | boolean | 是否处于活动状态 |
| `watched_dirs` | string[] | 监视的目录列表 |
| `events_processed` | number | 已处理的事件数 |
| `started_at` | string? | 启动时间 |

### 示例

```bash
curl "http://localhost:3000/api/project/1/watch/status"
```

## 工作原理

1. **文件监视**: 使用文件系统监视器检测文件变化
2. **防抖处理**: 短时间内的多个变化事件会被合并处理
3. **增量索引**: 只对变化的文件执行增量索引
4. **实时更新**: 索引更新后立即可用于搜索和查询

## 使用场景

1. **开发环境**: 在开发过程中实时更新代码索引
2. **持续集成**: 监视代码库变化并自动更新索引
3. **实时搜索**: 确保搜索结果始终反映最新代码

## 注意事项

- 文件监视会持续消耗系统资源
- 建议仅在需要时启动监视
- 防抖间隔过小可能导致频繁索引，影响性能
- 监视大量文件可能影响系统性能
