# 服务级后端基准补充与性能问题（2026-11-20）

## 范围与环境

本轮在 devs 上完成编译、契约测试和服务级基准采集。源码包为 `cce-src-ab17d436-benchmark.tar.gz`，`BUILD_INFO.txt` 记录了工作树改动。机器为 32 vCPU、31 GiB 内存、Linux 4.15、Rust 1.100.0；基准使用 `bench` profile，所有外部服务由 `scripts/devs-benchmark-services.sh` 按需启停，日志位于 `/data/kkkqkx/cce/service-logs/`。

| 依赖 | 版本 | 地址 | 契约测试 |
|---|---|---|---|
| Qdrant | 1.15.5 | `http://127.0.0.1:16333` | `vector_backend_contract`：2/2 通过 |
| Elasticsearch | 7.17.28 | `http://127.0.0.1:19200` | `fulltext_backend_contract`：2/2 通过 |
| PostgreSQL | 10.23 | `postgres://127.0.0.1:15432/postgres` | `metadata_backend_contract`：2/2 通过 |
| Lua 插件 | 仓库示例脚本改写 | 项目 `.cce/plugins.json` | 独立插件基准 |
| native 插件 | SDK 本地编译 `libcce_bench_native_plugin.so` | 由 `CCE_BENCH_NATIVE_PLUGIN` 注入 | 独立插件基准 |

`cargo check --workspace --all-targets --all-features` 通过。服务检索矩阵共 4 档组合 × 2 档文件规模，合计 3,024 次请求，错误数为 0。每次运行使用独立项目、Qdrant collection 和 ES index，配置中固定同一组 HNSW 参数（`m=16, ef_construct=100, ef_search=64, full_scan_threshold=10000`），TSV 记录完整后端标签和 `run_id`。

## 指标结果

### 索引阶段

| 文件档 | 组合 | 墙钟 ms | CPU ms | 峰值 RSS KiB | 写字节增量 |
|---|---|---:|---:|---:|---:|
| 500 | local/local | 11,287 | 9,300 | 330,052 | 150,376,448 |
| 500 | qdrant/local | 14,099 | 6,910 | 376,060 | 149,438,464 |
| 500 | local/es | 10,237 | 6,740 | 384,208 | 150,298,624 |
| 500 | qdrant/es | 12,649 | 6,870 | 387,384 | 149,233,664 |
| 5,000 | local/local | 131,005 | 88,660 | 1,228,932 | 1,684,377,600 |
| 5,000 | qdrant/local | 133,874 | 85,120 | 1,316,620 | 1,674,715,136 |
| 5,000 | local/es | 128,568 | 79,760 | 1,327,036 | 1,684,115,456 |
| 5,000 | qdrant/es | 124,922 | 83,260 | 1,330,196 | 1,674,522,624 |

两档规模均成功索引全部文件，错误数为 0。远端向量/全文分支自身的墙钟差异在 ±10% 左右；Qdrant 分支进程内 CPU 更低，但峰值 RSS 高约 45 MiB；ES 分支峰值 RSS 高约 50–100 MiB。ES 的 1 GiB JVM heap 在独立进程内，不计入基准进程 RSS。

### 检索阶段（节选，cold / c=1 与 c=4）

| 档位 | 组合 | 模式 | c=1 p50 / p95 ms | c=1 req/s | c=4 p50 / p95 ms |
|---|---|---|---:|---:|---:|
| 500 | local/local | bm25 | 1 / 1 | 599.9 | 1 / 1 |
| 500 | local/es | bm25 | 14 / 17 | 65.9 | 11 / 12 |
| 500 | qdrant/local | bm25 | 1 / 1 | 524.2 | 1 / 2 |
| 500 | qdrant/es | bm25 | 11 / 12 | 87.8 | 11 / 12 |
| 500 | 全部组合 | vector | 2–3 / 2–4 | 296–384 | 2–3 / 3–4 |
| 500 | 全部组合 | hybrid | 10–61 / 22–100 | 18–61 | 40–99 / 99–100 |
| 5,000 | local/local | bm25 | 1 / 2 | 545.5 | 2 / 3 |
| 5,000 | local/es | bm25 | 9 / 12 | 98.9 | 10 / 10 |
| 5,000 | qdrant/es | bm25 | 8 / 10 | 109.6 | 8 / 9 |

完整 72 行/档分位延迟、吞吐和资源指标见 `.bench_logs/service_remote_backend_matrix.tsv`。

### 插件与高亮

| 基准 | 结果 |
|---|---|
| 注册表加载（仅 Lua） | 0.550 ms，1 个插件 |
| 注册表加载（Lua + native） | 7.594 ms，2 个插件 |
| Lua 初始化 / native 初始化 | 0.135 ms / 0.021 ms |
| 每文件 `filter_file` | 128.5 µs |
| 每文件符号提取 | 160.3 µs |
| 每文件 BM25 文本生成 | 179.6 µs |
| 每文件 embedding 文本生成 | 176.1 µs |
| 查询期 `rewrite_query` | 141.8 µs/次 |
| 高亮构建（读文件 + 截取 + 标记） | 6.526 µs/次，其中读文件 4.490 µs |

## 发现的问题

### 1. Qdrant 1.15 兼容性缺陷阻断契约测试（已修复）

首次契约测试连续失败三处，均不是服务未启动：

- 写入返回 `status: "completed"` 被客户端判为未应用而反复重试；新版本 Qdrant 同时接受了 `ok` 与 `completed` 两种表示。
- `min_should: 1` 的旧写法被 1.15 拒绝，要求 `min_should: { conditions, min_count }`；该写法同时存在于检索过滤和批量删除过滤。
- `file_path` 的 `wildcard` 谓词在关键字 payload 上非法；Qdrant 只接受精确匹配，目录前缀过滤改为在客户端对返回 payload 二次过滤并恢复 limit 语义。

修复后 `vector_backend_contract` 本地与远端分支全部通过。基准首次创建集合耗时约 7 秒，超过契约测试的 5 秒 client timeout，已把该测试超时提高到 15 秒；服务基准使用默认 30 秒并把服务启动排除在计时之外。

### 2. Elasticsearch BM25 冷查询明显慢于本地 Tantivy

同为 500 文件档，`local/es` 的 bm25 c=1 冷查询 p50 为 14 ms、`local/local` 为 1 ms，相差约 14 倍；吞吐从约 600 req/s 降到 66 req/s。5,000 文件档差距略收敛，但仍约 9 ms 对 1 ms。热查询和 c=16 并发下差距缩小，说明主要成本是每次请求的远端往返和索引刷新语义，而不是本地 CPU。ES 分支的峰值 RSS 高约 50–100 MiB，同时还有未计入进程指标的 JVM heap。若线上要求稳定的首字节延迟，不应把 ES 分支当作 Tantivy 的等价替换；需要单独设定 SLA 与堆大小。

### 3. hybrid 冷查询的约 100 ms 悬崖与存储后端无关

四种组合的 hybrid c=4 冷请求 p50 都集中在 39–99 ms，p95/p99 约 99–100 ms，吞吐仅约 20–40 req/s；对应热请求恢复到 1–3 ms、1,700 req/s 以上。vector 与 bm25 单路同档位都没有该现象，因此 Qdrant/ES 都不是来源。该现象与前一轮在传输提供者内部观测到的约 55 ms 首次嵌入耗时一致；本轮矩阵没有增加更细的嵌入阶段拆分，下一步应在查询阶段 TSV 中区分“首次嵌入连接”“缓存未命中”和“fusion 初始化”。

### 4. 5,000 文件服务索引耗时远高于既有流水线基准

本轮服务级索引在四档组合中均为 125–134 秒，峰值 RSS 约 1.2–1.3 GiB；此前同机 `full_pipeline_perf` 修复后的 5,000 文件完整流水线为 26.8 秒。两者的夹具生成方式相同，但服务路径经过项目注册表、effective config 合并和 HTTP handler。由于当前 `IndexResult.stages` 没有被服务索引 TSV 采集，无法把这 100 秒差额分摊到具体阶段。应优先在服务索引入口输出阶段耗时，而不是据此猜测后端问题；本轮数据只能说明四档后端之间的差异不大。

### 5. PostgreSQL 契约通过不代表服务级 metadata 可用

`metadata_backend_contract` 只覆盖部分 `MetadataStorage` 方法。`cce-storage-metadb-pg` 中仍有大量 `remote_unimplemented!` 存根，例如项目元数据读写在真实服务请求中返回 `Validation failed: remote relation branch is not wired yet`。在放开配置校验后，服务级创建项目立即失败；因此本轮保留配置层的硬拒绝，只把远端 metadata 标为“组件级已测、服务级未接线”。项目注册表已移入 `cce-orchestrator` 并改为只依赖 `MetadataStorage` 契约；临时引入的元数据后端枚举与 3,400 行转发层因远端分支不可达、职责过重而已删除。后续正式方案见 `docs/plan/metadata-backend-dispatch-design.md`。后续必须先补齐存根方法，再放开配置。

### 6. 插件逐文件调用和查询钩子存在固定高额开销

Lua 脚本本身初始化仅 0.135 ms，native 动态库加载仅 0.021 ms；但每个文件的四类 hook 分别耗时 128–180 µs，查询改写为 142 µs/次。这个量级远超简单 Lua 函数调用，且四种 hook 非常接近，说明成本集中在每次调用的超时包装、线程调度或 Lua 桥接边界，而不是钩子业务逻辑。按 5,000 文件计算，仅逐文件 hook 就超过 2.5 秒。下一步应在插件运行时内区分“脚本执行”和“调度/超时”耗时，再决定是否引入批量调用或复用执行上下文。

注册表加载在加入 native 插件后从 0.55 ms 增至 7.59 ms，主要由 `dlopen` 及符号绑定贡献，属于一次性启动成本。

### 7. 真实高亮成本目前可忽略，但服务路径缺少阶段观测

独立 harness 显示 1,000 次高亮构建平均 6.53 µs/次，其中源文件读取 4.49 µs、片段截取 0.65 µs、标记构建 1.39 µs。该成本不会成为当前瓶颈。问题在于服务响应 DTO 和查询阶段没有高亮构建指标，无法把用户感知的展示延迟与检索延迟分离；本轮按方案只保留独立 harness，没有把高亮塞进 BM25 检索耗时。

### 8. 资源指标边界

`peak_rss_kb` 来自 `VmHWM`，是进程生命周期峰值；矩阵中所有组合在索引完成后运行，检索阶段的 RSS 增量多为 0。块 I/O 增量受页缓存影响，500 文件档读字节均为 0。外部服务进程的 CPU/内存需要通过脚本侧单独采集，不能由基准进程指标推断。

## 本轮代码与产物

- 新增 `service_remote_backend_matrix`：四档后端 × 冷/热 × 并发矩阵，TSV 带 `run_id`、`service_version`、后端与 HNSW preset 标签。
- `tests/common.rs` 增加 `test_env_with_backends`，不再依赖 `CCE_DB_*` 环境变量；契约测试仍使用 `CCE_TEST_*_URL`。
- 新增 `plugin_overhead`、`highlight_overhead` 两个独立基准；native 插件由 `crates/bench/cce-bench-native-plugin` 在 devs 本地编译。
- 命名与结构修正：元数据持久化句柄恢复为 `MetadataStore = Arc<SqliteClient>`；删除临时后端枚举及其转发层；`cce-codegraph` 中用于关系图索引的 `RelationStore` 未改动。
- 新增 `scripts/devs-benchmark-services.sh`，提供 `up/down/status/clean`，按 PID 停止服务，缓存压缩包，每次运行独立数据目录。
- `ProjectRegistry` 移入 `cce-orchestrator` 并只使用 `MetadataStorage::project_record`；服务装配继续使用单个 `Arc<SqliteClient>` metadata store。

## 证据路径

- 本地结果：`.bench_logs/service_remote_backend_index.tsv`、`.bench_logs/service_remote_backend_matrix.tsv`、`.bench_logs/plugin_overhead.tsv`、`.bench_logs/highlight_overhead.tsv`；
- devs 日志：`/data/kkkqkx/cce/check-ab17d436-final4.log`、`/data/kkkqkx/cce/build-ab17d436-bench2.log`、`/data/kkkqkx/cce/bench-ab17d436-remote-matrix*.log`、`/data/kkkqkx/cce/test-ab17d436-*-contract*.log`；
- 契约失败与修复过程：`/data/kkkqkx/cce/test-ab17d436-qdrant-contract{,2,3,4,5,6,7}.log`；
- 插件/native 构建：`/data/kkkqkx/cce/bench-ab17d436-plugin*.log`、`/data/kkkqkx/cce/build-ab17d436-native-plugin2.log`；
- 远端 metadata 服务级失败：`/data/kkkqkx/cce/bench-ab17d436-pg-smoke.log`。

## 后续修复状态

细化方案见 `docs/plan/service-backend-followup-remediation.md`，devs 服务现状与
环境配置见 `docs/dev/devs-service-environment.md`。

- 第 1 项已在发现阶段修复，回归单测保留；第 5 项的确定类型后端分发与配置放开
  已在当前 `main` 落地，并补充服务级 smoke。
- 第 2、3、4、6、7、8 项以阶段归因和运行时开销修复为主：嵌入首次连接与缓存命中
  透出到检索阶段 TSV，启动阶段预热连接；服务索引阶段进入响应 DTO；Lua hook
  去掉逐调用线程创建；响应渲染与高亮 harness 分离；外部服务进程指标由管理脚本采集。
- Elasticsearch 与本地 Tantivy 的冷查询差距属于后端固有成本，不作为代码缺陷消除，
  由 SLA 与堆配置单独约束。

## 明确未做

- 未执行 50,000 文件档；5,000 文件档先完成并确认资源占用，ES 不直接跑 Large。
- 矩阵首轮 TSV 的 `es_version` 列为 `unknown`：探针只读取顶层 `version` 字段，而 ES 7.x 返回 `version.number`；探针已修正，未为了补标签重跑矩阵。
- 未比较四档后端的召回质量；契约测试只验证语义一致性。
- 未把外部服务启动时间计入索引或检索耗时。
- 未放开 `metadata_backend = "remote"`；PG 仅完成组件级契约验证。
