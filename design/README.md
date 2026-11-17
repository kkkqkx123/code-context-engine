# design/

本目录存放 Web 端与桌面端共用的设计 token 数据文件。

## `tokens.json`

`tokens.json` 是 `frontend/src/app.css` 与桌面端（`crates/app/cce-desktop`）之间共享的设计 token
单一数据源，**不是设计稿或原型残留，不能删除**。

实际使用位置：

- 桌面客户端编译期嵌入：`crates/app/cce-desktop/src/design_tokens.rs`
  桌面端的主题、颜色、字号、间距等都从该文件读取。
- 前端同步检查：`frontend/scripts/check-design-tokens.mjs`
  作为 `npm run check:tokens` / `npm run check` 的一部分，校验 `tokens.json` 与
  `frontend/src/app.css` 中的 `:root` 变量保持一致（双向：任一侧增删改都会失败）。

其中 `chrome`、`ops`、`accent`、`gray`、`status`、`statusBg`、`statusDark`、`spacing`、
`borders`、`shape`、`breakpoints`、`nodes`、`typography` 各块与 `frontend/src/app.css` 的
`:root` 分组同步；`desktop` 块存放桌面端专属值（系统字体栈与按下态颜色，Web 侧无对应物）。

如需修改设计 token，请同时更新 `frontend/src/app.css` 与本文件，并运行：

```sh
cd frontend && npm run check:tokens
```

以及桌面端检查（清单路径，不经过根工作区）：

```sh
cargo test --manifest-path crates/app/cce-desktop/Cargo.toml
```
