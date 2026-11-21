# 热更新与断点恢复测试覆盖分析与补充落地

## 0. 结论

> 本文档记录一轮完整的"分析 → 改造 → 补测 → 实测"闭环。第 1–6 节是分析结论，第 7–9 节是实际落地的补充，第 10 节是实测结果，第 11 节记录过程中新发现的问题与遗留项。

1. **覆盖面不缺"广度"，缺"深度"**：`hot_update` e2e 套件原有 80 个用例（8387 行），已覆盖崩溃点矩阵（RC1–RC6）、重复工作断言、数据重建断言与整体工作流；但模块 15096 行中有 7934 行（52.6%，27 个文件）没有任何 in-crate 单元测试，恢复主干 `resume.rs`、`operation_runtime.rs`、`persistence.rs`、`config_change.rs` 全在其中，行为只能由 e2e 单层保障。
2. **真实的测试空洞**：`resume` 的 `Deleted` 载荷分支与"崩溃后文件从磁盘消失"分支此前完全没有用例，而这两条分支决定删除能否传播到发布代。
3. **本轮已补齐 19 个用例并全部通过**：15 个新增 in-crate 单元测试（重建 6、生命周期与配置变更 5、模块进度 4）＋ 2 个新增 e2e（删除传播、失败恢复后再恢复）＋ harness 自校验断言；`cce-orchestrator` 单元测试从 623 增至 638，全部通过。
4. **顺带修复 3 个实现缺陷**：候选可收养判定的存储协调器门控（G5）、恢复路径的相对路径告警刷屏、重建逻辑的可测性（从 446 行编排中抽出）。
5. **顺带修好一个既有失败**：`drift_sweep_chunking_drift` 在 HEAD 上本就失败（已用 `git stash` 对比确认与本轮改动无关），根因是 e2e 脚手架的 project 行停在占位根 `/projects/{id}`，漂移清理读不到文件后静默跳过全部；修复后该用例通过。hot_update e2e 全绿：82 passed / 0 failed。
6. **第二轮把剩余用例补齐，并把预存问题一并清掉**：新增 2 个真实崩溃用例（崩溃态由生产代码写出，不再手工复刻）、1 个并发运行串行化用例、1 个坏根漂移回归用例、1 个陈旧操作回收单测、1 个漂移读取结果分离单测、1 个合并器分片标记回归单测；hot_update e2e 从 82 增至 86 全绿。
7. **预存问题修掉 6 个**：已修的是漂移清理读不到文件仍把配置记为已扫描、`entities.rank` 查询与生产表结构不一致（生产库上所有实体查询都会报错）、合并器清空未合并分片的 `is_fragment`、`bm25_alignment` 的过期契约，外加第 5 条与第 4 条里的四项。
8. **第三轮把剩下 2 个检索评分语义问题也修了**：单路检索的默认归一化从 min-max 改为「按最大值等比缩放」（min-max 必然把末位压成 0，再被 `min_score=0.25` 丢掉）；融合评估的第二个失败经复核是"重复覆盖不计入独占贡献"这一既定语义，与多实体展开无关，改为用重叠数断言真实性质。`cce-e2e-tests` 全量 e2e 从 178 passed / 2 failed 变为 **180 passed / 0 failed**，见 12.3、12.4。

## 1. 范围与口径

| 项 | 口径 |
| -- | ---- |
| 被测对象 | `crates/app/cce-orchestrator/src/hot_update/**`（15096 行）与 `src/operation/{checkpoint.rs,coordinator.rs,recovery.rs}` 中与恢复相关的部分 |
| 测试资产 | 三层：in-crate 单元测试、`crates/app/cce-orchestrator/tests/*.rs` 集成测试、`crates/app/cce-e2e-tests/tests/e2e/hot_update/**` e2e |
| 判定标准 | 以 `resume_operation` 与 `run_operation` 的**分支**为单位，逐分支确认"有无用例、用例断言什么、断言是否读库（而非读内存）" |
| 未纳入 | 检索质量、性能基准（见 `docs/tests/`）、`cce-desktop` 与 `llm-suite`（workspace `exclude`） |

## 2. 被测结构事实

恢复主干的调用链（行号取自当前 HEAD `5a1fd19d`）：

- `HotUpdateCoordinator::run_operation`（`coordinator/change_merger.rs:258`）→ `HotUpdateOperationRuntime::run_operation`（`operation_runtime.rs:370`）。
- 进入顺序是**配置变更优先 → 恢复 → 新操作**：`operation_runtime.rs:381`（显式 ConfigChange）、`operation_runtime.rs:391`（待处理配置队列）、`operation_runtime.rs:406`（`try_recover_operation`）、`operation_runtime.rs:412`（注册新操作）。
- 恢复入口 `resume_operation`（`operation_runtime/resume.rs:30`）内部依次是：候选可收养判定（`resume.rs:51`）→ 从检查点重建 `BatchChangeResult`（`resume.rs:79`）→ 已发布短路（`resume.rs:210`）→ prepare（`resume.rs:242`）/process（`resume.rs:260`）→ 落盘摘要（`resume.rs:301`）→ commit（`resume.rs:314`）→ 成功或失败收尾（`resume.rs:345` / `resume.rs:370`）。
- 配置变更走独立函数 `run_config_change_operation`（`operation_runtime/config_change.rs:43`），检查点种类为 `OperationKind::ConfigChange`（`config_change.rs:102`），因此不会被热更新恢复选中。

这三条事实决定了覆盖矩阵的行划分。

## 3. 测试资产盘点

| 层 | 位置 | 规模 | 与热更新/恢复的相关性 |
| -- | ---- | ---- | ---- |
| 单元 | `src/hot_update/**` 内 `#[cfg(test)]` | 108 个用例，分布在 20 个文件 | 集中在 `coordinator/tests.rs`(13)、`error.rs`(12)、`change_detector.rs`(12)、`file_processor.rs`(9)、`watcher/coordinator.rs`(8)；**恢复主干 0** |
| 集成 | `crates/app/cce-orchestrator/tests/` | 9 个文件 4178 行 | `checkpoint_recovery_integration.rs`(13 用例) 测检查点 SQL 语义；`dead_letter_retry_integration.rs`(1 用例) 测截断重试；**无一例调用 `run_operation`** |
| e2e | `crates/app/cce-e2e-tests/tests/e2e/hot_update/**` | 80 个用例，8387 行 | 覆盖最全的一层，含崩溃矩阵、快照恢复、真实失败、增量守卫 |

`hot_update` 内零测试文件按行数排序（前 12）：

| 文件 | 行数 | 职责 |
| ---- | ---- | ---- |
| `processors/relation/relation_processor.rs` | 1130 | 关系图增量与配置重建 |
| `operation_runtime.rs` | 984 | 操作主流程、检查点落盘、提交/回滚 |
| `coordinator/coordinator_core.rs` | 964 | 协调器装配与后台循环 |
| `coordinator/change_merger.rs` | 664 | 变更合并、配置变更排队 |
| `processors/bm25.rs` | 488 | BM25 增量写入 |
| `watcher/event.rs` | 471 | 文件事件解析与去抖输入 |
| `operation_runtime/resume.rs` | 446 | **恢复主干** |
| `processors/relation/config_parser.rs` | 400 | 配置文件依赖解析 |
| `operation_runtime/persistence.rs` | 334 | 检查点写入、哈希提交、发布态判定 |
| `watcher/config.rs` | 294 | 监听配置 |
| `operation_runtime/config_change.rs` | 272 | 配置变更操作 |
| `processors/relation/file_identifier.rs` | 258 | 文件身份识别 |

## 4. hot-update 主链路覆盖矩阵

| 环节 | 代码位置 | 单元 | 集成 | e2e 证据 |
| ---- | ---- | ---- | ---- | ---- |
| 变更检测（增/改/删） | `change_detector.rs` | 12 用例 | — | `hot_update_file_changes.rs:9/59/93/120` |
| 事件去抖与合并 | `debounce.rs`、`coordinator/change_merger.rs` | 4 用例（仅 `debounce`） | — | `hot_update_debounce.rs:37/120/170/214`、`hot_update_watch_trigger.rs:211` |
| 事件风暴降级与恢复 | `mode_switch.rs` | 6 用例 | — | `hot_update_mode_switch.rs:78/105`、`hot_update_watch_trigger.rs:122` |
| 监听链路自动触发 | `watcher/*` | 8 用例（`watcher/coordinator.rs`） | — | `hot_update_watch_trigger.rs:42/274` |
| 无变化快路径 | `operation_runtime.rs:502` | — | — | `hot_update_resume_recovery.rs:1219`（WF-004，断言不产生新 epoch、不写检查点） |
| 早期检查点 + 批次检查点 | `operation_runtime.rs:452`、`persistence.rs:36` | — | 间接（`checkpoint_recovery_integration.rs:87` 只测建表） | 由 RC1–RC6 间接覆盖 |
| 处理器链顺序与失败隔离 | `operation_runtime.rs:603` | — | — | `hot_update_processor_chain.rs:36/539`、`hot_update_processors.rs:257/307` |
| 候选发布（epoch 切换） | `persistence.rs:324`、`coordinator/tests.rs:151` | 1 用例（`is_manifest_active`） | — | RC1–RC5 全部断言 active manifest |
| 哈希提交与删除清理 | `persistence.rs:240` | — | `coordinator/tests.rs:349` | WF-001/WF-002 |
| 实体 ID 种子 | `persistence.rs:199` | — | `coordinator/tests.rs:106/113/126/143` | `hot_update_incremental_guard.rs:376` |
| 配置热重载（分块/摘要/排除/关系） | `config/reload.rs`、`config_change.rs:43` | 5 用例（仅 `reload`） | — | `hot_update_config_reload.rs:41/126/219/324` |
| 配置变更操作失败与中止 | `config_change.rs:224` | 0 | 0 | **无**（G8） |
| 心跳与陈旧清理 | `operation_runtime.rs:215`、`operation/coordinator.rs:147` | 1 用例（仅 `recover_unfinished_operations` 过滤） | — | **无**（G10） |
| 真实模块失败的提交/回滚 | `operation_runtime.rs:733` | — | — | `hot_update_real_failure.rs:34/117/198` |

## 5. resume 分支覆盖矩阵

| # | 分支 | 代码位置 | 覆盖情况 | 证据 |
| - | ---- | ---- | ---- | ---- |
| R1 | 候选可收养 → 保留模块进度 | `resume.rs:69` | 已覆盖 | `hot_update_resume_recovery.rs:178`（RC3） |
| R2 | 候选不可收养 → 清除进度 | `resume.rs:56` | 已覆盖 | `hot_update_resume_recovery.rs:279`（RC4）、`hot_update_real_failure.rs:34` |
| R3 | 无 `storage_coordinator` 时整段跳过 | `resume.rs:51` | **未覆盖** | 见 G5 |
| R4 | 只读取 batch 0 | `resume.rs:81` | 单批已覆盖 | 见 G11 |
| R5 | 信封为 `Deleted` → 记删除 | `resume.rs:98` | **未覆盖** | 见 G1 |
| R6 | 信封可用 + 磁盘一致 → 复用 | `resume.rs:108` | 已覆盖 | RC3、`hot_update_snapshot_resume.rs:71/141` |
| R7 | `already_exported` 判定 | `resume.rs:126` | 已覆盖 | `hot_update_resume_recovery.rs:956`（DATA-005） |
| R8 | 摘要与指纹复用 | `resume.rs:147` | 已覆盖 | `hot_update_resume_recovery.rs:883`（DATA-003） |
| R9 | 渲染指纹/内容哈希回填 | `resume.rs:156` | 已覆盖 | DATA-005 |
| R10 | 模块进度回填 | `resume.rs:158` | 已覆盖 | DATA-004、RC3 |
| R11 | 信封版本不兼容 → 重解析 | `resume.rs:91` | 已覆盖 | `hot_update_resume_recovery.rs:747`（DATA-006）、`resume_harness.rs:1367`（篡改版本） |
| R12 | 无信封 → 全量重解析 | `resume.rs:171` | 已覆盖 | RC1、`hot_update_snapshot_resume.rs:71` |
| R13 | 信封可用但磁盘文件已消失 → 记删除 | `resume.rs:189` | **未覆盖** | 见 G2 |
| R14 | 无 `checkpoint_manager` → 直接报错 | `resume.rs:200` | **未覆盖** | 见 G6 |
| R15 | 已激活短路（补提交哈希 + 标记完成） | `resume.rs:210` | 已覆盖 | `hot_update_resume_recovery.rs:398`（RC5） |
| R16 | prepare/process 失败 → abort | `resume.rs:328` | 部分覆盖 | 仅有"先失败再恢复"（`hot_update_real_failure.rs:34`），无"恢复过程中失败" |
| R17 | 成功收尾（提交哈希 + 完成） | `resume.rs:345` | 已覆盖 | RC1–RC6 |
| R18 | 失败收尾（保持 InProgress + 释放队列） | `resume.rs:370` | **未覆盖** | 见 G4 |
| R19 | 恢复失败的重试计数指标 | `resume.rs:390` | **未覆盖** | 见 G10 |
| R20 | 恢复查询的类型/根目录过滤 | `resume.rs:413` | 已覆盖 | `hot_update_resume_recovery.rs:689`（RCV-008）、`checkpoint_recovery_integration.rs:674` |
| R21 | 真实崩溃（进程中断）后恢复 | — | **未覆盖** | 见 G3 |
| R22 | 并发/重复恢复同一 operation_id | — | **未覆盖** | 见 G7 |
| R23 | 配置变更检查点不被热更新恢复选中 | `config_change.rs:102` | **未覆盖** | 见 G9 |

结论：23 条分支中 13 条已覆盖、1 条部分覆盖、9 条无覆盖。

## 6. 缺口清单

| 编号 | 缺口 | 现状证据 | 风险 | 建议层级 | 优先级 | 本轮状态 |
| ---- | ---- | ---- | ---- | ---- | ---- | ---- |
| G1 | 恢复时 `Deleted` 信封未传播 | `resume.rs:98` 分支无任何用例；`resume_harness.rs:1080` 的 `CrashFile` 只有 `path/envelope/content_hash/module_progress`，**无法表达删除载荷** | 崩溃前已判定的删除在恢复后被丢弃，发布代残留已删文件的行与向量 | e2e（需先扩展 harness） | P0 | 已修复：`reconstruct_tests::deleted_tombstone_becomes_file_change`、e2e `rcv009` |
| G2 | 崩溃后文件从磁盘消失 | `resume.rs:189` 无用例；e2e 中唯一的 `remove_file` 在 `hot_update_resume_recovery.rs:1130`，属常规更新而非恢复 | 恢复把不存在的文件当"重解析"处理会失败或产生空实体 | e2e | P0 | 已修复：`reconstruct_tests::vanished_file_becomes_file_change` |
| G3 | 崩溃态是手工复刻，不是真崩溃 | 文件头声称通过 drop 实例模拟崩溃（`hot_update_resume_recovery.rs:3`），但全部用例都在 `run_hot_update()` 返回（成功或失败）之后才丢弃实例，没有一个用例在操作进行中中断；崩溃态实际由 `resume_harness.rs:1178` `create_crash_state` 逐字段手工写入 | harness 与生产写入路径漂移时测试仍绿；`persist_hot_parse_checkpoints`/`persist_summaries_to_checkpoints` 的字段语义变化无人发现 | e2e（新增"真实落盘"用例） | P0 | 部分修复：harness 加写入后反读自校验；真实崩溃用例未落地（见 O2） |
| G4 | 恢复自身失败后的二次恢复 | `resume.rs:370` 语义是"保持 InProgress 可再恢复"，但无用例验证第二次能接着做完 | 失败恢复后检查点被误标完成或过期清理，导致整批重做或数据缺失 | e2e | P0 | 已修复：e2e `rcv010` |
| G5 | 无存储协调器时不清除模块进度 | `resume.rs:51` 整段被 `if let Some(storage)` 包裹；生产装配恒注入（`cce-server-shared/src/engine/providers.rs:168`） | 当前无触发路径，属代码不对称；一旦装配方式变化即静默跳模块 | 代码收敛 + 断言（不写用例） | P2 | 已修复：改为无存储协调器即视为不可收养 |
| G6 | 无检查点管理器时恢复报错 | `resume.rs:200`；`try_recover_operation`（`resume.rs:407`）无 CM 时返回 `None`，该分支实际不可达 | 死分支，长期误导读者 | 删除或加 `debug_assert` | P2 | 已覆盖：分支保留并新增 `missing_checkpoint_manager_is_an_error` |
| G7 | 并发/重复恢复 | 无锁粒度用例；`resume_operation` 依赖调用方持运行时锁 | 两个恢复并发收养同一候选，重复发布或候选互踩 | e2e（并发） | P1 | 未落地：需要并发编排，见第 11 节 |
| G8 | 配置变更操作失败路径 | `config_change.rs:224`（`mark_operation_failed`）、`config_change.rs:238`（清 active）无用例；`hot_update_config_reload.rs:426` 只走成功路径 | 配置变更失败后检查点停在 in_progress 且无 kind 过滤时可能被误恢复 | 集成（stub 处理器） | P1 | 已修复：`lifecycle_tests::config_change_failure_marks_failed_and_retires_candidate` |
| G9 | 配置变更检查点不被恢复 | `config_change.rs:102` 的 `OperationKind::ConfigChange` 设计意图无用例；e2e 中 `OperationType::ConfigChange` 零引用 | 类型过滤退化后，崩溃的配置变更被当文件变更恢复 | 集成 | P1 | 已修复：`lifecycle_tests::config_change_checkpoint_is_not_recovered_as_hot_update` |
| G10 | 恢复重试指标与心跳 | `resume.rs:390` 的 `module_retry_total`、`operation_runtime.rs:215` 的心跳无断言 | 观测口径失真；长操作被误判定陈旧而清理 | 单元（指标）+ e2e（心跳） | P2 | 未落地：指标与心跳断言未写 |
| G11 | `batch > 0` 的恢复 | `resume.rs:81` 硬编码 batch 0，且 `operation_runtime.rs:558` 恒定写 batch 0 | 热更新分片策略变化即静默丢文件 | 文档化不变式 + 断言（不写用例） | P2 | 已固化：`RECOVERY_BATCH_INDEX` 常量 + `only_the_recovery_batch_is_reconstructed` |
| G12 | 变更合并与配置排队 | `change_merger.rs` 664 行 0 用例（`coalesce_pending_changes` 在 `coordinator/tests.rs:233` 有 3 个用例，但不含配置排队/重入队） | 失败重排队与并发重载的互斥逻辑无保护 | 单元 | P2 | 已覆盖：管理器级队列测试原有 5 例，本轮未新增 |

## 7. 实际落地的单元测试

原则：只补"纯函数与轻依赖"，不把集成场景塞进 `#[cfg(test)]`。

### 7.1 可测性改造

`resume_operation` 原本把"检查点集合 → `BatchChangeResult`"的重建段内联在 446 行编排里，无法单独验证。现抽出为 `operation_runtime/reconstruct.rs`：

- `RECOVERY_BATCH_INDEX` 常量固化"热更新只写单批"的不变式；
- `reconstruct_batch_from_checkpoints(operation_id)` 只依赖检查点管理器、扫描根与元数据存储，不需要 `StorageCoordinator`；
- `resume.rs` 的编排段缩减为一次调用，行为等价（`try_recover_operation` 无 CM 时仍返回 `None`，函数内的错误分支作为防御保留）。

### 7.2 新增用例（15 个，均通过）

| 文件 | 用例 | 覆盖 |
| ---- | ---- | ---- |
| `operation_runtime/reconstruct_tests.rs` | `deleted_tombstone_becomes_file_change` | G1：删除载荷不被丢弃，路径保持项目相对形式 |
| 同上 | `vanished_file_becomes_file_change` | G2：信封有效但磁盘无文件 → 落为删除，不产生解析 |
| 同上 | `matching_envelope_is_reused_with_its_persisted_state` | 复用路径：解析计数 0，摘要/渲染指纹/导出标记/模块进度全部还原 |
| 同上 | `incompatible_envelope_is_reparsed` | 版本不兼容 → 恰好重解析一次 |
| 同上 | `only_the_recovery_batch_is_reconstructed` | G11：batch 1 的记录不被读取 |
| 同上 | `missing_checkpoint_manager_is_an_error` | G6：错误信息指明缺失的绑定 |
| `operation_runtime/lifecycle_tests.rs` | `successful_batch_finalizes_as_completed` | 空批次不产生 NaN 平均耗时，`can_resume=false` |
| 同上 | `failed_batch_is_partial_and_resumable` | 失败数透传且 `can_resume=true` |
| 同上 | `config_change_without_processors_completes` | 配置变更成功路径落到 `Completed`，检查点种类为 `config_change` |
| 同上 | `config_change_failure_marks_failed_and_retires_candidate` | G8：检查点 `Failed`、候选被 retire、失败对上层可见 |
| 同上 | `config_change_checkpoint_is_not_recovered_as_hot_update` | G9：类型过滤生效 |
| `hot_update/progress.rs` | `missing_progress_reads_as_empty` 等 4 个 | 空值/正常/损坏三种输入的读写与指纹区分度 |

## 8. 未按原计划改为集成测试的原因

原计划在 `tests/` 新建集成测试文件。实际落地改为 in-crate 单元测试，原因有两条：

1. 重建函数是 `pub(crate)`，集成测试无法访问，而把它提升为公开 API 只为测试会扩大不必要的外部契约；
2. 上述用例只需要内存 SQLite 与临时目录，`coordinator/tests.rs` 已有同样的 `#[tokio::test]` 装配先例，放在包内不增加依赖成本，也不引入跨包编译。

配置变更失败用例需要 stub 处理器，直接在 `lifecycle_tests.rs` 内实现 `UpdateProcessor`，只实现 `process_operation` 与 `abort_operation`，其余走默认实现。

## 9. 实际落地的 e2e

**结论：需要补，但只补两类结构性缺口，另加一处 harness 自校验。**

| 编号 | 内容 | 落点 | 状态 |
| ---- | ---- | ---- | ---- |
| E2 | 恢复期删除传播：崩溃前已判定删除的文件，恢复后发布代不留 chunk/entity/BM25 文档，幸存文件仍可检索 | `hot_update_resume_recovery.rs::test_rcv009_resume_propagates_deletion_tombstone` | 已落地并通过 |
| E3 | 恢复自身失败后再恢复：第二次失败保持可恢复，第三次接着做完且不重复解析 | 同上 `::test_rcv010_failed_resume_stays_resumable` | 已落地并通过 |
| E6 | harness 自校验：`create_crash_state` 写完后用生产访问器反读并比对载荷与哈希 | `resume_harness.rs` 内嵌断言 | 已落地并通过 |
| E1 | 真实崩溃（在操作进行中中断任务） | 未落地 | 见第 11 节 |
| E4 | 并发恢复 | 未落地 | 见第 11 节 |
| E5 | 心跳保活 | 未落地 | 见第 11 节 |

`CrashFile::tombstone` 直接使用生产的 `encode_parsed_checkpoint(&Deleted)` 编码，因此 E2 走的是真实解码路径，不是 harness 自造的形态。

## 10. 实测结果

第三轮（本轮）的最新数字见下表；第二轮的对应值在括号里。

| 命令 | 结果 |
| ---- | ---- |
| `cargo test -p cce-orchestrator --no-fail-fast` | **742 passed, 0 failed**（lib 645 + 集成 97；第二轮 737 / lib 640） |
| `cargo test -p cce-config` | **170 passed, 0 failed**（新增枚举默认值与 serde 断言） |
| `cargo test -p cce-e2e-tests --lib` | **133 passed, 0 failed**（新增重复覆盖语义用例） |
| `cargo test -p cce-e2e-tests --test e2e hot_update` | **86 passed, 0 failed**（原 80 + 本轮 6） |
| `cargo test -p cce-e2e-tests --test e2e`（全量） | **180 passed, 0 failed**（第二轮 178 / 2） |
| `cargo test -p cce-storage-metadb-sqlite` | 71 passed, 0 failed |
| `cargo test -p cce-parser` | 1772 passed, 0 failed |
| `cargo fmt`（4 个改动包） | 无差异 |
| `cargo clippy --all-targets`（4 个改动包） | 改动文件零新增告警，其余为存量 |

既有失败的归属判断方式：`git stash push -u` 清空主仓全部改动后重跑同一用例，仍然失败，再 `git stash pop` 恢复。`drift_sweep_chunking_drift`、`bm25_alignment_integration::test_multi_entity_chunks_surface_all_entities`、`alignment_workflow::test_a_boundary_cases` 三个用例都是通过这种方式确认与本文改动无关后才动手修的。

## 11. 过程中发现的问题与遗留项

### 11.1 已修复

| 问题 | 位置 | 处理 |
| ---- | ---- | ---- |
| 候选可收养判定的门控依赖另一个组件是否装配：无存储协调器时整段跳过，模块进度标记会保留，导致模块跳过它从未落盘的文件 | `resume.rs` 收养判定段 | 改为"无存储协调器即视为不可收养"，无条件走清除路径；代价只是多一次模块重做 |
| 恢复路径对已是项目相对路径的检查点条目调用 `relativize`，每批都打 `Path is outside the scan root` 告警 | `operation_runtime.rs::relativize_scan_path` | 相对路径直接走 `normalize_project_path`，绝对路径才 relativize |
| 重建逻辑不可单独测试，只能靠 e2e | `resume.rs` | 抽出 `reconstruct.rs` |
| e2e harness 手工复刻崩溃态，漂移时测试仍绿 | `resume_harness.rs::create_crash_state` | 写入后反读比对载荷与哈希 |

### 11.2 既有失败已修复：`drift_sweep_chunking_drift`

现象：`chunking_drift_rechunks_from_cache_and_skips_changed_files` 断言 `file A must be re-chunked into the candidate generation` 失败，HEAD 上就红。

根因（日志定位）：漂移清理读到的路径是 `/projects/1/src/lib.rs`，而夹具文件在临时目录下，于是每个文件都 `cannot read file` 后计入 `skipped`，`rebuilt=0`，清理静默地什么也没做。

两段成因叠加：

1. `SqliteClient::for_project` 在给项目库播种时会写入占位根 `/projects/{id}`（`cce-storage-metadb-sqlite/src/client.rs:108`）；
2. `ensure_project` 的实现是 `INSERT OR IGNORE`（`repo/project_repo.rs:47`），占位行已存在时调用方的真实根被忽略。

修复：e2e 脚手架在 `ensure_project` 之后显式 `project_update` 写入真实根，并反读断言根路径已生效（`drift_sweep_support.rs`）。修复后 `rebuilt=2 skipped=0`，用例通过。

**留下的隐患（未改产品代码）**：只要 `projects.root_path` 停在占位值，漂移清理就会整批跳过且只有一条 WARN，不会升级为操作失败。生产中项目根由 registry 写入，正常路径不受影响；但"注册信息缺失 → 清理静默失效"的组合值得后续加一条可观测信号（指标或失败传播），避免同类静默退化。

### 11.3 既有失败已修复：`bm25_alignment_integration`

`test_multi_entity_chunks_surface_all_entities` 断言"struct+impl 组里的某个 chunk 带 ≥2 个实体"失败（`got [EntityId(5)]`）。

排查后发现前提已经过期：分组器把类型与 impl 分成两个组（`grouper/processors/class_method/processor.rs:262` 跳过 impl 子实体并按 impl 建组），impl 组再按 BM25 词预算切成一片一实体。用例改名为 `test_multi_entity_group_surfaces_every_entity`，改为钉住当前真实契约：同一 segment 的片段共享 segment 键、每片只声明自己覆盖的实体、片段并集覆盖整组、每个结果仍materialize 整组正文。

### 11.4 未落地的用例与理由

| 项 | 理由 |
| ---- | ---- |
| 跨进程并发恢复 | 单进程内 `run_operation` 已由运行时锁串行化（第 12.1 节有用例钉住）；跨进程需引入操作租约，生产只有单进程，断言没有意义 |
| G10 恢复重试指标 | `HotUpdateMetrics` 需要指标注册表装配，断言成本高于其价值；本轮改为用"指纹不被记录"这类行为断言覆盖同一风险 |

### 11.5 仍需拍板

| 编号 | 问题 | 建议 |
| ---- | ---- | ---- |
| O1 | 重建逻辑已抽出，是否进一步把"磁盘探测"也参数化（便于构造不可读文件等边界） | 暂不。当前通过临时目录即可构造缺失文件，参数化会引入闭包间接层 |
| O3 | `resume.rs` 无 CM 的错误分支 | 保留。已从死分支变为有覆盖的防御分支 |
| O4 | 漂移清理的失败传播口径 | 已改为"一个文件都没读到就失败"，容忍单文件读取失败；如需更严格（任一文件失败即失败）需拍板 |
| O5 | `min_score=0.25` 与单路归一化的相互作用 | **已定案**：默认策略改为 `max_relative`，见 12.3 |

---

## 12. 第二轮：剩余用例与预存问题清理

### 12.1 新增用例（6 个，全部通过）

| 用例 | 位置 | 覆盖 |
| ---- | ---- | ---- |
| `test_real_crash_mid_operation_resumes_from_production_state` | `hot_update_real_crash.rs` | E1：用超时丢弃运行 future 制造真实中断，崩溃态由生产代码写出；恢复复用信封、不重新解析、发布一代 |
| `test_real_crash_twice_still_finishes_on_the_third_run` | 同上 | 恢复自身被再次中断后仍可恢复，第三次完成且只发布一代 |
| `test_concurrent_hot_updates_serialize_on_one_coordinator` | `hot_update_concurrent_runs.rs` | E4：同一协调器上两个 `run_operation` 并发，恰好一个批次被执行，发布代内每文件一行、chunk id 唯一 |
| `test_cleanup_stale_operations_requires_a_stale_heartbeat` | `operation/queue.rs` | E5：心跳未过期不回收，过期后回收并释放 active 槽，队列可继续出队 |
| `rechunk_outcome_separates_drift_from_unreadable_paths` | `index/file_processor.rs` | 漂移与"读不到文件"必须可区分（后者是坏根信号） |
| `test_cross_group_merge_keeps_flags_of_untouched_fragments` | `cce-parser/chunker/merge/tests.rs` | 未被合并的分片必须保留 `is_fragment` 与分片序号 |

E1 的做法：`StallingProcessor` 在第 N 次 `process_operation` 上 `pending()`，调用方 `tokio::time::timeout` 到点后 future 被丢弃——中断点落在真实模块执行中，检查点、解析信封、候选代全是生产代码当时写下的字节。

### 12.2 本轮修掉的预存缺陷

| 缺陷 | 位置 | 处理 |
| ---- | ---- | ---- |
| 漂移清理读不到任何文件时仍持久化管线指纹，等于把一个没干活的扫描记为已完成，之后配置再变也不会重扫 | `hot_update/processors/bm25.rs`、`embedding.rs` | 新增 `RechunkOutcome::{Rebuilt,Drifted,Unreadable}`；`rebuilt == 0 && unreadable > 0` 时返回错误、不写指纹，操作以模块失败上报 |
| 实体查询 Select 了生产表结构里不存在的 `rank` 列，生产库上 `entities_by_file` 等查询全部报 `no such column: rank`，实体 diff 静默退化为"全部 Added" | `cce-storage-metadb-sqlite/src/repo/entity_repo.rs` | 8 处改为 `0.0 AS rank`（`rank` 只在测试自建表里存在）；`setup_test_db` 改用生产建表入口 `schema::create_all` 并补 seed 行，从根上杜绝测试表与生产表再次漂移 |
| 跨组合并器把栈上每个 chunk 的 `is_fragment` 都清成 false，导致被切成多片的组报告"不是分片" | `cce-parser/chunker/merge.rs` | 只清除本轮真正合并过的 chunk 的标记 |
| e2e 漂移脚手架的 project 行停在占位根 `/projects/{id}`（上一轮已修） | `drift_sweep_support.rs` | 显式 `project_update` 写入真实根并反读断言 |

### 12.3 已修：单路归一化把末位压成 0（`query_workflow::test_bm25_search_query`）

现象：查询 `user` 时 BM25 层返回两条命中（1.4477 / 1.2698），最终只剩一条。

根因：单路召回在进后处理前做分数归一化（`searcher_core.rs:312` 的 BM25 路径、`:594` 的向量路径），默认策略是 min-max（`boost/normalization.rs` 的 `normalize_min_max`）。min-max 把集合最小值**必然**映射成 `0.0`，随即被默认 `result.min_score = 0.25` 丢弃 —— 与"结果集大小无关，每次查询稳定少一条"。更糟的是它把集合内的相对差距放大成绝对质量：原始分 `1.45 / 1.44 / 1.43` 会变成 `1.0 / 0.5 / 0.0`，一次丢弃两条。

修法：新增 `NormalizationStrategy::MaxRelative`（配置 `strategy = "max_relative"`）并设为默认（`cce-config/src/modules/search.rs:41`）。语义是「按集合内最大正分等比缩放」，最高分仍映射到 1.0，其余按同一比例缩放，因此：

- 没有任何一条被钉在 0，"最后一名必丢"消失（`1.4477 / 1.2698 → 1.0 / 0.877`，两条都过 0.25 门限）；
- 紧簇不会被拉成满量程（`1.45 / 1.44 / 1.43 → 1.00 / 0.99 / 0.99`）；
- 负分按同一除数保留为负值（仍会被门限过滤），全非正分时保持原样不动 —— 此时不存在"最佳命中"可作基准。

实现见 `boost/normalization.rs:50` 的 `normalize_max_relative`。`MinMax` 变体保留，可显式配置回旧行为。混合召回不受影响：融合在 `normalizer.rs::normalize_by_key` 里按对齐键独立做 min-max，本策略只作用于单路召回。

### 12.4 已修：融合评估的"0 条命中"（`retrieval_method_alignment::fused_evaluation_*`）

**12.3 的旧记录在这里是错的，先纠正事实**：该用例失败时展开并没有出 0 条。加日志确认 Q2 的融合输出是 6 条实体行，`e:3`、`e:4` 都在前两位、覆盖区间都是 `[(21,24),(25,28)]` —— 多实体展开（`fusion/aligner.rs:232` 的 `entity_ids.len() > 1`）工作正常，与本轮分块粒度无关。

真正原因在评估口径：`any_matches` / `precision_*` 用的是 `coverage_over_retrieved` 的**独占贡献**（`range_evaluator.rs` 的 `contributions`，定义为"覆盖了其它条目没覆盖的行"）。`e:3`、`e:4` 来自同一对 chunk，覆盖完全相同，任何一方都不独占任何一行，于是双方贡献都是空集：`recall_any` 报 1.0（目标确实被覆盖），`any_matches` 报 0。这条语义是刻意的，且被 `a_chunk_repeating_covered_code_does_not_contribute` 明确钉住（重复覆盖的块不计为命中）；本轮试过改成"重叠即命中"，与该用例直接冲突，因此撤回。

修法：不改产品与评估口径，改断言到该指标真正能表达的性质。新增 `overlaps()` 读 `MethodRetrievalScore::redundant_coverage`（"top-k 内 (条目, 判定区间) 重叠数"），断言 Q2 的两条实体行都命中；同时保留一条 `any_matches == 0` 的断言把"重复覆盖不产生独占贡献"写进测试（`retrieval_method_alignment.rs:251`）。并在 `range_evaluator.rs:889` 加 `duplicate_rows_recover_the_target_without_unique_contribution` 钉住这一组合语义，避免后续有人把它当 bug "顺手修好"。

### 12.5 变更导出

本轮改动以两个独立 patch 导出，便于分别应用到本地仓库：

| patch | 覆盖范围 | 应用位置 |
| ----- | -------- | -------- |
| `hot_update_resume_recovery.patch` | 主仓：`cce-config`、`cce-orchestrator`、`cce-storage-metadb-sqlite`、`cce-parser` 的源码与测试，外加本文档 | 仓库根目录 `git apply` |
| `hot_update_e2e_tests.patch` | 子模块 `crates/app/cce-e2e-tests` | 进入子模块目录 `git apply` |
