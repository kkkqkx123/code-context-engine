# 死信队列截断重试设计方案（修订版）

## 背景

嵌入请求失败时，`UpdateStateTracker` 的重试机制（`Failed → Retrying` 指数退避）耗尽重试次数（3 次）后，模块进入 `DeadLetter` 终态，等待人工处理。该机制对**确定性错误**无效：以 SiliconFlow bge-m3 为例，单条输入超过 8192 tokens 时返回 HTTP 400（code 20015），其响应无任何语义信息，无法与真正的参数错误区分。确定性 400 每次重试必然再次失败，死信队列只堆积不自愈。

同时，token 估算器对代码密集文本存在系统性低估（已通过收紧 `SYMBOL_FACTOR` 缓解），但估算终究是近似——极端情况下仍可能有超限 chunk 流入嵌入请求。

## 对原设计的三处修订

调研代码事实后，原方案有三处前提不成立，本方案据此修订：

1. **不新增 `TruncatingRetrying` 状态**。`UpdateStateTracker` 是纯状态账本（不持有 embedder/storage/FileProcessor），且 `get_pending_retries()` / `get_modules_to_retry()` 在生产代码中零调用——"截断后回归正常重试轨道"没有执行驱动。执行器改放**编排层** `IndexOrchestrator`（它已持有 `file_processor` 与 `storage_coordinator`），重处理结果直接走现有 `mark_success` / `mark_failed` 轨道。
2. **死信模块的 chunk 正文不在 SQLite**（嵌入成功后才写记录，硬 400 直接中断），无法从库中取回原文重嵌。执行器改为从磁盘重建：`FileProcessor::rechunk_file_from_disk` 读取源文件并做**内容哈希校验**（与 files 表记录的 content_hash 比对，漂移即跳过），复用热更新 drift sweep 的既有先例。
3. **不靠重试结果判断超限**。超限可本地判定：重建出的 Embedding 路径 chunk 用生产同款 `estimate_tokens` 估算，超过 provider 输入限额即为截断对象。截断范围收敛到 **Embedding 模块 only**——BM25 走 tantivy，无 8192-token 限制，截断只会损失召回。

## 设计目标

1. 死信模块具备**可选的、有损的**自动恢复手段（截断后重试），而非只能人工介入。
2. 截断是有损操作，生产索引默认无损——截断重试必须是显式启用的恢复路径（定时扫描受开关控制；手动入口不受开关限制）。
3. 有损结果必须**可感知、不可反复**：`truncated` 标记贯穿 chunk 存储链路；每个文件/模块至多一次截断尝试。

## 执行器（编排层）

`IndexOrchestrator::retry_dead_letter_with_truncation()` 单次扫描处理，产出 `{ retried, succeeded, still_failed }` 报告：

1. 从 tracker 取候选：`state == DeadLetter && module == Embedding && !truncated`。
2. 前置条件：SQLite 元数据存储与 embedder 已配置（嵌入器缺失直接报错——否则只写记录的存储路径会伪造成功）；将存储协调器的 epoch **对齐到当前激活代际**（恢复出的编排器仍停在旧 epoch），并清除上一次操作遗留的 checkpoint 上下文。
3. 逐文件处理，**先消耗唯一一次截断配额**（`set_module_truncated` 前置），再做任何可能跳过的检查——缺失、漂移、解析失败的模块同样不再进入候选队列：
   - 记录的绝对/相对路径归一化为项目根相对路径；从 files 表取当前 content_hash；
   - `rechunk_file_from_disk`（哈希校验）重建 chunks；
   - 对 Embedding 路径 chunk 应用截断策略（见下），置 `truncated = true` 并刷新 `token_count`；
   - `store_vectors_batched` 重新嵌入并写入**激活代际**。
4. 成功 → `mark_success(Embedding)`；失败 → `mark_failed`（模块保持 DeadLetter）。

**幂等性**：chunk ID 由 `group_id + path + 序号` 确定性生成，SQLite chunks 表按 `(project_id, epoch, chunk_id)` UPSERT，Qdrant 点 ID 含相同 chunk 标识——重写入天然覆盖旧记录，无需先删后插。

## 截断策略

与 raw pipeline 基线（`baselines/full_pipeline_raw.rs`）共用 `cce-utils::token_estimation` 的共享工具：

- 估算不超限直接原样返回；
- 超限时按 80% 比例截短（行边界对齐后重估），循环直到达标；
- 触及最小长度下限（2000 字节）跳出循环，最后用 `find_split_point` 做一次绝对兜底；
- 每次截断打印日志：文件、chunk 标识、限额、截断前后 token/字节数。

## 配置

- `indexer.dead_letter_truncate_retry`（默认 false）：定时扫描开关；手动 API/CLI 不受此开关限制。
- `indexer.embed_input_token_limit`（默认 8192）：截断阈值（provider 输入上限）。
- `orchestrator.dead_letter_retry_interval_secs`（默认 300s）：定时扫描周期。

## 触发入口

1. **定时扫描**：服务端后台任务（`start_dead_letter_retry_task`）按周期遍历**已缓存**的编排器实例（不新建项目实例），跳过开关关闭与正被占用（try_lock 失败，即索引进行中）的项目。
2. **手动命令**：`POST /api/project/{id}/dead-letter/retry` 与 CLI `index retry-dead-letter -P <id>`，引擎层锁住项目编排器后执行同一路径；忽略开关，但仍受"至多一次截断尝试"约束。

## truncated 标记贯穿存储

- `ChunkedResult.truncated: bool`（解析→chunk 层）；
- Qdrant `Payload.truncated: Option<bool>`（仅在有值时序列化）；
- SQLite chunks 表新增 `truncated INTEGER NOT NULL DEFAULT 0` 列（schema 版本提升，开发期无向后兼容负担，不匹配即要求全量重建）；
- 检索侧 `SearchResult.truncated: bool`，稠密检索（payload）与实体富化（ChunkRecord）两条链路均填充——下游可感知该结果为有损嵌入。

## 数据一致性

- 截断重试成功后，该 chunk 的向量是**有损结果**，标记随存储链路持久化，检索侧可感知。
- 截断重试失败后回到 `DeadLetter`（`mark_failed` 对死信模块保持死信），不修改已入库数据。
- 截断重试只作用于失败模块自身（Embedding），不触发同文件 BM25 模块联动。
- 恢复写入落在**当前激活代际**，与查询读取一致。

## 不做的事

- 不在错误分类层区分"400=超限"——报错无语义，无法可靠分类；超限判定改为本地估算比对，与重试结果无关。
- 不把截断作为自动默认行为——有损恢复必须显式启用。
- 不对已截断过的模块反复截断——避免逐次削短形成静默数据退化。
- 不为 BM25 模块做任何截断——tantivy 无 provider token 限制，截断无收益只有召回损失。
- 不新增状态机状态——死信自愈是编排层的一次性修复动作，不是模块生命周期的新阶段。
