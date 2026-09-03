# 项目管理 API

## POST /api/project

创建新项目。

### 请求

**方法**: `POST`

**Content-Type**: `application/json`

**请求体**:

```json
{
  "name": "My Project",
  "root_path": "/path/to/project",
  "extensions": ["rs", "py", "js"],
  "exclude_dirs": ["target", "node_modules"],
  "respect_gitignore": true,
  "ignore_patterns": ["*.test.*"]
}
```

**请求字段**:

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|-----|------|------|--------|------|
| `name` | string? | 否 | 自动生成 | 项目名称 |
| `root_path` | string | 是 | - | 项目根目录路径 |
| `extensions` | string[] | 否 | `[]` | 要包含的文件扩展名 |
| `exclude_dirs` | string[] | 否 | `[]` | 要排除的目录 |
| `respect_gitignore` | boolean | 否 | `true` | 是否遵守 .gitignore |
| `ignore_patterns` | string[] | 否 | `[]` | 额外的忽略模式 |

### 响应

```json
{
  "success": true,
  "project": {
    "id": "proj_123",
    "name": "My Project",
    "root_path": "/path/to/project",
    "extensions": ["rs", "py", "js"],
    "exclude_dirs": ["target", "node_modules"],
    "respect_gitignore": true,
    "ignore_patterns": ["*.test.*"],
    "created_at": "2024-01-15T10:30:00Z",
    "last_indexed": null
  }
}
```

**响应字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `success` | boolean | 操作是否成功 |
| `project` | ProjectConfig | 项目配置 |

**ProjectConfig 字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `id` | string | 项目 ID |
| `name` | string | 项目名称 |
| `root_path` | string | 根目录路径 |
| `extensions` | string[] | 文件扩展名列表 |
| `exclude_dirs` | string[] | 排除目录列表 |
| `respect_gitignore` | boolean | 是否遵守 .gitignore |
| `ignore_patterns` | string[] | 忽略模式列表 |
| `created_at` | string | 创建时间 |
| `last_indexed` | string? | 最后索引时间 |

### 示例

```bash
curl -X POST "http://localhost:3000/api/project" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "My Project",
    "root_path": "/path/to/project",
    "extensions": ["rs", "py"]
  }'
```

---

## GET /api/project

列出所有项目。

### 请求

**方法**: `GET`

### 响应

```json
{
  "success": true,
  "projects": [
    {
      "id": "proj_123",
      "name": "My Project",
      "root_path": "/path/to/project",
      "extensions": ["rs", "py"],
      "exclude_dirs": ["target"],
      "respect_gitignore": true,
      "ignore_patterns": [],
      "created_at": "2024-01-15T10:30:00Z",
      "last_indexed": "2024-01-15T11:00:00Z"
    }
  ],
  "total": 1
}
```

### 示例

```bash
curl "http://localhost:3000/api/project"
```

---

## GET /api/project/:id

获取项目详情。

### 请求

**方法**: `GET`

**路径参数**:

| 参数 | 类型 | 描述 |
|-----|------|------|
| `id` | string | 项目 ID |

### 响应

```json
{
  "success": true,
  "project": {
    "id": "proj_123",
    "name": "My Project",
    "root_path": "/path/to/project",
    "extensions": ["rs", "py"],
    "exclude_dirs": ["target"],
    "respect_gitignore": true,
    "ignore_patterns": [],
    "created_at": "2024-01-15T10:30:00Z",
    "last_indexed": "2024-01-15T11:00:00Z"
  }
}
```

### 示例

```bash
curl "http://localhost:3000/api/project/proj_123"
```

---

## PUT /api/project/:id

更新项目配置。

### 请求

**方法**: `PUT`

**Content-Type**: `application/json`

**路径参数**:

| 参数 | 类型 | 描述 |
|-----|------|------|
| `id` | string | 项目 ID |

**请求体**:

```json
{
  "name": "Updated Project Name",
  "extensions": ["rs", "py", "js"],
  "exclude_dirs": ["target", "node_modules", "dist"],
  "respect_gitignore": false,
  "ignore_patterns": ["*.test.*", "*.spec.*"]
}
```

**请求字段**:

| 字段 | 类型 | 必填 | 描述 |
|-----|------|------|------|
| `name` | string? | 否 | 项目名称 |
| `extensions` | string[]? | 否 | 文件扩展名列表 |
| `exclude_dirs` | string[]? | 否 | 排除目录列表 |
| `respect_gitignore` | boolean? | 否 | 是否遵守 .gitignore |
| `ignore_patterns` | string[]? | 否 | 忽略模式列表 |

### 响应

```json
{
  "success": true,
  "project": {
    "id": "proj_123",
    "name": "Updated Project Name",
    "root_path": "/path/to/project",
    "extensions": ["rs", "py", "js"],
    "exclude_dirs": ["target", "node_modules", "dist"],
    "respect_gitignore": false,
    "ignore_patterns": ["*.test.*", "*.spec.*"],
    "created_at": "2024-01-15T10:30:00Z",
    "last_indexed": "2024-01-15T11:00:00Z"
  }
}
```

### 示例

```bash
curl -X PUT "http://localhost:3000/api/project/proj_123" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Updated Project Name",
    "extensions": ["rs", "py", "js"]
  }'
```

---

## DELETE /api/project/:id

删除项目。

### 请求

**方法**: `DELETE`

**路径参数**:

| 参数 | 类型 | 描述 |
|-----|------|------|
| `id` | string | 项目 ID |

### 响应

```json
{
  "success": true,
  "message": "Project deleted"
}
```

### 示例

```bash
curl -X DELETE "http://localhost:3000/api/project/proj_123"
```

---

## POST /api/project/:id/index

对项目执行索引操作。

### 请求

**方法**: `POST`

**路径参数**:

| 参数 | 类型 | 描述 |
|-----|------|------|
| `id` | string | 项目 ID |

**查询参数**:

| 参数 | 类型 | 默认值 | 描述 |
|-----|------|--------|------|
| `force_reindex` | boolean | `false` | 是否强制重新索引 |

### 响应

```json
{
  "success": true,
  "files_scanned": 100,
  "files_indexed": 95,
  "failed_files": 5,
  "total_entities": 1500,
  "total_relations": 300,
  "total_vectors": 1500,
  "elapsed_ms": 5000,
  "message": "Indexing completed...",
  "errors": []
}
```

响应格式与 [POST /api/index](./index.md#post-apiindex) 相同。

### 示例

```bash
# 索引项目
curl -X POST "http://localhost:3000/api/project/proj_123/index"

# 强制重新索引
curl -X POST "http://localhost:3000/api/project/proj_123/index?force_reindex=true"
```
