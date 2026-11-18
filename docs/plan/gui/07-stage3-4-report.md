# 阶段 3-4 落地报告

## 交付内容

阶段 3（高频业务面板）交付检索、图谱、实体、索引、项目五个面板，均包含视图、服务、
历史记录与空态/错误态；图谱面板承接检索结果的"送入图谱"能力。

阶段 4（运维面板组）交付工具、存储、监视、健康、指标、配置、任务队列七个面板，
并把构建文档、系统依赖与排障对照收敛到 `crates/app/cce-desktop/DEV.md`。

新增模块：

| 层次 | 文件 | 说明 |
|---|---|---|
| 服务 | `services/request.rs` | `Load<T>`（空闲/加载/完成/失败）+ `Failure`（消息键 + 传输细节）+ `spawn_into`，全局面板共用同一套请求生命周期 |
| 服务 | `services/history.rs` | 按面板种类持久化的近期输入，上限 20 条、单条 512 字符，空白与超长直接丢弃 |
| 服务 | `services/project_service.rs` | 列表、创建、删除、索引、配置读取与重载、`refresh_scope` |
| 服务 | `services/search_service.rs` | 单查询与聚合查询、查询类型、`graph_seed`（取首个稳定符号，缺失时回退 `file:line`） |
| 服务 | `services/graph_service.rs` | 邻域、路径、子图、连通分量、环、影响、模块、结构与导出 |
| 服务 | `services/entity_service.rs` | 详情、调用、被调用、调用链、调用路径、继承、实现、分类统计 |
| 服务 | `services/index_service.rs` | 全量、增量、单文件解析、摘要、统计 |
| 服务 | `services/tools_service.rs` | 折叠、压缩、诊断、关键词、符号、引用、定义 |
| 服务 | `services/storage_service.rs` | 状态、统计、按范围清空、按文件/实体删除、批量删除 |
| 服务 | `services/watch_service.rs` | 状态、启动、停止，默认 500ms 去抖 |
| 服务 | `services/ops_service.rs` | 健康四路探测、指标文本/快照/历史与清理、配置信息/校验/重载、重试队列与死信 |
| 构件 | `components/notice.rs` | 加载行、失败块、提示、空态、堆叠 |
| 构件 | `components/data_grid.rs` | 只读结果网格（列宽固定/自适应、四种色调）、表面容器、统计行 |
| 构件 | `components/form.rs` | 字段、字段组、操作行 |
| 构件 | `components/choice.rs` | 选项按钮行（目录键 ↔ 契约取值） |
| 视图 | `views/{search,graph,entity,index,projects,tools,storage,watch,health,metrics,config,tasks}_view.rs` | 十二个面板 |

改写的既有模块：`views/main_window.rs` 改为两组导航（工作区 5 项、运维 7 项）、面板惰性
创建、范围变化即整体丢弃、投递槽消费后切到图谱、状态栏显示活动项目；`state.rs` 增加
`ProjectEntry`、`GraphSeed` 与投递槽（单调生成号去重）；`settings.rs` 增加 `history` 表；
`views/connection_view.rs` 在连通后刷新项目范围；`messages/{en,zh}.json` 扩到 443 键，
`i18n.rs` 增加目录键集一致与键名命名空间化两个守卫测试。

## 验证结果

```
cargo test                             48 passed; 0 failed
cargo clippy --all-targets -- -D warnings   Finished（零警告）
cargo fmt --check                      通过
```

覆盖范围：`services` 全部十个模块的参数组装与边界、`state` 的范围回退与投递去重、
`settings` 的落盘往返（含历史记录）、`i18n` 的三条守卫、设计 token、`components` 的
网格与选项辅助函数。

链接期实测确认系统依赖缺一不可：只装 `libxkbcommon-dev` 时链接失败于
`unable to find library -lxkbcommon-x11`，补装 `libxkbcommon-x11-dev` 与 `libwayland-dev`
后测试可执行。该组合与新增排障行已写入 `DEV.md`。

## 与方案的偏差及原因

- **可停靠面板未做**：方案写"侧边导航加可停靠面板"，实际为侧边导航加固定内容区，
  面板内部用页签切换能力。组件库未提供可用的停靠实现，自建停靠会引入一套与本阶段
  无关的拖拽与布局状态机；页签已能让每个能力独立触发与验收。停靠留作独立事项。
- **布局未版本化持久化**：当前落盘的是窗口几何、主题、密度、语言与近期输入，不含
  导航选中项与面板布局。因停靠未落地，布局尚无稳定身份，此时持久化只会固化一个待推翻
  的结构。方案 05 的"布局升级后重置"因此暂不适用。
- **托盘仍未落地**：`crates/gpui-kit` 的 `tray` 等子模块未初始化，关闭窗口即持久化并
  退出进程。`DEV.md` 的前提与排障表已按此事实描述，未写成已支持。
- **表格导出未做**：方案提到批量导出放在服务层，实际只交付了图谱面板的后端导出接口，
  结果网格未提供 CSV 落盘。理由是导出格式（列选择、编码、落盘位置）缺少既定约定，
  先按只读呈现交付，避免自创格式后再改。
- **工具链下限上调**：`rust-version` 从 `1.86` 改为 `1.97`。`gpui-pre` 使用
  `std::hint::cold_path`，1.93 实测编译失败（`E0658`），1.97 确认通过。原值会让 cargo
  接受一个必定失败的版本，属误导性声明。
- **不新增依赖**：指标历史的时间窗口与 RFC3339 格式化在 `ops_service` 内手写，
  未引入时间库；`components/data_grid.rs` 未用组件库的 `DataTable`，只读结果不需要
  选择态与虚拟化状态，代码内已注明该取舍。
- **面板错误态统一为"消息键 + 传输细节"**：未对每类后端错误建模型，避免桌面端自创
  契约层不存在的语义。
- **日志页、摘要页**按方案不单独成面板：前者服务端无桌面消费方且契约层未建模，
  后者能力并入索引面板与实体面板。

## 现状订正

本报告写作后代码继续推进，上节四条偏差已不成立，以代码为准：

| 条目 | 现状 |
|---|---|
| 可停靠面板未做 | 已实现，见 `crates/app/cce-desktop/src/dock_layout.rs` 与 `views/main_window.rs` 的装载、折叠与落盘 |
| 布局未版本化持久化 | 已实现，`LAYOUT_VERSION` 校验 + 800ms 去抖保存 |
| 托盘仍未落地 | 已实现，`src/tray.rs` 的关闭决策与 `app.rs` 的初始化 |
| 表格导出未做 | 已实现，`src/services/export_service.rs` 的网格 CSV 导出 |

日志页与摘要页两条仍按原结论执行，其中日志是否进桌面端转由
`08-frontend-parity.md` 的开放点决定。功能差异的完整清单见同一文件。

## 待办

1. 桌面会话下的逐面板手工验收（写作环境无 Wayland/X11，只完成构建、静态检查与单测）；
   逐项对照 `DEV.md` 的面板表，确认每个面板在有后端时完成一次真实往返。
2. 托盘在真实桌面会话下验证一次关闭常驻与恢复（代码已接入，未做过平台实机验证）。
3. 停靠布局的跨版本升级体验验证：手工改坏 `layout.json` 后应回落到默认布局。
4. 导出格式目前只有 CSV，若需要 JSON 或 Markdown 落地，另立条目。

上述四项属验证与后续增强；功能差异与补齐计划见 `08-frontend-parity.md`。
