# T-6 后续：simvec 并发写与 WAL group-commit 设计方案

> 输入：`docs/plan/p6-large-scale-crash-analysis.md`（崩溃分析与 D1~D8 清单）、`docs/plan/p6-simvec-write-path-fixes.md`（D1~D8 细化方案）、本轮 T-6 机械盘重跑基线与 `strace` fsync 计数、`crates/infra/cce-storage-vector-local/benches/local_vector_store.rs` 修正后的并发行。
> 本轮工作：只出方案、基线与可判定门槛，**不改代码**。D1~D8 及其后三处收尾已落地并验证，本文承接其后仍未解决的两个问题：并发放大与 WAL group-commit 失效。
> 环境：`/dev/sdd` 机械盘（`rotational=1`，与分析文档同一介质）、12 逻辑核、约 14 GB 内存、`dim=128`；`CCE_BENCH_VECTOR_SCALES` 默认 2k/5k/10k。

---

## 一、当前基线（作为后续对照的起点）

### 1.1 已落地变更

| 阶段 | 内容 | 状态 |
|---|---|---|
| D1~D8 | `p6-simvec-write-path-fixes.md` 全部落地：payload 写入路径去同步化、目录文件摊还、WAL 字节阈值 checkpoint、pending 改互斥 Vec、repair 零 tombstone 早退、异步 drain + 按比例重建、压缩前 `statvfs` 预检、`max_points` 准入与 `memory_estimate()` | 已合入工作区并通过单测/集成/clippy |
| 收尾 1 | `max_points` 准入前移到 `wal.append` 之前（原实现把必然失败的记录写进 WAL，导致集合后续每次 `open` 都失败） | 已落地 |
| 收尾 2 | `apply_txn` 中 meta.bin 保存条件由「next_slot 或容量变化」收紧为「仅容量变化」；slot 水位交由 WAL 重放收敛 | 已落地 |
| 收尾 3 | 两个 bench 的 TSV 落盘（`benches/results` 缺失 + `.ok()` 静默吞错），并发段改为同事务数口径 | 已落地 |

### 1.2 机械盘基线（本轮实测，`cargo bench -p cce-storage-vector-local --bench local_vector_store`）

**修复前**（D1~D8 之前，每点 2~3 次 fsync + O(N²) 指针表重写；数据取自分析文档，口径为 2k/5k/1w + 5k×32 生产微批）：

| 场景 | 规模 | total_ms | pts/s | batch p50 | batch p99 |
|---|---|---|---|---|---|
| upsert_seq | 2k | 36137.1 | 55 | 18464.43 | 18464.43 |
| upsert_conc | 2k | 41373.3 | 48 | 0.87× | - |
| upsert_seq | 5k | 97604.7 | 51 | 18323.15 | 24828.25 |
| upsert_conc | 5k | 87841.6 | 57 | 1.11× | - |
| upsert_seq | 10k | 179231.4 | 56 | 18119.85 | 18548.68 |
| upsert_conc | 10k | 174662.5 | 57 | 1.03× | - |
| upsert_seq 生产微批(32) | 5k | 87479.5 | 57 | 551.05 | 723.87 |
| read_during_write p99 | 2k/5k/10k | - | - | - | 21836 / 18345 / 18295 |

**D1~D8 之后、meta.bin 收紧之前**（中间态，仅用于展示两处收尾的贡献）：

| 场景 | 规模 | total_ms | pts/s | batch p50 | batch p99 |
|---|---|---|---|---|---|
| upsert_seq | 2k | 82.8 | 24144 | 44.05 | 44.05 |
| upsert_conc | 2k | 188.5 | 10608 | 0.44× | - |
| upsert_seq | 5k | 201.1 | 24859 | 39.86 | 42.46 |
| upsert_conc | 5k | 244.1 | 20487 | 0.82× | - |
| upsert_seq | 10k | 480.0 | 20834 | 41.03 | 108.76 |
| upsert_conc | 10k | 587.5 | 17022 | 0.82× | - |
| upsert_seq 生产微批(32) | 5k | 3167.3 | 1579 | 19.93 | 25.65 |

**当前基线**（D1~D8 + 两处收尾；并发段已改为同事务数口径，故并发行与上两表不可直接比）：

| 场景 | 规模 | total_ms | pts/s | batch p50 | batch p99 | 并发口径 |
|---|---|---|---|---|---|---|
| upsert_seq | 2k | 62.8 | 31847 | 33.54 | 33.54 | - |
| upsert_conc | 2k | 55.9 | 35769 | 0.65 ms | 15.40 ms | 1.12×，2/8 任务，2 事务 |
| upsert_seq | 5k | 131.8 | 37935 | 26.94 | 29.50 | - |
| upsert_conc | 5k | 145.5 | 34364 | 0.73 ms | 21.87 ms | 0.91×，5/8 任务，5 事务 |
| upsert_seq | 10k | 348.9 | 28664 | 24.78 | 118.33 | - |
| upsert_conc | 10k | 407.4 | 24546 | 0.71 ms | 21.41 ms | 0.86×，8/8 任务，10 事务 |
| upsert_seq 生产微批(32) | 5k | 1266.8 | 3947 | 7.52 | 19.54 | - |
| read_idle | 2k/5k/10k | 2.51 / 1.56 / 2.28 ms | - | - | - | - |

### 1.3 fsync 计数（`strace -f -c -e trace=fsync,fdatasync,rename,renameat,renameat2`）

全程约 44000 点、194 个 upsert 事务（串行 17 + 并发 17 + 读抖动写 3 + 生产微批 157）。

| 阶段 | fsync | rename | 每点 fsync | 每事务 fsync |
|---|---|---|---|---|
| 修复前 | 推算 88000~132000（每点 2~3 次） | - | 2~3 | - |
| D1~D8 后、meta.bin 收紧前 | 679 | 226 | 0.015 | 3.50 |
| 当前基线 | **275** | **24** | **0.0063** | **1.42** |

剩余 fsync 以「每事务 1 次 WAL group-commit 落盘」为主（194 次，即每事务约 1 次）；其余为小数：单集合创建期的文件初始化（vectors.bin、keys.bin、payloads.bin、tracker.bin 各 1 次 + meta.bin 2 次，共约 6 次/集合 × 7 集合）与 `META_SAVE_INTERVAL=64` 触发的 meta.bin 保存（2 次 × 2 fsync）。合计约 240，与实测 275 的差为各路径初始化常数，未逐项核对到个位数。**可确定的结论是：每点 fsync 已归零，fsync 次数只随事务数缩放、不随批大小缩放。**

### 1.4 由基线推出的两个单位成本（后续方案的推导依据）

- **每事务 fsync ≈ 6~7 ms**：生产微批 32 点批 p50 7.52 ms，其中应用部分约 0.6 ms（32 点 × 约 19 µs/点），其余即 fsync；与分析文档记录的机械盘 5~10 ms/次一致。
- **每点应用 ≈ 19~21 µs**：千点批 p50 24.78~26.94 ms 扣除约 6 ms fsync 后按 1000 点摊算（2k 档仅 2 个样本且含冷启动，不计入）。

### 1.5 基线结论

1. **每点固定成本已消除**：吞吐从「与集合规模无关的 55 pts/s」进入「每事务一次 fsync + 每点约 20 µs 应用」区间；批 p50 从 18 s 级降到毫秒级；读尾延迟从 18 s 级降到 20 ms 级。
2. **并发仍然是负收益，且现在是干净的负收益**：10k 档 8 写入者 0.86×。并发越少越接近 1（5k 五写入者 0.91×，2k 两写入者 1.12×），说明瓶颈不在锁争用开销，而在**写入被完全串行化后没有从并发得到任何东西**。
3. **单写者吞吐由单事务路径决定**：3947 pts/s（32 点批）/~29000 pts/s（1000 点批），与并发无关。后续方案不应指望改善单写者数字，只应改善并发形态与读尾延迟。

---

## 二、问题定义

### W1（P0）WAL group-commit 从未成组

`Wal` 已实现 leader/follower group-commit（`storage/wal.rs:169-231`，共享 buffer + 单次 `write_all` + `sync_all`，`storage/wal.rs:25-41` 的 `GroupCommit` 状态机），但 `apply_txn`（`storage/storage.rs:536-567`）持 `inner.write()` 横跨 `wal.append(txn)`，因此**任意时刻最多只有一个线程能进入 `append`**：`state.pending` 在 leader 行动时恒为 1，leader/follower 分支永不生效。WAL 的全部 append 入口（`apply_txn`、`checkpoint`、`commit_compaction`）都在存储写锁内，该机制在整个引擎内是死代码。

### W2（P0）fsync 与应用同处一个临界区

写锁覆盖「WAL 追加 + fsync」与「全部内存/mmap 应用」两段。后果：
- 6~7 ms 的 fsync 期间，任何并发写入者只能在锁外排队，聚合无从发生（W1 的直接后果）。
- 应用段（每点约 20 µs）也被同锁串行化，无法与其它写入者的 fsync 重叠。
- 于是并发吞吐 ≤ 1/(fsync + apply)，并发任务数增加只增加等待，实测 8 写入者 0.86×。

### W3（P1）一次读要等整笔写完

搜索路径取 `inner.read()`（`storage/search.rs` 的快照构建），而写入持有 `inner.write()` 直到本事务全部落盘。读 p99 在读写并发时为 15~22 ms（与单笔写同量级）。`parking_lot::RwLock` 无公平性保证，写者占先时读尾延迟还会放大。D1~D8 之前该值是 18 s 级，现已收敛到「单笔写时长」，但**读延迟仍然与写批大小成正比**，随并发写入者增多会继续恶化。

---

## 三、方案：提交队列 + 单一应用线程

### 3.1 结构

把 `apply_txn` 的一个临界区拆成四段，只有第一段与第三段持存储写锁，fsync 在锁外：

**第一段 · 预留（短 `inner.write()`）**
- 校验全部 op（维度、有限性、`max_points` 准入）——已在 `wal.append` 之前，保持不动。
- 分配单调递增的 `lsn`（集合内计数，初始化自 `open` 时重放得到的 last txn）。
- 逐个 `Upsert` 解析 slot：id 已存在则沿用；否则分配 `next_slot++` 并预留。
- 在**同一临界区内**把记录字节追加进 group-commit buffer（`Wal::append` 的 `state.buffer.extend`）。这一步同时固定了日志顺序、`lsn` 顺序与 slot 预留顺序，三者在同一把锁内原子确定。
- 释放写锁，入队等待。

**第二段 · 落盘（无存储锁）**
由提交队列自身驱动一次 group-commit flush：可以是「leader 沿用现有协议」，也可以是**单一 flusher 线程**把当前积压一次性写 + fsync。后者天然成组，是 group-commit 真正生效的形状。

**第三段 · 应用（短 `inner.write()`，严格按 lsn 升序）**
- 由一个集合唯一的应用线程执行。
- 只应用「lsn ≤ 已完成 lsn」且尚未应用的连续前缀，按 lsn 升序执行 `apply_records_locked` 的等价逻辑。
- 应用完成后推进 `last_applied_lsn` 并唤醒等待该事务的调用者。

**第四段 · 完成**
调用方被唤醒，返回；引擎的 `after_mutation` 照旧在返回后执行（其读取的 `pending_len`、`needs_compaction` 等此时已反映本事务）。

### 3.2 正确性不变量（方案成立的依据）

1. **日志顺序 ≡ 应用顺序。** slot 分配依赖 `next_slot` 的顺序，而重放按日志顺序重新分配 slot。若两序不一致，重放后同一 id 会落在不同 slot，`reverse` 与 `keys.bin`/向量列的对应关系即被破坏，属数据损坏。第一段把 lsn 分配、slot 预留与日志追加压进同一临界区，正是为了保证这一点；第三段只按 lsn 升序应用，是不变量的另一半。
2. **先落盘后应用。** 应用只发生在「该 lsn 所属 flush 已 fsync 成功」之后；fsync 失败的批次整体不应用，避免出现「已应用但不可恢复」的写入。
3. **返回前已应用。** `apply_txn` 同步阻塞到本事务应用完成，`get`/`search` 的可见性语义与今天完全一致，不需要调用方改造。
4. **应用线程不可重入。** 应用路径只触碰 `Vectors`/`Keys`/`BlobDirectory`/`PayloadStore`/filter bitmap/索引 pending，这些路径都不调用 `apply_txn`；这一点必须以注释固化，并在测试中以断言守住。
5. **图事务无需跨集合原子性。** `LocalVectorEngine::apply_txn`（`engine.rs:796-825`）本就把 ops 按集合分组、按字典序逐个提交，各集合 WAL 独立。每集合一条提交队列即可，形状不变。

### 3.3 失败传播

- **flush 失败**：该批内所有事务以同一错误返回，均不应用。日志尾部可能残留撕裂记录，`replay` 在 CRC 不匹配处停止，语义与今天的「追加失败则整体失败」一致。
- **应用失败**：单条 op 的应用错误（如 IO）按今天的方式向上返回；该事务视为未应用，但其 WAL 记录仍在，重启重放会再次尝试。这是现有语义，不改变。
- **线程 panic**：应用线程 panic 会让等待者永久挂起。方案要求：等待带超时（例如 30 s）并在超时后返回内部错误，同时把队列标记为失效，后续写入直接报错而非静默挂死。集合的其余读取路径不受影响。

### 3.4 锁序与死锁面

固定顺序 `inner → pending → group.state`，无反向取锁点：
- 第一段：`inner.write()` →（其内）`group.state`。
- 第三段：应用线程 `inner.write()` → `pending`。
- 读路径：`inner.read()` → `pending`，不触碰 `group.state`。
- `checkpoint` 与 `compact` 在 `inner.write()`（+ `compact_mutex`/`build_mutex`）内追加 WAL 并 `truncate`，因此必须在进入前**静默提交队列**（见 3.6），否则日志截断会丢掉未应用事务。
- 调用方在等待完成时不持任何锁，因此不会与应用线程成环。

### 3.5 读路径（W3）

短期不改读锁粒度：第三段持 `inner.write()` 只覆盖应用，不再覆盖 fsync，读尾延迟与写批大小的耦合随之从「fsync + apply」降为「apply」。这已把 10k 档的理论读尾从约 7 ms 压到约 20 µs/点量级。

**可选增强（有独立验收门槛，未达标不做）**：应用循环按块让步——每应用若干 slot 释放一次写锁，让排队的读者插空。代价是总应用时间略增、代码多一个让步点；收益是读尾延迟与写批大小解耦。建议在基础版测出读尾仍不可接受后再引入，并在文档中留下对比数据。

### 3.6 必须配套调整的点

| 位置 | 调整 |
|---|---|
| `storage/storage.rs:505` 裸 `upsert` / `:514` 裸 `delete` | 删除。两者只被 `#[cfg(test)]` 使用（生产路径全部走 `apply_ops`/`apply_txn`），且它们绕过提交队列直接持写锁应用，会破坏「日志顺序 ≡ 应用顺序」与 slot 预留的独占性。删除可整类消除该风险 |
| `storage/storage.rs:1002` `checkpoint()` | 进入前等待提交队列排空（所有已提交 lsn 均已应用），再写 `Compact` 标记并截断。当前持写锁即可排除并发应用，但无法排除「已入队未应用」的事务 |
| `storage/compaction.rs:253` `compact()` / `:435` `commit_compaction()` | 同样在开排前静默队列。另注意 `commit_compaction` 内的 `pending.lock().clear()` 与索引失效：静默队列后 pending 已含全部已应用 slot，清空语义不变 |
| `storage/storage.rs:312` `open()` | 顺序为读 meta → 建派生视图 → `replay_wal` → `load_index`，全部不经队列；`lsn` 计数器以重放得到的最高 txn id 初始化。应用线程在首次 `apply_txn` 时惰性创建，`open` 只为多个集合创建轻量队列结构 |
| `storage/storage.rs:580` `apply_ops()` | 保持薄封装（自动 txn id），内部改走提交队列 |
| `engine.rs:912` `after_mutation()` | 不改。`apply_txn` 返回时本事务已应用，`maybe_schedule_*` 读到的状态与今天一致 |
| `index_lifecycle.rs:552` `drain_pending_hnsw()` | 不改。它由维护线程调用，持 `inner.write()` 后与队列的应用串行 |
| `metrics.rs` | 新增 `wal_flushes`、`wal_txns_flushed`（平均成组大小 = 后者/前者）、`apply_queue_depth`、`apply_wait_nanos`、`apply_latency`；把 `checkpoint_space_rejections` 之后的所有新计数一并纳入 `MetricsSnapshot` |

### 3.7 线程归属

按集合一条应用/flusher 线程，惰性创建，随集合 `Drop` 结束。集合数量在部署中是「每项目一个」量级，可接受；若出现单进程数百集合的场景，再改为引擎级共享池并配套跨集合队列（列为非目标，见第八节）。

---

## 四、备选方案与排除理由

| 方案 | 结论 | 理由 |
|---|---|---|
| A. 提交队列 + 单一应用线程（本方案） | 采纳 | 顺序天然正确（单一应用者）；group-commit 由 flusher 真正成组；读锁只与应用段争用 |
| B. 各写入者 fsync 后自行应用 + 「已落盘未应用前缀」抢占 | 排除 | 需维护 per-txn 状态、lsn 游标与条件变量唤醒，且失败传播（同批部分成功、撕裂尾部）路径复杂一个量级；性能收益与 A 相同 |
| C. 只把 fsync 移出锁、应用仍在锁内且由各写入者自己做 | 排除 | 应用顺序无法保证等于日志顺序（写入者完成 fsync 的先后与日志顺序无关），直接违反 3.2 不变量 1 |
| D. 按 slot 范围分片、每片独立锁与 WAL | 排除 | 等于重写存储层；跨片事务与 checkpoint/compaction 语义全部要重做，收益主要在多写者大集合，超出本轮问题范围 |
| E. 仅把 fsync 异步化、调用方不等 fsync（fire-and-forget 持久性） | 排除 | 破坏「先落盘后应用」的不变量 2，崩溃会丢已确认的写入，与引擎的崩溃安全语义冲突 |
| F. 包装层（`cce-storage-vector-local`）侧再聚批/并发 | 已否证 | 生产微批探针显示 fsync 按事务付，包装层只能降事务数；引擎内不修则无解（分析文档第 3.1 节结论仍然成立） |

---

## 五、分阶段实施

| 阶段 | 内容 | 独立可验证 |
|---|---|---|
| S1 | 引入提交队列骨架与 `lsn`；`apply_txn` 第一段预留 + 入队；应用线程按 lsn 顺序应用；落盘暂沿用现有 leader 协议 | T-6 单写者数字不回归 > 5%；全部单测通过 |
| S2 | flusher 改为单一线程批量刷盘；新增 `wal_flushes`/`wal_txns_flushed` 指标并断言成组大小 > 1 | 并发段加速比 ≥ 1.0×；strace fsync 次数随并发写入者增加而显著低于线性 |
| S3 | `checkpoint`/`compact` 入口静默队列；删除裸 `upsert`/`delete`，测试改走 `apply_ops` | 现有 recovery/compaction 测试全通过，新增「队列未排空时触发 checkpoint」用例 |
| S4 | 读路径让步（可选，带门槛） | 读尾 p99 相对 S2 明显下降且总写吞吐不回退 |

---

## 六、回归目标（可判定门槛）

以第 1.2 节「当前基线」为对照，机械盘口径：

| 指标 | 当前基线 | 目标 | 判定 |
|---|---|---|---|
| `upsert_conc` 10k 加速比 | 0.86× | ≥ 1.20× | T-6 重跑 |
| `upsert_conc` 5k 加速比 | 0.91× | ≥ 1.20× | T-6 重跑 |
| `upsert_conc` 2k 加速比 | 1.12×（仅 2 写入者） | ≥ 1.20× | T-6 重跑，或说明 2k 档并发度不足 |
| `upsert_seq` 三档 pts/s | 31847 / 37935 / 28664 | 回退不超过 5% | T-6 重跑 |
| 生产微批(32) pts/s | 3947 | 不低于基线 | T-6 重跑 |
| `read_during_write` p99 | 15.40 / 21.87 / 21.41 ms | 不回退；S4 后应显著低于写批时长 | T-6 重跑 |
| `wal_txns_flushed / wal_flushes` | 无此指标 | > 1（证明成组生效） | 新指标 |
| strace fsync 次数（并发 8 写入者，10k 档） | - | 不随写入者数线性增长 | `strace -f -c` |
| 单测/集成/clippy | 246 + 66 全过，零告警 | 保持 | `cargo test` / `cargo clippy --all-targets --all-features` |

显式**不承诺**绝对数字：单写者吞吐受「每事务一次 fsync + 每点 20 µs 应用」约束，方案只改变并发形态与读尾延迟；应用段本身仍串行（每点约 20 µs），这是单存储单 mmap 的固有下限，要再上一个数量级需要按 slot 分片（方案 D）。

---

## 七、验证方案

1. **正确性优先于性能**：为不变量 1 写针对性测试——并发多写入者提交后 `open` 重放，断言每个 id 的 slot、向量、payload 与崩溃前完全一致（不能只比对 id 集合）。
2. 队列未排空时触发 `checkpoint` 与 `compact`，断言无数据丢失、WAL 截断后重开一致。
3. flush 失败注入（临时把 WAL 目录设为只读或注入 IO 错误），断言批内全部事务报错、无一被应用、集合仍可 `open`。
4. 应用线程 panic 注入下，断言等待者超时返回错误而非永久挂死，且集合读路径仍可用。
5. T-6 重跑三档 + 生产微批探针 + 读写抖动，strace 全程计数，与 1.2/1.3 节基线逐项对照。
6. `cce-storage-vector-local` 集成测试与 orchestrator 层吞吐回归（`index_pipeline_stages` 手动计时口径）。

---

## 八、明确不做

- 不按 slot 分片、不做多 WAL（方案 D）：属于存储层重写，收益场景是「单集合多写者大吞吐」，与当前问题（并发零/负收益、读尾与写批耦合）不在一个量级。
- 不做引擎级共享应用线程池：集合数为「每项目一个」量级时线程数可控；出现数百集合再议，届时应连同跨集合队列一起设计。
- 不把 fsync 改为异步确认（方案 E）：违背崩溃安全语义。
- 不做包装层聚批/并发（分析文档已否证）：引擎内不修，包装层无解。
- 不借本轮改动调整 checkpoint 阈值（`WAL_CHECKPOINT_BYTES`）、`META_SAVE_INTERVAL` 的取值：二者已有验证数据，改动需另起基线对照。
- 不改 HNSW/IVF 索引构建路径：本轮问题全部在存储写入路径。

---

## 九、执行记录

- 环境：`/dev/sdd` 机械盘（`/sys/block/sdd/queue/rotational = 1`）、12 逻辑核、约 14 GB 内存。
- T-6 数据来源：`cargo bench -p cce-storage-vector-local --bench local_vector_store`，结果同时写入 `crates/infra/cce-storage-vector-local/benches/results/local_vector_store.tsv`（该目录原先缺失导致全程无记录，本轮已修）。
- fsync/rename 计数：`strace -f -c -e trace=fsync,fdatasync,rename,renameat,renameat2`，全程约 44000 点 / 194 事务。
- 并发段口径修正：旧形状按点数等分（250~1250 点/批）使事务数放大 1.6×~4×，测的是批大小而非并发；已改为相同 1000 点微批、round-robin 分发，并打印实际任务数与事务数。修正前的 0.87×/1.11×/1.03× 与修正后的 1.12×/0.91×/0.86× 不可直接对比。
- 本轮未修改 `crates/simvec` 任何代码。
