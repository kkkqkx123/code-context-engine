# 关系索引常驻内存收敛：下一节点任务设计

## 目标与边界

本节点继续收敛三类随项目规模增长的内存占用，不引入严格 RSS 硬上限，也不改变解析与关系推导算法：

- 扫描阶段的文件清单不再完整保存在 `FileIndexer.sorted_files`。
- 全量构建中的项目级全局名称与命名空间索引写入操作本地 SQLite，按需回源。
- 全量构建期间释放进程内持有的上一代查询快照，查询由磁盘分片快照继续服务。

内存路径仍保留原有 DashMap 行为，磁盘路径只在 `BuildSymbolView` 已安装时启用。两种路径共享同一份解析逻辑，增量构建和热更新不受影响。

---

## 任务一：扫描清单磁盘化

### 现状

`crates/app/cce-orchestrator/src/index/file_indexer.rs` 中 `FileIndexer` 持有 `Vec<FileEntry>`，并由 `get_batch()` 返回切片。虽然 `FSScanner` 已有 `scan_streaming_report()`，初始化与恢复仍调用 `scan_report()`，先把全部条目收集到内存，再排序、算哈希并分批。

调用方包括：

- `orchestrator/batch.rs`：逐批取 `FileEntry`。
- `orchestrator/checkpoint.rs`：校验批次首尾文件边界。
- `orchestrator.rs`：持久化文件记录和发布文件哈希时遍历全部文件。
- `orchestrator/finalize.rs`：激活 manifest 时发布文件哈希。

### 存储模型

新增操作本地 SQLite 清单，路径放在项目 `.cce/scan-index-{operation_id}`，由 `FileIndexer` 持有并在 drop 时清理。表结构只需要：

- `ordinal`：扫描序号，主键；
- `path`：绝对路径，唯一索引；
- `entry_json`：`FileEntry` 的 serde_json 序列化结果。

不再保存完整的 `Vec<FileEntry>`。排序交给 SQL 的 `ORDER BY path`，`ordinal` 写入时按排序结果重新分配，保证 `get_batch` 的区间查询和当前逻辑一致。

### 扫描流程

把 `initialize()` 和 `recover()` 从 `scan_report()` 切换为 `scan_streaming_report()`。回调收到一批 `FileEntry` 后直接写入清单表，批大小沿用 `batch.scan_batch_size`，因此任何时刻驻留的条目数只与单批有关。

扫描完成后按 `path` 顺序流式读取清单，更新 `DefaultHasher`，计算与现有 `compute_file_list_hash()` 完全相同的 path + content_hash 指纹。checkpoint 的总文件数、batch size、root 和哈希继续使用原字段，不扩展协议。

`recover()` 仍重新扫描并与已有 checkpoint 比较；差异只体现在清单来源从内存数组换成 SQLite 查询，恢复语义不变。

### 接口调整

`FileIndexer` 改为持有 `Arc<ScanFileStore>`，保留 `Clone` 语义。公开接口调整如下：

- `get_batch(index)` 返回 `Vec<FileEntry>`，由 SQL 区间查询装配，不再返回切片。
- `total_batches()` 通过清单行数计算。
- `validate_batch_boundary()` 只查询目标批次的首尾两行，不加载整批。
- `files()` 删除，改为 `for_each_file(visitor)` 和 `for_each_batch(batch_size, visitor)`。
- `file_list_hash()`、`scan_failures()` 和 checkpoint 访问器保持不变。

调用方同步调整：

- 批处理循环接收 owned `Vec<FileEntry>`，下游处理函数继续按 `&[FileEntry]` 使用。
- `ensure_file_records()` 和 `publish_file_hashes()` 改为分块消费；每块大小受配置控制，不能重新收集全量切片。
- 测试中的构造辅助函数改为先写入临时清单表。

### 失败与清理

扫描失败、清单写入失败、排序读取失败都使 `initialize()` 或 `recover()` 返回错误，不进入批处理。`ScanFileStore` 在 `Drop` 中删除目录；新一轮全量扫描先清理同路径的遗留目录。

### 验证

- 同一 fixture 下文件列表哈希、批次数量、每批首尾路径与修改前一致。
- checkpoint 恢复和边界回退路径通过既有测试。
- 新增一个大批次测试，确认峰值驻留条目不超过单批。
- medium 复测关注 `rss_after_scan_kb`，要求不高于当前值。

---

## 任务二：ProjectSymbolTable 全局索引磁盘化

### 现状

`ProjectSymbolTable` 的 `global_index` 与 `namespace_index` 随符号数线性增长，且 `capacity_counts()` 只是粗估。全量构建已安装 `BuildSymbolView`，但这两个索引仍在进程内 DashMap 中。

主要读写点：

- 写入：`insert_symbol()`、`insert_symbols_bulk_with_arity()`、包注册与移除。
- 读取：`get_by_qualified_name()`、`resolve_global_qualified()`、`resolve_in_namespace()`、限定名解析的 joined key 存在性检查。
- 重建：`rebuild_global_indices()`、`rebuild_indices()`。
- 清理：`clear()` 和文件级 prune。

`resolve_in_namespace()` 中还有一次 `global_index.iter()` 扫描，仅用于验证已知 entity id；磁盘路径应删除这次冗余扫描，改为直接查询 metadata。

### 存储模型

在 `BuildRelationStore` 现有 SQLite 数据库中新增：

- `global_symbols(key TEXT PRIMARY KEY, entity_id INTEGER NOT NULL)`；
- `namespace_symbols(key TEXT PRIMARY KEY, entity_id INTEGER NOT NULL)`。

key 的拼装规则完全复用当前字符串格式，避免解析算法分叉。两张表都建立主键索引，查询只按 key 精确命中，不做前缀扫描。

### 读写路由

在 `ProjectSymbolTable` 增加统一内部访问器，按 `build_view` 是否存在分流：

- `global_symbol(key)`：磁盘模式查询视图，内存模式读 `global_index`。
- `insert_global_symbol(key, entity_id)`：磁盘模式写入 SQLite，内存模式写 DashMap。
- `namespace_symbol(key)`、`insert_namespace_symbol(...)` 同理。
- 删除操作在磁盘模式执行 SQL delete，在内存模式保留原逻辑。

视图内部为两类查询各维护一个小型 LRU，容量由 `symbol_resolution.global_index_workset_entries` 控制。未命中回源 SQLite；查询结果可缓存，但删除必须同步失效对应 key。

### 调用点收敛

逐个替换所有直接访问 `global_index` / `namespace_index` 的位置，禁止在磁盘模式下继续构造这两个 DashMap。

- `get_by_qualified_name()` 与 `resolve_global_qualified()` 先走统一访问器，再保留原有层级回退。
- `qualified_resolution.rs` 的 joined key 检查改为精确查询。
- `registry.rs` 的包注册/移除改为逐 key 写入或删除。
- `rebuild_global_indices()` 在磁盘模式先清空两张表，再从 `simple_name_index` 对应来源流式写回；若来源本身已磁盘化，则直接按持久表重建，不再回灌内存。
- `clear()` 在磁盘模式只清缓存和工作集，不删除关系构建库中的符号行；全量构建库是 round-local，生成结束时整体丢弃。

### 指标

`SymbolViewMetrics` 增加：

- `global_index_queries`、`global_index_hits`；
- `namespace_index_queries`、`namespace_index_hits`；
- `global_index_entries`、`namespace_index_entries`，由 SQL count 获取。

这些指标进入 operation manifest 与 perf TSV。查询次数显著增长时可增大工作集，但不能通过静默保留全表解决。

### 验证

- 现有符号解析单测在内存路径全部通过。
- 新增磁盘路径对照测试：同一组包、模块、限定名在同一次构建中查询结果一致。
- 构建 artifact 的双指纹、实体计数、关系计数与修改前一致。
- medium 复测关注 `symbol_table_entries` 估算值下降，以及关系阶段 RSS 增量不回退。

---

## 任务三：旧快照释放

### 现状

`IndexOrchestrator.last_published_snapshot` 在发布成功后持有最新快照。全量构建期间该快照一直驻留；新快照发布时才被替换，因此峰值可能同时包含新旧两代。

查询服务优先使用 orchestrator 的内存快照，未命中时回退到 runtime 中基于 SQLite 的分片快照。runtime 已支持 `RelationSnapshotIndex::from_store()`，因此磁盘查询路径具备兜底能力。

### 生命周期

全量构建开始时，在获取操作锁并确认将重建关系 generation 后：

- 释放 orchestrator 持有的 `last_published_snapshot`；
- 清除 exporter 的 relation enhancement 快照引用；
- 确认 runtime 已指向当前 active epoch 的磁盘分片快照；若没有，先按 active epoch 构造。

构建期间不保留任何完整内存快照。在线请求持有的 Arc 不受影响，可继续完成；新的查询走 runtime 分片快照。发布成功后：

- 将新快照写入 `last_published_snapshot`；
- 分享给 exporter；
- runtime 切换到新 epoch。

发布失败时保持无内存快照状态，runtime 继续服务上一个有效 generation。

### 查询语义

在快照重建窗口内，查询接口不能返回空图或伪造成空结果。服务层需要明确区分：

- active generation 存在：从磁盘分片正常返回；
- 不存在 active generation：返回明确的“关系索引不可用”错误；
- 构建成功但尚未激活：继续读旧 generation，直到 manifest 激活完成。

`IndexCapabilities` 暴露 `relation_rebuilding` 状态，供 CLI、MCP 和 HTTP 层展示，不改变既有成功响应的结构。

### exporter 处理

NL 文档导出发生在关系发布之后，因此只需要新快照。构建开始前清除旧增强引用，避免 exporter 持有上一代图。导出阶段使用发布成功后的新快照；若发布失败，整个全量操作按现有错误语义失败，不做降级导出。

### 验证

- 单元测试覆盖快照开始为空、发布成功、发布失败三条状态转换。
- 查询测试覆盖重建窗口内 disk-backed 查询返回旧 generation 数据。
- exporter 测试确认不会在重建期间使用旧关系增强结果。
- medium 复测确认 `rss_after_batches_kb` 与峰值 RSS 不因旧快照叠加而上升。

---

## 实施顺序与整体验收

建议按扫描清单、全局索引、旧快照释放的顺序实施。前两项减少常驻对象，第三项处理峰值叠加；三者互不依赖新的文件格式或跨代迁移。

整体验收：

- workspace check、build、lib test、clippy 全部通过。
- medium 实体、向量、关系计数和双指纹不变，候选裁剪为零。
- 查询矩阵 cold/warm p95 无明显回退，错误数为零。
- 扫描哈希、批次边界和 checkpoint 恢复结果与当前实现一致。
- large 档只做最终冒烟：峰值不回到数 GiB，关系计数比例与 medium 一致。
