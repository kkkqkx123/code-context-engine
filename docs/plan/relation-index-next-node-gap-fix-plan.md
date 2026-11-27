# 关系索引常驻内存收敛：后续缺口与修补方案

## 结论

当前三项主体改动已落地：扫描清单磁盘化、项目级全局/命名空间索引磁盘化、全量构建期间旧快照释放。devs 上的 workspace check、CLI/server build、全 workspace clippy 和 lib test 均通过。

按 `relation-index-next-node-resident-design.md` 严格验收，仍有边界行为、展示链路和性能复测缺口，暂不判定为全部完成。本文记录问题清单与建议修补顺序。

---

## 已完成范围

- 扫描清单由操作本地 SQLite 承载，扫描按批写入，批次区间查询、边界查询和按路径流式遍历已切换。
- 文件记录与文件哈希发布改为按配置批大小逐块消费。
- 全局与命名空间符号索引写入 `BuildRelationStore`，由 `BuildSymbolView` 的小型 LRU 回源，统一访问器已覆盖主要读写点。
- 新增符号视图指标并接入 stage 结果、API DTO、perf TSV 和 OpenAPI。
- 全量关系重建开始时释放 orchestrator 旧快照和 exporter 增强引用；publisher 为 active epoch 准备磁盘分片快照并将 runtime 置为更新态。
- `IndexCapabilities` 与 runtime 能力信息暴露 `relation_rebuilding`。

---

## 问题清单

### 高优先级

#### 1. 重建失败时状态未完整复位

`prepare_full_rebuild()` 会把 runtime 置为 `Updating`，orchestrator 也会置 rebuilding 标志。但 `build_and_publish_relations()` 在关系库创建、构建配置缺失、符号注册、spool replay 等早期错误路径上直接返回，未通知 runtime 失败，也未复位 orchestrator 标志。engine 的硬失败路径只清理操作队列，不复位关系状态。

影响：查询侧可能长期显示“重建中”，或在无可用 generation 时持续处于非明确不可用状态。

涉及位置：`finalize.rs`、`relation_publisher.rs`、`engine/ops.rs`。

#### 2. 性能与基准验收未执行

尚未运行 medium/large 复测，缺少以下验收数据：`rss_after_scan_kb`、`rss_after_batches_kb`、峰值 RSS、`symbol_table_entries` 估算值、实体/向量/关系计数、双指纹、候选裁剪数、cold/warm p95、large 档峰值与关系计数比例。

影响：无法确认内存收敛目标达成，也无法确认无性能回退。

#### 3. 基线等价性缺少显式对照

现有测试验证了新实现的内部一致性，但没有针对同一 fixture 与旧实现的文件列表哈希、批次数量、每批首尾路径、artifact 双指纹和关系计数做显式对照。

影响：无法证明算法结果完全不变。

---

### 中优先级

#### 4. `relation_rebuilding` 展示链路不完整

`IndexCapabilities` 和 runtime 能力 JSON 已带该字段，HTTP tools capability 可见；但 MCP health 仍只输出 `rebuild_required`，CLI health DTO 与展示层没有该字段。

影响：设计要求的 CLI、MCP、HTTP 三端展示未全部落实。

#### 5. `clear()` 语义未完全覆盖

设计文档提到 `clear()` 在磁盘模式只清缓存和工作集。代码中没有独立的 `ProjectSymbolTable::clear()`；磁盘分支只加在 `rebuild_indices()`。`clear_cache()` 仍不会清理 `BuildSymbolView` 的两组 qualified LRU。

影响：未来若从其他清理入口进入，磁盘缓存可能残留。

#### 6. 磁盘索引重建路径存在潜在偏差

磁盘模式下 `rebuild_global_indices()` 使用 `module_path + simple_name` 重建 namespace key，而不是复用插入时的原始 qualified name；“来源本身已磁盘化则直接按持久表重建”的分支没有实现，也没有对应测试。

影响：当前全量构建路径较少触发该分支，但语义与文档不完全一致，后续启用增量清理时可能解析到不同 key。

#### 7. `symbol_table_entries` 指标口径与预期不一致

`capacity_counts()` 在磁盘模式将 SQL 行数计入该值，因此该指标不一定体现“常驻估算下降”，更接近“索引总条目数”。

影响：medium 复测若以该值判断收敛效果，可能得到与预期相反的结论。

#### 8. 发布成功/失败三态测试不够真实

orchestrator 测试通过直接改字段模拟，未通过真实 `publish()` / `publish()` 失败路径验证成功写入新快照、失败保持无内存快照且 runtime 继续服务旧 generation。

影响：状态转换回归保护不足。

---

### 低优先级

#### 9. 扫描清单错误被部分吞掉

`FileIndexer::total_batches()` 和 `file_count()` 在 SQL count 失败时返回 0，未向上传播；缺少清单写入失败、排序失败、读取失败的专门测试。

影响：异常场景下可能静默进入空批次循环。

#### 10. 大批次测试规模偏小

新增测试为 8 批乘 16 条，共 128 条，验证的是批次向量长度，不是真实 RSS 或更大批量下的驻留上限。

影响：对“大批次”场景的覆盖偏弱。

#### 11. `prepare_full_rebuild()` 失败处理偏弱

active epoch 的磁盘快照构造失败时只记录日志并继续执行，runtime 快照被失效，查询期返回不可用。文档要求“不返回空图或伪空结果”，当前行为是显式不可用，但是否应中止全量操作尚未明确落实。

---

## 修改方案

### 1. 建立统一的重建失败收口

在 orchestrator 中增加一条所有失败路径必经的收口逻辑，覆盖 `build_and_publish_relations()` 的正常结束、显式错误和意外错误。收口时：

- 清除 orchestrator 的 rebuilding 状态；
- 保持 `last_published_snapshot` 为空；
- 通过 publisher 通知 runtime 重建失败，使状态从 `Updating` 转入 `Degraded` 或明确不可用，同时保留旧 generation 供查询；
- 不清理 exporter 的新快照引用，因为失败路径不会导出。

engine 的硬失败路径也应调用同一关系失败通知入口，避免状态悬挂。

### 2. 补齐展示链路

- MCP health 输出中增加 `relation_rebuilding`；
- CLI health 响应模型增加该字段并在详细模式打印；
- HTTP 侧确认 health/status 与 tools capability 的字段一致；
- 保持既有成功响应结构不变，只增加诊断字段。

### 3. 明确清理入口

- 为 `ProjectSymbolTable` 增加明确的 `clear()` 入口，磁盘模式只清 qualified LRU 和解析缓存，不删除 round-local 构建库中的符号行；
- `clear_cache()` 也应清理 `BuildSymbolView` 的 qualified LRU，避免两个清理入口行为分叉；
- `rebuild_indices()` 继续复用统一入口。

### 4. 修正磁盘索引重建语义

- namespace key 的重建优先复用已持久化的原始 key，不再由 `module_path + simple_name` 重新推导；
- 若权威来源已完全磁盘化，直接按持久表重建，不把全量 key 回灌内存；
- 增加磁盘模式下“插入、清理、重建、再查询”的对照测试。

### 5. 修正指标口径并复测

- 区分“常驻估算条目数”和“持久化总条目数”；
- `symbol_table_entries` 继续反映常驻估算，新增或保留持久总量字段供诊断；
- medium 复测同时记录 RSS、候选裁剪、双指纹、实体/向量/关系计数和查询 p95；
- large 档只做冒烟，关注峰值 RSS 和关系计数比例。

### 6. 加强错误与等价性测试

- 为扫描清单写入失败、排序失败、读取失败增加单元或集成测试；
- `total_batches()` 与 `file_count()` 出错时向上返回错误，不静默归零；
- 新增一组 fixture 基线断言，固定文件列表哈希、批次数量、首尾路径和 artifact 双指纹；
- 用真实 publisher 覆盖发布成功与发布失败三态；
- 大批次测试扩大到能代表 medium 档的量级，并结合 RSS 采样。

---

## 建议实施顺序

1. 统一重建失败收口和状态复位。
2. 补齐 CLI/MCP 展示链路。
3. 明确清理入口并修正磁盘索引重建语义。
4. 修正指标口径，增加错误路径与基线等价性测试。
5. 在 devs 上执行 medium 复测和 large 冒烟。

---

## 验收标准

- workspace check、build、clippy、lib test 全部通过。
- 关系重建在成功、失败、早期错误三类路径下状态明确，无长期 `Updating` 悬挂。
- CLI、MCP、HTTP 均可观察到 `relation_rebuilding`。
- medium 实体、向量、关系计数和双指纹不变，候选裁剪为零，查询 p95 无明显回退。
- 扫描哈希、批次边界和 checkpoint 恢复结果与基线一致。
- large 档峰值不回到数 GiB，关系计数比例与 medium 一致。
