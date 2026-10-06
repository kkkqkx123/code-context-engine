# 死信队列可观测性与手动处理完善方案

## 背景与目标

现有系统存在三套重试/死信机制：查询层 `RetryQueue`（内存、drain 时超限静默丢弃）、索引层模块级重试与死信状态（`ModuleUpdateState::DeadLetter`，携带错误码与 truncated 标记）、死信截断重试执行器（周期扫描 + 手动全量 pass）。

主要缺陷：

1. 死信不可见：`get_dead_letters()` 仅为内部方法，无列表 API，前端无法得知哪些文件死信及原因。
2. 粒度缺失：重试只有全量 pass，无法单文件重试；无法"忽略/确认"永久死信文件，其永远留在候选集合。
3. 查询 RetryQueue 超限查询仅 warn 日志后丢弃，不可观测。
4. 查询 RetryQueue 纯内存，重启即丢（本方案不解决持久化，仅解决可观测性，持久化另立方案）。

## 修改内容

### 1. 死信列表 API

- `cce-orchestrator`：`UpdateStateTracker` 已有 `get_dead_letters()`，无需修改；在 `cce-server-shared` 的 `CodeContextEngine` 增加只读方法 `dead_letters(project_id)`，内部通过 orchestrator 的 state tracker 返回 `FileUpdateState` 列表（含 file_path、模块状态、retry_count、error_code、error_message、truncated）。
- `cce-api`：新增响应模型 `DeadLetterListResponse`（项目级列表，每项含文件路径、各模块失败摘要）。
- `cce-server`：新增 handler `handle_dead_letter_list`，路由 `GET /api/project/{id}/dead-letters`，注册到 openapi。

### 2. 单文件死信操作

- `index_state`：`ModuleUpdateRecord` 增加持久化字段 `acknowledged: bool`（serde 默认 false，兼容存量记录）。死信且被 acknowledge 的记录退出 truncate 候选集合与死信列表的"待处理"语义，但仍可查询（不改变 is_queryable 逻辑）。
- `index_state_tracker`：新增两个方法——
  - `acknowledge_dead_letter(file, module)`：将指定文件指定模块的死信标记为 acknowledged；支持 `all` 简写对所有模块生效。文件若任一模块仍为未确认死信则保留在死信列表，全部确认后从列表消失。
  - `get_dead_letters()` 过滤掉全部模块均已确认的文件。
- `retry_dead_letter_with_truncation` 的候选收集同步过滤 acknowledged 记录。
- 单文件重试：为避免大改执行器，单文件重试通过"临时重置该文件 Embedding 模块的 truncated 标记之外的状态"实现成本过高，改为提供 `reset_dead_letter(file, module)`：将死信记录重置为 `Failed`（retryable 语义，交由常规重试路径处理）；同时新增手动入口 `retry_dead_letters_for_files(project_id, files)`：在候选收集中按传入文件路径过滤，仅 pass 指定文件（沿用截断规则）。
- `cce-api`：新增 `DeadLetterFileActionResponse`；`cce-server` 新增 handler：
  - `POST /api/project/{id}/dead-letters/retry`（body：可选 files 列表，缺省全量）
  - `POST /api/project/{id}/dead-letters/acknowledge`（body：file + 可选 module）
- CLI 暂不增加子命令（后续按需）。

### 3. 查询 RetryQueue 超限可观测

- `RetryQueue`：`drain_ready()` 中超过 `max_retries` 的条目不再直接丢弃，移入内部 `dead: Vec<QueuedQuery>`（含 query 文本、入队时间、retry_count），保留条数上限（复用 max_queue_len，满则丢最旧）。
- 新增方法：`dead_len()`、`dead_snapshot()`（返回查询文本与元数据的只读列表）、`clear_dead()`。
- `cce-server-shared` engine 的 retry queue 状态接口透出 dead 计数；`RetryQueueStatusResponse` 增加 `dead_count` 字段；新增 `DELETE /api/retry-queue/dead` 清空死信。
- 周期/手动 process 重放不包含 dead 条目（避免无限循环）；用户修复服务后可手动 clear 或重发查询。

### 4. 不做的事

- 查询 RetryQueue 持久化到 SQLite：另行立项。
- 死信的自动优先级/退避策略调整：现状已够用。
- CLI 死信子命令：后续按需。

## 验证

- 单测：tracker 的 acknowledge/reset 行为、候选过滤、RetryQueue dead 收集与清理。
- `cargo clippy --all-targets --all-features`、相关模块 `cargo test`。
