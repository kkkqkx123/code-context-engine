# 服务后端补充指标问题分析与修改方案

## 依据与范围

本文基于 `docs/analysis/service-backend-followup-metrics-2026-11-20.md`，并以当前代码为准核对。
该文档已经确认 hybrid 的 100 ms 悬崖是离线 mock provider 的令牌桶等待，四档后端差异不是
5,000 文件索引慢的主因。本文不再重复这些采集结论，也不把已经消失的现象当成新增 bug；
重点是处理仍然成立的写路径成本、生产限流口径和 ES 成本边界。

前一轮方案见 `docs/plan/service-backend-followup-remediation.md`。本文只处理后续指标暴露出的
新问题，不重做阶段埋点、插件执行模型、PostgreSQL 服务接线等已完成工作。

## 原因归纳

### 1. 5,000 文件服务索引慢：实体写入的事务放大是主要原因

阶段 TSV 显示四档组合的墙钟为 130–143 秒，其中 `entity_store_ms` 占 68–78 秒，
`vector_store_ms` 占 29–31 秒。向量、全文后端组合之间的总耗时差异只有 ±10%，因此后端类型
不是主因。

代码层面的直接原因是 `StorageCoordinator::store_parsed_files` 按单文件循环，并对每个文件分别
调用 `files_ensure`、`file_by_path`、`entities_delete_by_file`、`entities_insert_returning_ids`
和逐 parent 的 `entities_set_parent`。SQLite 分支中前四类写调用各自经过 `with_transaction`，
每次都提交一次事务；嵌套实体多时还要再增加一次或多次 parent 更新事务。5,000 文件、约
6.7 万实体因此产生数万次文件型 SQLite 提交。WAL 与 `synchronous=NORMAL` 没有关闭，单次提交
的成本虽小，但乘以调用次数后成为主导项。

流水线基准使用 `:memory:` 的 SQLite，提交不落盘，因此同样的调用形态只表现为几秒。这不是
测量错误，而是 durability 口径不同：服务在支付真实磁盘提交成本，基准没有支付。不能通过把
服务改成内存库或降低同步级别来消除差额。

`vector_store_ms` 的 29–31 秒也是真实的本地落盘成本，但现有阶段数据还没有把它拆成 WAL
提交、HNSW 构建和批量碎片。当前 `VectorBatchWriter` 已经按 512 点聚合并在操作末尾 flush，
不应在没有 commit 次数和阶段数据的情况下继续调大阈值或改后端。

### 2. hybrid 100 ms 悬崖：测试夹具缺陷已修，生产 SLA 口径仍未固化

默认 provider 限流为 600/min，即 10 rps、burst 10。索引耗尽令牌后，下一次嵌入获取令牌的
等待上限正好是 100 ms。索引阶段与查询阶段共享同一个 `base_url::provider_id` 令牌桶，所以
索引会把预算耗尽；vector 单路在每轮组合中最先执行，仍可利用剩余令牌，因而没有出现同样的
双峰。该路径与 Qdrant、ES、fusion 和首次连接无关；`embedding_first_ms` 全为 0 是启动预热
按设计生效的结果，不能继续用它解释尾延迟。

当前代码中，服务后端基准已经显式传入 `embedding_rate_limit: 0`，配置解析也对本地 provider
保留的默认值做了 unlimited 处理。因此夹具修复已经落地，不能再重复实现。剩余问题是生产默认
仍然是 600/min，且限流同时覆盖 chat、embedding、rerank；如果没有把 rps、burst 和预期并发写进
SLA，任何查询路径只要命中同一个桶，都会周期性出现最多 100 ms 的等待。

### 3. ES 与本地 Tantivy 的差距：后端固有成本，不是代码缺陷

去掉限流后，ES bm25 冷请求仍为 8–12 ms，本地 Tantivy 为 1–3 ms；差距集中在远端往返与刷新
语义。ES 进程还有 1 GiB JVM heap 之外的约 0.5–0.6 GiB 常驻开销，这些成本不属于基准进程 RSS。
该差距不能用改写查询口径或伪装成 Tantivy 等价实现来消除，应按后端选择和容量治理处理。

## 修改方案

### 第一步：把逐文件实体写入合并为每批一次事务

在 `crates/infra/cce-storage-common/src/metadb.rs` 的契约声明中增加一个后端中立的批量实体
写入操作。输入按文件组织，每个文件携带一条 `FileEnsureEntry`、该文件的 `EntityRecord` 集合，
以及每个实体的同文件父序号；返回值是按输入顺序对齐的数据库行 id。父引用不用领域实体 id
直接穿透存储层，也不新增跨层动态类型。契约注释中的事务边界同步改为：该操作对整批原子提交。

在 SQLite 分支 `crates/infra/cce-storage-metadb-sqlite/src/contract.rs` 中实现该操作：

- 在一次 `with_transaction` 内完成文件 upsert、按 `(project_id, epoch, path)` 取回 file id、
  按文件和 epoch 删除旧实体、批量插入实体、按返回 id 更新 parent。
- `file_by_path` 不能留在事务外，否则本次事务内刚 ensure 的文件行对查询不可见。
- 复用现有 repository 中接受事务句柄的 insert/delete 方法；逐 parent 的 UPDATE 使用事务内
  预编译语句，不再为每个 parent 新建事务。
- 全部 SQL 成功后只提交一次；任意一步失败整体回滚，不保留半个文件的实体。

在 PostgreSQL 分支 `crates/infra/cce-storage-metadb-pg/src/files.rs` 与 `lib.rs` 中实现同样的
单一 pooled transaction：文件 upsert、file id 解析、旧实体删除、实体插入和 parent 更新全部
在同一事务内完成，按路径排序处理以避免并发批次间的锁顺序分歧。该分支保留现有错误分类和
重放幂等语义。

替换旧路径，而不是与旧路径并存：

- `crates/app/cce-orchestrator/src/index/storage_coordinator/entities.rs` 在内存里完成路径分类、
  序列化、scoped name 与重载判别式计算，然后对每个 `store_parsed_files` 调用只发起一次存储
  契约调用；单文件热更新传入一个元素的切片即可。
- `crates/app/cce-orchestrator/src/index/orchestrator/batch.rs` 保持每批一次调用的边界，不跨批次
  无限累积待写实体；检查点只在整批实体提交且派生写入成功后推进。
- `crates/app/cce-orchestrator/src/index/storage_coordinator/file_ops.rs` 的
  `hot_update_bm25_files` 当前仍逐文件调用 `store_parsed_files`，改为一次批量调用，避免热更新
  重新引入相同的事务放大。
- `crates/app/cce-orchestrator/src/hot_update/processors/embedding.rs` 已按批量传入解析结果，
  只需确认它走同一新操作。

可观测性同步补齐：`IndexStageTimings`、`IndexStageTimingsDto`、索引 handler 映射、服务索引阶段
TSV 和完整流水线阶段 TSV 增加 `entity_store_calls`。验收时该计数应与批次数同阶，而不是与
文件数或实体数同阶。

### 第二步：同 durability 口径重测，再决定向量写路径是否修改

给现有完整流水线基准增加一个文件型 SQLite 模式，复用服务相同的 WAL 与 `synchronous` 配置，
并在阶段 TSV 记录 metadata backend、SQLite 模式和 durability 标签。内存库结果保留为对照行，
不把两种口径混在同一趋势里比较。这样 `entity_store_ms` 和 `vector_store_ms` 才能与服务路径
逐阶段对照。

实体批量事务上线后先重跑 500 与 5,000 文件档，确认实体阶段不再主导墙钟。若向量阶段仍占
主要部分，再在同一基准上补充本地向量引擎的 commit 次数、批量大小分布、WAL 写入字节和 HNSW
构建耗时；根据数据决定是降低 commit 频率还是接受为引擎固有成本。在此之前不改
`VECTOR_WRITE_BATCH`、不引入跨操作的新缓冲层、不降低 flush 的持久化保证。

### 第三步：固化限流与查询延迟 SLA 口径

保留生产 provider 默认 600/min，不为了基准数字改变线上默认值。对测试与本地 provider，继续
使用显式 `rate_limit = 0`；在服务测试环境和远程后端基准的配置构造处增加断言，确保 mock
provider 没有继承默认令牌桶，避免夹具回归后再次把等待误判为后端问题。

在配置文档中明确每条查询路径的预算来源：600/min 对应 10 rps、burst 10，单令牌等待上限
100 ms；限流按 `base_url::provider_id` 共享，chat、embedding 与 rerank 同时占用预算。交互查询
若要低于该等待上界，应使用独立 provider 配置或显式关闭本地 provider 限流，并把 rps、burst
和峰值并发写入服务 SLA。配置参考文档补充该口径，并用单测锁定本地 provider 默认值的解析行为；
不自动改写用户显式配置。

### 第四步：ES 成本独立治理

维持本地 Tantivy 为低延迟默认后端，ES 继续作为显式选择的远端分支。基准报告、配置文档和
`docs/tests/benchmark_gaps.md` 记录 ES 的冷查询分位、进程常驻内存以及 JVM heap 边界，
并声明其 SLA 不能由本地 Tantivy 的数字推导。若必须使用 ES，由部署侧约束 heap、刷新间隔和
首字节延迟目标；代码侧不增加模拟刷新、降级单路或吞错逻辑。

## 验证

- 实体写入单测覆盖多文件、嵌套 parent、重载判别式、重复回放和事务中途失败回滚；测试使用
  `expect`，不使用 `unwrap`。
- SQLite 与 PostgreSQL 契约测试覆盖同一批次的原子性；索引恢复用例覆盖实体提交前失败、提交后
  派生写入失败两种检查点边界。
- `cargo fmt` 与 `cargo clippy --all-targets --all-features` 通过。
- devs 上以同一 profile、同一 HNSW preset 重跑 500 与 5,000 文件四档服务矩阵，错误数为 0，
  索引结果与 parent/FTS 查询可复现；对比阶段 TSV 时 `entity_store_calls` 接近批次数，
  `entity_store_ms` 显著下降，墙钟变化由阶段数据解释。
- 同 durability 口径的流水线与服务阶段 TSV 各产出一行；若向量阶段仍高，新增的引擎指标必须能
  解释其构成，否则不进入向量写路径改动。
- hybrid 冷查询复测确认 `embedding_overhead_ms` 保持为 0，不再出现 98–100 ms 双峰；
  ES 与本地全文的差距按独立 SLA 记录，不作为本次回归失败条件。

## 已完成的代码修改

- 存储契约新增 `entities_replace_files`：一次调用内按文件完成 ensure、file id 解析、旧实体删除、
  批量插入和 parent 回指，SQLite 与 PostgreSQL 都在单个事务中提交；失败整体回滚。
- `StorageCoordinator::store_parsed_files` 构造整批输入后只调用一次新契约；`hot_update_bm25_files`
  也不再逐文件调用。旧的逐文件多事务路径已删除，单文件调用走同一个批量入口传入一个元素。
- `IndexStageTimings` 与 API DTO 增加 `entity_store_calls`，并在服务索引阶段 TSV 和完整流水线
  阶段 TSV 输出；计数应与批次数同阶。
- 向量写指标通过后端中立的 `VectorWriteMetrics` 采集：本地分支从 simvec 快照取事务数、写入点数、
  WAL flush/txn、提交等待、checkpoint 与 HNSW 构建耗时；远端分支没有等价本地计数器，保持为零。
  编排器在运行前后取差值写入阶段 TSV，不改变 flush 与检查点语义。
- `index_pipeline_stages` 新增 `CCE_BENCH_METADATA=file` 模式，使用与服务一致的 WAL 和
  `synchronous` 配置；趋势文件用 `metadata` 列区分 `memory` 与 `file`。
- 服务后端基准断言 mock provider 的 `embedding_rate_limit` 为 0；配置单测锁定本地 provider
  默认 600/min 解析为不限流；配置参考文档补充限流预算与查询 SLA 的关系。

## 远端验证记录

本地不执行编译、测试和基准采集。按 `docs/dev/devs-sync.md` 打包同步到 devs 后，在 devs 上执行
检查、定向单测和指标复测；日志与 TSV 取回 `.bench_logs/`，结果回填下表。

| 项目 | 命令或产物 | 结果 |
|---|---|---|
| 全目标检查 | `cargo check --workspace --all-targets --all-features` | 待 devs 执行 |
| 定向单测 | `cargo test -p cce-config -p cce-storage-metadb-sqlite -p cce-orchestrator --lib` | 待 devs 执行 |
| 同 durability 对照 | `CCE_BENCH_METADATA=file cargo run -p cce-orchestrator --bench index_pipeline_stages` | 待 devs 执行 |
| 服务四档矩阵 | `cargo bench -p cce-server --bench service_remote_backend_matrix` | 待 devs 执行 |

## 明确不做

- 不把服务改成内存 metadata，不关闭 WAL，不降低 `synchronous`，不靠放宽持久化换取指标。
- 不新增版本化模块或旧路径兼容层；批量操作直接替换逐文件写入路径。
- 不在没有 commit 与构建阶段数据的情况下修改向量批量阈值或切换默认向量后端。
- 不把 mock provider 的限流修复重复实现到生产默认值；生产默认仍为 600/min。
- 不为 ES 增加降级单路、缓存伪造命中或忽略刷新语义的特殊分支。
