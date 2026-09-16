# 前端布局改造：侧栏 + 表单式内容区

> 适用范围：仅 `frontend/` 目录（SvelteKit）。`frontend-preview/` 由 `scripts/sync-frontend-preview.sh` 同步生成，不手工编辑。
> 文档约定：遵循仓库 `AGENTS.md` —— 代码与注释用英文，文档用中文；不在代码注释中引用本方案标识；本方案以自然语言描述改动，不粘贴完整代码片段。

## 1. 背景与结论

当前 `frontend/` 是一套「营销落地页」骨架：顶部水平导航（7 个链接）+ 居中容器 + 页脚，Dashboard 使用巨型标题与大面积留白。而 CCE 前端实际是「代码索引 / 检索 / 分析」工具控制台，功能页面有 11 个，水平导航无法承载完整功能地图，且缺少跨页面持久上下文（服务健康、当前项目）。

**结论：改为「固定侧栏导航 + 顶栏 + 表单式内容区」的管理后台模式。** 迁移的是布局骨架与信息架构，不照搬任何参考实现；保留既有 SvelteKit 多页路由与瑞士极简视觉风格（Space Grotesk/Mono、1px 边框、accent 红仅用于强调），仅把字号与间距从「展示密度」调整为「工具密度」。

## 2. 目标布局

- 左 `aside`（约 240px，深色 `#0a0a0a`）：品牌区 → 全局状态块（服务在线指示灯 + 当前项目下拉）→ 分组导航（Overview / Data / Search / System）→ 版本与离线提示。桌面持久，窄屏收为抽屉（复用汉堡逻辑）。
- 右 `main`：顶栏（58px，面包屑「分组 / 页面」+ 右侧存储组件健康点 + 时钟）→ 内容区（`+layout.svelte` 的 `.content`，统一内边距与横向溢出控制）。
- 移除全局页脚；Config / Summary / Projects 进入主导航，建立完整功能地图。

分组与路由映射（定义于 `frontend/src/routes/+layout.svelte`）：

| 分组 | 页面 |
|---|---|
| Overview | Dashboard `/` |
| Data | Projects `/projects`、Index `/index`、Watch `/watch` |
| Search | Search `/search`、Entities `/entities`、Summary `/summary` |
| System | Storage `/storage`、Tools `/tools`、Config `/config` |

## 3. 改动清单

### 3.1 新增文件

- `frontend/src/lib/components/ui/PageHeader.svelte`：页面头构件。左侧标题 + 一句描述，右侧可选 `children` 动作槽。取代各页巨型 `h1` / hero。
- `frontend/src/lib/components/ui/Toolbar.svelte`：列表/表格工具条构件。左槽放搜索与筛选，右槽放刷新与主操作。已在 Search 结果区试用，作为「筛选 → 查询 → 操作」统一形态的起点。

> 分组导航配置（`navGroups` 与面包屑解析 `resolveCrumb`）直接内联在 `frontend/src/routes/+layout.svelte` 的 `<script>` 中，而非独立文件。原因：同步脚本 `scripts/sync-frontend-preview.sh` 仅复制 `components/`、`stores/`、`api/`、`routes/`、`app.css`、`app.html`，不覆盖 `src/lib/` 根级文件；内联可保证 `frontend-preview` 无需额外改动即可构建。

### 3.2 修改文件

- `frontend/src/routes/+layout.svelte`：整文件重写为侧栏 + 顶栏骨架。导入 `nav`、`health`/`metrics`/`index`/`project` store；`onMount` 中 `loadProjects()` 并启动 health（15s）/ metrics（30s）自动刷新与时钟；`onDestroy` 清理。侧栏状态块读取 `$metricsState.lastUpdated` 与 `$metricsState.error` 推断服务在线；顶栏健康点读取 `$metricsState.storageStatus` 四项连接态。
- `frontend/src/lib/stores/project.ts`：`currentProjectId` 改为持久化（`$app/environment` 的 `browser` 守卫下读写 `localStorage`），使当前项目选择跨刷新保留。
- `frontend/src/app.css`：新增控制台布局辅助类——`.page`、`.kpi-grid`/`.kpi-card`、`.table-wrap`、`.form-grid-2`、`.quick-grid`/`.quick-tile`，沿用现有设计 token。
- 11 个路由页面（`+page.svelte` 即 Dashboard、`index`、`search`、`entities`、`entities/[id]`、`projects`、`storage`、`summary`、`tools`、`watch`、`config`）：外壳由 `<section class="section">` + 巨型 `h1`/hero 改为 `<div class="page">` + `<PageHeader>`；业务逻辑、store、组件调用均不变。Dashboard 额外增加 KPI 行（项目数 / 向量数 / BM25 文档数 / 服务状态）与快捷操作磁贴，去掉 hero。

### 3.3 各页面映射

| 页面 | 目标形态 |
|---|---|
| Dashboard | KPI 行 + 快捷操作磁贴，去 hero |
| Index / Watch / Storage / Tools / Config / Summary | PageHeader + 既有参数表单 / 进度日志（LogViewer 保留） |
| Projects / Search / Entities | PageHeader + 列表/表格；搜索结果区试用 Toolbar |
| Entities/[id] | 面包屑追加实体名（`resolveCrumb` + `page.params.id`），详情沿用既有卡片 |

## 4. 全局上下文

- **服务健康**：`healthState` / `metricsState` 已在各页按需轮询；现上收至 `+layout.svelte` 统一驱动，侧栏状态块与顶栏健康点共享，避免每页重复拉取。
- **当前项目**：侧栏「Current Project」下拉绑定 `currentProjectId`，选项来自 `projects` store（`projectApi.listProjects()`）；变更写入持久化 store。各页既有的 `currentProjectId` 引用无需改动即可获得跨页一致选择。

## 5. 风险与未做部分

- 不引入单页化 / `innerHTML` 切换，保留 SvelteKit 路由（可分享 URL、深链、代码分割）。
- 不引入登录页与账号体系。
- 不引入顶层标签页机制；Tools / Config / Summary 页面内部已存在的 tab 为既有实现，本次仅替换其外层页面壳，内部 tab 保持不变（属后续可选优化，见开放点）。
- 侧栏约 240px 固定宽度，窄屏依赖抽屉；桌面优先场景可接受。
- `frontend-preview/` 由同步脚本生成：同步会覆盖除 `client.ts`（mock 版）与 `mock/` 外的全部文件。本方案 patch 仅包含 `frontend/` 与 `docs/plan/`；应用 patch 后运行 `scripts/sync-frontend-preview.sh` 即可刷新预览。

## 6. 待拍板开放点

1. **项目下拉空态**：`projects` 为空时下拉仅显示 `#<id>`。是否在空态引导跳转 Projects 页面创建项目？
2. **Tools / Config / Summary 内部 tab**：是否后续拆分为侧栏独立入口或页内分节卡片（而非保留内部 tab）？
3. **KPI 指标来源**：Dashboard 目前从 `metricsState.storageStatus` 取向量 / BM25 文档数；实体数是否需新增接口或复用现有统计？
4. **侧栏宽度与分组命名**：240px 与 Overview/Data/Search/System 分组是否合适，是否需按实际埋点调整顺序？
5. **顶栏健康点交互**：当前仅作状态指示，是否点击展开详细诊断（跳转 Storage / Health）？

## 7. 验证方式

- 应用 patch 后：`cd frontend-preview && npm install && npm run dev`（mock 模式，无需后端）做视觉回归。
- 同步：`bash scripts/sync-frontend-preview.sh`（保留 mock `client.ts` 与 `mock/`）。
- 由于当前环境 npm registry 不可达，未执行 `svelte-check`；请在本地或预览环境完成类型/编译校验与视觉回归。
