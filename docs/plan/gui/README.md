# CCE 桌面应用（GPUI）设计方案

本目录回答一个问题：当前项目（服务端加命令行加 Web 前端）该如何补充一个 GPUI 原生桌面客户端，
同时让 Web 前端与桌面端共用同一套样式设计。

现状一句话：CCE 已有完整的远端调用面（`cce-api` 契约加 `cce-client` 类型化客户端加 OpenAPI 快照加前端
TypeScript 投影），桌面端不需要再发明调用层，只缺视图与交互状态层；样式层面缺的是跨端单一数据源，
linkrs 的 `design/tokens.json` 做法可直接借鉴，但要按 CCE 现有样式体系重分区，而不是照搬 linkrs 的
Tailwind 分区。

## 文档地图

- 现状与差距见 `01-current-state-and-gap.md`。
- 共用样式设计见 `02-shared-design-tokens.md`，这是本方案的前置条件，先做。
- 桌面端架构见 `03-desktop-architecture.md`，包括 crate 选址、工作区隔离、依赖方向与模块划分。
- 视图与服务映射见 `04-views-and-services.md`，即前端路由如何对应到桌面面板与服务。
- 分阶段落地计划与验收见 `05-implementation-plan.md`。

## 核心结论

- 新增同层 crate `crates/app/cce-desktop`，只做视图与交互状态，数据访问全部经由 `cce-client`。
- 新建仓内 `design/tokens.json` 作为样式单一数据源，Web 侧 `frontend/src/app.css` 与桌面侧编译期嵌入
  同一份文件，双向校验脚本保证不漂移。
- 桌面 crate 采用独立工作区根（仿 linkrs-gpui），不并入根工作区，通过清单路径构建与测试。
- 连接模型比 linkrs 简单：CCE 只有基地址加承载令牌，没有用户口令加会话加空间切换，桌面端不复制
  linkrs 的会话管理器形态。
- 范围分两批：先完成壳、导航、设计 token、设置持久化与搜索查询图谱实体等高频面板，再补齐运维类面板。
