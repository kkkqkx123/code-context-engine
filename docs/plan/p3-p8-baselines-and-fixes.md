# P-3~P-8 与服务层：基线采集、结论与采纳修改

> 输入：根目录 `perf-analysis.md`（T-2/T-3 已测）、`docs/plan/p2-chunker-and-baseline-collection-plan.md`（P-2 已落地、T-7/T-8 已采）、`docs/benchmark/perf-overhead-design.md`（P→T 映射）。
> 本次工作：补齐 T-1/T-4/T-5/T-6/T-9/T-10 六项基线（T-4/T-5 系历史残留行＋代码复核，T-1/T-9/T-10/T-6 为本机新跑），逐项给出结论与下一步采纳修改。P-2 结论不变，不复述。
> 环境：5 逻辑核、约 7 GB 内存；`cargo bench` 编译画像；合成输入确定性生成，跨轮以加速比与增长曲线为准。

## 一、基线总览（本轮结束后状态）

| 基准 | 对应 | 数据状态 | 核心数字（本机） | 结论关键词 |
|---|---|---|---|---|
| T-1 端到端 | P-9 | 新增第 2 行 | Small 500 文件 806.5 ms（前值 977.7 ms） | criterion 段有累积 artifact，口径以手动计时为准 |
| T-2 embedding 调度 | P-1 | 历史 2 行，不重跑 | 并发加速约 9.8× | 调度并发有效，无新动作 |
| T-3 chunker 并发 | P-2 | 历史 7 行，已落地 | bounded 中位约 2.65× | 见 P-2 文档 |
| T-4 元库写放大 | P-3 | 历史 2 行＋代码复核 | row 5.1~5.4 s vs batch 5.8 s（30 万行，内存库） | 前提已不存在：生产早已批量＋单事务；残留为逐实体 prepare |
| T-5 BM25 写 | P-4 | 历史 1 轮（2000 文档）＋代码复核 | 提交 x1 178.6 s / x10 7.5 s / x50 0.9 s；删除逐文件 4.1 s vs 合并 8 ms | 全量索引已收敛到每扫描批约 2 提交；残留为热更新逐文件提交/删除 |
| T-6 向量包装层 | P-6 | 历史 2 轮（tmpfs，1w/10w）＋本轮 3 轮（机械盘，2k/5k/1w＋探路 3w） | 机械盘：1w 串行 179231 ms、56 pts/s、批 p50 18119 ms、并发 1.03×、读尾 18 s；生产微批每点成本与 1000 点批一致 | 每点 2~3 次冗余 fsync；写完全串行；O(N²) 指针表重写是崩溃主因 |
| T-7 关系终结 | P-5 | 历史 3 行 | 100k 全量重建 512~1265 ms | 全量 O(N) 成立；生产已走稀疏候选＋增量发布 |
| T-8 快照发布 | P-7 | 历史 1 轮＋TSV 已补 | 共享 0.6/13.9/168.8 ms vs 深拷贝 2.2/36.4/458.4 ms | 共享路径同样重建派生索引，随规模增长 |
| T-9 扫描哈希 | P-8 | 本轮新跑 | 单线程 78.4 ms；8 线程 21.8 ms（3.59×）；5 线程 23.1 ms（3.39×）；重过滤 delta 373.7 ms/5000 文件 | IO 密集，并行有效且可超核；过滤匹配是第二成本 |
| T-10 服务并发 | 服务层 | 本轮新跑 | K=1 928 ms、K=16 16.0 s、K=64 64.1 s；吞吐恒 1.0 req/s | 请求完全串行，锁在 `Engine::search` |

## 二、各基准数据与解读

### 2.1 T-1（`index_pipeline_stages`，Small 500 文件）

- 手动计时：`small 500 806.5 ms 620.0 files/s`（前值 977.7 ms）。两轮差约 17%，落在机器负载噪声带内；该口径关闭向量/BM25/关系，仅含扫描→解析→分块→元库写入。
- criterion `end_to_end_small` 约 10.9~11.1 s/iter（前值约 12.2 s），且打印 `Unable to complete 10 samples in 8.0s`。根因：同一内存库上重复 `execute`，每轮把相同 500 文件再索引进不断增长的表，测的是累积而非稳态。本轮已修基准：criterion 每迭代建新库（`index_pipeline_stages.rs`），下轮起 criterion 口径恢复稳态；历史两行 criterion 值作废，只保留手动计时行。
-  bench 仍硬编码 `max_concurrency: 10`、`worker_threads(4)`，与 P-2 落地的核数默认值不一致。为保两行可比，本轮不动参数；后续修订再对齐，届时另起基线段。

### 2.2 T-4（`metadb_write_amplification`，内存库，30 万行）

历史两轮几乎一致：`row_by_row` 5095/5397 ms、`batch_prepared` 5815/5821 ms、`checkpoint_per_unit` 11.9~12.0 ms、`checkpoint_merged` 7.6~7.7 ms。

代码复核颠覆了该对照的字面含义（`cce-storage-metadb-sqlite/src/contract.rs:200-238` 等）：`RelationStorage::*_upsert` 早已是单事务＋复用预编译语句，与 `insert_batch` 做的是同一量级功；内存库下 fsync 成本为零，事务数同为 3，剩余差异只是 `insert_batch` 侧多余的 `Vec<Vec<&dyn ToSql>>` 装箱与逐行 `last_insert_rowid`。即：**P-3 描述的“逐行事务”在 files/entities/chunks 主路径上已不存在**，该基准的两个主场景测的是同一优化后路径的两种写法，`batch_prepared` 更慢不代表回归。

真正的残留（生产代码仍在付）：`storage_coordinator/entities.rs:156-242` 逐实体 `EntityRepository::insert`（`helpers.rs:51-60` 经 `tx.execute` 每次重 prepare 22 列 INSERT），虽在同一事务内，仍付 N 次 prepare。检查点侧生产已合并（`vector.rs:192`、`bm25.rs:115` 均调 `insert_work_units`），bench 的 per-unit 对照量的是已删除路径。

### 2.3 T-5（`bm25_write`，2000 文档，本地 tantivy）

历史单轮信号极强：`bulk commit x1` 178622.5 ms → `x10` 7504.7 ms → `x50` 882.8 ms（约 200×）；`delete per-file` 4082.7 ms vs `delete merged` 8.1 ms（约 500×）。机理：每次 commit 付新段落刷盘＋fsync＋`meta.json` 发布＋LogMerge 段合并＋reader reload 全套；N 次提交还带来 O(N log N) 合并债与反复的 writer 线程池启停（`client.rs:190-232` 重试环）。

生产侧已部分收敛：`storage_coordinator/bm25.rs:128-144` 把微批载荷合并后单次 `batch_index`（每扫描批约 2 提交：分块＋摘要），`batch.rs:11-28` 每次调用 commit＋`client.rs:204` reload。残留乘数全在变更路径：`file_ops.rs:103-119` 热更新逐文件 `store_bm25`（1 提交/文件）、`candidate/prepare.rs:42-51` 逐路径 scoped-epoch 删除（每次删除＝ search＋commit＋reload）、`summary.rs:377-387` 逐文件摘要删除。

TSV 缺陷已修：行内原无文档规模（2000 vs 默认 20000 无法区分），现追加 `# docs N` 注释行。

### 2.4 T-6（`local_vector_store`，dim=128，微批 1000）

历史两轮完整数据（1w/10w）：

| 场景 | 规模 | total_ms | pts/s | batch p50 | batch p99 |
|---|---|---|---|---|---|
| upsert_seq | 1w | 1417.0 / 1051.4 | 7057 / 9511 | 147.33 / 108.00 | 173.61 / 140.26 |
| upsert_conc | 1w | 1135.4 / 1034.8 | 8808 / 9664 | 1.25× / 1.02× | - |
| read_idle | 1w | 1.70 / 1.41 ms | - | - | - |
| read_during_write | 1w | - | - | 0.86 / 0.84 | 143.53 / 134.89 |
| upsert_seq | 10w | 49581.3 / 49899.7 | 2017 / 2004 | 467.76 / 458.83 | 3611.38 / 3645.52 |
| upsert_conc | 10w | 58991.8 / 48543.6 | 1695 / 2060 | 0.84× / 1.03× | - |
| read_idle | 10w | 35.60 / 34.27 ms | - | - | - |
| read_during_write | 10w | - | - | 87.16 / 45.30 | 1758.71 / 1719.00 |

50w 档两次尝试均把机器打挂（服务进程被 kill、后台任务随机器重启被取消），未取得数据。

同一份代码把索引挪到真实机械盘（`/dev/sdd`，rotational）后重跑完整扫描（2k/5k/1w，探路轮另跑过 3w）：

| 场景 | 规模 | total_ms | pts/s | batch p50 | batch p99 |
|---|---|---|---|---|---|
| upsert_seq | 2k | 36137.1 | 55 | 18464.43 | 18464.43 |
| upsert_conc | 2k | 41373.3 | 48 | 0.87× | - |
| upsert_seq | 5k | 97604.7 | 51 | 18323.15 | 24828.25 |
| upsert_conc | 5k | 87841.6 | 57 | 1.11× | - |
| upsert_seq | 1w | 179231.4 | 56 | 18119.85 | 18548.68 |
| upsert_conc | 1w | 174662.5 | 57 | 1.03× | - |
| upsert_seq 生产微批(32) | 5k | 87479.5 | 57 | 551.05 | 723.87 |
| read_during_write | 2k/5k/1w | - | - | 0.52 / 0.69 / 3.32 | 21836 / 18345 / 18295 |

三个结构信号：

1. **并发零增益甚至更慢**（0.87×/1.11×/1.03×）：写路径完全串行，`apply_txn` 持 `inner.write()` 横跨 WAL append＋fsync 与所有落盘写入（`simvec/src/storage.rs:519-551`），顺序 await 又蹭不到 group-commit；并发任务只在锁外排队，2k 档 0.87× 即锁争用开销。读侧尾延迟 18 s 级——一次读撞上在途写入要等整批落盘。
2. **写成本与介质强耦合，每点 2~3 次 fsync，且与集合规模无关**：tmpfs 上 1w 点 1051 ms/9511 pts/s，机械盘上 179231 ms/56 pts/s（约 170×）；2k→1w 的 pts/s 稳定在 51~57、批 p50 稳定在 18.1~18.5 s，即单点约 18 ms。WAL 记录已含完整 vector＋payload（`wal.rs:50-59`），页写与 tracker 保存的 fsync 属冗余同步。含义：本地后端在机械盘上的写入上限约 60 pts/s，与集合大小无关。
3. **生产微批探针否证了"包装层聚批"方案**：32 点/批的每点成本（551 ms/32 点 ≈ 17 ms/点）与 1000 点/批完全一致，总吞吐 57 vs 56 pts/s——fsync 按点付，与批大小无关。聚批只能减少 WAL 事务数，必须先修引擎内的 tracker 每点全量重写与冗余 fsync。

**崩溃根因与规模调整**：定量归因表明主因是引擎为每个点全量重写 payload 指针表（`PayloadStore::put` → `tracker.save`，`simvec/src/storage/payload_store.rs:223`、`tracker.rs:103-137`），单点 O(N)、整轮 O(N²)：10w 档产生约 60 GB 重复落盘（实测 50 s，约 1.2 GB/s，与 tmpfs 带宽吻合），50w 档将产生约 1.5 TB；又因 `/tmp` 是 3.6 GB tmpfs，索引、WAL 与压缩临时文件全部直接消耗内存，OOM 必然发生。完整机理与缺陷清单见 `docs/plan/p6-large-scale-crash-analysis.md`。

基准已改为对比导向：默认档位 2k/5k/10k，`CCE_BENCH_VECTOR_SCALES` 可放大；索引改落 `target/bench-vector/`（真实磁盘）而非 tmpfs；并发段共享同一份点集；读写抖动段只重发 1 个微批；新增 5k×32 生产微批探针；criterion 段移除（10 采样成本高于收益，手动 p50/p99 已记录）。

### 2.5 T-9（`scan_parallel`，Medium 5000 文件）

本轮新跑：单线程 78.4 ms（criterion 87.2 ms）；2/4/8/5 线程 44.3/36.7/21.8/23.1 ms，加速比 1.77×/2.14×/3.59×/3.39×（criterion 并发 29.3 ms）。与 T-3 的 CPU 密集形态相反：哈希含文件读，IO 密集，8 线程在 5 核盒上仍最优，允许小幅超配到约 8。

过滤开销：空规则 104.8 ms vs 200 exclude＋200 gitignore 478.5 ms，delta 373.7 ms（约 75 µs/文件），达扫描成本的 4.6 倍。生产 `walker.rs:779-874` 逐文件线性过全部 Glob（`glob.rs:41-76` 回溯匹配）＋`ignore.rs:198-228` 逐 pattern＋逐文件 `is_dir` 系统调用，文件级决策无缓存（目录级才有 `filter_cache`）。生产扫描今日零并行（全仓无 rayon/thread）。

### 2.6 T-10（`service_layer_concurrency`，3 文件夹具）

本轮新跑：K=1 928.2 ms、K=16 16028.0 ms、K=64 64091.1 ms，吞吐恒 1.0 req/s——墙钟严格线性，请求被完全串行。根因：`engine/ops.rs:464-467 Engine::search` 持 `Mutex<Searcher>` 横跨整个 `searcher.search().await`，而聚合路径 `search.rs:439` 早已是锁内克隆＋释放。另：单请求约 1 s 绝对值含测试 HTTP stub 嵌入往返（`tests/common.rs:187 spawn_llm_stub`），回归口径看 K 形态而非绝对值。基准 bug：criterion 段嵌套 `block_on` 必 panic，已重构（criterion 移出 runtime），待重跑验证。

## 三、采纳修改（按优先级排序）

> 只列本轮基线直接支撑的修改；P-2 六项已落地不重复。顺序按“信号强度 × 落地成本”排。

1. **服务层去串行（T-10，P0）**：`engine/ops.rs:464-467` 改为锁内 `clone` 后释放再 `search`（照抄 `search.rs:439` 既有模式），并审计同文件 `:60,77,92,110,342,396` 与 `relation.rs:509` 是否有跨 await 持锁。验证：T-10 重跑，K=16/64 墙钟应趋平、吞吐随 K 上升。风险低（`Searcher: Clone`，全字段 Arc）。
2. **BM25 变更路径批化（T-5，P0）**：新增 `delete_documents_by_file_paths_scoped_epoch`（单 search＋单 commit＋单 reload）并经 `FulltextStorage`＋`fulltext.rs:197-226` 暴露，替换 `candidate/prepare.rs:42-51` 循环；热更新 `file_ops.rs:103-119` 按批累积后单次 `store_bm25_batched`；合并每扫描批的第二次提交（分块＋摘要一次 `batch_index`，或 bulk 期间 reader 设 manual＋批尾 reload）。验证：2000 文档档删除场景 4.1 s→数十 ms 量级；`bm25_write.tsv` 添新行。
3. **向量写入去冗余 fsync（T-6，P0）**：T-6 生产微批探针已**否证**包装层聚批方案（32 点/批与 1000 点/批的每点成本一致，57 vs 56 pts/s），因此不再做 `vector.rs` 聚批。真正要改的在 simvec 子模块：`PayloadStore::put` 去掉每点的 `tracker.save` 与页写 fsync，改为在 WAL 事务边界 checkpoint 一次（D1/D1b，`payload_store.rs:223`、`tracker.rs:110`、`pages.rs:147`）。验证：T-6 机械盘口径 pts/s 从约 56 提升一个数量级以上，读尾延迟从 18 s 级回到毫秒级。修完再评估是否需要包装层聚批与有界并发 persist。
4. **实体写入预编译上提（T-4，P1）**：`entities.rs:240` 循环内 `EntityRepository::insert` 改为循环外一次 prepare＋循环内 execute（照抄 `contract.rs:269-321 entities_upsert` 形态）；`File/Entity::insert_batch`＋`execute_insert_batch` 的 last-rowid 路径标记废弃（与 upsert 的 ON CONFLICT/epoch 语义冲突）；删除 `insert_work_unit` 单事务 API 留合并版；T-4 头注释与场景名按实修正（`row_by_row→upsert_prepared` 等）。
5. **扫描并行＋过滤缓存（T-9，P1）**：walk/filter 保持单线程（保序＋`filter_cache: HashMap` 非 Sync），候选 `(path, metadata)` 收集后进 `rayon` 有界池（线程数 min(cpu,8)，IO 密集允许超配）；`should_include_file` 空规则短路、目录级排除缓存、单次扫描内 `(relative_path, dir_flag)` 记忆化、`is_dir` 上提。验证：T-9 单线程/criterion 对照加速比≥3×（5 核盒），过滤 delta 收窄。
6. **T-1 criterion 口径（已修，待验证，P2）**：每迭代新库已落地；下次 T-1 运行即验证（预期单 iter 回到约 1 s 量级、10 采样落进 8 s 测量窗）。另：`max_concurrency: 10`/`worker_threads(4)` 硬编码与生产默认偏离，留待下次修订对齐并另起基线段。
7. **T-10 criterion 重跑（P2）**：bench 重构已落地；与修改 1 的验证合并为一次重跑（预期 criterion `concurrent_16` 可执行且中位数与手动 16 s/16 请求量级一致，修后应显著更低）。

## 四、基准卫生修复（已落地，待一次集中验证）

| 文件 | 修复 | 验证方式 |
|---|---|---|
| `cce-server/benches/service_layer_concurrency.rs` | criterion 移出 `block_on`（原必 panic） | T-10 重跑 |
| `cce-storage-vector-local/benches/local_vector_store.rs` | `tokio::spawn`→`rt.spawn`（原 10k 后必 panic）；补 `local_vector_store.tsv` 落盘；规模降为 2k/5k/10k＋`CCE_BENCH_VECTOR_SCALES` 覆盖；索引改落 `target/bench-vector/` 真实磁盘；并发段共享点集；抖动段限 1 微批；新增 5k×32 生产微批探针；criterion 段移除 | 本轮 T-6 三轮重跑 |
| `cce-scanner/benches/scan_parallel.rs` | 补 `scan_parallel.tsv` 落盘（含线程数/加速比/过滤行） | `cargo check`（不重跑，本轮数字已记入二节） |
| `cce-orchestrator/benches/index_pipeline_stages.rs` | criterion 每迭代新库 | 下次 T-1 运行 |
| `cce-storage-bm25/benches/bm25_write.rs` | TSV 追加 `# docs N` | `cargo check` |

`cargo check -p cce-server -p cce-scanner -p cce-storage-vector-local -p cce-storage-bm25 --benches` 一次过即可覆盖全部；期间不与任何 bench 并行。

## 五、明确不做的事项

- T-4 按字面“批量预编译改造”重做 files/chunks 主路径：已是该形态，重做零收益；只做第 4 条的残留收尾。
- T-5 全量索引提交机制大改：已收敛到每批约 2 提交，继续压提交频率收益小且抬高崩溃丢失窗口；火力集中在变更路径（第 2 条）。
- T-6 在包装层复刻 WAL/group-commit：那是 simvec 子模块职权；包装层只降事务数、加并发（第 3 条）。
- 把 T-6 档位推回 10w/50w 追求绝对数字：主因是引擎 O(N²)，规模放大只会复现崩溃；等 simvec 的 D1/D2 修复落地后再议大档位。
- T-9 用无界线程或全流程并行：walk/filter 保序＋缓存结构决定只并行哈希段；线程数封顶约 8。
- 以 T-10 绝对 ms（约 1 s）为优化目标：含 stub HTTP 往返，目标是 K 形态（第 1 条），绝对值待真实 embedder 下再议。
- P-5/P-7 增量终结/快照：T-7/T-8 证实 O(N) 但生产已走稀疏候选＋增量发布；在终结确实成为尾部瓶颈前不投入。

## 六、执行记录

- T-9：`cargo bench -p cce-scanner --bench scan_parallel`，2m 编译；数字见 2.5；criterion `single_thread_hash` 约 87.2 ms、`bounded_concurrency_hash` 约 29.3 ms。
- T-10：`cargo bench -p cce-server --bench service_layer_concurrency`，19m 编译；手动三档落 TSV；criterion 段 panic（已修）。
- T-1：`cargo bench -p cce-orchestrator --bench index_pipeline_stages` 第二轮，手动 806.5 ms 落 TSV；criterion 约 11 s/iter（artifact，口径已修）。
- T-6：首轮 10k 顺序/并发＋空闲读出数后 panic 于读写抖动段（`tokio::spawn` 无 runtime 上下文，已修）；重跑收 100k/500k 中。
- T-4/T-5：沿用历史 TSV 行＋逐行代码复核，未重跑（T-5 全量 20k 档沙箱不可行；T-4 重跑不能回答新问题）。
