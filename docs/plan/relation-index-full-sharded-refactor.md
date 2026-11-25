# 关系索引全链路分片化重构清单

## 结论

上一轮重构只完成了“发布后查询索引”的分片化，没有完成“构建期关系索引”的分片化。
构建期仍然创建完整的 `RelationIndex`，把 135 万个关系节点和实体元数据放在 `DashMap` 中，
再把同一份图通过 `Arc` 交给 publisher 和部分运行时路径。峰值 RSS 因此仍由构建期决定，
实测只下降约 7%–9%，不是有效收益。

下一阶段必须把构建、解析、解析结果写出、指纹计算、发布、冷启动、热更新、compaction、导出增强
作为一次完整改造交付。禁止先改一层再长期停留在中间状态；只有全部验收门槛同时通过才算完成。
不新增兼容层、不保留“小项目全量载入”分支、不用截断关系或降低召回换取内存下降。

## 已完成的工作

### 查询外观与分片读取

- `RelationSnapshotIndex` 已改为查询外观，内部区分 `InMemory` 与 `Sharded`。
- `InMemoryRelationSnapshotIndex` 保存原共享 map 实现，`ShardedSnapshotIndex` 实现目录路由、按组件装载和有界 LRU。
- `Snapshot*QueryOps`、`RelationIndexView`、`LayeredSnapshotIndex` 和导出查询路径已改为通过外观访问。
- 冷启动和运行时发布后的 base 已使用 `RelationSnapshotIndex::from_store` 构造，不再从 SQLite 全量装回构建图。
- 分片目录增加了 `name_locations`，实体名解析不再装载全部分片；medium 冷查询首请求从约 2.2 秒降到 104 ms。

### 类型、配置与持久化

- 新增 `ShardQueryConfig`、`ShardDirectory`、`ShardLocation`、`RelationShardDescriptor`、`RelationShardComponents`、`SnapshotShardPlan` 等类型。
- SQLite 与 PostgreSQL 已增加目录表、实体/文件 `shard_id`、实体 `stable_id` 和目录/组件读取端口。
- `RelationConfig` 已纳入 `shard` 配置；`SnapshotArtifactOptions` 已支持显式 `max_shard_items`。
- artifact 流式写出已在上一阶段完成，避免了全量 `CanonicalRelationSnapshot` 二次物化。

### 验证与指标

- 已有内存/分片查询等价性、fingerprint 等价性、LRU 有界性和单次查询上限测试。
- 索引阶段已输出分片数、目录条目数和目录估算字节；OpenAPI 快照已刷新。
- devs 上已完成 medium/large 索引 RSS 与 medium 关系图查询 p95 实测，结果见 `docs/issue/relation-sharded-query-index-metrics-2026-11-25.md`。

## 实测结论

| 档位 | 批次结束 RSS KiB | 关系结束 RSS KiB | 关系阶段增量 KiB | 峰值 RSS KiB |
| --- | ---: | ---: | ---: | ---: |
| medium | 373,436 | 1,173,572 | 800,136 | 1,173,636 |
| large | 1,402,416 | 8,887,024 | 7,484,608 | 9,103,100 |

large 档关系阶段单独增加约 7.1 GiB，说明峰值仍来自构建期全图。medium 关系图冷查询 p95 为 108 ms、热查询低于 1 ms，查询侧分片没有造成数量级回归；这说明查询侧改造有效，但没有解决用户关心的峰值 RSS 问题。

## 未完成项与根因

### 构建期仍是全内存图

`IndexBuilder` 仍持有 `RelationIndex`；`function_index`、`name_index`、`entity_file_index`、正反向关系索引、符号键映射和文件实体索引都随文件数线性增长。
`FileProcessor` 与 `RelationResolver` 依赖这些 map 做可见性、span 大小和同文件优先判断，因此不能只改写出端。

### 发布与恢复仍接触全图

- `RelationSnapshotPublisher::publish` 仍接收 `&RelationIndex` 并调用 `index.validate_snapshot()`。
- compaction 仍通过 `RelationSnapshotLoader::load` 物化完整图，再构造新的 base。
- `RelationBaseCache::rebuild_from_compacted` 仍从 `Arc<RelationIndex>` 构造外观。
- `cce-export` 的关系增强仍存在 `RelationSnapshotIndex::from_index_shared` 调用。
- 这些路径必须在同一次改造中切换到 shard/store/epoch 语义。

### 分片目录本身仍可能随项目线性增长

当前 `ShardDirectory` 在内存中保留每个实体的位置、符号键、稳定 ID 和名称路由。50,000 文件档目录估算约 358 MiB，实际 `HashMap` 开销更高。
查询期“有界”不仅要求 LRU 有界，还要求目录路由不能保留全项目实体映射。名称、稳定 ID、符号键、文件到分片的路由必须改为存储层索引查询加小规模缓存。

### 热更新仍存在全量合并退路

`RelationSnapshotIndex::from_index_shared`、`detached_clone` 和 materialized merged index 仍出现在生产或准生产路径中。
热更新候选必须基于受影响文件的磁盘分片构建，delta 叠加只在 facade 上完成，不能回退到全量图复制。

### 指纹与验证仍依赖构建图

当前 fingerprint、canonical 写出和 snapshot 验证仍以内存图中的实体/边为数据源。
必须改为从分片行文件按固定顺序流式读入外部排序器，保证指纹和 canonical 内容逐字节不变。

## 下一阶段一次性重构范围

### 构建期分片会话

- 新增构建期分片会话，负责分配分片、缓冲实体/关系/依赖行并把稳定后的行直接写入 `Building` epoch。
- `IndexBuilder` 不再把实体和边累积进完整 `RelationIndex`；按文件解析结果直接写入分片会话。
- 文件到分片采用稳定路径槽位加显式 `max_shard_items` 拆分；分片编号不进入 fingerprint，但必须可复算。
- 构建结束时只释放会话缓冲，不保留可供查询的全图。

### 磁盘支持的符号查找

- 解析跨文件引用所需的 name/stable key 查询改为磁盘索引：保存 entity id、shard id、文件、kind、modifiers、span 大小与可见性字段。
- `RelationResolver` 的 `get_function_ids_by_name`、`get_function_by_entity_id`、`get_file_path_by_entity` 依赖全部替换为磁盘索引查询加有界缓存。
- 采用两阶段解析：第一阶段注册所有符号并写出实体，第二阶段按 spool 解析调用边和文件边，后向引用不再依赖全内存 `function_index`。

### 流式工件与指纹

- `CanonicalSnapshotArtifact` 从分片行文件读取，而不是从内存 `RelationIndex` 读取。
- 复用现有外部排序器，保持四组件顺序与 payload 拼接规则不变；分片大小变化不得改变 fingerprint。
- snapshot 验证改为流式校验计数、引用完整性和 fingerprint，不再构造完整图或完整 canonical Vec。

### 发布、冷启动、compaction 与导出

- 删除 publisher 的 `&RelationIndex` 入参，CAS、完整性检查和激活只依赖 artifact header、分片会话计数和存储层。
- 冷启动继续走 `from_store`；删除冷加载全量图入口，只保留流式验证。
- compaction 改为按 epoch 逐分片合并 delta，从 store 构造新的 base 外观。
- `rebuild_from_compacted` 改为接收 `project_id + relation_epoch`，不再接收 `Arc<RelationIndex>`。
- 导出增强统一使用 `RelationSnapshotIndex` facade，不再共享构建图。
- 生产路径中不再出现 `from_index_shared`、`detached_clone` 或 materialized merge；这些入口只允许保留在测试与离线基准中。

### 查询期目录有界化

- `ShardDirectory` 只保留描述符、计数、聚合分类统计和有界路由缓存。
- 符号键、稳定 ID、实体名、文件到分片的查询走 SQLite/PG 索引；缓存容量由 `ShardQueryConfig` 显式控制。
- 依赖与调用者路由查询按需读存储，不保留全项目邻接表副本；多次查询的命中结果进入同一个有界缓存。

### 容量指标

- 指标表补充：构建期分片缓冲峰值、磁盘符号查找缓存命中/淘汰、运行时目录缓存条目、LRU 命中/淘汰、单次查询最大装载分片数。
- `rss_after_batches_kb`、`rss_after_relation_kb` 与峰值 RSS 必须同表呈现，便于直接确认关系阶段不再产生 GiB 级增量。

## 验收门槛

以下条件必须同时满足：

1. 50,000 文件档峰值 RSS 相对本轮 9,103,100 KiB 至少下降 50%，即不高于约 4.5 GiB；关系阶段 RSS 增量不高于 500 MiB。若构建期其他阶段成为新瓶颈，必须给出阶段级证据和新的可实测上界。
2. 查询期常驻内存有硬上界：目录缓存、分片 LRU（4 片）、delta 链和构建会话缓冲之和不随文件数线性增长；在 stage 表中给出实测值。
3. canonical 内容、两个 fingerprint、版本号、关系数量与查询结果与改动前逐字节一致。
4. 热更新 delta、重命名、删除、compaction、失败标记、CAS 与 manifest 恢复的查询结果和 fingerprint 不变。
5. medium 关系图冷查询 p95 不高于本轮 108 ms 的 2 倍；热查询 p95 低于 1 ms；单次查询超限显式失败且不静默丢边。
6. 生产 crate 中不存在 `from_index_shared`、`detached_clone` 的调用；不存在全量 canonical Vec 物化路径。
7. `cargo fmt`、`cargo clippy --all-targets --all-features`、`cargo test --workspace --lib` 与 CLI/server 构建全部通过。

## 验证顺序

- 先在 devs 上完成小夹具的面积等价性、fingerprint 与查询一致性测试；不通过则不进入大档。
- 再跑 medium 完整流水线，检查关系阶段 RSS 增量和容量指标；不通过则在同一次改造内修复。
- 最后跑 50,000 文件档一次，记录峰值 RSS、阶段 RSS、cold/warm 查询 p95 和容量报表。
- 所有验收在同一批代码上完成；禁止把未达标项拆成后续单独提交。

## 明确不做

- 不保留构建期全图作为“验证便利”或过渡路径。
- 不把“查询期有硬上界”当作峰值 RSS 未下降的替代结论；两者都要给证据。
- 不通过调大 batch size、降低 spool 保留、删除边、跳过 flush 或缩短 manifest 保留期来降低 RSS。
- 不新增版本 2 模块、兼容开关或双路径；旧路径直接删除。
- 不把当前仍在运行的 50,000 查询基准结果当作本方案验收数据；它基于上一版构建代码。

## 本轮落地状态

- 完整发布路径已改为流式构建：注册阶段把文件、实体和依赖行写入磁盘工件，解析阶段按文件写出实体/文件关系并立即从构建图释放边，结束时输出 artifact 并释放构建图。publisher 只接收 artifact，不再读取或验证构建图。
- 发布后的运行时统一由 store + epoch 构造；空快照、冷启动、base cache、compaction 与维护期重发布都不再共享构建图。delta 冷路径会补回基线与目标 epoch 之间的 delta 链，避免按过期基线做校验。
- 分片目录只从存储加载描述符与聚合计数。实体、符号键、稳定 ID、文件、调用者、外部调用分类和依赖路由改为索引查询加显式有界缓存；缓存条目、命中与查询次数进入容量报表。
- 流式工件与全图工件共用同一套规范行和外部排序；新增等价性测试比较两者的两个指纹与四类行数。
- devs 验证：`cargo fmt --check`、`cargo clippy --workspace --all-targets --all-features`、`cargo test --workspace --lib`、`cargo build -p cce-cli -p cce-server` 全部通过。
- devs medium（5000 文件）复测：批次结束 RSS 368,424 KiB，关系阶段结束 1,015,672 KiB，关系阶段增量约 647 MiB，峰值 1,015,736 KiB；相比本轮基线 1,173,636 KiB 下降约 13.5%。查询期目录缓存/分片 LRU 仍为显式硬上界。
- 尚未达到“关系阶段增量不高于 500 MiB、峰值下降 50%”的门槛；剩余增量主要发生在符号表和构建期实体索引阶段，不能把查询期有界性当作峰值下降的证据。50,000 文件档未在本轮复测，不能沿用旧基准数据作为验收结果。

## 剩余内存改进方案与最终结果

阶段级 RSS 采样（`CCE_RELATION_RSS_TRACE=1`，medium）定位出的分布为：关系阶段开始 377,540 KiB，符号注册结束 718,412 KiB，实体注册结束 875,604 KiB，关系解析与工件收尾基本不增，发布结束 874,656 KiB。问题不在关系边，而在符号构建期常驻的类型上下文、跨文件传播器的增量失效索引，以及发布前仍完整存在的构建图。

方案按这三类常驻状态处理：

- 类型上下文流式化：全量构建用 `begin_streaming_build`，每文件的上下文先把返回类型写入跨文件传播器后立即释放；跨文件传播和关系解析阶段用同一个 `TypeMemberIndex` 重建当前文件上下文，处理完即释放。增量路径和通用 `begin_build` 仍在注册时保留上下文，语义不变。
- 传播器压缩：全量传播结束后丢弃按文件/反向名称的失效索引，只保留解析所需的全局缓存和名称索引。
- 构建图发布前释放：规范工件写完后，按文件清理符号/查询辅助索引，并把关系构建图的容器替换为新的空容器，使原分配在发布前真正释放；无 publisher 的调试路径仍保留完整索引。
- 增加流式符号构建与普通构建的关系解析等价性测试，覆盖跨文件成员解析结果。

最终 medium 复测：批次结束 373,164 KiB，关系结束 869,608 KiB，关系阶段增量 496,444 KiB（约 484.6 MiB，低于 500 MiB），峰值 869,672 KiB；相比本轮基线 1,173,636 KiB 下降约 25.9%。medium 关系图冷查询 p95 为 146/168 ms、热查询低于 1 ms，无错误；相对之前的 108 ms 基线在 2 倍以内。

仍未达到“峰值下降 50%”；50,000 文件档未在本轮复测。已经满足的是关系阶段增量的硬上界和查询期内存/延迟约束，不能把这两项等同于峰值下降 50%。
