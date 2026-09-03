# 搜索 API

## POST /api/search

执行代码搜索查询。

### 请求

**方法**: `POST`

**Content-Type**: `application/json`

**请求体**:

```json
{
  "project_id": 1,
  "project_path": "/path/to/project",
  "query": "function to parse file",
  "query_type": "hybrid",
  "limit": 10,
  "min_score": 0.5,
  "directory_prefix": "src/",
  "exclude_patterns": ["test_*"],
  "include_patterns": ["*"],
  "exclude_content_types": ["test", "generated"],
  "call_chain_depth": 3,
  "include_call_chain": true
}
```

**请求字段**:

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|-----|------|------|--------|------|
| `project_id` | number? | 二选一 | - | 项目 ID，用于应用项目级配置 |
| `project_path` | string? | 二选一 | - | 项目根目录路径（与 project_id 二选一） |
| `query` | string | **是** | - | 查询文本 |
| `query_type` | string | 否 | `"hybrid"` | 查询类型 |
| `limit` | number | 否 | `10` | 每页结果数 |
| `min_score` | number? | 否 | - | 最小相似度分数 |
| `directory_prefix` | string? | 否 | - | 目录前缀过滤 |
| `exclude_patterns` | string[] | 否 | `[]` | 排除模式 |
| `include_patterns` | string[] | 否 | `[]` | 包含模式 |
| `exclude_content_types` | string[] | 否 | `[]` | 排除的内容类型（test, generated, vendor） |
| `call_chain_depth` | number | 否 | `3` | 调用链深度 |
| `include_call_chain` | boolean | 否 | `false` | 是否包含调用链 |

**注意**: `project_id` 和 `project_path` 必须提供其中一个，但不能同时提供。系统会根据提供的信息加载对应的项目配置，并应用到搜索过程中。
- 使用 `project_id`：直接通过内部项目ID访问（需要先查询获取ID）
- 使用 `project_path`：直接通过项目根目录路径访问（更方便，系统会自动归一化路径并查找对应的项目）

**查询类型**:

| 类型 | 描述 |
|-----|------|
| `vector` | 向量搜索 |
| `bm25` | BM25 关键词搜索 |
| `hybrid` | 混合搜索（向量 + BM25） |
| `hierarchical` | 层次化搜索 |
| `summary` | 摘要搜索 |
| `semantic_with_relations` | 语义搜索（包含关系） |

### 响应

```json
{
  "success": true,
  "total": 25,
  "items": [
    {
      "entity_ids": [123, 456],
      "score": 0.95,
      "file_path": "src/parser.rs",
      "code_chunk": "fn parse_file(path: &Path) -> Result<ParsedFile> { ... }",
      "start_line": 10,
      "end_line": 25,
      "entity_type": "function",
      "source": "vector",
      "call_chain": [
        {
          "function_id": "snapshot-local:124",
          "function_name": "read_file",
          "file_path": "src/io.rs",
          "depth": 1,
          "relation_type": "callee",
          "call_line": 12
        }
      ]
    }
  ],
  "elapsed_ms": 150,
  "sources_used": ["vector", "bm25"]
}
```

**响应字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `success` | boolean | 操作是否成功 |
| `total` | number | 结果总数 |
| `items` | SearchResultItem[] | 搜索结果列表 |
| `elapsed_ms` | number | 耗时（毫秒） |
| `sources_used` | string[] | 使用的搜索源 |

**SearchResultItem 字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `entity_ids` | number[] | 与该结果关联的全部实体 ID（一个 chunk 可含多个实体；结果不再有单一主实体） |
| `score` | number | 相似度分数 |
| `file_path` | string | 文件路径 |
| `code_chunk` | string | 代码片段 |
| `start_line` | number | 起始行号 |
| `end_line` | number | 结束行号 |
| `entity_type` | string? | 实体类型（function, class 等） |
| `source` | string | 结果来源（vector, bm25, hybrid） |
| `call_chain` | CallChainNode[]? | 调用链 |

**CallChainNode 字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `function_id` | string | 函数 ID（snapshot-local:<实体ID>） |
| `function_name` | string | 函数名称 |
| `file_path` | string | 文件路径 |
| `depth` | number | 调用深度 |
| `relation_type` | string | 关系类型（caller/callee） |
| `call_line` | number? | 调用行号 |

### 示例

```bash
# 基本搜索（使用 project_id）
curl -X POST "http://localhost:3000/api/search" \
  -H "Content-Type: application/json" \
  -d '{"project_id": 1, "query": "parse file"}'

# 基本搜索（使用 project_path，更方便）
curl -X POST "http://localhost:3000/api/search" \
  -H "Content-Type: application/json" \
  -d '{"project_path": "/path/to/my/project", "query": "parse file"}'

# 向量搜索
curl -X POST "http://localhost:3000/api/search" \
  -H "Content-Type: application/json" \
  -d '{
    "project_id": 1,
    "query": "function to parse file",
    "query_type": "vector",
    "limit": 5
  }'

# 带过滤的搜索
curl -X POST "http://localhost:3000/api/search" \
  -H "Content-Type: application/json" \
  -d '{
    "project_path": "/home/user/projects/myapp",
    "query": "error handling",
    "query_type": "hybrid",
    "directory_prefix": "src/",
    "min_score": 0.7,
    "include_call_chain": true
  }'
```
