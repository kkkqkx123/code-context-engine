# 测试与基准规范

本文件是项目测试与性能基准的统一执行规范，覆盖功能正确性用例、后端契约测试、服务级基准及其数据生命周期。检索质量评测的设计与结论仍见 `docs/benchmark/`，专项测试方案仍见 `docs/tests/`；两者与本规范冲突时，以本规范为准。

## 基本原则

- 默认本机测试完全离线：本地向量引擎、本地全文索引、本地元数据存储与 loopback Mock 模型即可完成功能回归，不依赖外部网络或服务。
- 外部服务只在 devs 上按基准需要临时启用，不作为系统服务常驻，也不允许在测试结束后残留进程或数据。
- 每个用例和每次基准使用独立的工作区、数据库与远端对象；禁止复用上一次运行留下的数据。
- 测试与基准必须能重复执行。失败、超时或中断后，下一次运行应能从干净状态开始，不能依赖人工清理。
- 功能断言与性能阈值解耦：功能用例不写耗时断言，基准不充当正确性回归。
- 测试代码同样遵循项目编码约束：不使用 `unwrap`，失败点用 `expect` 给出上下文；新增 `unsafe` 或动态分发必须按项目要求登记。

## 测试分层与存放位置

| 类型 | 位置 | 约定 |
| --- | --- | --- |
| 单元测试 | 与被测源码同文件的 `#[cfg(test)]` 模块 | 文件过大时拆出独立 `test.rs`，由所属 `mod.rs` 引入；只测本模块职责。 |
| 集成测试 | 各 crate 的 `tests/` 目录 | 通过公开接口组装组件；需要共享夹具时放在同目录的 `common.rs`。 |
| 后端契约测试 | `cce-storage-common/tests/*_contract.rs` | 同一套语义分别跑本地与远端分支；远端分支由 `CCE_TEST_*` 地址控制，未配置时本地可跳过。 |
| 服务级冒烟 | `cce-server/tests/metadata_backend_smoke.rs` | 覆盖真实服务装配下的项目创建、索引、检索、热更新读取、检查点与导出。 |
| 端到端评测 | `crates/app/cce-e2e-tests/` | 复用仓库内 fixtures 与 Mock 嵌入；运行数据在 devs 现场生成，不进日常代码同步包。 |
| 性能基准 | 归属 crate 的 `benches/` | 微基准随 crate 演进；跨组件工作流基准可放在 `cce-orchestrator` 或 `cce-server`。 |

## 编写约定

- 断言必须说明意图：`expect("...")` 的文本应能定位到失败步骤，避免裸失败。
- 随机性只允许来自固定种子或确定性生成器；禁止用时间、进程号作为影响结果的输入。进程号只用于对象命名隔离。
- 测试之间不得共享固定路径，例如固定的 `/tmp/cce_test_*` 目录或固定的项目名；需要隔离时使用本规范提供的目录守卫或 `tempfile::TempDir`。
- 远端集合、索引、数据库项目名必须包含本轮 `run_id` 或进程级唯一后缀，避免跨轮读取到旧世代数据。
- 契约测试在 devs 上通过服务脚本执行时不得跳过；本地未配置远端地址时的跳过只适用于离线开发。
- 新增基准必须先说明度量的阶段与排除项，避免把服务启动、模型调用、夹具物化等一次性成本计入稳态耗时。

## 数据隔离与自动清理

- 临时目录由作用域守卫持有并在 `Drop` 时回收，禁止把清理写成函数末尾的手工删除：工作树临时目录用 `cce_bench_kit::BenchTempDir`，测试引擎数据目录用 `cce_server_shared::test_support::TestDir`，其他场景使用 `tempfile::TempDir`。
- `BenchTempDir` 在启动时清除同名旧目录，因此进程被中断后不会把上一轮夹具带入下一次基准；`TestDir` 为每个测试分配独立序号。
- 远端对象同样由作用域守卫回收：服务矩阵为每档组合生成唯一 Qdrant collection 与 ES index，并在该档退出时删除；PostgreSQL 冒烟用例在结束时删除本轮项目数据；契约测试各自清理自己创建的集合或索引。
- 允许持久化的只有结果文件与排障日志：TSV 趋势文件位于各 crate 的 `benches/results/`，运行日志位于 `.bench_logs/` 或 devs 的 `/data/kkkqkx/cce/`。构建缓存 `target/` 可以保留以提高编译速度，但永不参与代码同步。
- 服务运行数据不属于结果文件。每次运行结束后，`/var/lib/cce-services/` 下只应保留空的服务根目录和用于管理的 `run/` 文件，不应积累历史 `RUN_ID` 目录。

## 外部服务生命周期

devs 上使用的服务如下：

| 服务 | 地址 | 运行数据 |
| --- | --- | --- |
| Qdrant | `http://127.0.0.1:16333` | `/var/lib/cce-services/qdrant/<RUN_ID>` |
| Elasticsearch | `http://127.0.0.1:19200` | `/var/lib/cce-services/elasticsearch/<RUN_ID>` |
| PostgreSQL | `postgres://postgres@127.0.0.1:15432/postgres` | `/var/lib/cce-services/postgres/data` |

- 唯一推荐入口是 `scripts/devs-benchmark-services.sh run <target> -- <command>`。脚本先停止自己上次启动的进程，再启动本轮服务；若端口已被外部进程占用则直接失败。命令退出、失败或中断时，脚本按 PID 停止服务并删除本轮数据目录。
- `run` 同时导出 `CCE_TEST_QDRANT_URL`、`CCE_TEST_ES_URL`、`CCE_TEST_PG_URL` 与 cargo PATH。契约测试读取这些环境变量；服务级基准仍使用程序化配置，避免两套控制路径混用。
- `up`、`down`、`status`、`metrics`、`clean` 仅用于手工排障。`up` 不接管已健康的服务；`clean` 先确认服务已停止，端口仍被占用时拒绝删数据。脚本不使用全局 `pkill`，只管理自己记录的 PID。
- 不允许在测试结束后留下监听中的 16333、19200、15432 端口；验收时用 `status` 与端口检查确认三者均为停止状态。
- 安装目录和压缩包缓存位于 `/opt/cce-services/`，长期保留；旧的 `/data/kkkqkx/cce/services/` 不再作为运行时位置，只保留基准所需的 native 插件。

## 性能基准约定

- 统一使用 `cce-bench-kit` 的档位：`Small` 约 500 文件、`Medium` 约 5 000 文件、`Large` 约 50 000 文件；合成内容由固定 xorshift 生成器产生，确保输入可复现。
- 稳态耗时以 `criterion` 为准，单次计时与 TSV 追加用于快速观察趋势。报告表头由 `TsvReport` 校验，列发生变化时重置趋势文件，禁止把不同结构的数据混写在同一文件。
- 服务矩阵需记录 `run_id`、服务版本、向量/全文后端组合、档位、错误数、分位延迟及进程资源指标（CPU、峰值 RSS、RSS 增量、读写字节）。远端版本与本地进程指标分开采样。
- 结论对比只允许在同机器、同构建模式、同文件档位、同后端组合之间进行；跨后端切换要求全量重建索引，不能复用项目目录。
- 嵌入与重排使用 Mock，禁止真实模型调用掩盖存储差异；ES 冷查询与本地 Tantivy 不等价，需单独记录首字节延迟和堆大小。
- 大档位逐级推进：先完成 500 文件档并确认资源占用，再决定是否上 5 000 文件档；ES 不直接跑 50 000 文件档。
- 基准创建的服务端对象与本地夹具必须在同一作用域内回收，不允许以“供下次复用”为由保留。

## 执行与验收

- 常用命令：库测试 `cargo test --lib -- --nocapture`，按名运行 `cargo test <name>`，单文件集成测试 `cargo test --test <file>`；提交前执行 `cargo clippy --all-targets --all-features` 与 `cargo fmt`。
- 服务相关命令必须包裹在 `run` 中，例如 `run qdrant -- cargo test -p cce-storage-common --test vector_backend_contract`、`run all -- cargo bench -p cce-server --bench service_remote_backend_matrix`。
- 一轮测试通过的标准：用例与基准错误数为零；devs 上契约测试无跳过；远端对象与临时目录已删除；服务全部停止且端口关闭；只留下预期的 TSV 与日志。
- 若不满足上述条件，先修复清理或生命周期问题，再重跑；禁止通过手工删除残留数据掩盖问题。

## 相关文档

- `docs/dev/devs-service-environment.md`：devs 服务安装位置、版本与手工排障命令。
- `docs/dev/devs-service-benchmark-analysis.md`：服务矩阵的设计目标与验证范围。
- `docs/dev/devs-sync.md`：代码同步包内容与排除清单。
- `docs/tests/README.md`：本地性能开销与恢复测试的方向索引。
- `docs/benchmark/design.md`：检索质量基准设计。
