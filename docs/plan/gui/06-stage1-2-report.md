# 阶段 1-2 落地报告

## 交付内容

阶段 1（样式事实）：新增 `design/tokens.json`（86 个变量，与 `frontend/src/app.css`
的 `:root` 分组一一对应）与 `design/README.md`；新增
`frontend/scripts/check-design-tokens.mjs`（双向校验，任一侧增删改都失败），并通过
`check:tokens` 接入前端 `check` 流程；桌面端 `design_tokens.rs` 在编译期嵌入同一份
文件，提供颜色、长度、字体栈三类读取，缺失按 `None` 处理。

阶段 2（桌面骨架）：新增独立工作区 crate `crates/app/cce-desktop`，包含应用入口、
全局界面状态、设置持久化、运行时桥接、客户端持有、连接探测与心跳、主题应用、
国际化、通用构件（页面头、页面体、空态）、连接表单、主窗口（侧边导航、占位面板、
实时健康面板、状态栏）与构建说明 `DEV.md`。

## 验证结果

- 前端设计 token 校验通过（86 个变量）；前端既有单测 21 项全部通过。
- 桌面端 `cargo check --all-targets` 零警告；`cargo clippy --all-targets` 零警告；
  `cargo fmt --check` 通过；`cargo test` 15 项全部通过。
- 根工作区解析不受影响（`cargo metadata` 通过，既有成员不变）。

## 与方案的偏差及原因

- 根 `Cargo.toml` 的 `exclude` 新增 `crates/gpui-kit` 与 `crates/app/cce-desktop`。
  实测确认：缺少该声明时，桌面工作区同时依赖根成员与 gpui-kit 会使 gpui-kit 的
  工作区继承被归因到根工作区导致解析失败；声明后 gpui-kit 回到自身工作区解析。
  这是本仓库虚拟工作区形态下的必要声明，linkrs 的根包工作区不受此影响。
- 托盘推迟到阶段 4：本仓库 gpui-kit 的 tray 等子模块尚未初始化且环境无外部网络，
  当前关闭窗口即持久化几何并退出，`DEV.md` 已记录排障对照。
- `Section` 枚举仅覆盖桌面已消费的分组；`spacing`、`borders`、`breakpoints`、
  `statusDark` 暂留前端所有，随首个消费者补入。
- 深色外观不提前实现：`System` 主题解析为浅色，与 Web 侧保持一致。
- 可停靠面板随阶段 3 首个业务面板落地；阶段 2 为侧边导航加占位面板。
