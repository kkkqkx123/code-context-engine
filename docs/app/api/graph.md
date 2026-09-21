# 图 API

关系图遍历 API，基于项目的关系快照提供只读查询。

所有端点均需要项目的关系索引可用。若快照过期，响应中会包含 `relation_info` 字段提示。

---

## GET /api/project/{project_id}/graph/ego

查询实体的 ego 邻域（指定深度和方向的子图）。

### 请求参数

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|-----|------|------|--------|------|
| `entity_id` | string | **是** | - | 稳定符号 ID |
| `depth` | number | 否 | `3` | 遍历深度（受项目 `max_call_depth` 限制） |
| `direction` | string | 否 | `"both"` | 方向：`forward`/`down`、`backward`/`up`、`both`/`bidirectional` |

### 响应

`GraphSubgraphResponse` — 包含 `nodes`、`edges`、`relation_epoch`、`relation_info`。

---

## GET /api/project/{project_id}/graph/path

查询两个实体之间的最短路径。

### 请求参数

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|-----|------|------|--------|------|
| `start` | string | **是** | - | 起点稳定符号 ID |
| `end` | string | **是** | - | 终点稳定符号 ID |
| `max_depth` | number | 否 | `3` | 最大搜索深度 |

### 响应

`GraphPathResponse` — 包含 `path_found`、`nodes`、`edges`、`relation_epoch`。

---

## GET /api/project/{project_id}/graph/subgraph

查询多个实体诱导子图。

### 请求参数

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|-----|------|------|--------|------|
| `ids` | string | **是** | - | 逗号分隔的稳定符号 ID 列表（最多 200 个） |

### 响应

`GraphSubgraphResponse`。

---

## GET /api/project/{project_id}/graph/components

返回关系图的连通分量。

### 响应

`GraphComponentsResponse` — 包含 `components`（每个分量为稳定 ID 列表）。

---

## GET /api/project/{project_id}/graph/export

导出完整关系图（受节点数限制）。

### 请求参数

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|-----|------|------|--------|------|
| `limit` | number | 否 | `1000` | 最大导出节点数（上限 10000） |

### 响应

`GraphSubgraphResponse`。

---

## GET /api/project/{project_id}/graph/impact

查询文件变更影响范围。

### 请求参数

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|-----|------|------|--------|------|
| `file` | string | **是** | - | 文件路径 |

### 响应

`GraphImpactResponse` — 包含 `changed_file`、`direct_dependents`、`transitive_dependents`、`impact_score`。

---

## 通用数据结构

### GraphNode

| 字段 | 类型 | 描述 |
|-----|------|------|
| `id` | string | 节点 ID（稳定符号 ID） |
| `label` | string | 显示名称 |
| `kind` | string | 实体类型（function, class 等） |
| `source_file` | string | 源文件路径 |
| `source_location` | string | 源位置（`L<line>`） |

### GraphEdge

| 字段 | 类型 | 描述 |
|-----|------|------|
| `source` | string | 源节点 ID |
| `target` | string | 目标节点 ID |
| `relation` | string | 关系类型 |
| `confidence` | string | 置信度：EXTRACTED / INFERRED / EXTERNAL |
