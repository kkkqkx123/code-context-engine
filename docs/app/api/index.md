# 索引操作 API

## POST /api/index

执行完整的代码索引操作。

### 请求

**方法**: `POST`

**Content-Type**: `application/json`

**请求体**:

```json
{
  "project_id": 1,
  "path": "/path/to/project",
  "extensions": ["rs", "py", "js"],
  "exclude_dirs": ["target", "node_modules"],
  "respect_gitignore": true,
  "ignore_patterns": ["*.test.*"],
  "custom_gitignore": ".customignore"
}
```

**注意**: `project_id` 是必填字段。系统会根据项目ID加载对应的项目配置（包括 batch size、grouper 配置、ast_to_nl 配置等），并应用到索引过程中。

**请求字段**:

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|-----|------|------|--------|------|
| `project_id` | number | **是** | - | 项目 ID，用于应用项目级配置 |
| `path` | string | **是** | - | 要索引的根目录路径 |
| `extensions` | string[] | 否 | 见说明 | 要包含的文件扩展名 |
| `exclude_dirs` | string[] | 否 | 见说明 | 要排除的目录 |
| `respect_gitignore` | boolean | 否 | `true` | 是否遵守 .gitignore 规则 |
| `ignore_patterns` | string[] | 否 | `[]` | 额外的忽略模式 |
| `custom_gitignore` | string | 否 | - | 自定义 gitignore 文件路径 |

**注意**: `project_id` 或 `project_path` 是必填字段。系统会根据项目加载对应的项目配置（包括 batch size、grouper 配置、ast_to_nl 配置等），并应用到索引过程中。

**默认扩展名**: `["rs", "py", "js", "ts", "c", "cpp", "java"]`

**默认排除目录**: `["node_modules", "target", ".git", "vendor"]`

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

**响应字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `success` | boolean | 操作是否成功 |
| `files_scanned` | number | 扫描的文件总数 |
| `files_indexed` | number | 成功索引的文件数 |
| `failed_files` | number | 失败的文件数 |
| `total_entities` | number | 提取的实体总数 |
| `total_relations` | number | 提取的关系总数 |
| `total_vectors` | number | 存储的向量总数 |
| `elapsed_ms` | number | 耗时（毫秒） |
| `message` | string | 结果描述消息 |
| `errors` | string[] | 错误列表 |

### 示例

```bash
# 通过 project_id 启动全量索引
curl -X POST "http://localhost:3000/api/project/1/index"

# 指定路径进行索引（POST /api/index）
curl -X POST "http://localhost:3000/api/index" \
  -H "Content-Type: application/json" \
  -d '{
    "project_id": 1,
    "path": "/path/to/project",
    "extensions": ["rs", "py"],
    "exclude_dirs": ["target"],
    "respect_gitignore": true
  }'
```

---

## POST /api/index/incremental

执行增量索引操作。

### 请求

**方法**: `POST`

**Content-Type**: `application/json`

**请求体**:

```json
{
  "project_id": 1,
  "files_to_index": ["src/new_file.rs", "src/modified_file.rs"],
  "files_to_remove": ["src/deleted_file.rs"],
  "incremental_mode": "scan"
}
```

**请求字段**:

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|-----|------|------|--------|------|
| `project_id` | number? | 二选一 | - | 项目 ID |
| `project_path` | string? | 二选一 | - | 项目根目录路径（与 project_id 二选一） |
| `files_to_index` | string[] | 否 | `[]` | 要索引的文件列表（新增或修改） |
| `files_to_remove` | string[] | 否 | `[]` | 要删除的文件列表 |
| `incremental_mode` | string | 否 | `"scan"` | 增量模式（scan/track） |

### 响应

```json
{
  "success": true,
  "files_indexed": 2,
  "files_removed": 1,
  "total_entities": 50,
  "total_vectors": 50,
  "elapsed_ms": 500,
  "errors": []
}
```

**响应字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `success` | boolean | 操作是否成功 |
| `files_indexed` | number | 索引的文件数 |
| `files_removed` | number | 删除的文件数 |
| `total_entities` | number | 提取的实体总数 |
| `total_vectors` | number | 存储的向量总数 |
| `elapsed_ms` | number | 耗时（毫秒） |
| `errors` | string[] | 错误列表 |

### 示例

```bash
# 使用 project_path
curl -X POST "http://localhost:3000/api/index/incremental" \
  -H "Content-Type: application/json" \
  -d '{
    "project_path": "/path/to/my/project",
    "files_to_index": ["src/new_file.rs"],
    "files_to_remove": ["src/old_file.rs"]
  }'

# 或使用 project_id
curl -X POST "http://localhost:3000/api/index/incremental" \
  -H "Content-Type: application/json" \
  -d '{
    "project_id": 1,
    "files_to_index": ["src/new_file.rs"],
    "files_to_remove": ["src/old_file.rs"]
  }'
```

---

## POST /api/parse

解析单个文件并返回解析结果（不存储）。

### 请求

**方法**: `POST`

**Content-Type**: `application/json`

**请求体**:

```json
{
  "file_path": "src/main.rs",
  "language": "rust"
}
```

**请求字段**:

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|-----|------|------|--------|------|
| `file_path` | string | 是 | - | 要解析的文件路径 |
| `language` | string | 否 | 自动检测 | 编程语言 |

### 响应

```json
{
  "success": true,
  "file_path": "src/main.rs",
  "language": "rust",
  "encoding": "utf-8",
  "entities": [
    {
      "id": 1,
      "kind": "function",
      "name": "main",
      "signature": "fn main()",
      "start_line": 1,
      "end_line": 10,
      "doc_comment": "Main entry point"
    }
  ],
  "relations": [
    {
      "caller_id": 1,
      "callee_id": 2,
      "relation_type": "calls",
      "line": 5
    }
  ],
  "elapsed_ms": 50
}
```

**响应字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `success` | boolean | 操作是否成功 |
| `file_path` | string | 文件路径 |
| `language` | string | 编程语言 |
| `encoding` | string | 文件编码 |
| `entities` | EntityInfo[] | 实体列表 |
| `relations` | RelationInfo[] | 关系列表 |
| `elapsed_ms` | number | 耗时（毫秒） |

**EntityInfo 字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `id` | number | 实体 ID |
| `kind` | string | 实体类型（function, class, struct 等） |
| `name` | string | 实体名称 |
| `signature` | string | 实体签名 |
| `start_line` | number | 起始行号 |
| `end_line` | number | 结束行号 |
| `doc_comment` | string? | 文档注释 |

**RelationInfo 字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `caller_id` | number | 调用者 ID |
| `callee_id` | number | 被调用者 ID |
| `relation_type` | string | 关系类型 |
| `line` | number | 关系所在行号 |

### 示例

```bash
curl -X POST "http://localhost:3000/api/parse" \
  -H "Content-Type: application/json" \
  -d '{
    "file_path": "src/main.rs"
  }'
```
