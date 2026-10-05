# 前端 Graph 功能集成分析与 Preview 模拟数据补充方案

## 一、Graph 功能集成现状分析

### 1.1 结论

**主前端（frontend/）与预览应用（frontend-preview/）的 graph 功能集成是完整的。** 两端以下文件经 diff 确认完全一致：

| 文件 | 职责 | 状态 |
| --- | --- | --- |
| `src/lib/api/graph.ts` | graph API 封装（ego/path/subgraph/components/export/impact） | 一致 |
| `src/lib/stores/graph.ts` | graph 状态管理（累积、缓存失效、过滤） | 一致 |
| `src/lib/components/graph/GraphCanvas.svelte` | Cytoscape 画布渲染 | 一致 |
| `src/lib/components/graph/EntityGraphView.svelte` | 实体详情页内嵌视图 | 一致 |
| `src/lib/components/graph/GraphFilterPanel.svelte` | 域过滤/搜索面板 | 一致 |
| `src/lib/components/graph/GraphToolbar.svelte` | 视口工具栏 | 一致 |
| `src/lib/utils/entity-graph.ts` | 数据到渲染元素的映射 | 一致 |
| `src/lib/utils/graph-style.ts` | 关系域配色与置信度样式 | 一致 |
| `src/routes/graph/+page.svelte` | 图谱探索页 | preview 独有 |

后端契约（`frontend-preview/src/lib/api/schema.d.ts` 中 `GraphNode`/`GraphEdge`/`GraphSubgraphResponse`/`GraphPathResponse`/`GraphComponentsResponse`/`GraphImpactResponse`）与 `crates/app/cce-server/src/api/handlers/graph.rs` 的响应结构对齐，`graphApi` 各方法的路径与后端路由一致。

### 1.2 唯一缺口：preview 的 mock 层未覆盖 graph

preview 采用 `VITE_USE_MOCK=true` 的静态 mock 机制：

- `src/lib/api/client.ts` 在 mock 模式下将请求转发给 `src/lib/mock/client.ts`；
- `mock/client.ts` 按端点匹配返回 `mock/data.ts` 中的静态数据。

但 `mock/client.ts` 的 GET 路由表中**没有任何 `/graph/*` 端点**，全部落入兜底分支：

```
console.warn(`[Mock] Unhandled GET endpoint: ...`)  →  return {} as T
```

后果：mock 模式下打开 `/graph` 页面时，store 拿到空对象（无 `nodes`/`edges`/`success` 字段），画布为空，无法调试 graph 的任何交互（ego 扩展、路径查找、社区分组、影响分析、过滤、布局等）。

### 1.3 其他观察

- `mock/client.ts` 的 `/api/entities/search` 返回空结果，`/api/tools/references`、`/api/tools/definition` 返回空 `{}`，与 graph 页面无直接耦合，不在本次修改范围内。
- graph 页面默认种子策略为 overview（`loadOverview(400)`），因此 mock 数据需要能支撑一次有意义的批量导出（数十个节点、多关系域），而非仅 3~5 个孤立节点。

## 二、Preview 模拟数据补充方案

### 2.1 需要补充的端点 → mock 数据映射

| API 方法 | 请求路径（concretePath 展开后） | 需要的 mock 数据 |
| --- | --- | --- |
| `getEgo` | `/api/project/{id}/graph/ego` | `mockGraphEgo`（`GraphSubgraphResponse` 形状） |
| `getSubgraph` | `/api/project/{id}/graph/subgraph` | 同上（可复用） |
| `getPath` | `/api/project/{id}/graph/path` | `mockGraphPath`（`GraphPathResponse` 形状，`path_found: true`） |
| `getComponents` | `/api/project/{id}/graph/components` | `mockGraphComponents`（`components: string[][]`） |
| `exportGraph` | `/api/project/{id}/graph/export` | `mockGraphExport`（大子图，支撑 overview 种子） |
| `getImpact` | `/api/project/{id}/graph/impact` | `mockGraphImpact`（direct/transitive dependents + score） |

注意 `dispatch()` 会先用 `concretePath()` 把 `{project_id}` 替换成实际数字再交给 mock 匹配，因此 mock 路由必须用**正则**（如 `/\/graph\/ego$/`）匹配，而非 `===` 字符串比较；query 参数（`entity_id`、`depth` 等）会被 mock 忽略，静态返回即可，ego 语义（depth/direction）在 mock 层不做真实模拟。

### 2.2 数据集设计

构造一个自洽的小型 Rust 项目关系图，与现有 mock 数据（`group_entities`/`parse_file` 等）语义呼应，使用与后端一致的字段：

- **节点**（约 20 个）：`id`/`kind`（function/class/struct/interface/module…）/`label`/`source_file`/`source_location`；
- **边**：`source`/`target`/`relation`（calls/imports/contains/uses/implements…）/`confidence`（`extracted`/`inferred`/`external`），覆盖 `graph-style.ts` 中全部关系域以便测试过滤与图例；
- **连通性**：设计 1 个主连通分量（约 15 节点，支撑 ego 扩展、路径查找）+ 1~2 个小分量（支撑 communities 列表与分量加载）；
- `relation_epoch` 统一取 `1`，`success: true`。

### 2.3 实施要点

1. 在 `mock/data.ts` 中新增共享的节点/边数组 + 各响应对象；
2. 在 `mock/client.ts` 的 GET 路由中用正则追加 6 条 graph 路由（放在现有 `/api/project/...` 匹配之后、兜底之前，且注意顺序——`/api/project/\w+$` 等短正则不能误吞 graph 路径，graph 路由需排在其前或写得更具体）；
3. 数据集中在 `mock/data.ts`，`mock/client.ts` 只做路由，保持现有分层。

## 三、修改清单（本次实施）

1. `frontend-preview/src/lib/mock/data.ts`：新增 graph 节点/边数据集及 `mockGraphEgo`/`mockGraphPath`/`mockGraphComponents`/`mockGraphExport`/`mockGraphImpact`。
2. `frontend-preview/src/lib/mock/client.ts`：GET 路由表新增 6 条 graph 端点正则匹配。

主前端 `frontend/` 无需改动（无 mock 层，直连真实后端）。
