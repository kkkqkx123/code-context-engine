# Operation 与 Index State 模块设计文档

## 概述

`operation` 和 `index_state` 两个模块在代码上下文引擎中扮演不同但互补的角色。本文档澄清两者的职责边界，避免开发者混淆。

## 核心区别

### 一句话总结

- **operation**：全局视角，回答"**这个操作能运行吗？**"
- **index_state**：文件视角，回答"**这个文件处理到哪里了？**"

### 详细对比表

| 维度 | operation | index_state | 含义 |
|-----|-----------|------------|------|
| **关注对象** | 整个操作 | 单个文件 | 范围 |
| **决策点** | 能否开始/执行操作 | 文件如何处理/重试 | 用途 |
| **约束** | 同时只有一个活跃 | 多文件可并行处理 | 并发 |
| **生命周期** | Queued → Active → Completed | 快速，文件处理期间 | 持续时间 |
| **持久化** | SQLite 存储（崩溃恢复） | 内存 + DB（失败记录） | 存储位置 |
| **粒度** | 操作/批次/文件/模块（4层） | 文件/模块（2层） | 检查点 |
| **重试策略** | 整个操作重试 | 模块级独立重试 | 重试 |
| **版本管理** | 无 | 文件版本控制 | 冲突处理 |

---

## 架构层次关系

```
┌─────────────────────────────────────────────────────────┐
│  应用层                                                    │
│  IndexOrchestrator / HotUpdateCoordinator                │
└────────────────────┬────────────────────────────────────┘
                     │
┌────────────────────▼─────────────────────────────────────┐
│  协调层 (OPERATION LEVEL)                                 │
│  OperationCoordinator                                     │
│  ├─ OperationQueue (优先级队列)                          │
│  ├─ CheckpointManager (4层检查点)                        │
│  └─ RecoveryManager (故障恢复)                           │
└────────────────────┬─────────────────────────────────────┘
                     │
┌────────────────────▼─────────────────────────────────────┐
│  处理层                                                    │
│  UpdateProcessor                                         │
│  └─ 调用各个模块处理器                                   │
└────────────────────┬─────────────────────────────────────┘
                     │
┌────────────────────▼─────────────────────────────────────┐
│  执行层 (INDEX_STATE LEVEL)                              │
│  ModuleProcessor                                         │
│  ├─ RelationProcessor                                    │
│  ├─ SummaryProcessor                                     │
│  ├─ EmbeddingProcessor                                   │
│  ├─ Bm25Processor                                        │
│  └─ ExportProcessor                                      │
│                                                          │
│  UpdateStateTracker (FileUpdateState 管理)              │
└──────────────────────────────────────────────────────────┘
```

---

## operation 模块详解

### 职责

1. **操作调度**：决定哪个操作何时运行
2. **全局约束**：确保没有并发冲突
3. **生命周期管理**：Queued → Active → Paused → Completed/Failed
4. **故障恢复**：从进程崩溃中恢复未完成的操作

### 核心类型

```rust
// 操作类型（全局视角）
pub enum OperationType {
    FullIndexing,      // 独占运行
    HotUpdate,         // 可与 Incremental 冲突
    IncrementalUpdate, // 可与 HotUpdate 冲突
}

// 操作阶段（生命周期）
pub enum OperationPhase {
    Queued,     // 在队列中等待
    Active,     // 正在执行
    Paused,     // 暂停（可恢复）
    Completed,  // 成功完成
    Failed,     // 失败
}

// 4层检查点
CheckpointRecord        // Operation 级别元数据
├─ BatchCheckpointRecord  // Batch 级别进度
├─ FileCheckpointRecord   // File 级别状态
└─ ModuleRetryRecord      // Module 级别重试
```

### 使用场景

```rust
// 场景 1：检查是否可以启动操作
if !coordinator.has_active_full_index().await? {
    coordinator.request_hot_update(op_id, root_dir).await?;
}

// 场景 2：获取活跃操作
if let Some(active) = coordinator.get_active_operation().await? {
    println!("Current operation: {}", active.operation_id);
}

// 场景 3：标记操作完成
coordinator.mark_operation_completed(operation_id).await?;

// 场景 4：恢复未完成的操作
let recovered = coordinator.recover_unfinished_operations().await?;
println!("Recovered {} operations", recovered);
```

---

## index_state 模块详解

### 职责

1. **文件状态追踪**：记录每个文件的处理进度
2. **模块独立管理**：5个模块（Relation/Summary/Embedding/Bm25/Export）独立追踪
3. **版本控制**：防止旧更新覆盖新更新
4. **重试管理**：模块级别的重试和死信队列

### 核心类型

```rust
// 文件级操作类型（文件处理上下文）
pub enum IndexOperationType {
    Full { total_batches, batch_size, root_dir },
    Hot { trigger: ChangeTrigger },
    Incremental { base_version },
}

// 模块级状态（每个模块独立）
pub enum ModuleUpdateState {
    Pending,                  // 等待处理
    Updating,                 // 正在处理
    Success,                  // 成功
    Failed,                   // 失败（待重试）
    Retrying { next_attempt },// 指数退避重试中
    DeadLetter,              // 最大重试次数已达，需手工介入
}

// 文件状态（包含5个模块的状态）
pub struct FileUpdateState {
    pub file_path: String,
    pub version: u64,                              // 版本控制
    pub change_type: FileChangeType,
    pub operation_type: IndexOperationType,
    pub module_states: HashMap<ModuleType, ModuleUpdateRecord>,
    pub checkpoint: Option<Checkpoint>,            // 可恢复的检查点
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### 使用场景

```rust
// 场景 1：创建文件状态
let state = FileUpdateState::new("src/main.rs".to_string(), 1, FileChangeType::Modified);

// 场景 2：查询模块状态
if state.get_module_state(ModuleType::Relation).state == ModuleUpdateState::Success {
    println!("Relation module completed");
}

// 场景 3：标记模块成功
state.mark_module_success(ModuleType::Summary);

// 场景 4：处理失败并自动重试
state.mark_module_failed(ModuleType::Embedding, "API timeout".to_string());
// → 自动进入 Retrying 状态，下次会重试

// 场景 5：检查文件是否可查询
if state.is_queryable() {
    // 至少有一个模块成功，可以用旧数据查询（最终一致性）
    return Some(partial_results);
}

// 场景 6：获取需要重试的模块
let to_retry = state.get_modules_to_retry();
for (module, next_attempt) in to_retry {
    if next_attempt <= Utc::now() {
        retry_module(module).await?;
    }
}
```

---

## 协作模式

### 典型处理流程

```
1. 应用层发起请求
   └─ IndexOrchestrator.index_files() 或 HotUpdateCoordinator.process()

2. 协调器检查操作能否运行 [OPERATION 层]
   └─ OperationCoordinator.get_active_operation()
   └─ 检查：是否有全量索引正在运行？

3. 协调器创建操作检查点 [OPERATION 层]
   └─ CheckpointManager.create_checkpoint()
   └─ 持久化到 SQLite，支持崩溃恢复

4. 处理器逐个处理文件 [INDEX_STATE 层]
   ├─ UpdateStateTracker.create_update() 创建文件状态
   ├─ 调用各个模块处理器
   └─ 对每个模块：
       ├─ 如果成功：state.mark_module_success(module)
       └─ 如果失败：state.mark_module_failed(module, error)
           → 自动进入 Retrying（指数退避）或 DeadLetter

5. 保存文件处理进度 [OPERATION 层]
   └─ CheckpointManager.save_file_checkpoint()
   └─ 持久化 FileUpdateState 到 DB

6. 如果有模块进入 DeadLetter [OPERATION 层]
   └─ CheckpointManager.record_module_retry()
   └─ 持久化到 ModuleRetryRecord，支持后续手工恢复

7. 崩溃后恢复 [OPERATION + INDEX_STATE 层]
   ├─ OperationCoordinator.initialize() 加载持久化数据
   ├─ RecoveryManager 重建检查点
   ├─ 从 CheckpointRecord 恢复 OPERATION 层状态
   └─ 重新加载 FileUpdateState 和 ModuleRetryRecord
       └─ UpdateStateTracker 重建内存状态用于重试
```

### 关键交互点

#### 交互 1：启动操作

```rust
// operation 层：检查和创建操作
let op_id = "op_202501211230";
coordinator.request_full_index(op_id.clone(), "/project".to_string()).await?;

// index_state 层：初始化文件状态
let tracker = UpdateStateTracker::new();
let operation_id = tracker.start_full_index(
    files.len(),           // 总文件数
    100,                   // 批大小
    "/project".to_string(),
).await;
```

#### 交互 2：处理文件

```rust
// 获取文件状态
let mut state = tracker.get_state(&file_path).await
    .ok_or("State not found")?;

// 处理模块
for module in ModuleType::all() {
    match process_module(&file_path, module).await {
        Ok(_) => {
            // index_state 更新
            state.mark_module_success(module);
            tracker.mark_success(&file_path, module).await?;
        }
        Err(e) => {
            // index_state 更新（自动重试）
            state.mark_module_failed(module, e.to_string());
            tracker.mark_failed(&file_path, module, e.to_string()).await?;
            
            // 如果进入 DeadLetter，operation 层记录
            if state.get_module_state(module).state == ModuleUpdateState::DeadLetter {
                checkpoint_manager
                    .record_module_retry(
                        &active_op.operation_id,
                        &file_path.to_string_lossy(),
                        module.as_str(),
                        &e.to_string(),
                    )
                    .await?;
            }
        }
    }
}

// operation 层保存进度
checkpoint_manager.save_file_checkpoint(&checkpoint_record).await?;
```

#### 交互 3：恢复

```rust
// operation 层：恢复操作和检查点
let unfinished = checkpoint_manager.get_unfinished_operations().await?;

for checkpoint in unfinished {
    // index_state 层：重建文件状态
    let files = checkpoint_manager.get_batch_files(
        &checkpoint.operation_id,
        checkpoint.current_batch_index,
    ).await?;
    
    for file in files {
        if file.overall_status != OverallStatus::Completed {
            // 重新加载模块状态用于重试
            let mut state = FileUpdateState::new(
                file.file_path.clone(),
                1,  // 新版本
                FileChangeType::Modified,
            );
            // ... 重新处理
        }
    }
}
```

---

## 常见问题（FAQ）

### Q1：何时使用 OperationType vs IndexOperationType？

**使用 OperationType 当：**
- 检查"一个操作是否可以开始"（调度决策）
- 存储操作的类型用于恢复（崩溃后）
- 实现全局约束（防止并发冲突）

**使用 IndexOperationType 当：**
- 追踪"一个文件在什么上下文中处理"（处理决策）
- 需要从检查点恢复文件处理（批大小、触发源等）
- 需要版本控制（防止旧更新覆盖）

### Q2：为什么 FileUpdateState 和 ModuleRetryRecord 都有重试信息？

**FileUpdateState.ModuleUpdateRecord**（内存）
- 用于文件处理过程中的快速查询
- 轻量级，支持快速状态转换
- 包含：retry_count, error_message, state, last_attempt

**ModuleRetryRecord**（数据库）
- 用于崩溃恢复后重新加载
- 持久化，支持跨应用重启
- 包含：同样的字段 + 时间戳用于调度重试

**关系**：
```
处理失败 → FileUpdateState 标记为 Retrying
        → ModuleRetryManager.record_retry() 持久化到 ModuleRetryRecord

崩溃恢复 → 加载 ModuleRetryRecord
        → 重建 FileUpdateState 用于重试
```

### Q3：如果 operation 和 index_state 都需要修改某个状态怎么办？

**原则：分工负责**

operation 模块：只修改持久化的 CheckpointRecord（DB）
- `mark_operation_completed()`
- `update_checkpoint_status()`
- `update_current_batch_index()`

index_state 模块：只修改内存的 FileUpdateState
- `mark_module_success()`
- `mark_module_failed()`
- `update_module_state()`

**如果需要协调**：
```rust
// 正确做法：operation 持久化，index_state 更新内存
coordinator.checkpoint_manager().save_file_checkpoint(&record).await?;
tracker.mark_success(&file_path, ModuleType::Summary).await?;

// 错误做法：混淆职责
tracker.update_operation_metadata(...)?;  // ✗ 不应该
coordinator.update_file_state(...)?;      // ✗ 不应该
```

### Q4：性能上有什么区别？

| 操作 | operation | index_state | 速度 |
|-----|-----------|-----------|------|
| 查询状态 | SQLite 查询 | 内存 HashMap | index_state 快 |
| 更新状态 | 事务提交 | 直接修改 | index_state 快 |
| 恢复 | 从 DB 加载 | 从 DB 重建 | 类似 |
| 并发 | 受限（单一活跃） | 支持多文件并行 | index_state 灵活 |

**结论**：
- index_state（内存）用于实时处理（快速）
- operation（DB）用于持久化和恢复（可靠）

### Q5：我需要实现一个新的 UpdateProcessor，应该如何处理状态？

**模板代码**：

```rust
pub struct MyUpdateProcessor {
    tracker: Arc<UpdateStateTracker>,
    checkpoint_manager: Arc<CheckpointManager>,
}

impl MyUpdateProcessor {
    pub async fn process_file(&self, file_path: &Path) -> Result<()> {
        // 1. 获取 index_state 层的文件状态
        let mut state = self.tracker.get_state(file_path).await
            .ok_or("State not found")?;
        
        // 2. 处理各个模块
        for module in ModuleType::all() {
            // 跳过已完成
            if state.get_module_state(module).state == ModuleUpdateState::Success {
                continue;
            }
            
            // 标记为处理中
            state.update_module_state(module, ModuleUpdateState::Updating);
            self.tracker.update_module_state(file_path, module, ModuleUpdateState::Updating).await?;
            
            // 执行处理
            match self.execute_module(file_path, module).await {
                Ok(_) => {
                    state.mark_module_success(module);
                    self.tracker.mark_success(file_path, module).await?;
                }
                Err(e) => {
                    state.mark_module_failed(module, e.to_string());
                    self.tracker.mark_failed(file_path, module, e.to_string()).await?;
                    
                    // 如果进入死信，持久化到 operation 层
                    if state.get_module_state(module).state == ModuleUpdateState::DeadLetter {
                        if let Some(op_id) = self.tracker.current_operation_id().await {
                            self.checkpoint_manager
                                .record_module_retry(&op_id, &file_path.to_string_lossy(), module.as_str(), &e.to_string())
                                .await?;
                        }
                    }
                }
            }
        }
        
        // 3. 保存最终检查点（operation 层持久化）
        let checkpoint_record = convert_to_db_record(&state);
        self.checkpoint_manager.save_file_checkpoint(&checkpoint_record).await?;
        
        Ok(())
    }
}
```

---

## 总结

| 方面 | operation | index_state |
|-----|----------|-----------|
| **问题** | "这个操作能运行吗？" | "这个文件处理到哪里了？" |
| **范围** | 全局 | 文件 |
| **生命周期** | 长期（跨应用重启） | 短期（文件处理期间） |
| **持久化** | 重要（故障恢复） | 轻（只记录失败） |
| **并发** | 严格约束 | 高度并行 |
| **API** | 协调器 | 处理器 |

**记住**：它们是互补的，不是竞争的。operation 提供全局框架，index_state 提供本地执行。
