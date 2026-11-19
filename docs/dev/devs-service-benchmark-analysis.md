# devs 外部服务基准补充方案

## 目标

在 devs 上补齐当前离线基准未覆盖的外部依赖路径。服务不作为系统服务常驻，只在执行对应基准前由脚本按需启动，执行结束后按 PID 停止并保留日志与数据目录供复核。

代码实现问题的独立改造方案见 `docs/plan/service-benchmark-code-remediation.md`。

## 环境调查

- devs 可正常使用 Docker daemon，但本地没有 Qdrant、Elasticsearch、PostgreSQL 镜像。
- Docker Hub 与 GCR 访问超时；`ghcr.io` 返回未授权，不能直接依赖现有镜像仓库。
- GitHub Releases、Elastic artifacts、Ubuntu archive 可访问。Elasticsearch 压缩包较大，下载耗时明显，需要断点续传并只下载一次。
- `podman` 在当前内核上因 cgroups v1 不可用。
- 端口 6333、9200、5432 当前未被占用；为避免与未来任务冲突，基准使用 16333、19200、15432。
- PostgreSQL 服务本身可启动，但当前代码只把远端 metadata 作为前向脚手架，配置校验和引擎装配都不接受 `metadata_backend = "remote"`，因此不能通过服务级矩阵覆盖它。

## 服务优先级与可行性

| 依赖 | devs 可行性 | 本轮定位 | 说明 |
|---|---|---|---|
| Qdrant | 高 | 优先补 | GitHub Release 提供 33 MB 静态 musl 二进制，下载、解压、前台启动即可。向量维度需与 Mock 嵌入一致，设为 384。 |
| Elasticsearch | 中 | 次优先补 | 官方 tar 包约 600 MB，内置 JDK；需要非 root 运行、限制 heap、单机模式并关闭安全认证。 |
| PostgreSQL | 中 | 仅组件级 | 契约测试已有 `CCE_TEST_PG_URL` 开关，PG 客户端也会自动迁移 schema；但服务级装配未接线，不能算服务缺口已补充。 |
| 真实插件 | 高 | 可并行补 | 不依赖服务。Lua 脚本可用仓库内示例，native 插件需本地编译 `.so`，通过项目 `.cce/plugins.json` 指向。 |
| 真实高亮 | 高 | 可并行补 | 不依赖服务。需要单独基准记录源文件读取、片段截取与高亮标记成本；当前服务路径没有可观测的高亮阶段。 |
| 真实模型服务 | 低 | 暂不补 | 存储后端基准应复用现有 loopback Mock，避免模型耗时掩盖后端差异。 |

## 按需启动设计

新增单一管理脚本，提供 `up`、`down`、`status`、`clean` 子命令，不接受系统服务安装方式：

- 下载缓存放在 `/data/kkkqkx/cce/services/cache`，只下载一次。
- 每个服务有独立目录：`bin/`、`data/`、`logs/`、`run/`，数据不落在 repo 内。
- 进程用 `pid` 文件管理，`down` 只停止脚本启动的进程，不使用全局 `pkill`。
- 启动后通过 HTTP 健康检查确认可用；健康检查失败时脚本直接失败，不进入基准。
- 每次基准使用新的数据目录或唯一索引名，运行前清理旧数据，避免跨轮污染。
- 日志统一写到 `/data/kkkqkx/cce/service-logs/`，结果仍追加到 `.bench_logs/` 对应 TSV。

建议端口与用途：

| 服务 | 端口 | 健康检查 |
|---|---|---|
| Qdrant | 16333 | `GET /healthz` |
| Elasticsearch | 19200 | `GET /` |
| PostgreSQL | 15432 | `pg_isready -h 127.0.0.1 -p 15432` |

## Qdrant 补充方式

下载 musl 静态包并解压到服务目录，以 `./qdrant --config-path` 或直接默认配置启动，数据目录指向本次运行的独立路径。不要打开 CCE 配置中的 `auto_start`，避免应用和脚本双重管理同一进程。

服务级验证分两层：

1. 契约测试：`CCE_TEST_QDRANT_URL=http://127.0.0.1:16333 cargo test -p cce-storage-common --test vector_backend_contract`，确认过滤语义与本地分支一致。
2. 性能基准：服务检索矩阵增加 `qdrant/local` 档位，使用程序化配置覆盖，不依赖 `config.toml` 的环境变量装载路径。

Qdrant 集合名由项目工作区路径派生，因此每个基准使用独立临时项目即可天然隔离；仍建议在每轮结束后删除集合或丢弃数据目录。

## Elasticsearch 补充方式

下载单个 ES tar 包并缓存。运行时限制 `ES_JAVA_OPTS` 的堆大小，先用 1 GiB，再按 500 文件档位结果决定是否提高；打开单机发现并关闭安全认证，避免证书与账号配置干扰基准。

服务级验证同样分两层：

1. 契约测试：`CCE_TEST_ES_URL=http://127.0.0.1:19200 cargo test -p cce-storage-common --test fulltext_backend_contract`。
2. 性能基准：服务检索矩阵增加 `local/es` 与 `qdrant/es` 两档，索引前显式设置唯一 index name，结束后删除该 index。

ES 包大、启动慢，基准脚本应先等待集群状态变为可用，再开始计时；启动时间单独记录，不混入索引或检索耗时。

## PostgreSQL 补充方式

如果只补外部依赖验证，可从 Ubuntu archive 下载 PostgreSQL 包并在本地目录解包，用 `initdb` 初始化独立 data dir，`pg_ctl` 按需启停。运行契约测试：`CCE_TEST_PG_URL=postgres://... cargo test -p cce-storage-common --test metadata_backend_contract`。

但服务级矩阵不应在此时新增 `pg` 档位，因为当前生产装配明确拒绝远端 metadata。要使它可测，需要先完成配置校验与引擎装配的接线工作，这属于代码缺口，不是部署问题。

## 基准代码需要的变化

1. `crates/app/cce-server/tests/common.rs` 增加可注入后端组合的 `test_env` 变体。现有 `test_env()` 直接构造 `AppConfig::default()`，不会走 `env_loader`，所以不能只靠导出环境变量来切换后端。
2. 新增服务级远程后端矩阵，维度为 `local/local`、`qdrant/local`、`local/es`、`qdrant/es`；每档独立建项目、独立索引，记录索引墙钟、检索分位延迟、错误数、CPU、峰值 RSS 与块 I/O。
3. TSV 增加 `vector_backend`、`fulltext_backend`、`run_id`、`service_version` 列，对比时只允许同机器、同构建模式、同文件档位、同后端组合。
4. Mock 嵌入与本地 SQLite 保持不变，避免把远端元数据未接线的影响混入向量/全文后端结论。
5. 契约测试继续使用 `CCE_TEST_*_URL` 环境变量；服务基准使用程序化配置，二者不要混用同一套控制路径。

## 执行顺序

1. 启动 Qdrant，跑 vector 契约测试与 `qdrant/local` 服务基准，停止 Qdrant。
2. 启动 ES，跑 fulltext 契约测试与 `local/es` 服务基准，停止 ES。
3. 同时启动 Qdrant 与 ES，跑 `qdrant/es` 服务基准，随后全部停止。
4. 可选启动 PostgreSQL，只跑 metadata 契约测试；不把它计入服务矩阵。
5. 插件与高亮基准不需要外部服务，可穿插执行，但结果单独成表。

## 验收标准

- 每个外部服务的契约测试通过，且无跳过。
- 每个服务级基准错误数为 0，结果 TSV 带完整后端标签。
- 服务启动、健康检查、停止都有日志；停止后对应端口不再监听。
- 500 文件档先完成并确认资源占用，再决定是否上 5000 文件档；ES 不直接跑 Large。
- 若 PostgreSQL 仍不可装配，文档中明确标注“组件级已测、服务级未接线”，不把契约测试通过当作服务缺口关闭。

## 主要风险

- ES 包下载慢且占用磁盘；失败时应保留缓存并支持续传，不重复整包下载。
- ES 堆设置过大会挤占基准进程内存；先在 500 文件档验证 RSS。
- Qdrant 与本地向量引擎的 HNSW 参数不一致会导致性能不可比，需要固定 preset 并在 TSV 记录配置摘要。
- 后端切换要求全量重建索引；不同档位不能复用同一项目目录。
- 插件和真实高亮不改变服务缺口状态，只能作为独立基准补充。
