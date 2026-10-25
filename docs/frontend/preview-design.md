# Frontend Preview 运行模式设计

## 概述

预览能力是 `frontend` 的一种**运行模式**,不再是独立镜像项目。通过 Vite 的 `mode` 机制,`preview` 模式将 API 客户端替换为基于本地 fixture(mock 数据)的实现,用于在无后端环境下审查前端设计;默认 `development`/`production` 模式使用真实后端,且构建产物中不包含任何 mock 代码。

## 技术栈

- 框架:SvelteKit + Svelte 5 (runes API)
- 构建工具:Vite 8
- 请求层:openapi-fetch,真实客户端直连后端,preview 客户端可分发到 mock 数据

## 架构原理

```
┌────────────────────────────────────────────────────────────┐
│                        frontend/src                        │
├────────────────────────────────────────────────────────────┤
│  api/client.ts        │ 默认(真实)客户端                    │
│  api/client.preview.ts│ preview 专属:mock 分发或透传后端     │
│  preview/mock/data.ts │ 硬编码响应,全部以 OpenAPI 类型注解    │
│  preview/mock/client.ts│ 字符串路由到对应 mock 数据           │
├────────────────────────────────────────────────────────────┤
│  .env.preview        │ VITE_USE_MOCK=true 启用 mock 模式     │
└────────────────────────────────────────────────────────────┘
```

**关键点**:

- 所有 API 服务模块从 `#api/client` 别名导入。默认模式经 tsconfig `paths` 解析到 `api/client.ts`(真实客户端);preview 模式由 Vite `resolve.alias` 覆盖为 `api/client.preview.ts`。
- 未选中的分支及其依赖(`preview/mock/`、`client.preview.ts`)不会进入默认构建的模块图,Rollup 不会打包。
- 共享代码禁止直接 import `preview/` 下的模块;mock 数据以 OpenAPI 契约类型注解,漂移会在类型检查时暴露。
- preview 构建在启动时校验 `VITE_USE_MOCK`,误配时输出警告,避免生产误启用 mock。

## 使用方式

```shell
npm run dev           # 默认模式,直连真实后端
npm run dev:preview   # preview 模式,mock 数据,无需后端
npm run build         # 默认构建,构建后自动执行产物隔离检查
npm run build:preview # preview 构建
npm run check         # 默认模式类型检查(真实客户端严格类型)
npm run check:preview # preview 模式类型检查(宽松类型)
```

## 验收

- `npm run build` 产物不包含任何 mock/preview 代码(`scripts/check-build-isolation.mjs` 自动校验)。
- `npm run build:preview` 可正常构建并使用 fixture 数据运行。
- `npm run check` 与 `npm run check:preview` 均通过。
- mock 数据更新直接修改 `src/lib/preview/mock/`,两种模式天然同步,无需任何同步脚本。
