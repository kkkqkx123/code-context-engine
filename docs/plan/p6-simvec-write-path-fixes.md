# T-6 引擎侧写入路径修复方案（simvec）

> 输入：`docs/plan/p6-large-scale-crash-analysis.md`（崩溃分析，D1~D8 缺陷清单）。
> 本轮工作：把分析文档中定位的引擎缺陷（D1~D8）落成代码修改。改动全部位于
> `crates/simvec`（git 子模块，工作区内直接修改，不做提交）。
> 基准侧修复（规模封顶、真实盘 scratch、criterion 移除、生产微批探针）已在分析文档
> 第五节落地，不在本轮范围。

## 一、总体设计

崩溃根因是写入路径把「每点固定成本」放大成了「二次方落盘 + 每点多达 5 次 fsync」。
修复遵循一条原则：

> **WAL 是唯一的权威日志；vectors / keys / payloads.bin / payloads_store（pages +
> bitmask + tracker）全部是可由 WAL 重放重建的派生状态，只在事务边界之外的
> checkpoint 处成批持久化，不再按点同步。**

由此推出四个结构性改动：

1. **put 路径去同步化**：payload 页写去掉 fsync 与整页重映射；指针表（tracker）与
   块位图（bitmask）改为纯内存更新，持久化收敛到 `PayloadStore::checkpoint()`。
2. **目录文件摊还化**：`keys.bin` / `payloads.bin` 的每次追加去掉 fsync，映射按
   几何余量增长，只在超过已映射区间时重映射（append 回到 O(1) 摊还）。
3. **WAL 字节阈值 checkpoint**：新增 `CollectionStore::checkpoint()`，在后台维护线程
   执行——成批 fsync 全部派生文件后写 `Compact` 标记并截断 WAL。持久性边界是一条
   完整的状态点，WAL 字节数有上限，纯插入负载不再无限增长（D5）。
4. **pending 队列改互斥Vec**：`ArcSwap<Vec<u32>>` 整表克隆（O(P²)）改为
   `Mutex<Vec<u32>>`，push 为 O(1) 摊还，读路径仍取快照（D2）。

配套的维护调度修正：guardrail 同步 drain 改为异步维护任务（D4）；repair 在零
tombstone 时早退（D3）；压缩前做目标卷空间预检（D6）；集合新增可选容量上限与内存
估算（D8）。

## 二、逐项细化方案

### D1 + D1b + D7：payload 写入路径（P0）

现状：`PayloadStore::put` 每点依次触发 bitmask 落盘 + fsync（旧块释放、新块分配各
一次）、payload 页写 fsync、整页重映射、tracker 全量序列化 + 临时文件 + fsync +
rename。单点代价 O(N)，机械盘上还有约 4 次 fsync。

改法：

- `payload_store/bitmask.rs`：`allocate` / `free_blocks` 只改内存态；新增
  `persist()`（整表写文件 + fsync）供 checkpoint 调用。
- `payload_store/pages.rs`：新增脏页集合，`write_value` 去掉 `sync_all` 与重映射
  （页创建时建立的映射在 Linux 上与 pwrite 走同一 page cache，读侧天然可见），仅登记
  脏页；新增 `sync()` 只 fsync 脏页。`ensure_page` 去掉建页 fsync（页文件是派生
  状态，重放会重建）。
- `payload_store/tracker.rs`：`save()` 保持原子写（临时文件 + fsync + rename），
  但调用点收敛。
- `payload_store.rs`：`put` 只更新内存 tracker；新增 `checkpoint()`（bitmask
  persist + tracker save + 脏页 sync）；`compact_to` 结束时对新库调用一次
  `checkpoint()`，压缩路径原有的外部 tracker 单独保存随之删除。

正确性依据：WAL 事务在 apply 之前已 append + fsync，且记录含完整 vector 与
payload；任何一次崩溃后 `open` 会先读盘上（可能过期的）tracker/bitmask，再全量重放
WAL，put 幂等，内存态收敛到崩溃前状态。tracker/bitmask 过期只造成「重放时按过期
位图重新分配、极少量块泄漏」，不产生错误数据。

### 目录文件摊还化（与 D1b 同源，keys.bin 每点一次 fsync + 整文件重映射）

- `storage/directory.rs`：`DirView` 记录 `mapped_len`；`append_blob` 去掉每次
  fsync，仅当逻辑末尾超过已映射区间时才 `set_len` 扩出几何余量并重映射；新增
  `sync()`（脏标记 + fsync）与 `open` 对「物理长度 ≥ 逻辑长度」的容忍（余量是未引用
  空间）。`grow_to` / `replace_from` / `create` 同步维护 `mapped_len`。

### D5：WAL 字节阈值 checkpoint

- `storage/wal.rs`：追加字节计数器（打开时按文件长度初始化，append 累加，truncate
  归零），暴露只读查询。
- `storage.rs`：新增常量 `WAL_CHECKPOINT_BYTES`（64 MiB）、`needs_checkpoint()`
  与 `checkpoint()`。checkpoint 全程持 `inner.write()`（与压缩的竞争回退路径同构），
  顺序为：payload store checkpoint → keys/payloads/vectors 增量 fsync →
  `meta.bin` 保存 → WAL 写 `Compact` 标记（该 append 自身 fsync，保证此前所有事务
  已落盘）→ `wal.truncate()`。持写锁期间只阻塞 upsert；读路径只取 `inner.read()`
  快照，不受影响。
- `engine.rs`：`after_mutation` 增加 checkpoint 调度（去重键 `checkpoint:{name}`），
  维护循环新增 `MaintenanceJob::Checkpoint`；语义与压缩调度并列，互斥触发。

### D2：pending 队列整表克隆

- `storage.rs`：字段改 `Mutex<Vec<u32>>`；`index_lifecycle.rs` 的全部读写点
  （load_ivf/load_hnsw、register_slot、build 收养、drop_index、drain_pending_hnsw、
  pending_len）、`compaction.rs` 的清空、`search.rs` 的快照改用该互斥体。锁序固定
  为 inner → pending（register_slot 持 inner.write 取 pending；search 持 inner.read
  取 pending），无死锁面。

### D3：repair 全图扫描

- `index/hnsw.rs`：`repair()` 开头以 `tombstones.count_ones() == 0` 早退（分块
  位图 popcount 成本可忽略），零 tombstone 时不再逐层拿写锁。

### D4：HNSW 增量维护退化为全量重建 + 同步 drain

- `engine.rs`：删除 `after_mutation` 里的同步 drain guardrail（65536 点同步入图，
  单批被拖到数秒）。改为 pending 达到 `PENDING_DRAIN_ENQUEUE`（4096）时投递
  `MaintenanceJob::Drain`（去重键 `drain:{name}`），由维护线程执行；tick 分支原有
  的 drain 保留。
- `sweep_hnsw` / `load_hnsw`：pending 不再无条件触发重建。仅当 pending 规模达到
  图中节点数的一半（`HNSW_PENDING_REBUILD_RATIO`）时才 schedule_build；否则留给
  drain 增量入图。drain 的每点成本与重建的单点插入相同，增量路径才是正解。

### D6：压缩双倍落盘 + 持写锁全程

- `compaction.rs`：写临时文件前用 `statvfs` 预检目标卷剩余空间，需求量为
  vectors(+quant) + keys + payloads_store 临时文件之和；不足则返回带明确信息的错误
  并记指标，不再把 ENOSPC 留在半写状态。竞争回退路径维持「重写在锁外、提交在锁内、
  有限次重试后整体持锁」的现有协议（该路径只在持续写压力下触发，且有界）。

### D8：规模 / 内存准入

- `types.rs`：`CollectionConfig` 新增可选 `max_points`（builder + `#[serde(default)]`）；
  `Meta` 新增同名字段并持久化（`FORMAT_VERSION` 2 → 3，同时移除 v1 兼容回退——
  项目预发布期不留旧格式路径）。
- `storage.rs`：`ensure_capacity` 在容量增长前校验 `max_points`，超限返回
  `CollectionCapacityExceeded`（新增错误变体，带集合名、上限与请求点数）。
- `storage.rs`：新增 `memory_estimate()`，按 vectors（容量 × 维 × 4）+ payload 指针表
  （next_slot × 12）+ payload 数据页 + 目录文件 + HNSW 图（节点 × 每节点边上限）+ WAL
  字节数求和；`engine.rs` 的 `collection_info` 透出 `memory_bytes`，调用方可在写入前
  预判集合规模。

## 三、回归测试与验证

单元 / 集成测试（`crates/simvec`）：

1. payload store：连续 N 次 `put` 后，盘上 `tracker.bin` 仍保持旧状态（无逐点全量
   重写），调用 `checkpoint()` 后才反映全部 N 条；覆盖 D1 的二次方回归。
2. payload store：`put` 后 `checkpoint()`，重开库数据完整；脏页 / bitmask 在
   checkpoint 后落盘。覆盖 D1b 持久性。
3. recovery：checkpoint 截断 WAL 后崩溃重开，数据与未截断路径一致；checkpoint 中途
   崩溃（未截断）时 WAL 全量重放可恢复。覆盖 D5。
4. HNSW：大量 upsert 后 `drain_pending` 入图，pending 归零且搜索可见；零 tombstone
   时 `repair` 早退（通过行为断言不改动图）。覆盖 D2/D3/D4。
5. `max_points` 超限写入报错且不改变已有数据。覆盖 D8。

手动验证（机械盘口径）：重跑 T-6 基准 2k/5k/10k 三档，预期每点成本与批大小无关地
下降（fsync 归零后只受 WAL group-commit 限制），批 p50 从 18 s 级降到毫秒级，读
尾延迟回到毫秒级；`strace -c -e trace=fsync,rename` 确认每点 fsync 次数归零。

## 四、执行记录与后续

本轮实际验证结果：串行吞吐 2k/5k/10k 分别 37222/38440/28985 pts/s（对照修复前
55/51/56），批 p50 从 18 s 级降到 24.8~33.5 ms，读尾 p99 从 18 s 级降到
15.4~21.9 ms，`strace` 每点 fsync 归零（每事务约 1.4 次）。验证同时暴露出两处
收尾（`max_points` 前移、meta.bin 按容量保存）与 bench 的 TSV 落盘缺陷，均已修复。

**仍未解决**：并发写零收益（10k 档 8 写入者 0.86×）与 WAL group-commit 从未成组。
设计方案、完整基线与可判定门槛见
`docs/plan/p6-simvec-concurrency-and-wal-design.md`。
