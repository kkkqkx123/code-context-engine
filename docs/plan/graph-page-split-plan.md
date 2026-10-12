# Graph 页面拆分方案

## 背景

`frontend/src/routes/graph/+page.svelte` 当前共 2073 行（script 约 528 行，模板约 1545 行），是 frontend 中唯一超出 1200 行阈值的手写源文件。另一个超阈值文件 `src/lib/api/schema.d.ts` 为自动生成产物，不纳入本次处理。

该页面将至少 7 块相互独立的功能全部内联实现：

1. 种子搜索与符号候选（seedMode / seedId / candidates / URL 参数联动）
2. 影响面分析（impactFile / impactMode / impactDepth / runImpact）
3. 环检测（cyclesLevel / runCycles / sortedCycles / loadCycleSubgraph）
4. 结构化关系查询（structuralKind / runStructural / focusPeer）
5. 模块信息（runModule / moduleData：exports、caller_files、imports）
6. 社区检测与下钻（communities / loadCommunity）
7. 画布视口控制与导出（exportPng / fitViewport / zoomViewport / relayout / centerOn）

此外还有横切状态：过期标记（isStale / cyclesStale 等基于 relation_epoch 的派生态）、错误与加载态在各功能块间重复出现。已有组件 `GraphCanvas`、`GraphFilterPanel`、`GraphToolbar`、`EntityGraphView` 仅覆盖渲染层，逻辑层与面板模板仍堆在页面里。

## 结论

职责过多，需要拆分。拆分方向是"一个功能块 = 一个组件 + 一个状态模块"，页面只保留布局与模块间编排。

## 拆分方案

### 1. 按功能块提取 Svelte 组件（新增于 `src/lib/components/graph/`）

- `SeedPanel.svelte`：种子输入、模式切换、符号候选下拉与 URL 参数回填。入参为当前 seed，回调 `onRun(seed)`。
- `ImpactPanel.svelte`：影响面表单、执行结果消息与"上溯调用文件"快捷入口。
- `CyclesPanel.svelte`：环级别/上限设置、环列表展示、点击环加载子图。
- `StructuralPanel.svelte`：结构化查询表单与关系结果列表（含过期提示）。
- `ModulePanel.svelte`：模块 exports / caller_files / imports 三段展示。
- `NodeInspector.svelte`：选中节点的详情卡片与选中边列表（含 cfg_condition 徽标）。
- `CommunityPanel.svelte`：社区列表与下钻加载。

各面板组件自行持有表单输入态（如输入框内容），分析结果态通过回调上抛或以 props 传入；页面对每个面板只保留"数据 + loading + error"三个统一形状的属性。

### 2. 提取共享的"过期感知查询"模式

各功能块重复实现 `xxxLoading / xxxError / xxxEpoch / xxxStale` 与基于 epoch 比较的派生逻辑。将其收敛为一个可复用的模块（如 `src/lib/graph/stale-query.svelte.ts`）：封装"发起请求 → 记录 epoch → 派生 stale → 竞态丢弃"的模式，五个查询块共用。

### 3. 逻辑层归入状态与工具模块

- 视口控制（exportPng / fitViewport / zoom / relayout / centerOn）抽为 `graph-viewport.ts`，以 Cytoscape 实例为参数的纯函数，便于单测。
- `domainLabel`、`serverFilter`、`clampOverviewLimit`、RELATION_DOMAINS / NODE_SHAPE_LEGEND 等常量与纯函数移入 `src/lib/graph/` 下的既有或新模块，页面不再定义。

### 4. 页面文件最终形态

`+page.svelte` 保留：顶层 `$derived` 的 store 装配、初始加载、节点选中/激活事件分发、组件编排与整体布局，目标控制在 300 行以内。拆分后单文件均应低于 400 行。

## 实施顺序

1. 先做纯函数与常量外移（第 3 点，风险最低，无 UI 变化）。
2. 提取 `NodeInspector` 与各结果展示面板（纯展示，改动可逐个目检）。
3. 提取各表单面板并接入统一的 stale-query 模式（第 2 点，行为回归点集中在竞态与过期提示）。
4. 每步完成后以 `npm run check`（svelte-check）与既有测试验证；最后人工走查种子→影响面→环→结构化→模块的完整链路。

## 验收标准

- `+page.svelte` 低于 300 行，所有新组件低于 400 行。
- 无 `any`、无行为回退；过期提示与竞态防护在所有查询块行为一致。
- 拆分仅为结构重组，URL 参数联动、键盘事件、导出等功能保持原语义。
