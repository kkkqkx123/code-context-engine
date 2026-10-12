# Frontend 大文件分析（>800 行）

## 清单与结论

| 文件 | 行数 | 结论 |
|---|---|---|
| `src/lib/api/schema.d.ts` | 6871 | 自动生成产物（OpenAPI schema），不手工拆分 |
| `src/routes/graph/+page.svelte` | 2073 → 557 | 已拆分（见 `graph-page-split-plan.md`） |
| `src/lib/utils/graph-style.ts` | 871 → 已删除 | 已拆分为三个职责模块 |
| `src/routes/config/+page.svelte` | 862 → 405 | 已拆分 |

## `src/lib/utils/graph-style.ts`（871 行，已拆分）

原文件混合了三类关注点，已按消费方拆分并删除原文件（引用直接更新）：

- `graph-domains.ts`（447 行）：领域/节点类型元数据与标签函数（`RELATION_DOMAINS`、`NODE_KINDS`、`relationLabel`、`domainLabel`、`NODE_SHAPE_LEGEND`、`CONFIDENCE_META` 等），供图例、过滤面板、详情面板使用
- `graph-elements.ts`（147 行）：Cytoscape 元素转换（`GraphElement` 系列、`toElementNode` / `toElementEdge`、`edgeElementId`），供画布与 store 使用
- `graph-stylesheet.ts`（303 行）：Cytoscape 样式表（`graphStylesheet`、`NODE_SHAPE_STYLE`），仅画布初始化使用

`parseList` 一类通用文本工具归入 `src/lib/utils/text.ts`。

## `src/routes/config/+page.svelte`（862 行，已拆分）

六块功能全部提取为 `src/lib/components/config/` 下的卡片组件：

| 组件 | 行数 | 职责 |
|---|---|---|
| `ConfigTokenCard.svelte` | 99 | 远程访问令牌 |
| `ConfigGlobalCard.svelte` | 153 | 全局配置快照（只读） |
| `ConfigValidationCard.svelte` | 154 | 全局校验结果 |
| `ConfigProjectBasicsCard.svelte` | 170 | 项目基础信息表单 |
| `ConfigRuntimeCard.svelte` | 203 | 运行时配置 JSON 编辑器 |

页面 405 行，保留全部请求编排与表单状态（所有表单值通过 `bind:` 下传），因此未保存的编辑在 tab 切换时依然保留，与拆分前语义一致。卡片组件纯展示，样式随组件走（未做全局化，避免与其他页面的同名 scoped 类串扰）。

## graph 页面拆分结果（已实施）

- 页面 2073 → 557 行，只保留编排与布局。
- 新组件（`src/lib/components/graph/`）：`GraphSeedBar`、`GraphCandidatePanel`、`GraphNodeInspector`、`GraphCommunitiesPanel`、`GraphCyclesPanel`、`GraphModulePanel`、`GraphLegend`，均在 530 行以内。
- 新共享模块：`src/lib/stores/stale-query.svelte.ts`（epoch 感知查询状态机）、`src/lib/utils/graph-params.ts`（种子输入/过滤/数值钳制纯函数）、`src/lib/utils/graph-viewport.ts` 增补视口操作纯函数。
- `svelte-check` 0 errors / 0 warnings。
