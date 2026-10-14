# T-6 大数据量崩溃分析与 simvec 写入路径缺陷

> 输入：`crates/infra/cce-storage-vector-local/benches/local_vector_store.rs`（T-6 基准）、`crates/simvec`（嵌入式向量引擎）、`docs/benchmark/perf-overhead-design.md`。
> 触发事件：T-6 在 10w/50w 向量两档反复把机器打挂（opencode 服务进程被 kill、机器重启），本机 `/tmp` 为 3.6 GB tmpfs。
> 本次工作：缩小基准规模改为“对比导向”，并从代码层面定位崩溃根因，区分“基准自身缺陷”与“引擎实现缺陷”。

## 一、结论摘要

1. **崩溃不是内存放不下数据，而是写入路径的二次方复杂度把机器拖死。** 500k 档的索引本身约 300 MB（128 维 × 4 字节 × 50 万），但引擎为每个点都要**全量重写一次 payload 指针表**（`PayloadStore::put` → `tracker.save`），单点代价 O(N)，整轮代价 O(N²)：500k 档会产生约 **1.5 TB** 的重复落盘，全部写在 tmpfs（＝内存）上，机器必然被打挂。
2. **同一模式下 10w 档慢 50 秒、50w 档必然超时，与数据量呈平方关系**，可用历史基线定量验证（见第三节表）。
3. 除主因外还有四处实现缺陷：pending 队列整表克隆（O(P²)）、每次 mutation 全图扫描（O(N)）、HNSW 增量维护退化为全量重建＋同步 drain、WAL 只在 tombstone 比例超限时截断导致纯插入场景无限增长。
4. **把索引挪到真实机械盘后同一份代码慢约 170 倍（1w 点 1051 ms → 179231 ms，9511 → 56 pts/s）**，暴露出第二层、也是更普遍的问题：引擎为每个点做 2~3 次 fsync，而 WAL 事务里已经带了完整 payload，这些 fsync 对持久性是冗余的。含义是本地向量后端在机械盘上的写入上限约 60 pts/s，与集合规模无关。
5. 基准自身也有四个缺陷（无规模上限、点集重复物化三份、索引写在 tmpfs、criterion 成本失衡），本轮已修；其中“写在 tmpfs”是把引擎的磁盘压力直接转成内存压力的关键放大器。

## 二、崩溃复现链条

```
upsert_points（每点）
  ├─ WAL append + fsync                       一次事务一次，正常
  ├─ vectors.write_slot                       mmap 定点写，O(1)
  ├─ keys.append_key                          mmap 目录追加，O(1) 摊还
  ├─ payloads.put(slot, payload)              ← 每个点：
  │    ├─ pages.write_value：seek + write + fsync + 整页重新 mmap
  │    └─ tracker.save：序列化整个指针数组（12 字节 × N）+ 写临时文件 + fsync + rename
  │         → 单点 O(N)，整轮 O(N²)
  ├─ register_slot                            ← HNSW 已发布时：pending Vec 整表克隆后 push
  └─ filter_bitmap / payload_indexes 注册      O(1)
after_mutation（每次 upsert_batch）
  ├─ pending ≥ 65536：在调用者线程同步 drain 全部未入图节点
  ├─ repair_hnsw：遍历全部节点，逐层拿写锁做 retain（即使零 tombstone）
  └─ maybe_schedule_compaction：tombstone 比例超限才排压缩
```

关键环境事实：`/tmp` 是 tmpfs，容量 3.6 GB，占整机 7.1 GB 内存的一半。基准把真实 mmap 索引、WAL、压缩临时文件全写在临时目录里，**这些写入直接消耗内存**。因此“磁盘压力”在这里就是“内存压力”，引擎的二次方写入会被内核 OOM killer 直接兑现为进程/机器死亡——这就是“大数据量反复崩溃”的机理。

## 三、定量佐证（历史 T-6 实际数据）

`ValuePointer` 为 3 个 `u32`，共 12 字节；`tracker.save` 每次写全表，故单点代价 ∝ 12·N 字节，整轮总量 = 12·N²/2。

| 档位 | points | tracker 单点写 | 整轮 tracker 写量 | 实测串行耗时 | 实测吞吐 | batch p50 |
|---|---|---|---|---|---|---|
| 1w | 10k | 120 KB | 0.6 GB | 1051~1507 ms | 6.6k~9.5k pts/s | 108~147 ms |
| 10w | 100k | 1.2 MB | 60 GB | 49.6~49.9 s | 2.0k pts/s | 459~468 ms |
| 50w | 500k | 6 MB | **1.5 TB** | 未完成（机器被打挂） | — | — |

- 10w 档 60 GB / 50 s ≈ 1.2 GB/s，与 tmpfs 顺序写带宽吻合，说明时间几乎全花在反复重写指针表上，而非向量计算。
- batch p50 从 1w 的 108 ms 涨到 10w 的 459 ms（4.2×），与 tracker 单点写量 10× 的增长同向，剩余部分是 WAL 与 mmap 的固定开销。
- 空闲读延迟 1.4 ms（1w）→ 34 ms（10w）：`search_hnsw` 对 pending 队列做暴力扫描，pending 越多读越慢，这是同一缺陷的读侧表现。
- p99 3.6 s（10w）：`PENDING_DRAIN_GUARDRAIL = 65536` 触发时在 upsert 调用者线程同步把 6.5 万个节点插入图中。

### 3.1 同一份代码放到真实磁盘：慢约 170 倍，暴露 fsync 放大

本轮把基准索引从 tmpfs 挪到真实文件系统（`/dev/sdd`，**机械盘**）后重跑完整扫描（2k/5k/1w，dim=128）：

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

（tmpfs 口径对照：1w 串行 1051 ms、9511 pts/s、批 p50 108 ms；本表 1w 串行 179231 ms、56 pts/s、批 p50 18119 ms → **163× / 168×**。）

三条结论：

1. **吞吐与集合规模无关，与介质强相关**：2k→1w 的 pts/s 稳定在 51~57，批 p50 稳定在 18.1~18.5 s。单点成本 18 s / 1000 点 = **18 ms/点**，机械盘一次 fsync 约 5~10 ms，与"每点 2~3 次 fsync"吻合：payload 页写一次 `sync_all`（`pages.rs:147`）、tracker 保存一次 `sync_all`＋`rename`（`tracker.rs:110-112`，ext4 上 rename 另有一次日志提交）。规模项（O(N²)）在此被 fsync 项完全淹没，要等 fsync 修掉才会重新显露。
2. **生产微批与 1000 点微批的每点成本完全相同**（551 ms/32 点 ≈ 17 ms/点，与 1000 点口径一致；总吞吐 57 vs 56 pts/s）。这直接**否证了"包装层聚批能改善本地向量写入"这一方案**：fsync 次数是按点付的，与批大小无关，聚批只减少 WAL 事务数。
3. **并发无增益甚至更慢**（0.87×/1.11×/1.03×）：`apply_txn` 全程持 `inner.write()`，并发任务只在锁外排队；2k 档 0.87× 是 8 个任务在同一把写锁上的争用开销。读侧尾延迟 18 s 级——一次读撞上在途写入就要等整批落盘完成。

**关键点：那 2~3 次 fsync 对持久性是冗余的。** WAL 记录里已经带了完整 vector 与完整 payload（`wal.rs:50-59` `WalPoint { id, vector, payload: Option<String> }`），并在事务边界做 group-commit 落盘（`wal.rs:163-207`）。payload 页与 tracker 都是从 WAL 可重建的派生状态，本不需要每点同步。

这条把问题的严重度从"大数据量才崩"提升为"**任何规模都在为每点付 2~3 次 fsync**"：本地向量后端在机械盘上的写入上限约 60 pts/s（约 3600 点/分钟），与集合大小无关。此前 tmpfs 掩盖了这一层，只露出 O(N²) 那一层。

纯插入场景下 compaction 从不触发（`compact` 在 `tombstone_count == 0` 时直接返回），所以 **WAL 也不会被 checkpoint/截断**，WAL 与索引一起在 tmpfs 上线性堆积，是崩溃的第二增量。

## 四、代码级缺陷清单

> `crates/simvec` 是 git 子模块，下列修改属于上游引擎改动，需在子模块仓提交；本仓库本轮只做基准侧修复与文档。每条给出 file:line 与修复方向。

### D1（P0，崩溃主因）payload 指针表每点全量重写

`crates/simvec/src/storage/payload_store.rs:223`：`put()` 每条 payload 写完都调用 `tracker.save()`；`crates/simvec/src/storage/payload_store/tracker.rs:103-137`：`save()` 把**整个** `pointers` 数组序列化成临时文件、`sync_all()`、再 `rename`。slot 数为 N 时单点写 12·N 字节，整轮 O(N²)。

修复方向：`put` 只更新内存 tracker；持久化移到 WAL 事务边界（`apply_txn` 提交时一次）或改为追加式指针日志，并在容量变化/集合关闭时兜底刷一次。语义不变（WAL 已是权威日志，tracker 是可重建缓存），但单点代价回到 O(1)。

### D1b（P0，与 D1 同一个调用点）每点 2~3 次 fsync

`crates/simvec/src/storage/payload_store/pages.rs:147` 每次 payload 写都 `sync_all()`；`tracker.rs:110-112` 每次保存都 `sync_all()` + `rename`（机械盘上 rename 另有一次日志提交）。二者都是派生状态同步：WAL 事务内已落盘完整 vector＋payload（`wal.rs:50-59`），可整体重建。

修复方向：`put` 去掉 tracker 保存与页写 fsync，只在 `apply_txn` 提交点做一次 tracker checkpoint；页文件 fsync 一并去掉。预期机械盘写入吞吐从 58 pts/s 提升到只受单次 WAL group-commit 限制。

### D2（P0）pending 队列每次 push 整表克隆

`crates/simvec/src/storage/index_lifecycle.rs:202-204`（以及 `:103-109`、`:163-172`、`:211-213`）：`let mut p = (**self.pending.load()).clone(); p.push(slot); self.pending.store(Arc::new(p));`。pending 在两个维护 tick（30 s）之间持续增长，批量插入时可达数万，累计 memcpy 为 O(P²)。

修复方向：pending 改为 `Mutex<Vec<u32>>` 或分片结构，push 为 O(1) 摊还；读路径仍可取快照。`pending_len()` 可由计数器提供。

### D3（P1）每次 mutation 全图扫描 repair

`crates/simvec/src/engine.rs:856-871`：`after_mutation` 每次 upsert_batch 都调 `store.repair_hnsw()`；`crates/simvec/src/index/hnsw.rs:1242-1268`：`repair` 遍历 `self.nodes` 全表，逐层取写锁做 `retain`，即使一个 tombstone 都没有也全额执行。

修复方向：函数开头加 `tombstones.count_ones() == 0` 早退（`TombstoneBits` 已按 64k 位分块，`count_ones` 很便宜，见 `crates/simvec/src/storage/tombstones.rs:50-55`）；进一步可维护脏节点集合替代全表扫描。

### D4（P1）HNSW 增量维护退化为全量重建＋同步 drain

`crates/simvec/src/engine.rs:1046-1066`：只要 `pending_len() > 0` 就 `schedule_build`（全量重建）；`crates/simvec/src/engine.rs:857-859` + `index_lifecycle.rs:542-585`：pending 达到 65536 时在**调用者线程**同步 drain，单个 upsert 批次被拖到数秒。

修复方向：tick 分支已经会先 drain 再 sweep，问题在于重建条件过宽与同步 drain；应改为按 pending 比例决定是否重建，并把 guardrail 降到远小于 65536 或改为非阻塞（记入下一 tick）。

### D5（P1）WAL 只在 tombstone 比例超限时截断

`crates/simvec/src/storage/compaction.rs:258`：`tombstone_count == 0` 直接返回；`maybe_schedule_compaction` 只按 tombstone 比例触发。纯插入/覆盖为主的负载下 WAL 只增不减，重启时还要全量 replay。

修复方向：增加 WAL 字节数阈值触发 checkpoint（`wal.truncate()`，`crates/simvec/src/storage/wal.rs:273-276` 已具备该能力），与 tombstone 比例阈值并列。

### D6（P2）压缩双倍落盘＋持写锁全程

`crates/simvec/src/storage/compaction.rs:275-347`：`write_temp_files` 先写 `vectors_tmp.bin`/`quant_tmp.bin`/`keys_tmp.bin`/`payloads_store_tmp` 再原子换入，峰值占用翻倍；`:253-267` 的竞争回退路径在整个重写期间持有 `inner.write()`，期间全部 upsert 与查询停顿。

修复方向：压缩前检查目标卷剩余空间；竞争路径改为分段重写或让锁只覆盖提交阶段。

### D7（P2）每次 payload 写都整页重新 mmap 并 fsync

`crates/simvec/src/storage/payload_store/pages.rs:125-155`：`write_value` 每次 `seek + write_all + sync_all`，随后 `MmapOptions::new().map(&entry.file)` 重新映射整个页文件。页大小固定，写放大随页大小线性增长；fsync 部分与 D1b 合并处理。

修复方向：WAL 已保证持久性，页写可去掉 `sync_all`；mmap 在页创建时建立一次，扩容时只映射新增区间。

### D8（P2）缺少规模/内存准入

引擎没有内存预算或容量上限配置（对比 BM25 分支有 `writer_memory_budget`），调用方无法预判某个集合会不会把机器压垮；基准此前也没有规模上限，二者叠加才让问题在测试环境反复出现。

修复方向：为本地引擎暴露索引内存估算（vectors + graph + 指针表 + WAL 上限）与可选容量上限；基准保留规模封顶与环境变量覆盖（本轮已做）。

## 五、基准侧缺陷与已落地修改

| 缺陷 | 处置 |
|---|---|
| 无规模上限，50w 档必然打挂机器 | 默认档位降为 2k/5k/10k，`CCE_BENCH_VECTOR_SCALES` 可在安静大内存机器上放大；对比形态（串行/并发吞吐、批延迟增长、读抖动）不变 |
| 每档点集物化三份（顺序、并发、读写抖动各一份） | 并发阶段改为任务共享同一 `Arc` 点集、只克隆自己负责的微批；读写抖动段只重发 1 个微批 |
| 索引写在 tmpfs，磁盘压力直接变成内存压力 | 新增 `ScratchDir`，索引落在 `target/bench-vector/`（真实磁盘），退出即清理 |
| criterion 段在同一 warm store 上反复重插，只累加 WAL；且 criterion 0.5 强制 ≥10 采样 | 直接移除 criterion 段：fsync-bound 路径上 10 采样要 6 分钟，换来的数字与手动扫描已记录的每批 p50/p99 重复 |
| 微批 1000 与生产 32 不一致，系统性低估 fsync 压力 | 新增 5k 向量 × 32 点/批的生产微批探针行 `upsert_seq_prodbatch` |

## 六、验证方案

1. 缩小后的基准重跑：若 D1 是主因，batch p50 应随档位近似线性增长（1w→3w→5w），且总耗时应从“分钟级/档”降到“十秒级/档”，不再打挂机器。
2. simvec 侧单测（子模块）：为 `PayloadStore::put` 增加“连续 N 次 put 只产生 O(N) 级落盘字节”的回归测试（统计写字节数或注入计数型 `File`），防止二次方行为回归。
3. 修复后重跑 T-6 与生产 batch preset 对照，预期串行吞吐提升一个数量级以上（10w 档从 2k pts/s 回到 6k~9k pts/s 区间），读抖动 p99 从秒级回到毫秒级。
4. 机械盘口径下验证 D1b：同一负载的 pts/s 应从约 60 提升到不再由每点 fsync 决定（可用 `strace -c -e trace=fsync,rename` 计数确认每点 fsync 次数归零，或直接用 pts/s 判断）。

**本轮已取得的验证数据（第 1 条的反向证据）**：索引改落真实机械盘后，2k/5k/1w 三档 pts/s 稳定在 51~57、批 p50 稳定在 18.1~18.5 s，说明当前成本几乎全是每点固定成本（2~3 次 fsync），规模项被淹没；并发 0.87×~1.11×；读尾延迟 18 s 级。生产微批（32 点）探针的每点成本与 1000 点微批完全一致（551 ms/32 点 vs 18.1 s/1000 点，均约 17~18 ms/点），因此**包装层聚批对本路径无效**，必须先修 D1/D1b。修复后再按 2k/5k/10k 观察增长曲线，规模项才会重新显露。

## 七、执行记录

- 环境：5 逻辑核、7.1 GB 内存、`/tmp` 为 3.6 GB tmpfs、`/` 为 336 GB **机械盘**（当时可用 129 GB，磁盘空间不是瓶颈；`/sys/block/sdd/queue/rotational = 1`）。
- T-6 历史两轮完整数据（1w/10w）来自 `crates/infra/cce-storage-vector-local/benches/results/local_vector_store.tsv`；50w 档两次尝试均导致机器重启/后台任务被取消。
- 真实磁盘口径本轮数据（2k/5k/1w 全档）：2k 串行 36137.1 ms / 55 pts/s / 批 p50 18464.43 ms、并发 41373.3 ms / 0.87×、空闲读 259.92 ms、读写并发 p99 21836.74 ms；5k 串行 97604.7 ms / 51 pts/s、并发 87841.6 ms / 1.11×、读写并发 p99 18345.90 ms；1w 串行 179231.4 ms / 56 pts/s、并发 174662.5 ms / 1.03×、读写并发 p99 18295.74 ms；生产微批探针 5k×32 点 87479.5 ms / 57 pts/s / 批 p50 551.05 ms。3w 档（546746.0 ms / 55 pts/s）来自改档前的探路运行，已被 2k/5k/1w 完整轮取代。
- 代码定位依据：`payload_store.rs:223`、`tracker.rs:103`、`pages.rs:151`、`index_lifecycle.rs:202`、`engine.rs:857`、`hnsw.rs:1251`、`compaction.rs:258`，均为只读核对，未修改 `crates/simvec`。
