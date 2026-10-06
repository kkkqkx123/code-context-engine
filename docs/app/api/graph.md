# 图 API

关系图遍历 API，基于项目的关系快照提供只读查询。

所有端点均需要项目的关系索引可用（否则返回 `503`）。快照过期时，响应中会包含 `relation_info` 字段提示。

---

## 符号种子（symbol seed）

所有需要定位实体的参数都接受 **符号种子**，四种形式：

| 形式 | 含义 |
|------|------|
| `sym_…` | 精确稳定符号 ID（`sym_` 前缀） |
| `path/to/file#name` | 指定文件内的名称 |
| `#name` | 任意文件内的名称 |
| `name` | 项目内任意位置的裸名称 |

解析走快照的名称索引，代价为 O(同名命中数)，不会全项目扫描。

- 唯一命中 → 直接解析成功。
- 多个命中 → `400` `AMBIGUOUS_SYMBOL`，`details` 中带候选列表（`stable_id`/`file_path`/`scoped_name`/`kind`），客户端取其中一个 `stable_id` 重试。
- 无命中 → `404` `ENTITY_NOT_FOUND`。

**不接受运行时实体 ID**。实体 ID 每个进程重新分配、跨重建不稳定，旧 ID 会静默命中无关实体。

---

## 通用过滤参数

`domains` 与 `include_external` 在除 `components` 外的所有图端点上一致生效：

| 字段 | 类型 | 默认值 | 描述 |
|------|------|--------|------|
| `domains` | string | `""`（不过滤） | 逗号分隔的粗粒度关系域 |
| `include_external` | bool | `true` | 是否保留指向项目外部的边 |

关系域取值：`call`、`dependency`、`structural`、`reference`、`template`、`other`。空值表示所有域都保留。

**域过滤约束的是遍历本身，而不只是返回结果**：`domains=call` 下的最短路径不会穿越 dependency 边，连通分量也会按 call 边重新划分分区，而不是把同一分区换个皮。

---

## 分页语义

`offset` / `limit` **只作用于节点**。返回的 `edges` 是当前节点页上诱导出的全部边，因此每一页都是自洽子图，可直接渲染。

- 边不会再独立地做一次 `offset`/`limit`（那会让第二页起几乎恒返回 0 条边，并可能返回指向缺失节点的悬空边）。
- `total_nodes` 为分页前（过滤后）的节点数。
- `total_edges` 为分页前（过滤后、整节点集上）的边数。

---

## GET /api/project/{project_id}/graph/ego

查询实体的 ego 邻域。

### 请求参数

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|------|------|------|--------|------|
| `entity_id` | string | **是** | - | 符号种子 |
| `depth` | number | 否 | `2` | 遍历深度（受项目 `max_call_depth` 限制） |
| `direction` | string | 否 | `both` | `in`/`backward`/`up`、`out`/`forward`/`down`、`both`/`bidirectional` |
| `offset` | number | 否 | `0` | 节点分页偏移 |
| `limit` | number | 否 | `2000` | 节点分页上限 |
| `domains` | string | 否 | `""` | 关系域过滤 |
| `include_external` | bool | 否 | `true` | 保留外部边 |

单次 ego 遍历最多展开 10000 个节点。

### 响应

`GraphSubgraphResponse`。

---

## GET /api/project/{project_id}/graph/path

查询两个实体之间的最短路径。

### 请求参数

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|------|------|------|--------|------|
| `start` | string | **是** | - | 起点符号种子 |
| `end` | string | **是** | - | 终点符号种子 |
| `max_depth` | number | 否 | `10` | 最大搜索深度（受项目 `max_call_depth` 限制） |
| `domains` | string | 否 | `""` | 关系域过滤 |
| `include_external` | bool | 否 | `true` | 是否允许穿越外部边 |

### 响应

`GraphPathResponse` — `path_found` 为 `false` 表示不可达（端点缺失也归为此种，不报错）。

---

## GET /api/project/{project_id}/graph/subgraph

查询多个实体诱导的子图。

### 请求参数

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|------|------|------|--------|------|
| `ids` | string | **是** | - | 逗号分隔的符号种子（1–200 个，超出返回 `400`） |
| `offset` / `limit` | number | 否 | `0` / `2000` | 节点分页 |
| `domains` | string | 否 | `""` | 关系域过滤 |
| `include_external` | bool | 否 | `true` | 保留外部边 |

### 响应

`GraphSubgraphResponse`。

---

## GET /api/project/{project_id}/graph/components

返回关系图的连通分量。

### 请求参数

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|------|------|------|--------|------|
| `offset` | number | 否 | `0` | 分量分页偏移 |
| `limit` | number | 否 | `2000` | 分量分页上限（1–5000，超出返回 `400`） |
| `domains` | string | 否 | `""` | 关系域过滤（仅通过过滤的边参与合并） |
| `include_external` | bool | 否 | `true` | 保留外部边 |

### 响应

`GraphComponentsResponse` — `components` 按**规模降序**排列（截断时优先保留结构性重要的分量，同规模按最小成员排序保证确定性），分量内成员升序。`total_components` 为分页前的分量总数。

---

## GET /api/project/{project_id}/graph/export

导出关系图，按 hub 优先（出度降序）截断。

### 请求参数

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|------|------|------|--------|------|
| `limit` | number | 否 | `2000` | 最大导出节点数（1–10000，超出返回 `400`） |
| `offset` | number | 否 | `0` | 节点分页偏移 |
| `domains` | string | 否 | `""` | 关系域过滤 |
| `include_external` | bool | 否 | `true` | 保留外部边 |

### 响应

`GraphSubgraphResponse`。

---

## GET /api/project/{project_id}/graph/impact

查询**文件**变更影响范围。

### 请求参数

| 字段 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `file` | string | **是** | 文件路径（不可为空） |

### 响应

`GraphImpactResponse`

| 字段 | 类型 | 描述 |
|------|------|------|
| `changed_file` | string | 变更文件 |
| `direct_dependents` | string[] | 恰好一跳的依赖方 |
| `indirect_dependents` | string[] | 两跳及以上的依赖方 |
| `impact_score` | number | `[0, 100)` 的相对影响度 |

`direct_dependents` 与 `indirect_dependents` **不相交**，两者长度可直接相加而不重复计数。

`impact_score` = `100 × w / (w + 20)`，其中 `w = 2 × |direct| + |indirect|`：直接依赖方权重是间接的两倍，分数渐近逼近 100 而非被单个 hub 顶到上限。

---

## GET /api/project/{project_id}/graph/entity-impact

查询**实体**变更影响范围（实体级影响分析的出口）。

### 请求参数

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|------|------|------|--------|------|
| `entity_id` | string | **是** | - | 符号种子 |
| `max_depth` | number | 否 | `10` | 最大遍历深度（1–50，超出返回 `400`） |

### 响应

`GraphEntityImpactResponse`

| 字段 | 类型 | 描述 |
|------|------|------|
| `changed_entity` | string | 变更实体的稳定符号 ID |
| `direct_dependents` | string[] | 一跳调用方 |
| `indirect_dependents` | string[] | 两跳及以上调用方，与上一项不相交 |
| `impact_score` | number | 同文件级影响分析的公式 |

实体边直接由已解析的关系边推导，不存在需要另行同步的第二份实体依赖图。

---

## GET /api/project/{project_id}/graph/cycles

查询依赖环。

### 请求参数

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|------|------|------|--------|------|
| `level` | string | 否 | `entity` | `entity`（调用图）或 `file`（文件依赖图） |
| `limit` | number | 否 | `100` | 最多返回的环数（1–5000，超出返回 `400`） |

### 响应

`GraphCyclesResponse`

| 字段 | 类型 | 描述 |
|------|------|------|
| `level` | string | 实际使用的层级 |
| `cycles` | GraphCycle[] | 每个环的成员按遍历顺序排列，末成员回到首个成员 |
| `total_cycles` | number | 本次探测到的环数 |
| `truncated` | bool | 是否存在超出 `limit` 的环 |

`cycles[].members` 在 `level=entity` 时为稳定符号 ID，`level=file` 时为项目相对路径。

环检测使用显式栈的迭代 DFS，深调用链不会栈溢出。

---

## GET /api/project/{project_id}/graph/structural

查询结构关系与前端（标记语言）关系。这是 `domains` 粗粒度过滤无法替代的类型化入口。

### 请求参数

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|------|------|------|--------|------|
| `entity_id` | string | **是** | - | 符号种子 |
| `kind` | string | **是** | - | 关系族，见下表 |
| `direction` | string | 否 | `out` | `out`/`outgoing`/`forward` 或 `in`/`incoming`/`backward` |
| `limit` | number | 否 | `200` | 最大返回条数（1–2000，超出返回 `400`） |

### 关系族（`kind`）

| `kind` | 含义 |
|--------|------|
| `trait_bound` | 以该实体为 trait bound 的类型（Rust） |
| `child_elements` | 组件/元素的子元素 |
| `parent_element` | 元素的父元素 |
| `event_handlers` | 元素绑定的事件处理器 |
| `handler_elements` | 绑定到某事件处理器的元素 |
| `parameter_bindings` | 组件声明的 prop 绑定 |
| `template_references` | 元素发出的模板引用（`ref`/`bind:this`） |
| `template_ref_owners` | 引用该实体的组件/元素 |

每个族都有 `out` 与 `in` 两个方向，二者互为反向。

### 响应

`GraphStructuralResponse` — `relations[]` 每项含 `entity_id`/`label`/`relation`/`domain`/`source_file`；`total_relations` 为 `limit` 之前的条数，`truncated` 表示是否还有未返回的条目。

---

## GET /api/project/{project_id}/graph/module

查询单个文件的模块级关系。

### 请求参数

| 字段 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `file` | string | **是** | 文件路径（不可为空） |

### 响应

`GraphModuleResponse`

| 字段 | 类型 | 描述 |
|------|------|------|
| `file` | string | 查询的文件 |
| `exports` | string[] | 该文件导出的稳定符号 ID |
| `caller_files` | string[] | 模块级边指向该文件的文件 |
| `imports` | ModuleRelation[] | 以该文件为发起方的模块级边（import/use 等） |

`ModuleRelation` 含 `target`（源码中的原始目标）、`relation`、`domain`、`entity_id`（文件自身发起的边为空串）。`imports` 最多返回 5000 条。

---

## 通用数据结构

### GraphNode

| 字段 | 类型 | 描述 |
|------|------|------|
| `id` | string | 节点 ID（稳定符号 ID；无符号键时为 `entity:<id>`；外部节点为 `external::<name>`） |
| `label` | string | 显示名称 |
| `kind` | string | 实体类型（`function`、`class`、`interface`、`external`…） |
| `source_file` | string | 源文件路径 |
| `source_location` | string | 源位置（`L<line>`，未知时为空串） |
| `scoped_name` | string? | 作用域符号名 |
| `signature` | string? | 定义实体的签名 |

`id` / `label` / `source_file` 有长度上限，超限时以 `500` 报错而非静默截断。

### GraphEdge

| 字段 | 类型 | 描述 |
|------|------|------|
| `source` | string | 源节点 ID |
| `target` | string | 目标节点 ID |
| `relation` | string | 关系类型字符串（如 `call.direct`、`inheritance`、`contains.element`） |
| `domain` | string | 粗粒度关系域，**由后端判定**，前端不应从 `relation` 重新推导 |
| `confidence` | string | `EXTRACTED`（源码中显式写明）/ `INFERRED`（解析期推导）/ `EXTERNAL`（指向项目外） |
| `call_context` | string? | 调用方式（`direct`、`instance_method`…） |
| `is_external` | bool | 边是否指向索引范围之外 |

`confidence` 为大写蛇形；`call_context` 与 `is_external` 只在边携带调用上下文时出现。

---

## 相关端点

图之外，实体级关系查询由 Entity 组端点提供：

| 端点 | 用途 |
|------|------|
| `GET /api/project/{pid}/function/{id}/calls` | 直接被调用的实体 |
| `GET /api/project/{pid}/function/{id}/callers` | 调用该实体的实体 |
| `GET /api/project/{pid}/call-chain/{id}` | 前向/后向调用链 |
| `GET /api/project/{pid}/call-path` | 两点调用路径 |
| `GET /api/project/{pid}/class/{id}/inheritance` | 继承闭包（祖先 + 后代，含跳数 `depth`） |
| `GET /api/project/{pid}/class/{id}/implementations` | 接口实现关系 |
| `GET /api/project/{pid}/relations/classification/{c}` | 按外部调用分类筛选关系 |
| `GET /api/project/{pid}/relations/classification/stats` | 外部调用分类统计 |
| `POST /api/tools/references` | 查找符号引用 |
| `POST /api/tools/definition` | 跳转到定义 |
| `POST /api/tools/symbols` | 列出文件内符号 |

这些端点的 `{id}` 同样接受符号种子，并额外支持 `exclude_tests`、`directory_prefix`、`excluded_files`、`domains`、`include_external` 过滤参数。

---

## CLI

```shell
cce graph ego --id <seed> --project-id <id> [--depth N] [--direction both]
cce graph path --from <seed> --to <seed> --project-id <id> [--depth N] [--domains call]
cce graph subgraph --ids <seed,seed> --project-id <id>
cce graph components --project-id <id> [--limit N] [--domains call]
cce graph export --project-id <id> [--limit N]
cce graph impact <file> --project-id <id>
cce graph entity-impact <seed> --project-id <id> [--max-depth N]
cce graph cycles --project-id <id> [--level entity|file] [--limit N]
cce graph structural <seed> --kind <kind> --project-id <id> [--direction out]
cce graph module <file> --project-id <id>
```

所有子命令支持 `--format json` 获取原始响应。