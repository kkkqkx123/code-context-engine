# 聚合搜索 API

## POST /api/search/aggregated

执行多查询并行检索，支持同时执行多个子查询并使用RRF（Reciprocal Rank Fusion）算法融合结果。

### 与单一查询端点的对比

Code Context Engine 提供两个搜索 API 端点：

- **`POST /api/search`** - 单一查询端点（默认），适合简单的关键词或语义搜索
- **`POST /api/search/aggregated`** - 多查询聚合端点（高级），适合复杂的多路召回场景

**选择建议**：

| 场景 | 推荐端点 |
|------|----------|
| 新用户快速上手 | `/api/search` |
| 简单搜索需求 | `/api/search` |
| 需要精细控制检索策略 | `/api/search/aggregated` |
| 跨语言/跨技术栈搜索 | `/api/search/aggregated` |
| 追求极致检索质量 | `/api/search/aggregated` |

**性能对比**：

| 指标 | `/api/search` (hybrid) | `/api/search/aggregated` |
|------|------------------------|--------------------------|
| 延迟 | ~100-200ms | ~120-250ms（并行后接近单查询） |
| 查全率 | 中等 | 高（多路召回） |
| 查准率 | 良好 | 优秀（RRF 融合） |
| 复杂度 | 低 | 中（需要设计子查询） |

### 请求

**方法**: `POST`

**Content-Type**: `application/json`

```json
{
  "project_id": 1,
  "project_path": "/path/to/project",
  "sub_queries": [
    {
      "text": "authentication function",
      "query_type": "vector",
      "weight": 1.5
    },
    {
      "text": "login auth",
      "query_type": "bm25",
      "weight": 1.0
    }
  ],
  "limit": 20,
  "min_score": 0.5,
  "directory_prefix": "src/",
  "exclude_content_types": ["test", "generated"],
  "exclude_patterns": ["**/tests/**"],
  "include_patterns": ["**/auth/**"]
}
```

**注意**: `project_id` 和 `project_path` 必须提供其中一个，但不能同时提供。
- 使用 `project_id`：直接通过内部项目ID访问（需要先查询获取ID）
- 使用 `project_path`：直接通过项目根目录路径访问（更方便，系统会自动归一化路径并查找对应的项目）

**请求字段**:

| 字段 | 类型 | 必填 | 描述 |
|-----|------|------|------|
| `project_id` | number? | 二选一 | 项目 ID，用于应用项目级配置 |
| `project_path` | string? | 二选一 | 项目根目录路径（与 project_id 二选一） |
| `sub_queries` | array | 是 | 子查询列表 |
| `limit` | number | 否 | 返回结果数量限制，默认20 |
| `min_score` | number | 否 | 最小分数阈值 |
| `directory_prefix` | string | 否 | 目录前缀过滤 |
| `exclude_content_types` | string[] | 否 | 排除的内容类型（test, generated, vendor） |
| `exclude_patterns` | string[] | 否 | 排除的文件模式（glob格式） |
| `include_patterns` | string[] | 否 | 包含的文件模式（glob格式） |

**sub_queries 字段**:

| 字段 | 类型 | 必填 | 描述 |
|-----|------|------|------|
| `text` | string | 是 | 查询文本 |
| `query_type` | string | 否 | 查询类型（vector, bm25, hybrid, summary），默认hybrid |
| `weight` | number | 否 | RRF融合权重，默认1.0 |

### 响应

**Content-Type**: `application/json`

```json
{
  "success": true,
  "total": 15,
  "items": [
    {
      "id": "abc123",
      "score": 0.95,
      "file_path": "src/auth/login.rs",
      "start_line": 10,
      "end_line": 45,
      "language": "rust",
      "entity_type": "function",
      "entity_name": "authenticate_user",
      "snippet": "fn authenticate_user(username: &str, password: &str) -> Result<User> { ... }",
      "metadata": {
        "signature": "fn authenticate_user(username: &str, password: &str) -> Result<User>",
        "description": "Authenticates a user with username and password"
      }
    }
  ],
  "elapsed_ms": 85,
  "sources_used": ["vector", "bm25"]
}
```

**响应字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `success` | boolean | 操作是否成功 |
| `total` | number | 返回的结果总数 |
| `items` | array | 搜索结果列表 |
| `elapsed_ms` | number | 查询耗时（毫秒） |
| `sources_used` | string[] | 使用的搜索源 |

**items 字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `id` | string | 结果ID |
| `score` | number | 相关性分数（0-1） |
| `file_path` | string | 文件路径 |
| `start_line` | number | 起始行号 |
| `end_line` | number | 结束行号 |
| `language` | string | 编程语言 |
| `entity_type` | string | 实体类型 |
| `entity_name` | string | 实体名称 |
| `snippet` | string | 代码片段 |
| `metadata` | object | 元数据 |

### 示例

```bash
# 使用 project_path（推荐，更直观）
curl -X POST "http://localhost:3000/api/search/aggregated" \
  -H "Content-Type: application/json" \
  -d '{
    "project_path": "/path/to/my/project",
    "sub_queries": [
      {
        "text": "user authentication",
        "query_type": "vector",
        "weight": 1.5
      },
      {
        "text": "login validate",
        "query_type": "bm25",
        "weight": 1.0
      }
    ],
    "limit": 10
  }'

# 或使用 project_id
curl -X POST "http://localhost:3000/api/search/aggregated" \
  -H "Content-Type: application/json" \
  -d '{
    "project_id": 1,
    "sub_queries": [
      {
        "text": "user authentication",
        "query_type": "vector",
        "weight": 1.5
      },
      {
        "text": "login validate",
        "query_type": "bm25",
        "weight": 1.0
      }
    ],
    "limit": 10
  }'
```

### 工作原理

1. **并行检索**: 所有子查询并行执行，提高性能
2. **RRF融合**: 使用Reciprocal Rank Fusion算法合并多个查询结果
   - RRF公式: `score = Σ(1 / (k + rank))`，其中k=60
   - 权重高的查询对最终排名影响更大
3. **去重排序**: 自动去重并按融合分数排序
4. **统一过滤**: 所有子查询共享相同的过滤条件

### 使用场景

1. **混合语义和关键词搜索**: 同时使用向量搜索（语义）和BM25（关键词）
2. **多角度查询**: 从不同角度描述同一概念
3. **加权查询**: 某些查询比其他查询更重要
4. **复杂需求**: 需要结合多种搜索策略的场景
