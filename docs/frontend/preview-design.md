# Frontend Preview 项目设计方案

## 概述

`frontend-preview` 是一个独立的前端项目，用于在无后端环境下审查前端设计风格是否符合预期。项目从 `frontend` 镜像除 API 客户端外的全部源文件（组件、路由、stores、工具函数、类型、静态资源），通过 Mock API 层提供硬编码数据，渲染效果与正式前端一致，仅数据来源不同。

**项目目的**：在无后端环境下，完整预览前端页面的视觉效果、交互行为和整体设计风格。

## 技术栈

- 框架：SvelteKit + Svelte 5 (runes API)，与主前端同版本
- 构建工具：Vite 8
- 样式：原生 CSS（Swiss Minimalist 设计系统）
- 图渲染：cytoscape 系依赖，与主前端一致
- 请求层：openapi-fetch，主前端用其直连后端，预览用其做类型检查并可切换真实后端

## 架构原理

```
┌─────────────────────────────────────────────────────────────┐
│                      frontend-preview                        │
├─────────────────────────────────────────────────────────────┤
│  routes/（含 ops/*、+layout.ts） │ 镜像自 frontend           │
│  components/ / stores/（含 auth）│ 镜像自 frontend           │
│  lib/*.ts（如 dead-letter-actions）│ 镜像自 frontend        │
│  api/*（除 client.ts）           │ 镜像自 frontend           │
├─────────────────────────────────────────────────────────────┤
│  api/client.ts     │ 预览专属：Mock 分发或透传真实后端       │
│  mock/data.ts      │ 硬编码响应，全部以 OpenAPI 类型注解      │
│  mock/client.ts    │ 字符串路由到对应 mock 数据              │
├─────────────────────────────────────────────────────────────┤
│  .env              │ VITE_USE_MOCK=true 启用 mock 模式       │
└─────────────────────────────────────────────────────────────┘
```

**关键点**：

- 镜像范围是除客户端、Mock 树、环境文件外的全部前端源码，删除的文件也会被同步删除。
- 仅 API 客户端层为预览专属实现；Mock 数据以 OpenAPI 契约类型注解，漂移会在类型检查时暴露。
- 环境变量 `VITE_USE_MOCK=true` 走 Mock 数据，为空或 false 时透传真实后端。
- 真实模式下行为与主前端一致：同样读取已存储的准入令牌并注入请求头；区别是默认直连后端地址而非同源代理。

## 目录结构

```
frontend-preview/
├── package.json                # 名称保留，依赖与主前端同步
├── svelte.config.js
├── vite.config.ts              # 无后端代理，用后端直连地址
├── tsconfig.json
├── .env                        # VITE_USE_MOCK=true
├── src/
│   ├── app.html
│   ├── app.css
│   ├── lib/
│   │   ├── *.ts                # 顶层辅助函数，镜像
│   │   ├── api/
│   │   │   ├── client.ts       # 预览专属（Mock 分发＋真实透传）
│   │   │   ├── schema.d.ts     # 镜像，OpenAPI 生成
│   │   │   └── <各域门面>      # 镜像：含 search、entities、graph、
│   │   │                       #   dead-letters、config、metrics 等
│   │   ├── components/         # 镜像
│   │   ├── stores/             # 镜像，含 auth、graph、project
│   │   ├── utils/              # 镜像
│   │   └── mock/
│   │       ├── data.ts         # Mock 数据，契约类型注解
│   │       └── client.ts       # Mock 客户端实现
│   ├── types/                  # 镜像，第三方模块垫片
│   └── routes/                 # 镜像，含 ops/* 与 +layout.ts
```

## Mock 数据设计

在 `src/lib/mock/data.ts` 中定义所有 API 的硬编码响应数据，按项目管理、索引统计、健康状态、搜索结果、实体详情、关系图谱、存储状态、系统指标、文件监听、配置信息、开发工具、摘要生成、Qdrant 进程分类组织。

约束：

- 每个响应常量必须以对应 OpenAPI 响应类型注解，新增或变更后端字段后先改 Mock 形状再合入。
- Mock 客户端按端点字符串路由，未覆盖的新端点只告警并返回空对象，需在合入新页面或新接口时同步补齐。

## 同步策略

使用 `scripts/sync-frontend-preview.sh` 脚本同步 `frontend` 的变更，同步为镜像语义。

### 同步内容

- `static/**/*` 静态资源
- `src/app.html`、`src/app.css`、`src/app.d.ts`
- `src/lib/*.ts` 顶层辅助函数
- `src/lib/components/**/*` 所有组件
- `src/lib/stores/*.ts` 所有 store
- `src/lib/utils/*.ts` 工具函数
- `src/lib/api/*.ts`（除 client.ts）API 模块
- `src/types/*.d.ts` 环境声明
- `src/routes/**/*` 所有路由
- `package.json` 的 dependencies 与 devDependencies

### 保留内容（不覆盖）

- `src/lib/api/client.ts` 预览专属版本
- `src/lib/mock/*` Mock 数据文件
- `.env` Mock 模式配置

## 开发流程

主前端变更后运行同步脚本，再补 Mock 数据与路由，最后做类型检查与启动验证。新增 API 端点时需同步在 Mock 数据文件和 Mock 客户端中添加对应数据和路由，否则新页面在 Mock 模式下只能拿到空对象。

## 验证清单

- 所有页面正确渲染（含运维、死信、关系图谱页）
- UI 组件与正式前端一致
- 交互行为正常（按钮点击、开关切换、搜索等）
- Mock 数据正确显示（项目列表、搜索结果、实体详情等）
- 预览项目类型检查零错误
- 响应式布局在移动端正常
- 无障碍特性（焦点、aria属性）正常

## 维护说明

当 `frontend` 项目更新时：

1. 运行同步脚本拉齐镜像文件。
2. 新增 API 端点时补齐 Mock 数据和路由。
3. 运行预览项目的类型检查，确认 Mock 形状与契约一致。
4. 启动服务验证。
