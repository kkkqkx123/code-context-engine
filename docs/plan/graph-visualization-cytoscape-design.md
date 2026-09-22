# 引入 Cytoscape.js 实现类 graphify 图展示功能 — 设计方案

> 范围：在 code-context-engine（CCE）现有关系图数据能力之上，新增一个基于 Cytoscape.js 的交互式图展示层，对标 graphify 的图展示体验并做交互增强。
> 结论先行：**后端图数据 API 已基本就绪，真正的缺口在前端可视化层**；本方案以「复用现有 6 个 `/graph/*` 接口 + 新增前端 Cytoscape 视图」为核心，避免重复造轮子。

---

## 0. 摘要

| 维度 | 现状 | 本方案目标 |
|------|------|-----------|
| 后端图数据 | 6 个 `/graph/*` 接口已实现并接线，输出 node-link 结构 | 基本不动，仅按前端需要补少量字段 |
| 前端图展示 | 仅线性 SVG 调用链 + HTML 继承列表，无图库 | 新增 Cytoscape.js 交互式图视图 |
| 参照对象 graphify | 用 **Mermaid** 生成**静态**文档型图（可平移缩放） | 用 Cytoscape.js 做**实时可探索**的图（点击展开、过滤、影响分析） |

---

## 1. 背景与目标

graphify 的核心卖点之一是把代码关系「画出来」——以分区（community/section）为单位生成调用流图、附调用明细表与统计。CCE 已具备更强的关系底座（可版本化、可多模式遍历的关系图），却只在前端以线性 SVG 呈现单条调用链，未能把「调用 / 依赖 / 继承 / 引用」四域关系作为一个整体图可视化出来。

目标：引入 **Cytoscape.js** 作为图渲染引擎，在 CCE Web 仪表盘中提供：

- 把任意 ego 子图、子图、路径、影响面渲染为交互式网络图；
- 对标 graphify 的「分区概览 + 明细」体验，并叠加实时探索能力（点击节点按需扩边、按关系类型过滤、按实体定位）；
- 直接复用现有后端接口，不做大规模后端改造。

---

## 2. 事实核查：CCE 后端图能力已就绪

### 2.1 六个图遍历接口已接线

路由注册于 `crates/app/cce-server/src/api/router.rs:120-144`，处理器实现于 `crates/app/cce-server/src/api/handlers/graph.rs`：

| 路由 | 处理器 | 行号 | 用途 |
|------|--------|------|------|
| `GET /api/project/{id}/graph/ego` | `handle_graph_ego` | `graph.rs:172` | 以某实体为中心、指定深度/方向的邻域子图 |
| `GET /api/project/{id}/graph/path` | `handle_graph_path` | `graph.rs:214` | 两实体间最短路径 |
| `GET /api/project/{id}/graph/subgraph` | `handle_graph_subgraph` | `graph.rs:257` | 指定实体集合的子图（上限 200 id） |
| `GET /api/project/{id}/graph/components` | `handle_graph_components` | `graph.rs:308` | 连通分量（社区） |
| `GET /api/project/{id}/graph/export` | `handle_graph_export` | `graph.rs:350` | 全量导出（默认上限 2000） |
| `GET /api/project/{id}/graph/impact` | `handle_graph_impact` | `graph.rs:387` | 某文件变更的影响面（直接/传递依赖者 + 影响分） |

以 `handle_graph_ego`（`graph.rs:172-211`）为例，它已通过 `convert_subgraph(&graph)` 把内部图转换为 `GraphSubgraphResponse { success, relation_epoch, nodes, edges, relation_info }` 返回。**数据结构与 Cytoscape 的 `elements` 模型天然同构。**

### 2.2 数据契约（请求/响应模型）

定义于 `crates/app/cce-api/src/models/graph.rs`：

- `GraphNode { id, label, kind, source_file, source_location }`（`graph.rs:11`）
- `GraphEdge { source, target, relation, confidence }`（`graph.rs:21`）
- 查询参数：`EgoQuery { entity_id, depth(默认2), direction(默认"both") }`（`graph.rs:30`）、`GraphPathQuery`、`SubgraphQuery { ids }`、`ExportQuery { limit }`、`ImpactQuery { file }`
- 响应：`GraphSubgraphResponse`（`graph.rs:68`）、`GraphPathResponse`（`graph.rs:79`）、`GraphComponentsResponse`（`graph.rs:93`）、`GraphImpactResponse`（`graph.rs:103`）
- 所有响应均带 `relation_epoch` —— 这是**前端缓存失效的关键版本号**（见 §10）。

内部领域模型亦有对应：`crates/parser/cce-relation/src/types.rs:96-102` 的 `CallChainGraph { nodes, edges }`。

### 2.3 关系类型已四维分类（驱动着色与过滤）

`crates/core/cce-types/src/types/relation/classification.rs:189-314` 的 `RelationType` 枚举覆盖四类，并提供判定谓词：

| 域 | 代表变体 | 判定方法 | 行号 |
|----|----------|----------|------|
| 调用 Call | `DirectCall`/`InstanceMethodCall`/`AsyncCall`/`ConstructorCall` 等 13 种 | `is_call()` | `:421` |
| 依赖 Dependency | `IncludeLocal`/`ImportStandard`/`Use`/`ModuleDependency` 等 10 种 | `is_dependency()` | `:441` |
| 结构 Structural | `Inheritance`/`Implementation`/`TraitBound`/`Contains`/`Embedding` 等 | `is_structural()` | `:458` |
| 引用/模板 Reference & Template | `TypeReference`/`FieldAccess`/`ParameterBinding`/`EventCallback` 等 | — | `:293-313` |

`relation` 字段以字符串序列化（如 `call.direct`、`inheritance`、`dependency.import.named`），可直接作为 Cytoscape 的边样式选择器键。

---

## 3. 事实核查：graphify 的图展示实现（参照对象）

### 3.1 数据 schema 与 CCE 同构

`graphify/ARCHITECTURE.md` 的「Extraction output schema」定义：

```json
{ "nodes": [{"id","label","source_file","source_location"}],
  "edges": [{"source","target","relation","confidence":"EXTRACTED|INFERRED|AMBIGUOUS"}] }
```

与 CCE 的 `GraphNode`/`GraphEdge` 几乎一一对应（`confidence` 语义在 CCE 中同样存在，见 §7.3）。这意味着**「类 graphify 图展示」在数据结构层面无阻抗失配**。

### 3.2 渲染选型：Mermaid，而非 Cytoscape

`graphify/graphify/callflow_html.py:1708` 通过 CDN 引入 `mermaid@11`：

```html
<script src="https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.min.js"></script>
```

即：graphify 把图当成**「文本声明 → SVG 图」的文档产物**，本质是一次性导出，不是可程序化交互的图引擎。

### 3.3 交互与产物形态（值得借鉴的部分）

`callflow_html.py` 在 Mermaid SVG 之上手搓了一组交互（`callflow_html.py:1876-1966`）：

- 平移缩放工具条：放大/缩小/适应宽度/Fit/Reset（`callflow_html.py:1876-1885`），缩放区间 0.25–3（`callflow_html.py:1895-1919`）；
- 拖拽平移 + Ctrl/Cmd+滚轮缩放（`callflow_html.py:1937-1966`）；
- 按 community/section **分段**（每 section 一张 flowchart + 一张调用明细表）；
- **hyperedges**（分组/群关系）与**统计面板**（节点/边/社区计数、置信度分布）。

这些「分段概览 + 明细 + 统计 + 平移缩放」是 graphify 体验的精华，本方案应在 Cytoscape 上复现并增强。

---

## 4. 差距分析（Gap）

| 能力 | graphify | CCE 现状 | 缺口 |
|------|----------|----------|------|
| 图数据底座 | NetworkX + JSON | 6 个 `/graph/*` 接口 + `relation_epoch` | ✅ 已具备（更优：可版本化、可遍历） |
| 图渲染 | Mermaid（静态 SVG） | 仅 `CallGraph.svelte` 线性 SVG、`InheritanceTree.svelte` HTML 列表 | ❌ 无通用图渲染 |
| node-link 数据模型 | 有 | `GraphNode`/`GraphEdge` 有 | ✅ 已具备 |
| 前端图 API client | — | `frontend/src/lib/api/` 无 `graph.ts` | ❌ 需新增 |
| 点击节点实时扩边 | 无（静态） | 无 | ❌ 需新增（Cytoscape 强项） |
| 关系类型过滤 | 无（仅分段） | 后端 `RelationType` 已分类 | ❌ 前端需兑现 |
| 影响面分析 | 无 | `/graph/impact` 已就绪 | ❌ 前端未接 |
| 平移/缩放/适配 | 有（手搓 toolbar） | 无 | ❌ 需新增（Cytoscape 内置 + 借鉴 graphify toolbar） |

**结论：缺口集中在前端可视化层与「接口→视图」的连接，后端几乎无需改造。**

---

## 5. 选型：为什么是 Cytoscape.js

| 候选 | 适配度 | 关键理由 |
|------|--------|----------|
| **Cytoscape.js** | ✅ 推荐 | 专为图论/网络可视化设计；多种布局（cose/concentric/breadthfirst/compound）；CSS 选择器式样式；生态成熟（cy-spread、cy-context-menus、cytoscape-cose-bilkent 等）；canvas 渲染可承载大图 |
| Mermaid（graphify 用） | ⚠️ 不推荐 | 文本→SVG 的文档工具，**无逐节点事件、无程序化布局控制、大图/环图易崩**，不适合实时探索 |
| vis-network | ◯ 备选 | 物理布局开箱即用、上手快，但复合节点（社区分组）与样式精细度弱于 Cytoscape |
| D3 force | ◯ 备选 | 最灵活但开发量大、需自管渲染与交互 |
| AntV G6 | ◯ 备选 | 能力强但生态偏国内、与 Svelte 集成示例少 |

> Cytoscape 的「compound node（父节点分组）」可直接映射 graphify 的 section/community 分段；「selector 样式」可直接吃 `RelationType` 的字符串值做边着色。**这两点正是「类 graphify 体验」与「CCE 关系四域」的最佳契合点。**

---

## 6. 总体架构设计

分层原则：**后端只读复用 + 前端新增视图层**，不改动 Rust 图计算逻辑。

```
┌──────────────────────────────────────────────────────────┐
│ Frontend (SvelteKit + Svelte 5)                            │
│  routes/graph/[id]/+page.svelte   ← 新增独立图视图路由      │
│  lib/components/graph/                                            │
│     GraphCanvas.svelte        ← Cytoscape 容器(onMount 初始化)  │
│     GraphToolbar.svelte       ← 平移/缩放/Fit/过滤（借鉴 §3.3） │
│     GraphFilterPanel.svelte   ← 按 RelationType 域 / kind 过滤  │
│     EntityDetailDrawer.svelte ← 点击节点详情                   │
│  lib/api/graph.ts             ← 新增：封装 6 个 /graph/* 调用   │
│  lib/stores/graph.ts          ← 图状态（当前 project、elements）│
└───────────────────────────┬──────────────────────────────┘
                             │ HTTPS /api/project/{id}/graph/*
┌───────────────────────────┴──────────────────────────────┐
│ Backend (Rust, cce-server)  — 基本不动                       │
│  handlers/graph.rs  →  GraphSubgraphResponse ...            │
│  models/graph.rs    →  GraphNode / GraphEdge                │
│  RelationRuntime    →  relation_epoch 版本号                 │
└──────────────────────────────────────────────────────────┘
```

**SSR 注意事项**：Cytoscape 依赖 `window`，必须在 `onMount`（或 `browser` 守卫）内初始化，禁止在 SvelteKit 服务端渲染阶段调用。

---

## 7. 数据契约与映射

### 7.1 `GraphSubgraphResponse` → Cytoscape `elements`

后端返回 `nodes: GraphNode[]` 与 `edges: GraphEdge[]`，前端做 1:1 转换：

- 节点 `data`：`{ id, label, kind, sourceFile, sourceLocation }`
- 边 `data`：`{ id: source+'->'+target+'@'+relation, source, target, relation, confidence }`

（仅做字段名映射与边唯一 id 拼接，不新增后端字段即可满足第一阶段需求。）

### 7.2 `RelationType` → 颜色/线型映射（前端样式表，对标 graphify 分段着色）

| 关系域 | Cytoscape 边样式键（data.relation 前缀） | 建议视觉 |
|--------|------------------------------------------|----------|
| 调用 `call.*` | `edge[relation ^= "call."]` | 实线、主色（如 `#38bdf8`） |
| 依赖 `dependency.*` | `edge[relation ^= "dependency."]` | 虚线、灰蓝 `#64748b` |
| 结构 `inheritance`/`implementation`/`trait*`/`contains`/`embedding`/`mixin` | `edge[relation = "inheritance"], ...` | 粗线、紫 `#a78bfa` |
| 引用/模板 `type_reference`/`field_access`/`parameter.binding`/`callback.event`/`contains.element` | 对应选择器 | 细线、低饱和 `#94a3b8` |

节点 `kind`（function/class/interface/struct…）用形状/填充区分（如 class=矩形、function=圆角、interface=虚线边框）。

### 7.3 `confidence` 处理

CCE 边含 `confidence` 字段（与 graphify 的 `EXTRACTED|INFERRED|AMBIGUOUS` 同语义）。建议：

- `EXTRACTED` → 实线不透明；
- `INFERRED` → 半透明；
- `AMBIGUOUS` → 虚线 + 警示色，并在详情抽屉提示「待人工确认」。

---

## 8. 布局与样式策略

| 场景 | Cytoscape 布局 | 说明 |
|------|----------------|------|
| 默认 ego / 子图 | `cose`（或 `cose-bilkent` 插件） | 力导向，自动避开交叉 |
| 调用链/继承层级 | `breadthfirst`（direction: LR/TB） | 对标 graphify 的 flowchart 走向 |
| 社区/section 概览 | `concentric` + **compound node 分组** | 父节点 = community/section，子节点入组，复刻 graphify 分区 |
| 全量导出（>500 节点） | `cose` + 渐进加载（见 §10） | 避免一次性布局卡顿 |

样式采用 Cytoscape 原生 `stylesheet`（`selector` + `style`），与 §7.2 映射表一一对应，无需改后端。

---

## 9. 交互功能设计（对标 graphify + 增强）

1. **平移/缩放工具条**（直接借鉴 `callflow_html.py:1876-1966` 的交互设计）：放大/缩小/Fit/Reset + 拖拽平移 + Ctrl/Cmd+滚轮缩放。Cytoscape 内置 `cy.zoom()`/`cy.fit()`/`cy.pan()`，实现量小。
2. **点击节点 → ego 扩展**：点击 `GraphCanvas` 中节点，调用 `GET /graph/ego?entity_id=...&depth=1&direction=both`，把返回的子图 `merge` 进当前 `elements`（去重 id）。这是 graphify 静态图做不到的实时探索。
3. **关系类型 / kind 过滤**：`GraphFilterPanel` 用 §7.2 的域分组做复选，前端对 `elements` 做显示/隐藏（`cy.$(...).style('display','none')`），无需回源。
4. **实体定位/搜索**：输入实体名或 id，调用 `GET /graph/subgraph?ids=...` 或复用现有实体搜索，定位并 `cy.center()`/`cy.animate()`。
5. **详情抽屉**：点击节点弹出 `EntityDetailDrawer`，展示 `label/kind/source_file:source_location` 与「查看调用者/被调用者/继承」快捷入口。
6. **影响分析**：选中文件或实体后调用 `GET /graph/impact`，把 `direct_dependents`/`transitive_dependents` 高亮为红色，呼应 graphify 无此能力但 CCE 已具备的优势。
7. **社区/section 概览**：`GET /graph/components` 取连通分量，用 compound node 分组 + `concentric` 布局，复刻 graphify 的「分区概览」。

---

## 10. 增量加载与性能

- **`relation_epoch` 缓存**：所有图响应带 `relation_epoch`（`handlers/graph.rs` 各函数均返回）。前端 `lib/stores/graph.ts` 以 `(project_id, relation_epoch)` 为键缓存 `elements`，索引重建后 epoch 变化即失效——直接复用后端已有的版本机制，零额外成本。
- **拒绝一次性全量导出**：`/graph/export` 默认上限 2000（`models/graph.rs:126`）。第一阶段**不默认拉全量**，改为「实体页进入时先取 ego(depth=2)，用户点击再扩边」的渐进策略，控制单屏节点数。
- **大图保护**：当 `nodes` 超过阈值（建议 800）时，提示用户改用过滤/子图，或切换为 `cose-bilkent` 并发布局。
- **布局防抖**：连续扩边时对新节点做局部布局或 `cy.layout().run()` 前 `stop()`，避免整图抖动。

---

## 11. 实现步骤与文件清单

**P0 — 依赖与脚手架（前端）**
- `frontend/package.json`（`frontend/package.json:12-23`）新增 `cytoscape` 与可选 `cytoscape-cose-bilkent`、`cytoscape-fcose`；`npm install`。
- 新建 `frontend/src/lib/api/graph.ts`：封装 ego/path/subgraph/components/export/impact 六个调用，统一返回 `GraphNode[]`/`GraphEdge[]` 与 `relation_epoch`。
- 新建 `frontend/src/lib/stores/graph.ts`：维护 `projectId`、`elements`、`relationEpoch`、缓存与 `mergeEgo()`。

**P1 — 核心画布**
- 新建 `frontend/src/lib/components/graph/GraphCanvas.svelte`：`onMount` 内 `cytoscape({ container, elements, layout: cose, style })`；导出 `getCy()` 供 toolbar/filter 调用。
- `frontend/src/lib/components/graph/GraphToolbar.svelte`：缩放/Fit/Reset（交互逻辑对标 `callflow_html.py:1876-1966`）。
- 样式表实现 §7.2 的 `RelationType`→颜色/线型映射与 §7.3 的 `confidence` 处理。

**P2 — 路由与实体页集成**
- 新建 `frontend/src/routes/graph/[id]/+page.svelte`：独立图视图，进入时加载当前 project 的 ego 概览。
- 在 `frontend/src/routes/entities/[id]/+page.svelte` 现有实体详情中嵌入「在图中查看」按钮，跳转/联动 `GraphCanvas`。

**P3 — 交互增强**
- `GraphFilterPanel.svelte`：关系域 + kind 过滤。
- 点击节点 → `mergeEgo()` 扩边（§9.2）。
- `EntityDetailDrawer.svelte`：节点详情。
- 影响分析入口接 `/graph/impact`（§9.6）。

**P4 — 概览与性能**
- `/graph/components` → compound node 社区分组 + `concentric`（§9.7）。
- `relation_epoch` 缓存接入（§10）。

**后端（可选小补，非必须）**
- 若前端需要节点「所属社区 id」以驱动分组，可在 `GraphNode`（`models/graph.rs:11`）增 `community: Option<String>` 字段并由 `convert_subgraph` 填充——属增强项，第一阶段可先用 `/graph/components` 在前端自行归组。

---

## 12. 风险与待拍板开放点

**风险**
- **包体积**：Cytoscape 约 300KB+，叠加布局插件会增大 `frontend` bundle；建议动态 import 或在图视图路由懒加载。
- **SvelteKit SSR**：Cytoscape 强依赖 `window`，初始化须置于 `onMount`/浏览器守卫，否则构建期报错。
- **大图性能**：全量导出（2000 节点）力导向布局可能卡顿，需依赖 §10 的渐进策略与阈值保护。
- **样式/设计语言**：现有前端为「Swiss Minimalist / Tech Industrial」黑白风（`frontend/README.md:7-12`），Cytoscape 配色需对齐该基调（克制用色、硬边框）。

**待拍板开放点（需产品/架构确认）**
1. **入口形态**：独立 `/graph/[id]` 路由 vs 在实体详情页内嵌画布？还是两者都要？
2. **默认首屏子图**：进入图视图时默认拉「全项目 ego(depth=2)」还是「仅当前选中实体」？需结合典型项目规模定阈值。
3. **社区分组数据源**：前端用 `/graph/components` 自行归组，还是后端在 `GraphNode` 补 `community` 字段？影响 §11 P4 与后端是否改动。
4. **图库引入方式**：npm 打包 vs 沿用 graphify 式的 CDN `<script>`？CCE 为自托管仪表盘，倾向 npm 打包（可控、离线可用），但需确认 CSP/构建策略。
5. **与现有 `CallGraph.svelte` 的关系**：保留线性调用链视图（适合单链阅读）还是用 Cytoscape 的 `breadthfirst` 布局直接替代？建议「并存、各有侧重」。
6. **置信度可视化强度**：是否对 `AMBIGUOUS` 边默认折叠/弱化，避免噪声淹没主结构？
7. **多语言/深色主题**：Cytoscape 容器是否跟随现有深色仪表盘主题，样式变量如何注入？

---

> 附：关键事实溯源
> - 路由：`crates/app/cce-server/src/api/router.rs:120-144`
> - 处理器：`crates/app/cce-server/src/api/handlers/graph.rs:172/214/257/308/350/387`
> - 模型：`crates/app/cce-api/src/models/graph.rs:11/21/30/68/79/93/103`
> - 关系类型：`crates/core/cce-types/src/types/relation/classification.rs:189-314,421,441,458`
> - 前端现状：`frontend/src/lib/components/entities/CallGraph.svelte`、`InheritanceTree.svelte`、`frontend/package.json:12-23`
> - 参照 graphify：`graphify/ARCHITECTURE.md`（Extraction output schema）、`graphify/graphify/callflow_html.py:1708,1876-1966`
