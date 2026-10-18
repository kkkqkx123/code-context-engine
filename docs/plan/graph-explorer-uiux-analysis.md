# 图查询页面（Graph Explorer）功能与 UI/UX 分析

分析对象：`frontend/src/routes/graph/+page.svelte` 及其组件、`frontend/src/lib/stores/graph.ts`、
`frontend/src/lib/utils/graph-*.ts`、`frontend/src/lib/api/graph.ts`。

`frontend-preview` 是从 `frontend` 镜像而来（见 `docs/frontend/preview-design.md`），其图查询页与
`frontend` 逐字节一致，因此本分析以 `frontend` 为准。

---

## 1. 功能清单

### 1.1 载入策略（GraphSeedBar，四种互斥模式）

| 模式 | 输入 | 行为 | 后端端点 |
|------|------|------|----------|
| Focus entity | 符号名 / `path#name` / `#name` / 稳定 id；depth 1–4；dir both/out/in | 载入 ego 邻域并**替换**工作集 | `GET /graph/ego` |
| Project overview | limit 1–10000（默认 400） | 按 hub 优先（出度降序）导出项目切片并**替换** | `GET /graph/export` |
| Find path | start、end、depth 1–100 | 两点最短关系路径并**替换**；无路径保留原图并报 `NO_PATH` | `GET /graph/path` |
| Subgraph | 逗号分隔 id（前端截断 200） | 给定 id 集合的诱导子图并**替换** | `GET /graph/subgraph` |

- 支持 Enter 提交。
- 支持从实体详情页通过 `?entity=` / `?file=` 跳转，自动聚焦并预载文件影响分析。

### 1.2 画布与视口（GraphToolbar + GraphCanvas）

- 渲染引擎：Cytoscape（懒加载）+ cose-bilkent；扩展加载失败自动回退 `cose`。
- 视口操作：放大/缩小（×1.25 等比步进）、Fit、Reset、Re-layout、Export PNG。
- 五种布局：Force / Force(basic) / Hierarchy / Concentric / Grid。
- 实时计数：节点数 / 边数。
- 交互提示：拖拽平移、Ctrl/Cmd+滚轮缩放、**双击节点扩一圈邻域**（合并式、幂等）。
- 800 节点渲染上限 + 超限横幅；空图占位提示。

### 1.3 节点检查器（GraphNodeInspector）

- 元信息：label、kind（含描述）、`file:line` 位置、所属 Community。
- 动作：Open entity（跳详情页）、Expand、Center。
- 已载入关系列表（截断 40 条）：IN/OUT 徽章、domain、关系标签、对端 id、置信度、`cfg(...)` 守卫。
- Structural relations：8 种关系族 × out/in × limit，逐条 Open / Focus。

### 1.4 影响分析（Impact）

- File 模式：变更文件 → 直接/间接依赖方，画布高亮（直接=粗蓝边，间接=细蓝边），底部信息横幅汇总数量，可 Clear。
- Entity 模式：变更实体 + scope（entity/file）+ depth，同上。

### 1.5 模块关系（Module）

- 输入文件路径 → Exports（点击 Focus 到画布）、Caller files（点击触发文件影响分析）、Imports（模块级出边列表）。
- 与种子栏的 Module 按钮共用同一查询槽。

### 1.6 社区与依赖环

- Communities：连通分量，按规模降序；点击把成员作为诱导子图载入；数据仅在关系索引前进时按需重取。
- Cycles：entity / file 级别、limit；结果按环大小降序；entity 级提供「Load as subgraph」。

### 1.7 过滤与图例

- 关系域开关：6 个域（call/dependency/structural/reference/template/other），纯客户端控制边可见性；不在图中的域置灰禁用。
- 节点搜索：≥2 字符时高亮匹配节点并淡化其余（边随之淡化）。
- 边标签开关。
- 高级服务器过滤（折叠）：relation types、external edges（下次 Load 生效）。
- 图例：关系域线型/颜色 + 节点形状。

### 1.8 状态与跨页复用

- 歧义消解：符号名多命中（`AMBIGUOUS_SYMBOL`）时给出候选列表，点选后用稳定 id 重试。
- 陈旧 epoch 横幅（手动 Reload）、truncated 横幅、错误横幅、各面板 loading。
- 实体详情页的 Call Graph / Inheritance 通过 `EntityGraphView` 复用同一渲染器与样式模型。

---

## 2. UI/UX 合理性评估

### 2.1 设计亮点

1. **视觉编码有单一事实源且不冗余。** `graph-domains.ts` 统一产出域颜色/线型、节点形状、置信度透明度；
   `graph-stylesheet.ts`、`GraphLegend`、`GraphFilterPanel` 均从它读取。注释明确「宽度编码依赖强度、
   颜色与线型编码域、形状编码实体类型、透明度编码置信度/条件」，避免同一事实被多重编码。
2. **渐进披露。** 高级服务器过滤默认折叠；边标签默认关闭；布局切换收在工具栏。
3. **语义正确。** 区分 `replaceGraph`（载入）与 `mergeGraph`（扩展）；`mergeGraph` 在 epoch 前进时丢弃旧数据；
   `StaleQuery` 让面板查询 epoch 感知；800 节点上限避免渲染失控。
4. **交互反馈明确。** 陈旧横幅给显式 Reload（不静默丢弃）；错误横幅；歧义候选；loading 文案。
5. **无障碍基础。** aria-label / aria-pressed / role、skip link、focus-visible、prefers-reduced-motion、
   对比度令牌带比值注释。
6. **渲染器复用。** `EntityGraphView` 复用同一画布，实体详情页不维护第二套渲染逻辑。
7. **纯函数拆分。** `graph-params.ts` / `graph-viewport.ts` / `graph-elements.ts` 可独立测试。

### 2.2 问题清单

| 编号 | 维度 | 问题 | 证据 | 影响 |
|------|------|------|------|------|
| A1 | 信息架构 | 种子栏职责过载、无视觉分区 | `GraphSeedBar.svelte` 单个 flex-wrap 容器内混入 4 种种子模式、影响分析（file/entity + scope + depth）、模块查询、高级服务器过滤，约 15+ 控件，仅 mode-btn 一组有分组 | 认知负荷高，难以建立心智模型 |
| A2 | 布局 | 三栏宽度固定、画布被压缩 | `+page.svelte` 的 `.workspace` 为 `220px minmax(0,1fr) 280px`；inspector 关系列表 `max-height:420px` 内滚 | 1440 宽屏下画布仅约 880px，节点多时频繁滚动 |
| A3 | 布局 | 结果面板整宽堆叠在画布下方 | `GraphCommunitiesPanel` / `GraphCyclesPanel` / `GraphModulePanel` 顺序渲染为整宽横条；与画布无空间关联 | 页面纵向很长，结果无法映射回图上位置 |
| A4 | 交互 | 影响分析结果无列表、不可逐项跳转 | `runImpact()` 仅设置 `impactMessage` 计数与 `meta.impactDirect/impactTransitive` 高亮，无依赖方列表 UI | 用户看到数量但无法聚焦具体依赖方 |
| A5 | 信息架构 | 图例位于页面最底部且与过滤器重复 | `GraphLegend.svelte` 在页面末尾；域开关已带色块 | 需滚到底才能理解颜色；域信息重复 |
| B1 | 反馈 | 扩展/载入时画布无反馈 | `expand()` 置 `store.loading`，仅体现在工具栏计数；画布无遮罩/骨架 | 用户感知不到正在加载 |
| B2 | 交互 | 搜索只淡化、不聚焦 | `GraphCanvas.applyDecorations()` 对非匹配节点加 `dimmed`，无居中/放大、无匹配计数 | 大图中仍需自行寻找匹配节点 |
| B3 | 引导 | 手势仅靠底部一行小字提示 | `canvas-hint` 文案 | 新用户难以发现双击扩展等能力 |
| B4 | 交互 | Fit / Reset / Re-layout 语义易混 | `GraphToolbar.svelte` 三个按钮仅 title 提示 | 用户需试错区分 |
| C1 | 一致性 | stale Reload 文案与行为存在张力 | 横幅文案只说 epoch 前进，但 Reload 调用 `runSeed()` 会 `replaceGraph` 重建当前视图 | 用户可能误以为只是刷新数据 |
| C2 | 状态 | 载入逻辑重复 | `loadInitialGraph()` 与 `runSeed()` 均包含种子分支 | 后续改动需同步两处 |
| C3 | 反馈 | 歧义候选覆盖旧图上下文 | 候选面板仅在 `store.error` 时出现，旧图仍保留但无提示 | 上下文丢失 |
| D1 | 可访问性 | 画布无键盘操作 | 节点为 Cytoscape canvas 元素，非 DOM；`role="application"` 但无键盘选择/缩放 | 键盘与读屏用户不可用 |
| D2 | 响应式 | 小屏塌成单列且无 sticky 画布 | `@media (max-width:1200px)` 三栏变单列，顺序为过滤器→画布→inspector | 页面过长，画布不在视口内 |
| D3 | 可访问性 | 小字号灰色文本对比不足 | `--gray-400 #8a8a8a` 注释标注 3.31:1（仅装饰），但 inspector 的 `relation-target` / `relation-confidence` / `panel-empty` 等使用 gray-400/500 | 可读性偏弱 |
| E1 | 结构 | 控件样式多处复制 | `.inline-field` / `.inline-label` / `.seed-input` / `.ghost-btn` / `.small-btn` 在 `GraphSeedBar`、`GraphNodeInspector`、`GraphCyclesPanel` 各自重复定义 | 样式漂移风险 |
| E2 | 结构 | 视口函数未复用 | `EntityGraphView.svelte` 自行实现 fit/zoom/reset/export，未使用 `graph-viewport.ts` | 行为可能不一致 |
| E3 | 结构 | 依赖追踪脆弱 | `GraphCanvas.svelte` 的 `$effect` 中手工 `void visibleDomains; ...` 追踪依赖 | 依赖 Svelte 运行时细节，易漏更新 |

---

## 3. 修改方案

方案按「先信息架构、再交互反馈、后可访问性与结构」的顺序推进，避免在未稳定结构前做细节打磨。

### 3.1 信息架构（优先级最高）

1. **拆分种子栏为三个语义区。** 将「载入」（4 种种子模式 + 高级服务器过滤）、「影响分析」（file/entity + scope/depth + Clear）、「模块查询」分为三个带标题与分隔的独立区块，或改为标签页/抽屉。目标是让「载入视图」「高亮分析」「模块查询」三类操作的边界清晰。
2. **结果面板与画布建立主从关系。** Communities / Cycles / Module 改为可折叠抽屉或 inspector 的标签页，点击结果项时画布联动（Focus/Center/加载子图）。
3. **去重 Community 概念。** inspector 的 Community 字段与 Communities 面板表达同一事实，保留其一，或在 inspector 内联展示分量摘要。
4. **图例前置并精简。** 将图例移到画布附近（如工具栏右侧或悬浮卡片），并移除与域开关重复的部分，仅保留节点形状与线型说明。

### 3.2 画布反馈与交互

5. **补充加载反馈。** 画布加载/扩展时显示遮罩或进度指示，避免仅靠工具栏计数。
6. **搜索增强。** 显示匹配计数，并提供「聚焦匹配」操作（居中并适度放大匹配节点）。
7. **工具栏语义澄清。** 为 Fit / Reset / Re-layout 补充tooltip或分组说明，明确三者差异。
8. **首次访问引导。** 一次性 onboarding 提示双击扩展、滚轮缩放等核心手势。
9. **影响分析增加依赖方列表。** 直接/间接依赖方分组展示，支持逐项 Focus / Center，与画布高亮联动。

### 3.3 可访问性与响应式

10. **键盘可达。** 为画布提供等效的键盘操作（如方向键选择、Enter 扩展、+/- 缩放、F 适配），或提供节点列表视图作为 canvas 的可访问替代。
11. **小屏布局。** 画布 sticky 于顶部，结果面板默认折叠，减少纵向滚动。
12. **文本对比提升。** 正文/辅助文本统一使用 `--gray-500` 及以上，`--gray-400` 仅保留给非文本装饰。

### 3.4 一致性收敛

13. **抽取共享控件样式。** 将 `.inline-field` / `.inline-label` / `.seed-input` / `.ghost-btn` / `.small-btn` 收敛为共享样式或 UI 组件，消除多处复制。
14. **复用视口函数。** `EntityGraphView` 改用 `graph-viewport.ts`，与主图查询页保持一致行为。
15. **统一载入逻辑。** 合并 `loadInitialGraph()` 与 `runSeed()` 的种子分支，消除重复。
16. **统一 stale 文案与行为。** 明确 Reload 会按当前种子模式重建视图，必要时提供「保留扩展、仅刷新元数据」的选项。

### 3.5 实施顺序建议

| 阶段 | 内容 | 理由 |
|------|------|------|
| 一 | 3.1 全部 + 3.2 第 5、9 项 | 先稳定信息架构与画布反馈，收益最大且互不依赖 |
| 二 | 3.2 其余 + 3.3 | 交互细节与可访问性在结构稳定后收敛 |
| 三 | 3.4 全部 | 结构收敛放最后，避免反复改动已稳定的组件 |
| 四 | 回归验证 | 复用现有 `frontend/tests/*.test.mjs` 的 passthrough 断言风格，补充对新增共享样式/视口复用的约束 |

---

## 4. 验证方式

- 复用 `frontend/tests/` 现有 node:test 断言风格（对源码文本做结构约束），在拆分/抽取后确认关键标记仍存在。
- `npm run check`（svelte-check）保持 0 error / 0 warning。
- `npm run lint` / `npm run format:check` 通过。
- 手动核对：种子栏分区、结果面板联动、影响分析列表、键盘操作、小屏布局。
