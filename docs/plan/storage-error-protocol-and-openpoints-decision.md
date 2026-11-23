# 存储错误协议结构化改造与剩余开放点设计决策（第三轮）

## 0 文档信息

| 项 | 值 |
| --- | --- |
| 主题 | ① `UNIQUE constraint failed` 文本识别专项分析；② 上一轮 `storage-backend-abstraction-symmetry-followup.md` 第 11 节 O-A~O-E 的设计决策与落地 |
| 仓库 | `kkkqkx123/code-context-engine`，递归拉取含全部子模块 |
| 环境 | Rust 1.93.0（workspace 声明 1.97，用 `--ignore-rust-version`）；ES/PG/Qdrant 外部服务未启动 |
| 约束 | 遵循 `AGENTS.md`：代码与配置用英文、文档用简体中文、禁 `unwrap`、少用 `dyn`、代码注释不得引用计划编号 |
| 与既有文档关系 | 承接 `storage-backend-abstraction-symmetry-followup.md`（S-01~S-11、O-A~O-E）；本轮推翻其"对 O-E 错误文本维持现状"的结论，结构化改造错误协议 |
| 文档约定 | 每条结论给出 `路径:行号`；diff 仅示意要点 |

## 1 背景与问题缘起

上一轮分析把 `UNIQUE constraint failed` 初判为"有意实现的跨后端错误协议，不是缺陷"，据此维持现状。本轮应要求重新审查该设计的健壮性，并逐项拍板 O-A~O-E。核查后发现文本协议存在**语义不对称、抽象泄漏、脆弱匹配**三类实质问题，因此推翻旧结论并实施结构化改造。

## 2 UNIQUE constraint failed 完整链路（事实）

| 环节 | 位置 | 现状 |
| --- | --- | --- |
| 契约声明 | `cce-storage-common/src/metadb.rs:78-85` | 注释要求唯一冲突的错误文本包含 `UNIQUE constraint failed` |
| SQLite 产生 | `cce-storage-metadb-sqlite/src/repo/project_repo.rs:25-37` | `StorageError::insert("projects", format!("Failed to insert project: {e}"))`，rusqlite 原生文本透传 |
| PG 产生 | `cce-storage-metadb-pg/src/error.rs:25-31` | 检测 SQLSTATE `23505` 后**主动伪造** SQLite 文案 `format!("UNIQUE constraint failed: {error}")` |
| 上层识别① | `cce-server-shared/src/services/project.rs:116-129` | `msg.contains("UNIQUE constraint failed")`，`msg` 为 `e.to_string()` |
| 上层识别② | `cce-server/src/api/handlers/project/management.rs:96-105` | 对 `ProjectCreateError::Storage(String)` 再做一次 `m.contains(...)` |
| 错误类型 | `cce-types/src/types/error/storage.rs:12-78` | 无专门唯一冲突变体；`Validation` 兼作唯一冲突容器 |

`e.to_string()` 的实态：SQLite 为 `Insert into projects failed: Failed to insert project: UNIQUE constraint failed: projects.name`，PG 为 `Validation failed: UNIQUE constraint failed: ...`。两处 `contains` 依赖这段经过两层 `format!` 包装的文案。

## 3 当前设计是否有问题——分析结论

**结论：有问题。依赖文本统一识别是脆弱且语义错位的设计。** 四项独立证据：

| 编号 | 问题 | 事实依据 | 影响 |
| --- | --- | --- | --- |
| U-1 | **语义不对称**（新发现） | SQLite 映射为 `StorageError::Insert`（`project_repo.rs:37`），`storage.rs:225` 判为可重试；PG 映射为 `Validation`（`error.rs:29`），`storage.rs:260` 判为不可重试 | 同一"项目重名"在两个后端的可重试性相反；`contains` 绕过了这层不一致，但错误分类消费者会读到矛盾信号 |
| U-2 | **抽象泄漏 / 反向适配** | PG 主动把 `23505` 格式化成 SQLite 私有文案（`error.rs:29`），远程实现必须知道并模仿本地驱动的文案 | 新增第三种后端仍要复制该魔法字符串；本地文案变更会波及全体后端 |
| U-3 | **文本协议脆弱** | 上层匹配 `e.to_string()`（`project.rs:119`、`management.rs:98`），其值是 `StorageError::Insert`/`Validation` 的 `#[error]` 模板叠加两层 `format!` | 任一 `#[error]` 模板、`format!` 前缀、驱动文案变动都会让 409 静默退化为 500 |
| U-4 | **重复识别 + 类型丢失** | `ProjectCreateError::Storage(String)`（`project.rs:43`）把结构化错误摊平为字符串，`management.rs:98` 对其二次 `contains` | 冲突判定分散在两处；service 已转 `Conflict` 后，二次识别实为死代码 |

补充：`client.rs:480-494` 已有 `sqlite_primary_code` 结构化错误码实践（disk-full 用 primary code 13），说明项目本身已有正确范式，唯独 project insert 退化为文本。

## 4 设计决策：结构化错误协议

核心原则：**分类信号由产生错误的一端按原生结构化信号给出，消费端用类型判定，不解析文本。**

| 决策 | 内容 |
| --- | --- |
| D-1 | 在 `StorageError` 新增变体 `UniqueViolation(String)`，配套 `unique_violation()` 构造、`is_unique_violation()` 判定、`error_code()` 返回 `STORAGE_UNIQUE_VIOLATION`（`storage.rs:45-50,136-145,192`） |
| D-2 | 归类为永久性、不可重试：`is_retryable=false`、`is_permanent=true`（`storage.rs:234-237,271-279`），消除 U-1 的两端不一致 |
| D-3 | SQLite 端按结构化 code 判定：`SqliteFailure.extended_code & 0xFF == 19`（SQLITE_CONSTRAINT）→ `unique_violation`（`project_repo.rs:37-52`）；projects 表仅 `name`/`root_path` 两个 UNIQUE 约束（`schema/tables/core_tables.rs:9-12`），无 CHECK 等其它约束，判定精确 |
| D-4 | PG 端按 SQLSTATE `23505` 判定 → `unique_violation`（`error.rs:23-32`），删除文案伪造 |
| D-5 | 上层 service 用 `e.is_unique_violation()`（`project.rs:119`）；删除 handler 的二次文本识别（`management.rs:96-99`），因其上游 `Storage` 分支不再可能承载唯一冲突 |
| D-6 | 契约注释改为承诺结构化变体（`metadb.rs:78-85`），不再描述错误文本 |

兼容性：项目 `AGENTS.md` 明确无向后兼容要求；`UniqueViolation` 为新增变体，未删除 `Validation`，其它 `Validation` 构造点（约 40 处，`cce-codegraph`/`relation_publisher` 等）不受影响；`filter.rs:221-224` 的 `match` 有 `_` 兜底，安全。

## 5 剩余开放点 O-A~O-E 设计决策

| 编号 | 原开放点 | 决策 | 理由与落地 |
| --- | --- | --- | --- |
| O-A | 远程 fulltext 是否改为真正延迟发布 | **维持远程"stage 即发布"，但修正契约注释矛盾** | 原注释 `fulltext.rs:159-163` 称 "without publishing"，与默认实现（`:171` 立即 `batch_index`）和远程实现（`contract.rs:146-154`）冲突。改为诚实描述"可见性时点属后端特定、不纳入契约"（`fulltext.rs:159-166`）。真正延迟发布需 ES 环境验证失败补偿，独立立项 |
| O-B | 是否推翻 O-04 统一两端 FTS 语义 | **维持 O-04，不推翻** | 需 PG 启用 `tsvector` 并接受排序变化，影响既有查询行为，独立立项；本轮不动代码 |
| O-C | 是否扩充共享契约 suite 至全部方法 | **部分扩充（低风险、两端语义一致的方法）** | vector suite 补 `backend_name`/`delete_by_group_epoch`/`scroll_all_points`/`count_all_points`/`clear_collection`（`vector_backend_contract.rs:420-470`）；fulltext suite 补 `backend_name`/stage→commit→finish 往返（`fulltext_backend_contract.rs:394-435`）。generation/snapshot 状态机仍留待后续（需构造大量前置记录，风险高） |
| O-D | 是否拆分约 150 方法的组合 `MetadataStorage` | **维持现状** | 宏已生成 9 个窄域 trait（`metadb/contract.rs:14-128`）缓解调用侧依赖；进一步让调用方依赖窄 trait 会扩大改动面，收益不明确，独立立项 |
| O-E | 契约注释是否彻底去除后端差异描述 | **部分调整：仅改错误文本相关项** | `metadb.rs:78-85` 的错误文本描述随 D-6 改为结构化承诺；其余后端差异注释是对实际行为的诚实说明，删除会降信息量，维持 |

## 6 修复方案与落地对照（diff 要点）

### D-1/D-2 结构化错误变体

```diff
 // cce-types/src/types/error/storage.rs
+    #[error("Unique constraint violation: {0}")]
+    UniqueViolation(String),
+    pub fn unique_violation(reason: impl Into<String>) -> Self { ... }
+    pub fn is_unique_violation(&self) -> bool { matches!(self, Self::UniqueViolation(_)) }
+    Self::UniqueViolation(_) => "STORAGE_UNIQUE_VIOLATION",
-    Self::StorageFull(_) => false,
+    Self::StorageFull(_) | Self::UniqueViolation(_) => false,  // is_retryable
+    | Self::UniqueViolation(_)                                  // is_permanent
```

### D-3/D-4 两端结构化映射

```diff
-// sqlite project_repo.rs
-.map_err(|e| StorageError::insert("projects", format!("Failed to insert project: {e}")))?;
+  primary = SqliteFailure.extended_code & 0xFF;   // SQLITE_CONSTRAINT = 19
+  if primary == 19 { StorageError::unique_violation(...) } else { StorageError::insert(...) }
```
```diff
-// pg error.rs: SQLSTATE 23505
-return StorageError::validation(format!("UNIQUE constraint failed: {error}"));
+return StorageError::unique_violation("project name or root path already exists");
```

### D-5 上层判定

```diff
- let msg = e.to_string();
- if msg.contains("UNIQUE constraint failed") { ... }
+ if e.is_unique_violation() { ... }
```
handler 的 `Storage(m)` 分支移除二次 `contains`，统一返回 `STORAGE_ERROR`。

### O-A 契约注释 / O-C suite 补强

见第 5 节所述位置；suite 新增断言均选两端已实现且语义一致的方法，staging 往返前置一次 `commit_staged` 清空计数以避免依赖历史状态。

## 7 验证计划与结果

| 项 | 命令 | 结果 |
| --- | --- | --- |
| 针对性编译 | `cargo check --ignore-rust-version -p cce-types -p cce-storage-common -p cce-storage-bm25 -p cce-storage-metadb-sqlite -p cce-storage-metadb-pg -p cce-server-shared -p cce-orchestrator` | 通过（`Finished dev profile`，无 error/warning） |
| cce-server 编译 | `cargo check --ignore-rust-version -p cce-server` | 见第 9 节落地状态 |
| 测试 | `cargo test --ignore-rust-version -p cce-storage-common -p cce-storage-metadb-sqlite -p cce-types --tests` | 见第 9 节落地状态；远程分支因无 `CCE_TEST_*` 跳过 |
| 残留引用 | 全仓 `crates/**/*.rs` 检索 `UNIQUE constraint failed` | 0 处命中，生产代码文本判定与产生点已全部移除 |

## 8 风险与残余

- D-3 以 primary code 19 判定，前提是 `projects` 表除两个 UNIQUE 外无其它可触发 code 19 的约束；已核验 `core_tables.rs:9-12` 成立。若未来给 projects 增加 CHECK/FK，需同步细化 extended code 判定。
- D-5 删除 handler 二次识别的前提是：所有唯一冲突都在 service 层经 `is_unique_violation()` 转为 `Conflict`。已核验 `ProjectCreateError::Storage` 仅由 `project_path_exists` 失败与"非唯一的 project_insert 失败"两处构造（`project.rs:98,125`），不会承载唯一冲突。
- O-C 的 staging 往返断言依赖"两端 `commit_staged` 都返回本批计数"；已核验本地 `client.rs:316-336`（返回 pending）与远程 `contract.rs:156-159`（返回 drain 数）语义一致。若后续远程改为真正延迟发布，需同步调整该断言。
- 远程三分支仍靠 `CCE_TEST_*` 环境变量启用，本环境无服务，涉及外部服务的语义需在 CI 验证。

## 9 落地状态汇总

| 编号 | 状态 | 落地文件 |
| --- | --- | --- |
| D-1/D-2 | 已落地 | `crates/core/cce-types/src/types/error/storage.rs` |
| D-3 | 已落地 + 单测 | `crates/infra/cce-storage-metadb-sqlite/src/repo/project_repo.rs` |
| D-4 | 已落地 | `crates/infra/cce-storage-metadb-pg/src/error.rs` |
| D-5 | 已落地 | `crates/app/cce-server-shared/src/services/project.rs`、`crates/app/cce-server/src/api/handlers/project/management.rs` |
| D-6 | 已落地 | `crates/infra/cce-storage-common/src/metadb.rs` |
| O-A | 已落地（注释） | `crates/infra/cce-storage-common/src/fulltext.rs` |
| O-C | 已落地（部分） | `crates/infra/cce-storage-common/tests/vector_backend_contract.rs`、`fulltext_backend_contract.rs` |
| O-B/O-D/O-E（其余） | 维持现状 | 无代码改动，独立立项 |

编译验证：`cargo check --ignore-rust-version`（9 个相关 crate，含 cce-server）通过，无 error/warning。环境 rustc 1.93.0 低于 workspace 声明的 1.97，首次 check 被 MSRV 拦截，故加该标志；依赖经 rsproxy 镜像下载。

测试验证：`cargo test --ignore-rust-version -p cce-storage-common -p cce-storage-metadb-sqlite --tests` 全部通过——cce-storage-common lib 单测 17、fulltext 契约 2、metadata 契约 3、vector 契约 2，cce-storage-metadb-sqlite 单测 73（含新增的 `duplicate_project_insert_is_structured_unique_violation`），0 failed；另 `cce-types` 单测 218 passed。远程分支因未设置 `CCE_TEST_*` 按设计跳过（既有行为）。`crates/**/*.rs` 检索 `UNIQUE constraint failed` 为 0 命中。

## 10 测试覆盖新增

- vector：`backend_name`、`delete_by_group_epoch`（按 epoch 精确删除）、`scroll_all_points`、`count_all_points`、`clear_collection`（清空后集合仍在）。
- fulltext：`backend_name`、stage→commit→finish 往返（计数一致 + 提交后可检索）。
- sqlite：`duplicate_project_insert_is_structured_unique_violation`（重复插入返回 `UniqueViolation`，且不可重试）。

## 11 后续开放点

| 编号 | 事项 | 建议 |
| --- | --- | --- |
| O-A' | 远程 fulltext 真正延迟发布 | 需 ES 环境验证 stage→commit 的可见性与失败补偿，独立立项 |
| O-C' | 补 generation/snapshot 状态机契约测试 | 前置记录构造复杂，建议按域分批补，远程纳入独立 CI job |
| O-D' | 调用方改用窄域 trait 依赖 | 评估将 `MetadataStorage` 组合 trait 的依赖点替换为 9 个窄 trait 之一 |
| O-B' | 统一两端实体 FTS 语义 | 沿用既有 O-04，需 PG `tsvector` 与排序变更，独立立项 |
