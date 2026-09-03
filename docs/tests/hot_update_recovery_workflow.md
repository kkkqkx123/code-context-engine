# 热更新与断点恢复测试方案

## 1. 测试目标

| 目标 | 说明 |
| ---- | ---- |
| T1 整体工作流 | 验证「全量索引 → 热更新 → 查询」全链路各阶段产物与数据一致性 |
| T2 恢复数据获取 | 验证 resume 能从 checkpoint 获取所需数据（parsed_file / summary / 模块进度 / 指纹），无需重新解析 |
| T3 避免重复工作 | 验证 resume 不重复解析、不重复嵌入、不重复生成摘要/导出 |
| T4 正确性回归 | 覆盖已知缺陷（P1 模块进度标记失效、P2 恢复类型过滤、P3 检查点校验）的回归用例 |

本文档配套 `docs/plan/hot_update_recovery_improvements.md` 的修复方案，测试用例编号引用其中的问题编号。

---

## 2. 测试分层与位置

| 层 | 位置 | 内容 |
| -- | ---- | ---- |
| 单元测试 | 各 crate 内 `#[cfg(test)]`（沿用现有 mod tests 约定） | 标记生命周期、过滤逻辑、校验逻辑、指纹计算 |
| e2e 工作流 | `crates/cce_e2e_tests/tests/e2e/hot_update/`（新增 `hot_update_resume_recovery.rs`，在 `tests/e2e/hot_update.rs` 挂载） | 崩溃注入矩阵、resume、全链路一致性 |
| 回归 | `tests/`（根目录，如 `symbol_table_coordinator_integration.rs` 同类） | 与既有数据层集成回归保持一致 |

用例编号沿用现有命名：`TC-E2E-WF-xxx`（工作流）、`TC-E2E-RCV-xxx`（恢复）、`TC-E2E-DATA-xxx`（数据获取）。

运行方式：

```shell
cargo test -p cce_e2e_tests --test workflow          # e2e 工作流套件
cargo test -p cce_orchestrator --lib                 # orchestrator 单元测试
cargo clippy --all-targets --all-features && cargo fmt
```

---

## 3. 测试基础设施

### 3.1 现有可复用组件

- `EmptyFixture` + `IndexWorkflowTest`（e2e helper）：临时目录、初始索引；
- `MockEmbeddingServer`：确定性向量服务，可配置到 embedding provider，断言向量写入无需外部 LLM；
- summary 使用 rule-based 生成器（确定性、无 LLM），避免外部依赖；
- 各存储后端的 in-memory SQLite 构造（`SqliteClient::in_memory`）。

### 3.2 新增：崩溃模拟器（helper）

提供「阶段化执行 + 中断重建」能力，封装三类操作：

1. **阶段执行**：调用 coordinator / orchestrator 执行到指定阶段后丢弃（drop），保留磁盘与 SQLite 中间态；
2. **状态断言**：断言中间态（checkpoint 记录、manifest 状态、candidate_ready、epoch、module_progress 列）；
3. **重建恢复**：用同一 project_id + 同一数据库路径重建 `HotUpdateCoordinator`（复用 `with_checkpoint_manager` 接线），执行 `run_operation` 完成恢复，返回最终结果与发布态。

崩溃点的枚举驱动表（见 5.1），同一 harness 覆盖 crash 与 abort 两条路径：crash 直接 drop；abort 通过「让某个处理器失败」触发（如指向不可写路径、或 mock 存储失败）。

### 3.3 新增：解析/嵌入计数探针

为「避免重复解析/重复嵌入」提供可断言的证据：

- **解析计数探针**：`FileProcessor` / `ParseCoordinator` 增加测试专用构造入口，注入 `Arc<AtomicUsize>` 统计实际触发解析（tree-sitter 运行）的次数；恢复路径复用 envelope 时不递增；
- **嵌入计数探针**：embedding 处理器测试入口注入计数，统计实际向量写入的批次；
- **摘要生成计数探针**：summary 处理器统计实际生成次数（区别于从 checkpoint 复用）。

计数探针仅通过测试专用构造暴露，不影响生产代码路径。

### 3.4 新增：发布态断言工具

- active manifest 断言：`data_epoch`、`relation_epoch`、`operation_id`；
- 数据代断言：指定 epoch 下 chunks / entities / file_summaries / files 的行数与内容；
- 查询断言：BM25-only 路径（默认优先，遵循 workflow README 约束）命中新增符号；向量路径在 mock embedding server 下断言点数与内容 hash 对应关系。

---

## 4. T1：整体工作流验证用例

| 编号 | 用例 | 步骤与断言 |
| ---- | ---- | ---- |
| TC-E2E-WF-001 | 全链路增量 | 建项目 → full index（断言 active manifest、chunks、entities、导出文档）→ 修改一个文件 → 热更新（run_operation + 完整处理器链）→ 断言：仅受影响文件的实体/向量/BM25/摘要更新，未变文件数据保持、epoch 从 N 切到 N+1、查询能命中新符号 |
| TC-E2E-WF-002 | 删除链路 | 删除文件 → 热更新 → 断言：发布代中该文件相关数据（向量/BM25/chunks/摘要/导出文档）全部移除 |
| TC-E2E-WF-003 | 新增链路 | 新增文件 → 热更新 → 断言：新文件可被检索，其他文件数据不变 |
| TC-E2E-WF-004 | 空操作路径 | 无任何变化 → 热更新 → 断言：快速返回、不产生新 epoch、不写 checkpoint |

补充：WF-001 在 relation 开启时追加断言「关系快照随 epoch 发布、调用链查询命中」。

---

## 5. T2/T3：恢复与数据获取验证用例

### 5.1 崩溃点矩阵（TC-E2E-RCV-001 系列）

每个崩溃点独立用例：构建中间态 → 重建 → resume → 断言完成与数据完整。

| 崩溃点 | 中间态特征 | 恢复后断言 |
| ------ | ---------- | ---------- |
| RC 1：解析后、候选克隆前 | checkpoint 已写、manifest 未 building | 重新克隆；全部文件重新处理；无遗留数据 |
| RC 2：候选克隆中 | manifest building 且 `candidate_ready=0` | 不收养 → 重新克隆（安全回退） |
| RC 3：embedding 完成、bm25 未完成 | 候选可收养、module_progress 部分存在 | 收养候选；已嵌入文件跳过嵌入、bm25 只补未完成文件；发布代数据完整 |
| RC 4：abort 后（模块失败） | manifest 置 failed、module_progress 残留（P1） | **不收养 → 清除模块进度 → 全部模块重做 → 发布代数据完整无缺失**（P1 回归） |
| RC 5：activate 后、hash 提交前 | manifest active、checkpoint in_progress | 短路完成：补 hash 提交、标记完成、不重复发布 |
| RC 6：全部完成 | checkpoint completed | resume 不触发（跳过恢复） |

### 5.2 避免重复工作断言（TC-E2E-RCV-002 系列）

| 编号 | 用例 | 断言方式 |
| ---- | ---- | -------- |
| TC-E2E-RCV-002 | 恢复不重复解析 | RC3 场景：解析计数探针不递增（envelope 复用）；磁盘 hash 校验通过 |
| TC-E2E-RCV-003 | 恢复不重复嵌入 | RC3 场景：嵌入计数探针只覆盖未完成文件；已嵌入文件跳过 |
| TC-E2E-RCV-004 | 摘要复用 | resume 后 summary_config_fingerprint 一致 → 摘要不重新生成（生成计数不变）且导出内容一致 |
| TC-E2E-RCV-005 | 导出跳过 | render_fingerprint 一致 + 导出文档存在 → 文档 mtime 不变 |
| TC-E2E-RCV-006 | 磁盘内容变化强制重解析 | resume 前修改磁盘文件 → 内容 hash 不匹配 → 重新解析（计数递增）、模块重做 |
| TC-E2E-RCV-007 | 配置变化强制重做 | resume 前改变 chunking 配置（配置指纹变化）→ 该模块重做，其他模块跳过 |
| TC-E2E-RCV-008 | 类型/根目录过滤（P2） | 同项目存在 in_progress 的 full_index 检查点 → 热更新 resume 不误选它（按 operation_type/root_dir 过滤） |

### 5.3 恢复数据获取断言（TC-E2E-DATA 系列）

验证 resume 重建的 `BatchChangeResult` 携带全部所需数据：

| 编号 | 数据 | 断言 |
| ---- | ---- | ---- |
| TC-E2E-DATA-001 | parsed_file | 每个文件 envelope 反序列化成功、`is_compatible()` 为真、change_type 与 checkpoint 一致 |
| TC-E2E-DATA-002 | 磁盘校验 | `disk_matches`（磁盘 hash == checkpoint content_hash）判定正确 |
| TC-E2E-DATA-003 | 摘要 | `file_summary` 随 envelope 恢复，summary 配置指纹正确携带 |
| TC-E2E-DATA-004 | 模块进度 | `module_progress` map 与持久化 JSON 一致（embedding/bm25/summary 三键齐全） |
| TC-E2E-DATA-005 | 导出标记 | `already_exported` 标记与文件系统存在性一致；render_fingerprint 正确恢复 |
| TC-E2E-DATA-006 | 版本不兼容 | envelope 版本字段篡改 → 走重新解析路径（计数递增） |
| TC-E2E-DATA-007 | 陈旧 checkpoint 兜底 | 无 module_progress / 无指纹的旧 checkpoint → 保守全量重做 |

---

## 6. 单元测试补充

随修复方案同步补充 orchestrator 层单元测试（`crate 内 #[cfg(test)]`）：

| 关联问题 | 测试内容 |
| -------- | -------- |
| P1 | `is_candidate_adoptable` 判定表（building+candidate_ready+epoch 合法 → true；failed/非 building → false）；resume 清除 module_progress 的 SQL 行为（清除后 `read_module_progress` 为空） |
| P2 | `validate_and_recover_checkpoint` 类型/根目录过滤：多类型 in_progress 混合时只命中匹配项 |
| P3 | `validate_checkpoint` 补 file_list_hash 与边界比对：hash 不同 / 首尾文件不同 → Err；一致 → Ok |
| P4 | `recover_unfinished_operations` 时效过滤：超窗口 in_progress 标记 Failed 不入队；窗口内正常入队 |
| P5 | 批量 hash 读取与逐文件查询结果等价（随机文件集对比，含 epoch 隔离） |
| P6 | `check_changes` 增删同批场景不再漏检删除 |

---

## 7. 实施顺序

1. **Phase 0**：测试基础设施 —— 崩溃模拟器、计数探针、发布态断言工具（3.2–3.4），先落地 RC3 最简场景验证 harness 可用；
2. **Phase 1**：TC-E2E-RCV-004 全矩阵（含 RC4 复现 P1 —— 修复前应失败，作为回归基线）；
3. **Phase 2**：TC-E2E-DATA 系列 + RCV-002/003（重复工作断言）；
4. **Phase 3**：TC-E2E-WF 系列（整体工作流）；
5. **Phase 4**：单元测试补齐（第 6 节）与全量回归。

RC4 建议在 P1 修复提交前先跑一次并记录失败，作为「缺陷 → 修复 → 转绿」的回归闭环。

---

## 8. 约束与注意

- 遵循 `tests/workflow/README.md` 约束：工作流测试默认走 BM25-only 路径，不依赖 LLM 实际调用；输出快照验证不放入 workflow；
- 崩溃模拟通过「丢弃实例 + 重建」实现，不注入 panic/信号，保证测试确定性；
- 涉及 epoch/发布态的断言一律读库验证，不依赖内存状态；
- 时间类等待（debounce、风暴阈值）使用最小配置或绕过（如直接调用 `update()` 而非 watcher 事件循环），避免 flaky。
