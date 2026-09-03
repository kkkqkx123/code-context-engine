# Frontend Preview 项目设计方案

## 概述

`frontend-preview` 是一个独立的前端项目，用于审查前端设计风格是否符合预期。项目从 `frontend` 目录完整复制所有源文件（组件、路由、stores），通过 Mock API 层提供硬编码数据，渲染效果与正式前端完全一致，仅数据来源不同。

**项目目的**：在无后端环境下，完整预览前端页面的视觉效果、交互行为和整体设计风格。

## 技术栈

- 框架：SvelteKit + Svelte 5 (runes API)
- 构建工具：Vite 8
- 样式：原生 CSS（Swiss Minimalist 设计系统）
- 运行时依赖：无（仅 devDependencies）

## 架构原理

```
┌─────────────────────────────────────────────────────────────┐
│                      frontend-preview                        │
├─────────────────────────────────────────────────────────────┤
│  routes/           │  完整复制自 frontend，渲染真实页面      │
│  components/       │  完整复制自 frontend，使用真实组件      │
│  stores/           │  完整复制自 frontend，使用真实 store    │
├─────────────────────────────────────────────────────────────┤
│  api/client.ts     │  Mock-enabled 版本，拦截所有请求       │
│  mock/data.ts      │  硬编码的 API 响应数据                 │
│  mock/client.ts    │  Mock 客户端，路由到对应 mock 数据      │
├─────────────────────────────────────────────────────────────┤
│  .env              │  VITE_USE_MOCK=true 启用 mock 模式     │
└─────────────────────────────────────────────────────────────┘
```

**关键点**：
- 组件、路由、stores 完全一致，确保渲染效果相同
- 仅 API 客户端层被替换为 mock 实现
- 通过环境变量 `VITE_USE_MOCK=true` 控制是否使用 mock 数据

## 目录结构

```
frontend-preview/
├── package.json
├── svelte.config.js
├── vite.config.ts
├── tsconfig.json
├── .env                          # VITE_USE_MOCK=true
├── src/
│   ├── app.html
│   ├── app.css                   # 完整复制自 frontend
│   ├── lib/
│   │   ├── api/
│   │   │   ├── client.ts         # Mock-enabled 版本
│   │   │   ├── index.ts          # 完整复制
│   │   │   ├── search.ts         # 完整复制
│   │   │   ├── entities.ts       # 完整复制
│   │   │   ├── health.ts         # 完整复制
│   │   │   ├── storage.ts        # 完整复制
│   │   │   ├── metrics.ts        # 完整复制
│   │   │   ├── watch.ts          # 完整复制
│   │   │   ├── tools.ts          # 完整复制
│   │   │   ├── summary.ts        # 完整复制
│   │   │   ├── qdrant.ts         # 完整复制
│   │   │   └── config.ts         # 完整复制
│   │   ├── components/           # 完整复制自 frontend
│   │   │   ├── ui/               # UI 基础组件
│   │   │   ├── index/            # 索引管理组件
│   │   │   ├── search/           # 搜索组件
│   │   │   ├── entities/         # 实体组件
│   │   │   └── tools/            # 工具组件
│   │   ├── stores/               # 完整复制自 frontend
│   │   │   ├── index.ts
│   │   │   ├── search.ts
│   │   │   ├── entities.ts
│   │   │   ├── health.ts
│   │   │   ├── storage.ts
│   │   │   ├── metrics.ts
│   │   │   ├── watch.ts
│   │   │   ├── toast.ts
│   │   │   ├── network.ts
│   │   │   └── project.ts
│   │   └── mock/
│   │       ├── data.ts           # 所有 API 的 mock 数据
│   │       └── client.ts         # Mock 客户端实现
│   └── routes/                   # 完整复制自 frontend
│       ├── +layout.svelte
│       ├── +page.svelte          # Dashboard
│       ├── index/                # 索引管理
│       ├── search/               # 搜索
│       ├── entities/             # 实体浏览
│       ├── storage/              # 存储管理
│       ├── watch/                # 文件监听
│       ├── tools/                # 开发工具
│       ├── config/               # 配置管理
│       ├── summary/              # 摘要生成
│       └── projects/             # 项目管理
```

## Mock 数据设计

在 `src/lib/mock/data.ts` 中定义所有 API 的硬编码响应数据：

### 数据分类

| 类别 | Mock 数据 | 覆盖页面 |
|------|-----------|----------|
| 项目管理 | `mockProjects` | /projects, /index |
| 索引统计 | `mockIndexStats` | / |
| 健康状态 | `mockHealthStatus`, `mockQdrantHealth` 等 | /storage |
| 搜索结果 | `mockSearchResults` | /search |
| 实体详情 | `mockFunctionDetail`, `mockCallChain` 等 | /entities |
| 存储状态 | `mockStorageStatus` | /storage |
| 系统指标 | `mockMetricsData` | / |
| 文件监听 | `mockWatchStatus` | /watch |
| 配置信息 | `mockConfigInfo`, `mockConfigValidation` | /config |
| 开发工具 | `mockCompressResult`, `mockDiagnoseResult` | /tools |
| 摘要生成 | `mockSummaryResult` | /summary |

### Mock 客户端

`src/lib/mock/client.ts` 实现了与 `ApiClient` 相同的接口，根据 endpoint 路由到对应的 mock 数据：

```typescript
export const mockClient = {
  async get<T>(endpoint: string): Promise<T> {
    // 根据 endpoint 返回对应的 mock 数据
  },
  async post<T>(endpoint: string, data?: unknown): Promise<T> {
    // 根据 endpoint 返回对应的 mock 数据
  },
  // ...
};
```

## 同步策略

使用 `scripts/sync-frontend-preview.sh` 脚本同步 `frontend` 的变更：

### 同步内容

- `src/app.html`, `src/app.css` - 全局样式
- `src/lib/components/**/*` - 所有组件
- `src/lib/stores/*.ts` - 所有 store
- `src/lib/api/*.ts` (except client.ts) - API 模块
- `src/routes/**/*` - 所有路由
- `package.json` devDependencies

### 保留内容（不覆盖）

- `src/lib/api/client.ts` - Mock-enabled 版本
- `src/lib/mock/*` - Mock 数据文件
- `.env` - Mock 模式配置

## 开发流程

### 1. 初始化项目

```bash
# 复制 frontend 项目基础配置
cp frontend/package.json frontend-preview/
cp frontend/svelte.config.js frontend-preview/
cp frontend/vite.config.ts frontend-preview/
cp frontend/tsconfig.json frontend-preview/

# 修改 vite.config.ts，移除后端代理配置
# 修改 package.json name 为 cce-frontend-preview
```

### 2. 复制源文件

```bash
# 完整复制 src 目录
cp -r frontend/src frontend-preview/src

# 创建 mock 目录
mkdir -p frontend-preview/src/lib/mock

# 创建 mock 数据文件（见 src/lib/mock/data.ts）
# 创建 mock 客户端（见 src/lib/mock/client.ts）
# 修改 api/client.ts 添加 mock 支持
```

### 3. 配置 Mock 模式

创建 `.env` 文件：

```
VITE_USE_MOCK=true
```

### 4. 运行和验证

```bash
cd frontend-preview
npm install
npm run dev
# 访问 http://localhost:3002
```

## 验证清单

- [ ] 所有页面正确渲染（Dashboard、索引、搜索、实体、存储等）
- [ ] UI 组件与正式前端完全一致
- [ ] 交互行为正常（按钮点击、开关切换、搜索等）
- [ ] Mock 数据正确显示（项目列表、搜索结果、实体详情等）
- [ ] 响应式布局在移动端正常
- [ ] 无障碍特性（焦点、aria属性）正常

## 维护说明

当 `frontend` 项目更新时：

1. 运行同步脚本：
   ```bash
   ./scripts/sync-frontend-preview.sh
   ```

2. 如果新增了 API 端点，需要在 `src/lib/mock/data.ts` 和 `src/lib/mock/client.ts` 中添加对应的 mock 数据和路由。

3. 启动服务验证：
   ```bash
   cd frontend-preview
   npm run dev
   ```
