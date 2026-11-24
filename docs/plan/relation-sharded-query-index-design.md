# 关系查询索引分片化重构方案

## 目标

关闭 `docs/plan/relation-index-memory-analysis.md` 的剩余主项：让查询侧关系索引不再要求
“整个项目的实体与关系全部驻留内存”。发布完成后，运行时只保留一个体积固定的分片目录与
有界的已装载分片；构建期 `RelationIndex` 在 publication 返回后即可释放，冷启动同样不装载全图。

验收口径沿用分析文档：50,000 文件档峰值 RSS 相对基线至少下降 50%，或给出可实测的查询期内存硬上界；
快照 fingerprint、canonical 内容、关系数量、查询结果与热更新融合结果保持不变；冷查询 p95 不出现数量级回归。

## 现状约束

- `RelationSnapshotIndex` 当前把构建期 `DashMap` 零拷贝交给运行时，查询方法直接读这些 map，
  因此构建图的生命周期被运行时延长。`LayeredSnapshotIndex` 的 delta 叠加、导出增强、热更新基础缓存、
  API 查询都依赖这一具体类型。
- `RelationSnapshotStore` 只有“读整份 canonical 快照”的端口，冷加载路径会把全部行装回 `RelationIndex`。
- manifest 已有 `Building / Ready / Active / Delta` 状态机与 CAS 发布流程，规范快照写入已改为
  磁盘工件 + 分块写库（见 `docs/plan/relation-index-streaming-write-design.md`）。
- `SnapshotDelta` 语义、manifest 状态机与指纹计算路径不允许改变。

因此不能直接删除 `RelationSnapshotIndex` 类型，而要把它改造成“查询外观”，在内部区分
构建期内存实现与发布期分片实现，并让所有调用方继续使用同一个公开类型。

## 总体架构

```text
构建期 RelationIndex（唯一全图，发布后释放）
    -> CanonicalSnapshotArtifact（已实现）
    -> SQLite/PG 组件表（已实现）
    -> 分片目录 + 分片行（本阶段新增）
    -> ShardedSnapshotIndex（有界 LRU，按时装载）
    -> RelationSnapshotIndex 外观
    -> LayeredSnapshotIndex（delta 叠加不变）
    -> 运行时 / 导出增强 / API
```

分片定义只依赖规范化的文件路径：对路径做稳定哈希得到固定数量的逻辑槽位，再按显式
`max_shard_items` 把槽位内的实体与边拆成有界分片。分片元数据写入已激活 epoch 的 manifest 关联表，
不新增快照版本，也不改变 canonical 内容；旧 epoch 随 manifest GC 一起回收。

## 持久层改造

### 分片目录表

新增 `relation_snapshot_shards` 表，按 `(project_id, relation_epoch, shard_id)` 唯一标识一行，
记录该分片包含的文件数、实体数、关系数、依赖数、估算字节数和稳定路径前缀范围。
分片在 `Building` epoch 写入，`Ready` 时全部就绪，由 manifest 的原子激活统一对外可见；
任一分片写失败都走既有 `Failed` 标记流程，不发布半个 epoch。

### 查询端口扩展

在 `RelationSnapshotStore` 上增加面向分片的只读方法，全部带 `project_id + relation_epoch`：

- 读取分片目录与分片组件行：按 `shard_id` 返回该分片的 files / entities / relations / dependencies。
- 实体定位：`entity_id -> shard_id + file_path`，`symbol_key -> entity_id`，`stable_id -> entity_id`，
  以及实体所在文件的反向查询。
- 反向边定位：`callee_entity_id -> caller shard_id 列表`，用于跨分片调用者查询；关系表已有
  target 索引，可直接查询，不维护额外的全量反向表。
- 聚合查询：文件数、实体数、关系数、分类计数等，直接在存储层做 count/group by，结果大小与输出同阶。
- 文件依赖：按路径读取直接依赖/被依赖边，供传递依赖查询在 SQL 侧逐跳完成。

为支持稳定 ID 的点查，在实体表增加持久化的 `stable_id` 列并建立索引。该字段由既有
`StableSymbolKey::stable_id()` 计算，属于派生列；开发阶段不做兼容迁移，旧数据通过全量重建补齐。
其余唯一索引沿用现有 schema，不引入版本化表。

### 分片大小上界

`max_shard_items` 是显式容量参数，不随项目规模自动放大。构建分片目录时以实体与边的数量为预算：
一个槽位超出预算就追加新的分片编号。由此得到可证明的驻留上界：

```text
峰值关系内存 <= LRU 分片数上限 × 单分片预算 + 固定目录 + delta 链
```

目录只保存 ID 映射与计数，不保存实体内容；它的规模是线性但轻量的，与当前符号表同量级，纳入容量报表。

## 查询侧重构

### 类型分层

把现有结构拆成两层，公开名称保持不变：

- `InMemoryRelationSnapshotIndex`：现有共享 `DashMap` 的实现，只用于构建期校验、`to_canonical_snapshot`
  验证路径和单元测试；不再作为发布后的运行时后备。
- `RelationSnapshotIndex`：查询外观，内部为“内存实现”或“分片实现”的确定性枚举，不允许动态分发。
  对外保留现有构造入口、指纹入口与所有 `Snapshot*QueryOps` 实现；新增从 store + manifest 构造分片实现的入口。

外观上的每个查询方法按 backing 分发：内存实现保持原逻辑；分片实现先查目录，再把请求路由到
一个或多个分片。分片命中后通过既有 `RelationSnapshotLoader::load_canonical` 的组件构造能力组装临时
`RelationIndex`，包装成只读分片视图并放入 LRU。分片是只读的，淘汰后可由持久层重新装载。

### 查询路由

- 按实体 ID、实体名称、符号键、文件路径发起的点查：目录定位单个分片，只读入该分片。
- 调用链查询：caller 定位到分片；caller 的反向边若落在其他分片，通过 `callee -> shard` 目录
  取得调用者分片集合，逐片装载并合并结果。合并语义与当前全内存反向索引一致。
- 文件级边、imports、exports、依赖：文件到分片的映射唯一定位，单分片查询。
- 全局统计与 `stable_symbol_keys()` 这类全量输出：直接在存储层流式聚合或分批读取，结果按 API 返回，
  不作为分片常驻；`stable_symbol_keys_in_files()` 只按请求文件集合定位分片。
- `function_count / resolved_relation_count / file_count`：从分片目录的计数读取，叠加 delta 修正。

LRU 采用分片数量与估算字节数双上限，命中计数与淘汰计数纳入容量指标。分片构造失败必须向上返回错误，
不允许静默返回空结果；这与“不用截断关系换内存”的约束一致。

### 外观与增量层的接口清理

`LayeredSnapshotIndex` 及 `snapshot_view` 中少量直接访问 base 内部 map 的代码改为调用外观提供的方法：
按名称回退查找实体、按文件取依赖、按文件集取符号键等。外观为这些操作提供分片感知实现，
不再暴露具体 map 字段。`materialize_merged_index()` 仅保留为测试/验证工具，显式装载全部分片，
不出现在请求路径。

## 发布与热更新

- 完整发布：artifact 写库完成后，publisher 用已激活 epoch 从 store 构造分片外观，替换运行时 base；
  随后释放 builder 与 artifact。export enhancer 使用同一个外观实例，避免第二份索引。
- 冷启动：`RelationRuntime` 与 `RelationBaseCache` 不再调用全量 `RelationSnapshotLoader::load`，
  改为按 active epoch 构造分片外观；delta 链仍通过现有 store 端口读取。
- 热更新：候选构建继续从 `RelationIndexView` 读取跨文件上下文；外观实现该 trait 时，按请求文件集合
  装载分片并按 delta 叠加，保证 prepopulation、依赖传播与指纹范围只覆盖受影响文件。
- delta 叠加：`LayeredSnapshotIndex` 的查询融合逻辑不变，base 改为外观后，remove/add/重命名语义由
  现有 delta 链逐条应用；全局计数按链修正。分片目录不含 delta，delta 只影响已装载视图。
- compaction：合并后的快照重新生成分片目录并原子切换；compaction 期间旧 epoch 继续服务，完成后
  清理失效缓存条目并 GC 旧 manifest。
- CAS 与 activation：沿用现有流程。CAS 失败时不构造分片、不替换运行时；写入失败仍标记 `Failed`。

## 指纹与一致性

- 快照 fingerprint 与 input fingerprint 继续由 artifact 的外部排序路径产生，分片化不参与计算。
- 文件范围指纹（`fingerprint_in_files`）改为“按文件集合确定分片 -> 从分片组件构造规范组件 ->
  复用 `fingerprint_from_components`”，与全量快照在同文件集合上逐字节一致。
- delta 链指纹继续由 merged view 的 canonical 输出计算；新增分片实现后必须复用同一排序与哈希入口。
- 分片目录只做定位与计数，不缓存任何参与 fingerprint 的可变字段，避免指纹漂移。

## 配置与容量

新增显式参数，不放进“按项目大小自动切换”的双路径：

- `max_shard_items`：单分片实体/边预算。
- `shard_lru_shards` 与 `shard_lru_bytes`：同时驻留的分片数与字节上限。
- `shard_query_timeout`：单次查询最多装载的分片数；超限时返回明确错误，提示先做 compaction 或
  调整预算，绝不通过丢弃边来降低内存。

容量报表新增：分片数、目录条目数、目录估算字节、当前已装载分片数、LRU 命中/淘汰数、
单次查询最大装载分片数、发布后运行时索引估算字节。与现有 `rss_after_relation_kb` 与
canonical 写入指标同表输出。

## 验证计划

1. **等价性**：构造含跨文件调用、文件级边、外部调用、依赖与 exports 的夹具，逐一对比
   `Snapshot*QueryOps` 与 `RelationIndexView` 在全内存实现和分片实现上的返回值；
   比较 canonical 输出、fingerprint、关系数量与查询顺序。
2. **有界性**：用小预算强制多次淘汰，断言常驻分片数不超过配置；用大项目夹具检查目录与分片行数；
   输出可复算的内存上界公式并在阶段表中核对。
3. **冷/热路径**：冷启动加载 active epoch + delta 链；热更新后叠加 delta；触发 compaction 后重新分片；
   三种状态的查询结果与 fingerprint 一致。
4. **CAS 与恢复**：保留现有 epoch 冲突、失败标记、manifest 恢复与 GC 用例；新增“分片写入中途失败”
   用例，断言不激活、不替换运行时。
5. **性能**：medium 档记录冷查询首字节延迟与 p95；50,000 档只跑必要查询集，确认无数量级回归。
6. **全量回归**：`cargo clippy --all-targets --all-features`、`cargo fmt`、`cargo test --lib`、
   publisher/base cache/export/e2e 相关用例全部通过；不启用任何兼容层或旧路径开关。

## 执行顺序

1. 扩展 `RelationSnapshotStore` 与 SQLite/PG 的分片读取端口，先补存储层测试。
2. 实现分片目录构建与写入，接入 artifact 发布后的写库阶段；验证 fingerprint 不变。
3. 拆分 `RelationSnapshotIndex` 为内存实现 + 外观，先让外观在内存 backing 下保持现有测试全绿。
4. 实现分片 backing 的 LRU、查询路由与 `RelationIndexView`；补齐等价性测试。
5. 切换 runtime、base cache、publisher、export enhancer 到分片外观，删除冷启动全量装载路径。
6. 接入容量指标，按 medium -> 50,000 档复测 RSS 与查询延迟，回写 issue 文档。

## 明确不做

- 不保留“小项目全量载入、大项目分片”的双路径；容量只由显式参数表达。
- 不新增版本 2 模块或兼容层，不通过截断关系、丢弃边或降低召回换取内存下降。
- 不改变 `SnapshotDelta` 语义、CAS 流程、manifest 状态机与既有恢复用例。
- 不把构建中间态当作可查询数据源；发布唯一出口仍是已激活的规范快照。
- 不为降低峰值关闭 WAL、降低同步级别或跳过 flush。
