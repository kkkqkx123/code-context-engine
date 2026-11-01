# 索引流水线设计（cce-orchestrator index 模块）

本文描述全量/增量索引流水线的编排设计，对应代码位于 `crates/app/cce-orchestrator/src/index/`。数据在解析、存储各环节的具体形态见 `docs/architecture/data-stream/` 下的数据流文档（实体、关系、摘要），本文不重复展开。operation/index_state 的状态机与操作持久化见 `docs/architecture/state/OPERATION_AND_INDEX_STATE_DESIGN.md`，本文仅引用其接口。

## 1. 整体阶段划分

全量索引入口是 `IndexOrchestrator::execute`（`index/orchestrator.rs:515`），阶段大致为：

1. **准备阶段**：通过 `IndexOrchestrator::new` / 一系列 `with_*` 构造器装配嵌入、BM25、向量库（本地/Qdrant）、元数据存储、全文索引、关系存储、进度跟踪器、checkpoint 管理器与插件注册表（`index/orchestrator.rs:133-479`）。扫描参数来自 `IndexOptions`（`index/options.rs:13`），可通过 `from_settings`（`index/options.rs:81`）从全局配置生成，并转换为扫描器的 `to_scan_options`（`index/options.rs:146`）。
2. **输出模式判定**：`determine_output_mode`（`index/orchestrator.rs:910`）按路径决定各文件的输出形态，批处理时按路径感知的模式处理。
3. **关系构建器初始化**：`init_relation_builder_async` / `create_relation_builder`（`index/orchestrator.rs:800-866`）在扫描期完成一次构建系统扫描，缓存构建配置解析结果，供后续注入合成配置节点。
4. **批量处理**：`run_batch_loop`（`index/orchestrator/batch.rs:32`）按确定性批次循环（见下节）。
5. **失败重试**：dead letter 重试（见第 4 节）。
6. **收尾**：`finalize_index`（`index/orchestrator/finalize.rs:25`）——先 `build_and_publish_relations` 发布关系索引，再 `export_nl_documents` 导出 NL 文档，最后 `finalize_manifest` 写清单。

增量索引则由 `index_file` / `remove_file`（`index/orchestrator/incremental.rs:22,56`）提供单文件粒度的索引与删除，供热更新子系统调用（见 `docs/app/orchestrator/hot_update/architecture.md`）。

## 2. 批处理与并发模型

`run_batch_loop` 的关键设计（`index/orchestrator/batch.rs`）：

- **确定性批次边界**：批次由 `FileIndexer` 按 `batch_config.scan_batch_size` 预先切分，循环从 `ctx.start_batch` 开始，保证 resume 后批次划分一致。
- **per-batch 立即落库**：每个批次处理完立即持久化实体/向量/BM25/摘要，内存占用被批次大小约束（`batch.rs:26-31` 模块注释明确说明）。批次前调用 `storage.begin_batch(batch_num)`（`batch.rs:56`）建立存储批次上下文。
- **bounded concurrency**：单个批次内文件通过 `process_batch_with_mode`（`batch.rs:631`）以有界并发处理，避免无限并行打爆嵌入/存储后端。
- **状态投影**：仅对真正产出结果的文件做成功状态投影（`succeeded` 集合，`batch.rs:153-165`），失败文件保持初始状态以便 resume 重试。
- **错误分级**：`BatchResult`（`batch.rs:872`）区分文件级失败与批次级（基础设施）错误；`classify_batch_action`（`batch.rs:908`）决定 Continue / Abort 等动作——文件失败不中断循环，批次级错误立即停止（对应三个测试 `batch.rs:923-940`）。

## 3. checkpoint 与断点恢复

checkpoint 贯穿两个层次：

- **批次 checkpoint**：每批开始时 `create_batch_checkpoint` 记录操作 ID、批号、首末文件与文件数（`batch.rs:70-96`），为状态表提供外键依赖，同时作为 resume 的起点（`ctx.start_batch`）。
- **解析结果 checkpoint**：`persist_parsed_checkpoint`（`batch.rs` 引用，定义于 `index/orchestrator/checkpoint.rs:518`）将 `ParsedCheckpointPayload` 编码入库；批次恢复时通过 `get_batch_files` 读回，校验 `content_hash` 与兼容性后反序列化复用（`batch.rs:99-134`），避免重复解析。
- **恢复流程**：`recover_file_indexer`（`checkpoint.rs:32`）重建文件索引器；`validate_recovered_hashes`（`checkpoint.rs:178`）校验恢复记录的哈希与当前文件一致；`replay_recovered_relations`（`checkpoint.rs:291`）与 `accumulate_recovered_export`（`checkpoint.rs:379`）把已恢复的解析结果重新回放到关系 spool 与导出累积器中。

## 4. dead letter 与截断重试

嵌入失败的文件进入 dead letter。`retry_dead_letters_for_files`（`index/orchestrator/dead_letter.rs:72`）对失败文件重试；`retry_dead_letter_with_truncation`（`dead_letter.rs:60`）在重试时启用截断策略；`apply_embed_budget`（`dead_letter.rs:293`）控制重试的嵌入预算，避免失败风暴下无限制消耗资源。重试结果以 `DeadLetterRetryReport`（`dead_letter.rs:27`）汇总。该能力由 `with_dead_letter_config`（`orchestrator.rs:187`）与 `dead_letter_truncate_retry_enabled`（`orchestrator.rs:198`）配置。

## 5. storage_coordinator：多后端存储编排

`StorageCoordinator`（`index/storage_coordinator.rs:51`）是批处理写入存储的统一入口，聚合：

- **向量存储**：本地向量库（`with_vector`）与 Qdrant 远端（`with_qdrant`，`storage_coordinator.rs:112-127`），`initialize_vector` / `initialize_qdrant`（`storage_coordinator.rs:322-337`）负责初始化；具体实现在 `storage_coordinator/vector.rs` 与 `index/vector_store/`（`vector.rs` / `fulltext.rs` / `relation.rs` 子模块）。
- **BM25 / 全文索引**：`with_bm25` 指向 BM25 服务，`with_fulltext_store` 指向本地全文存储（`storage_coordinator.rs:130-142`），`fulltext_backend_name` 报告后端类型（`storage_coordinator.rs:145`）；实现在 `storage_coordinator/bm25.rs`。
- **关系存储**：`with_relation_store`（`storage_coordinator.rs:178`）。
- **元数据存储**：`with_metadata_store`（`storage_coordinator.rs:167`）承载文件元数据与状态表。
- **epoch / batch 上下文**：`with_epoch` / `with_batch_id`（`storage_coordinator.rs:205-214`）为每次写入打上数据纪元标识；`align_epoch_to_active_generation`（`storage_coordinator.rs:270`）把写入 epoch 对齐到当前活跃 generation，配合 `storage_coordinator/generation/`（lifecycle、gc、sqlite/external compaction）实现旧数据纪元的垃圾回收与压缩。
- **候选生命周期**：`storage_coordinator/candidate/`（`prepare.rs`、`lifecycle.rs`）负责候选 chunk 的准备与落库生命周期。
- **嵌入阶段超时**：`set_embedding_stage_timeout`（`storage_coordinator.rs:159`）限制嵌入阶段耗时，配合容错策略（见 `docs/architecture/fault-tolerance-and-resilience.md`）。

## 6. 两阶段关系解析与发布

关系构建是"两遍扫描 + spool 落盘"的设计：

- **spool 中转**：批处理期间，每个成功解析文件的实体被追加进 `RelationBuildSpool`（`index/relation_build_spool.rs:93`），以编码条目（`RelBuildEntrySnapshot`，`relation_build_spool.rs:42`）落盘，`for_each` / `collect_all` 按插入序回放（`relation_build_spool.rs:150,182`）。spool 构造时会清除上次崩溃遗留的孤儿目录，Drop 时自清理（`relation_build_spool.rs:309,226`）。构建配置文件（Cargo.toml、package.json 等）的占位实体不进 spool（`batch.rs:170-180`），由 finalize 阶段统一注入。
- **两阶段解析**：`build_and_publish_relations`（`finalize.rs:86`）先注入合成配置节点（`finalize.rs:96-157`），再按内存预算选择回放方式——估算解码体积低于 `MEMORY_CACHE_LIMIT_BYTES`（256 MiB）且条目数 < 10 万时整体加载进内存做"注册 + 解析"两遍（`finalize.rs:172-210`）；否则走磁盘回放，注册与解析各扫一遍 spool（`finalize.rs:211-215`）。注册遍调用 `register_file_entities`（及插件符号注册），解析遍调用 `resolve_file_relations`（及插件关系注入），并通过 `inject_governance_edges` 注入 `source_file -> config_file` 治理边（`finalize.rs:365`）。
- **发布**：解析结果通过 `RelationSnapshotPublisher` trait（`index/relation_publisher.rs:29`）发布为 `RelationPublication`（`relation_publisher.rs:13`）快照，`maybe_compact`（`relation_publisher.rs:75`）在发布后触发压缩；`share_published_snapshot_with_exporter`（`finalize.rs:39`）把已发布快照共享给 NL 导出器，避免重复构建。
- **解析管线服务**：`ResolutionPipelineService`（`index/resolution_pipeline.rs:16`）提供 `allocate_and_write` → `activate` 的快照分配-激活流程（`resolution_pipeline.rs:45,129`），失败时 `mark_failed`（`resolution_pipeline.rs:143`）保证旧 epoch 快照继续生效（测试 `failed_epoch_preserves_previous_active_epoch`，`resolution_pipeline.rs:275`），`validate_snapshot`（`resolution_pipeline.rs:155`）拒绝缺失内部目标的快照。这与 storage_coordinator 的 epoch 机制共同构成关系索引的原子切换语义。

## 7. 导出 spool 与最终清单

`ExportSpool`（`index/export_spool.rs:24`）以同样的 spool 模式中转 NL 导出 chunk：批处理期 `append` 落盘（`export_spool.rs:62`），finalize 阶段 `load_chunks` 批量读回（`export_spool.rs:103`），`cleanup` / Drop 清理目录（`export_spool.rs:141,155`）。`export_nl_documents`（`finalize.rs:437`）在关系索引发布后一次性导出文档，`remove_stale_documents`（`finalize.rs:639`）清理失效文档，`finalize_manifest`（`finalize.rs:572`）写最终清单。

## 8. 失败恢复语义小结

- 文件级失败：不中断批次循环，记录 dead letter，状态表留在可重试状态，resume 时经 checkpoint 重放或 dead letter 重试补齐。
- 批次级（基础设施）失败：立即停止循环，checkpoint 记录到当前批，resume 从 `start_batch` 继续。
- epoch/generation：所有写入绑定 epoch，解析失败保留旧活跃 epoch；generation 模块负责旧纪元回收与压缩，保证中途崩溃后可回滚到一致快照。
- spool 孤儿目录：启动时清除上次崩溃残留，避免脏数据参与回放。

持久化的操作状态、phase 迁移与模块更新状态（`IndexPhase`、`ModuleType`、`ModuleUpdateState` 等，`batch.rs:23` 引用）不在本文展开，见 `docs/architecture/state/OPERATION_AND_INDEX_STATE_DESIGN.md`。
