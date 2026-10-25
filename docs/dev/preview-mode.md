# Frontend 预览模式(Preview Mode)

预览能力已内化为 `frontend` 的运行模式,`frontend-preview` 独立项目与同步脚本已删除。

## 用法

```shell
cd frontend
npm run dev:preview   # 无后端预览,mock 数据
npm run build:preview # 预览构建
npm run check:preview # 预览模式类型检查
```

## 机制

- Vite 以 `--mode preview` 运行时,通过 `resolve.alias` 将 `#api/client` 指向 `src/lib/api/client.preview.ts`(mock 分发实现);默认模式经 tsconfig `paths` 解析到真实客户端 `src/lib/api/client.ts`。
- `frontend/.env.preview` 设置 `VITE_USE_MOCK=true` 启用 mock 数据。
- mock 数据位于 `frontend/src/lib/preview/mock/`,直接修改即可,两种模式自动保持一致。

详细设计见 `docs/frontend/preview-design.md`。
