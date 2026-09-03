# 实体搜索 API

## POST /api/entities/search

使用SQLite FTS5进行实体全文搜索，支持按名称和签名快速查找实体。

### 请求

**方法**: `POST`

**Content-Type**: `application/json`

```json
{
  "query": "auth*",
  "project_id": 1,
  "project_path": "/path/to/project",
  "limit": 20,
  "kind_filter": "function"
}
```

**注意**: `project_id` 和 `project_path` 必须提供其中一个，但不能同时提供。
- 使用 `project_id`：直接通过内部项目ID访问（需要先查询获取ID）
- 使用 `project_path`：直接通过项目根目录路径访问（更方便，系统会自动归一化路径并查找对应的项目）

**请求字段**:

| 字段 | 类型 | 必填 | 描述 |
|-----|------|------|------|
| `query` | string | 是 | FTS5查询字符串（支持前缀匹配、短语匹配等） |
| `project_id` | number? | 二选一 | 项目 ID，用于应用项目级配置 |
| `project_path` | string? | 二选一 | 项目根目录路径（与 project_id 二选一） |
| `limit` | number | 否 | 返回结果数量限制，默认20 |
| `kind_filter` | string | 否 | 实体类型过滤（如 "function", "class", "module"） |

**FTS5查询语法**:

- **前缀匹配**: `auth*` 匹配 "authenticate", "authorization" 等
- **短语匹配**: `"test function"` 匹配完整短语
- **布尔运算符**: `AND`, `OR`, `NOT`
- **字段特定搜索**: `name:main` 仅在名称字段中搜索

### 响应

**Content-Type**: `application/json`

```json
{
  "success": true,
  "total": 5,
  "items": [
    {
      "id": 123,
      "name": "authenticate_user",
      "kind": "function",
      "file_id": 456,
      "signature": "fn authenticate_user(username: &str, password: &str) -> Result<User>",
      "span_start_row": 10,
      "span_end_row": 45,
      "depth": 2,
      "parent_id": null,
      "project_id": 1,
      "rank": 1.0
    }
  ],
  "elapsed_ms": 5
}
```

**响应字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `success` | boolean | 操作是否成功 |
| `total` | number | 返回的结果总数 |
| `items` | array | 搜索结果列表 |
| `elapsed_ms` | number | 查询耗时（毫秒） |

**items 字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `id` | number | 实体ID |
| `name` | string | 实体名称 |
| `kind` | string | 实体类型（function, class, module等） |
| `file_id` | number | 所属文件ID |
| `signature` | string | 函数/类签名 |
| `span_start_row` | number | 起始行号 |
| `span_end_row` | number | 结束行号 |
| `depth` | number | AST深度 |
| `parent_id` | number | 父实体ID |
| `project_id` | number | 项目ID |
| `rank` | number | FTS5相关性排名 |

### 示例

```bash
# 搜索以 "auth" 开头的实体（使用 project_path）
curl -X POST "http://localhost:3000/api/entities/search" \
  -H "Content-Type: application/json" \
  -d '{
    "query": "auth*",
    "project_path": "/path/to/my/project",
    "limit": 10
  }'

# 搜索包含 "test" 的函数（使用 project_id）
curl -X POST "http://localhost:3000/api/entities/search" \
  -H "Content-Type: application/json" \
  -d '{
    "query": "test",
    "project_id": 1,
    "kind_filter": "function"
  }'
```

### 使用场景

1. **符号查找**: 快速定位函数、类等实体
2. **自动补全**: IDE自动补全功能
3. **代码导航**: 跳转到定义
4. **实体浏览**: 探索项目中的实体结构
