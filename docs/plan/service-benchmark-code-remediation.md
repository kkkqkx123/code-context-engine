# 服务基准代码改造方案

## 目标

补齐阻止 devs 上外部服务基准落地的代码实现问题。服务部署本身见 `docs/dev/devs-service-benchmark-analysis.md`；本文档只处理代码结构、配置接线、基准入口和验证方式。

本方案不改变召回与重排口径，不引入 trait object，不新增“v2”模块，也不保留旧路径兼容层。存储后端选择沿用 `VectorStore` / `FulltextStore` 的枚举分发方式。

> 实施说明：远端 metadata 已按 `docs/plan/metadata-backend-dispatch-design.md` 完成接线：PostgreSQL 契约方法补齐，`MetadataStore` 改为确定类型后端分发，配置层放开远端 metadata。下文保留原始问题分析，具体实现以正式设计文档和当前代码为准。

## 问题清单

### 1. 测试环境无法切换后端

`crates/app/cce-server/tests/common.rs` 的 `test_env()` 直接构造 `AppConfig::default()`，没有经过 `ConfigLoader::load`，因此 `CCE_DB_VECTOR_BACKEND`、`CCE_DB_FULLTEXT_BACKEND` 等环境变量不会生效。当前服务检索矩阵只能在本地向量、本地全文、SQLite 元数据组合下运行。

### 2. 服务检索矩阵没有后端维度

`crates/app/cce-server/benches/service_retrieval_matrix.rs` 固定在同一套本地后端上，没有 `vector_backend`、`fulltext_backend`、`run_id`、`service_version` 等标签，无法比较 `local/local`、`qdrant/local`、`local/es`、`qdrant/es` 四档。

### 3. 远端 metadata 未接线

`crates/core/cce-config/src/global.rs` 明确拒绝 `metadata_backend = "remote"`；`crates/app/cce-server-shared/src/engine.rs` 直接构造 `SqliteClient`；`crates/app/cce-orchestrator/src/index/vector_store/metadata.rs` 把 `MetadataStore` 定义为 `Arc<SqliteClient>`。虽然 `cce-storage-metadb-pg` 已有完整的 `PostgresClient`、`MetadataStorage` 实现和 schema 迁移，但生产装配无法使用它。

### 4. 项目注册表绑定 SQLite 私有接口

`crates/infra/cce-storage-metadb-sqlite/src/project_registry.rs` 直接持有 `SqliteClient`，并通过 `read_connection()` 与 `ProjectRepository` 读取项目记录。该模块不能接受后端中立的 `MetadataStorage`，因此即使引入 `MetadataStore` 枚举，项目注册表仍会把服务锁回 SQLite。

### 5. 插件与高亮缺独立基准入口

现有性能基准没有真实插件加载、插件逐文件调用和查询钩子开销的采集；服务响应路径也没有可观测的高亮构建阶段。两项都不是服务部署问题，需要独立基准入口或最小埋点。

## 阶段一：基准接线

修改 `tests/common.rs`，提取基础配置构造函数，并新增可注入后端组合的测试环境构造入口。该入口直接修改 `DatabaseConfig`，不依赖文件配置和环境变量装载。

修改服务检索矩阵或其并列新增远程后端矩阵，后端组合维度限定为：

- `local/local`
- `qdrant/local`
- `local/es`
- `qdrant/es`

每个组合独立建项目、独立索引、独立收集资源指标。结果 TSV 增加后端标签和运行标识；对比时只允许同机器、同构建模式、同文件档位、同后端组合。

为 Qdrant 和 Elasticsearch 设置固定版本探测和健康检查。健康检查失败时基准直接失败，不记录为请求错误；这能区分服务未启动和后端实现异常。

契约测试继续沿用现有环境变量：

- `CCE_TEST_QDRANT_URL`
- `CCE_TEST_ES_URL`
- `CCE_TEST_PG_URL`

契约测试验证语义一致性，性能矩阵只负责耗时、吞吐和资源，不重复召回质量断言。

## 阶段二：远端 metadata 枚举化

在 `cce-orchestrator` 引入 `MetadataStore` 枚举，分支为本地 SQLite 客户端和远端 PostgreSQL 客户端，并为其实现 `MetadataStorage` 契约。转发实现可用宏展开，但运行时仍是确定类型分发，不使用 `dyn`。

替换 `MetadataStore = Arc<SqliteClient>` 类型别名，所有索引、查询、热更新、检查点和导出路径统一使用新的 `MetadataStore` 枚举。此处的 `MetadataStore` 表示元数据持久化后端选择，必须与 `cce-codegraph` 中表示关系图索引的 `RelationStore` 区分。需要同步调整 `MetricsAggregator` 的类型参数和检查点相关泛型约束。

`MetadataStore` 提供：

- `backend_name()`
- `is_local()` / `is_remote()`
- 后端中立的健康与诊断信息
- 仅用于初始化路径的具体客户端访问方法

`open_metadata_store` 根据 `DatabaseConfig` 选择本地或远端分支；远端分支创建 `PostgresClient` 并调用 `ensure_migrated()`，迁移失败在引擎启动阶段暴露。

## 阶段三：项目注册表去 SQLite 化

把 `ProjectRegistry` 从 `cce-storage-metadb-sqlite` 移到不依赖具体后端的位置，并改为接受 `MetadataStore`。读取项目记录时只调用 `MetadataStorage::project_record()`，不再使用 SQLite 私有连接池和 `ProjectRepository`。

`ProjectUpdateRecord` 与 `ProjectRecord` 已在共享契约中定义，迁移只调整注册表依赖，不改变缓存语义、singleflight 行为或指标名称。

完成后，engine 的 `project_registry`、`metrics_aggregator`、索引装配和查询装配都必须从同一个 `MetadataStore` 实例构建，不得出现第二个元数据客户端。

## 阶段四：配置校验与引擎装配

从 `global.rs` 移除对远端 metadata 的硬拒绝，并把已支持的后端矩阵收敛为显式列表。官方矩阵扩展到 `local/local/local`、`local/local/remote`、`qdrant/local/local`、`qdrant/local/remote`。

`CodeContextEngine::build` 按 `DatabaseConfig` 一次性构造 `MetadataStore`、`VectorStore` 和 `FulltextStore`。远端 metadata 与远端 fulltext 的关系在配置层校验，避免运行中出现混合世代数据。

引擎启动阶段只做连接与 schema 迁移，不清理项目数据；每个基准运行使用独立项目和独立索引名，跨后端切换仍要求全量重建。

## 阶段五：插件与高亮基准入口

新增插件性能基准，分别记录：

- 插件注册表加载耗时
- Lua/native 插件初始化耗时
- 每个文件调用的文件过滤、文本生成、符号提取开销
- 查询期插件钩子开销

Lua 插件使用仓库现有示例，native 插件在 devs 本地编译；插件配置通过临时项目 `.cce/plugins.json` 注入，不改动全局配置。

高亮基准先用独立 harness 记录源文件读取、片段截取和匹配标识构建耗时。若产品接口需要返回高亮结果，再补充响应 DTO 与查询阶段埋点；本阶段不把高亮塞进 BM25 检索耗时。

## 验证顺序

1. `cargo check --all-targets` 覆盖 `cce-config`、`cce-storage-common`、`cce-storage-metadb-sqlite`、`cce-storage-metadb-pg`、`cce-orchestrator`、`cce-server-shared`、`cce-server`、`cce-e2e-tests`。
2. 无外部服务的本地契约测试全部通过，确认枚举化没有改变本地分支行为。
3. 启动单个外部服务后运行对应契约测试；三项都通过后，再运行服务基准矩阵。
4. `cargo bench -p cce-server --bench service_remote_backend_matrix` 覆盖四档后端组合，错误数必须为 0。
5. 对比本地分支与 Qdrant/ES 分支的索引吞吐、检索分位延迟、CPU、峰值 RSS 和块 I/O；PostgreSQL 分支只验证服务级索引与查询链路可用，不纳入召回质量对比。
6. 插件与高亮基准单独输出 TSV，不并入服务后端矩阵。

## 明确不做

- 不把 PostgreSQL 契约测试通过当作服务级 metadata 已接线。
- 不为旧 `MetadataStore` 类型别名提供兼容层。
- 不把远端服务启动耗时计入索引或检索耗时。
- 不在离线环境执行真实远程后端基准。
- 不处理与后端切换无关的既有 staged 文件变更。
