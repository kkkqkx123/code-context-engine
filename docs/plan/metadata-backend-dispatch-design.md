# 元数据后端分发重构设计

## 背景

服务基准补充阶段曾在 `cce-orchestrator` 引入一个 `MetadataStore` 枚举，并生成 3,400 多行的 trait 转发实现。它试图把本地 SQLite 与远端 PostgreSQL 两套元数据客户端统一成一个确定类型，避免使用 trait object。

该实现已经删除，原因有三点：

- 配置层仍明确拒绝 `metadata_backend = "remote"`，远端分支运行时不可达，转发代码没有实际收益。
- PostgreSQL 客户端仍有大量未实现的契约方法，契约测试只覆盖其中一部分；统一转发会把失败推迟到深层调用。
- 转发文件只是机械复制 `MetadataStorage` 的 146 个方法签名，与契约定义重复，且生成方式不可复现。

当前状态：元数据持久化句柄恢复为 `Arc<SqliteClient>` 的 `MetadataStore` 类型别名；`ProjectRegistry` 保留在 `cce-orchestrator`，因为它已经改为只依赖 `MetadataStorage` 契约，与后端选择无关；外部服务基准、插件与高亮基准均保留。

## 目标

在 PostgreSQL 真正满足 `MetadataStorage` 契约后，以单一事实来源实现本地/远端元数据后端分发，同时满足：

- 运行时使用确定类型分发，不使用 `Arc<dyn MetadataStorage>`。
- 契约方法清单只维护一份，不允许在两个 crate 中各写一遍。
- 每个模块只承担一个层次的职责，不出现承载上百个无关方法转发的“上帝文件”。
- 后端能力差异在配置或启动阶段暴露，不在任意业务调用中途失败。
- 索引、查询、热更新、checkpoint、导出和项目注册表共享同一个元数据句柄实例。

## 命名边界

`metadata-delegration` 的重构必须同时消除命名混淆：

- `MetadataStore` / `metadata_store` 只表示元数据持久化句柄，包括项目记录、generation manifest、checkpoint、实体/chunk、snapshot 与 admission audit。
- `cce-codegraph` 中的 `RelationStore` 表示关系图的内存索引与快照，负责符号、调用关系和图查询；该类型和模块名不得改动。
- 两者不能互相借用命名，也不应在同一函数里以相近名字出现。后端分发相关的新类型统一使用 `MetadataBackend`、`MetadataBackendStore` 等明确前缀。

在远端分支正式启用前，`MetadataStore` 继续作为本地 SQLite 客户端的类型别名存在，不引入任何新的后端枚举名。

## 推荐方案

### 契约与转发由同一份声明生成

把 `MetadataStorage` 的方法清单作为唯一事实来源，放在 `cce-storage-common` 中。用一个声明宏描述方法名、参数和返回类型，从这份声明同时展开：

- `MetadataStorage` trait 本身；
- 后端分发枚举的 trait 实现骨架；
- 编译期方法集合一致性检查。

装配层只保留一次宏调用，不保存展开后的代码。这样 trait 增加或删除方法时，转发实现自动同步，不再依赖人工更新巨大的生成文件。

如果宏方案因 RPITIT 返回值和异步闭包生命周期限制而代价过高，则采用可复现的代码生成脚本：脚本从 `metadb.rs` 读取方法清单，输出带明确生成标记的文件，并在校验流程中执行 `--check` 防止漂移。生成产物不能作为手写业务模块维护。

### 按契约域拆分实现

`MetadataStorage` 当前混合了十个业务域。正式实现后端分发时，应先把它拆成若干子契约，再由组合 trait 统一暴露：

- 项目目录与 profile；
- generation manifest 与 override；
- 文件、实体、chunk 与映射；
- checkpoint 与 work unit；
- relation snapshot 的读写与增量链；
- generation 物化与 GC；
- 热更新候选与文件级状态投影；
- admission audit；
- 容量与诊断。

每个子契约对应一个独立的转发模块，单个文件只包含一个业务域。这样即使最终仍需展开转发代码，每个文件的职责也可审查、可测试，且变更影响范围明确。

### 分发类型保持薄

后端分发类型只负责三件事：保存具体客户端、暴露后端名和健康状态、按项目返回作用域句柄。它不承载任何业务方法实现，也不为特定调用方暴露具体客户端的逃逸接口。

需要具体客户端的能力应在契约层表达，例如同步快照加载接口或批量读取接口；不得通过 `local_client()` 这类方法把分支细节散播到调用方。若某个能力只有本地分支支持，应在配置或启动阶段明确拒绝该组合，而不是运行到中途再返回错误。

### 保留组件级 PostgreSQL 验证

PostgreSQL 客户端的契约测试继续保留，作为组件级语义验证。服务级启用必须同时满足：

- `remote_unimplemented!` 存根清零；
- 远端 schema 迁移在启动阶段完成；
- 项目注册表、索引、查询、热更新、checkpoint 与导出链路的服务级 smoke 测试通过；
- 配置层放开远端 metadata，并明确与远端 fulltext 的组合约束。

在此之前，配置层继续拒绝远端 metadata，文档中保持“组件级已测、服务级未接线”的结论。

## 实施顺序

第一步先补齐 PostgreSQL 契约存根，并以服务级 smoke 测试作为完成标准。只有远端分支真正可用，才进入分发设计；否则任何枚举或转发代码都会再次变成不可达 scaffolding。

第二步在 `cce-storage-common` 内整理契约声明，建立单一事实来源，并增加契约方法集合的一致性检查。

第三步按业务域拆分 `MetadataStorage`，让调用方仍然只看到一个组合契约，但实现和转发可以按域独立维护。

第四步引入后端分发类型，只在装配层构造一次；索引、查询、热更新、checkpoint、导出和项目注册表从同一个实例获取句柄。

第五步放开配置校验，补服务级矩阵中 metadata 维度，并更新基准 TSV 标签。远端 metadata 不参与召回质量对比，只验证服务链路可用性。

## 验证要求

- workspace 全目标、全特性编译通过。
- `cce-storage-common` 的契约测试在本地与 PostgreSQL 分支都通过，不再有跳过或存根。
- 本地分支的既有索引与查询测试结果保持不变。
- 服务级 smoke 覆盖项目创建、索引、检索、热更新、检查点恢复与导出。
- 契约声明与转发实现的一致性检查通过；若采用代码生成，生成产物必须与当前契约一致。
- `cce-codegraph` 的 `RelationStore` 相关测试保持不变，确认命名边界未被破坏。

## 非目标

- 本阶段不引入 `Arc<dyn MetadataStorage>`。
- 本阶段不修改关系图快照的加载语义。
- 本阶段不把 PostgreSQL 契约测试通过等同于服务级可用。
- 本阶段不恢复已删除的 3,400 行转发文件。

## 实施状态

本设计已落地：`cce-storage-common` 用一份声明宏生成组合契约、分域子契约和 `Arc` / 后端分发的转发实现；`cce-storage-metadb-pg` 的契约存根已清零；`MetadataStore` 成为确定类型的元数据后端句柄；配置层允许 `metadata_backend = "remote"`（与远端全文同开时仍需高级开关）；服务级 smoke 覆盖项目创建、索引、检索、热更新读路径、检查点往返与图导出。

devs 上 `cargo check --workspace --all-targets`、`cargo clippy --workspace --all-targets --all-features`、`cargo build -p cce-cli -p cce-server` 与 `cargo test --workspace --lib` 均通过。PostgreSQL 已按 `docs/dev/devs-service-environment.md` 安装并开始验证；远端契约 3/3 与服务级 smoke 1/1 通过。smoke 过程中发现同步快照桥接会在单线程运行时上饿死连接池维护任务，已改为把连接驱动固定在专用运行时，并由普通线程进入该运行时完成同步读取。
