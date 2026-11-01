# 热更新子系统设计

## 概述

热更新子系统在服务运行期间对文件变更做增量索引，避免全量重建。实现位于 `crates/app/cce-orchestrator/src/hot_update/`。设计原则：全局 debounce + 批处理、以 SQLite 哈希比对为唯一变更事实源、解析器级实体变更检测、统一下游处理。

## 事件驱动架构

```
文件系统
    ↓
WatchCoordinator（文件监听） 或 PeriodicScanTask（周期扫描兜底）
    ↓
GlobalDebounce（批控，DebounceConfig 可调）
    ↓
Scanner（扫描变更文件，排除规则见 exclude_rules.rs）
    ↓
ChangeDetector（SQLite 哈希比对，change_detector.rs）
    ↓
FileProcessor（解析 + 实体级变更检测，file_processor.rs）
    ↓
UpdateProcessor 系列（下游处理）
```

- **事件循环**：`EventLoopManager`（event_loop.rs）管理事件处理循环的生命周期与优雅停机，状态机 `NotStarted → Running → Stopping → Stopped / Error`，附处理/失败计数统计。
- **事件队列**：`WatchChangeQueue`（watch_change_queue.rs）缓存 watch 事件产生的待处理文件变更，容量有界、溢出时施加背压。
- **模式切换**：`ModeStateMachine` / `ModeSwitchConfig`（mode_switch.rs）管理与 watch 模式相关的模式迁移。

## 变更检测与增量计算

- `ChangeDetector` 以 SQLite 元数据库中已索引状态（内容哈希）为基准做比对，输出 `FileChange`（新增/修改/删除）与解析后的 `EntityChange`（实体级增/删/改），无需重新解析未变更文件。
- 实体级变更支撑下游**最小化处理**：只有实际变化的实体触发摘要/嵌入更新，`change_merger.rs`（coordinator/）合并重叠变更。

## UpdateProcessor 系列（processors/）

统一 trait `UpdateProcessor`（trait_def.rs），经 `ProcessorCollection` / `ProcessorConfig`（factory.rs）组装：

| Processor | 职责 |
|---|---|
| `Bm25UpdateProcessor` | BM25 全文索引的增量更新 |
| `EmbeddingUpdateProcessor` | 变更实体的向量重新嵌入 |
| `SummaryUpdateProcessor` | 摘要重新生成 |
| `NlDocumentUpdateProcessor` | 文档类实体的 NL 更新（export_processor 复用） |
| `RelationUpdateProcessor` | 关系图增量更新；`ExternalPackageData` 提供外部依赖数据 |
| deletion（`process_deletions` / `remove_file_from_storage`） | 文件/实体删除的全存储清理 |
| rechunk | 变更导致的重切分场景 |

处理器共享 `ProcessorContext`（connection、storage、指标等运行时依赖）。

## OperationRuntime：锁拆分与断点续跑

`HotUpdateOperationRuntime`（operation_runtime.rs）持有操作关键的可变状态（变更检测器、元数据库、CheckpointManager、StorageCoordinator、OperationCoordinator、待处理变更队列），**自持一把独立互斥锁**：后台 worker 执行完整操作（解析 + 嵌入 + 发布）期间只锁 runtime，coordinator 锁保持空闲——事件累积、变更风暴检测与配置处理等 API 在整段操作期间始终可用（operation_runtime.rs 模块文档）。

- 子模块 `resume.rs` / `persistence.rs` / `config_change.rs`：检查点驱动的断点续跑、操作持久化与配置变更引发的运行时调整。
- 与 `operation` 模块的关系：热更新操作复用全局 OperationCoordinator / CheckpointManager / OperationContext（模块职责边界见 `docs/architecture/state/OPERATION_AND_INDEX_STATE_DESIGN.md`）。

## 配置热重载

`config/` 子模块提供 `ConfigReloadManager` / `ConfigVersion` / `ConfigVersionRegistry`：配置文件变更经版本注册表登记后生效，运行中的热更新管线按版本切换配置，避免读到撕裂的配置组合。

## 隔离与事务发布

`coordinator/temp_db.rs`：临时数据库隔离变更中间态，最终结果一次性原子发布到正式存储，发布失败不影响线上的已索引状态。

## 恢复演练

崩溃/中断后的恢复工作流验证见 `docs/tests/hot_update_recovery_workflow.md`。
