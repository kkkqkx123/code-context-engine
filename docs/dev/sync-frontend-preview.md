# Frontend Preview 同步脚本说明

## 概述

`sync-frontend-preview.sh` 脚本用于将 `frontend` 项目的完整源文件同步到 `frontend-preview` 项目，确保预览环境与主项目保持一致。脚本会保留 preview 专属的 mock 数据和配置。

## 位置

```
scripts/sync-frontend-preview.sh
```

## 使用方法

### 从项目根目录执行

```bash
./scripts/sync-frontend-preview.sh
```

### 从 scripts 目录执行

```bash
cd scripts && ./sync-frontend-preview.sh
```

## 同步内容

脚本会同步以下文件：

| 源文件 | 目标文件 | 说明 |
|--------|----------|------|
| `frontend/src/app.html` | `frontend-preview/src/app.html` | HTML 模板 |
| `frontend/src/app.css` | `frontend-preview/src/app.css` | 全局样式和设计系统 |
| `frontend/src/lib/components/**/*` | `frontend-preview/src/lib/components/**/*` | 所有组件（ui、index、search、entities、tools） |
| `frontend/src/lib/stores/*.ts` | `frontend-preview/src/lib/stores/*.ts` | 所有 store |
| `frontend/src/lib/api/*.ts` | `frontend-preview/src/lib/api/*.ts` | API 模块（跳过 client.ts） |
| `frontend/src/routes/**/*` | `frontend-preview/src/routes/**/*` | 所有路由页面 |
| `frontend/package.json` | `frontend-preview/package.json` | 仅同步 devDependencies |

## 保留内容（不覆盖）

以下文件会被保留，不会被同步覆盖：

- `frontend-preview/src/lib/api/client.ts` - Mock-enabled 版本，支持 mock 模式
- `frontend-preview/src/lib/mock/*` - Mock 数据文件
- `frontend-preview/.env` - Mock 模式配置（`VITE_USE_MOCK=true`）

## 前置条件

- `jq` 命令行 JSON 处理工具（用于同步 package.json）

### 安装 jq

```bash
# Ubuntu/Debian
sudo apt-get install jq

# macOS
brew install jq
```

## 工作流程

当 `frontend` 项目更新后：

1. 运行同步脚本：
   ```bash
   ./scripts/sync-frontend-preview.sh
   ```

2. 如果新增了 API 端点，需要手动更新 mock 数据：
   - 编辑 `frontend-preview/src/lib/mock/data.ts` 添加新的 mock 数据
   - 编辑 `frontend-preview/src/lib/mock/client.ts` 添加新的路由

3. 进入 preview 目录安装依赖（如有新增依赖）：
   ```bash
   cd frontend-preview
   npm install
   ```

4. 启动预览服务验证：
   ```bash
   npm run dev
   ```

## 添加新 API 的 Mock

当 `frontend` 新增 API 端点时：

### 1. 在 `src/lib/mock/data.ts` 中添加 mock 数据

```typescript
export const mockNewEndpoint = {
  success: true,
  data: {
    // 硬编码的响应数据
  }
};
```

### 2. 在 `src/lib/mock/client.ts` 中添加路由

```typescript
export const mockClient = {
  async get<T>(endpoint: string): Promise<T> {
    // ...
    if (endpoint === '/api/new-endpoint') return mockData.mockNewEndpoint as T;
    // ...
  }
};
```

## 自动化

可将此脚本集成到 CI/CD 流程中，确保 `frontend-preview` 与 `frontend` 保持同步：

```yaml
# .github/workflows/sync-preview.yml
jobs:
  sync-preview:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install jq
        run: sudo apt-get install -y jq
      - name: Sync frontend-preview
        run: ./scripts/sync-frontend-preview.sh
      - name: Check for changes
        run: git diff --exit-code frontend-preview/
```
