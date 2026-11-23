# FTS 统一必要性评估与两端独立优化方案

## 0. 文档元信息

- 范围：`entities_search_fts`（`cce-storage-common/src/metadb.rs:491-498`）在 SQLite 与 PostgreSQL 两端的实现语义差异。
- 缘起：第四轮方案 `storage-openpoints-detailed-design.md` 的 O-B' 项，当时结论为"统一 FTS 语义、独立立项"。本轮重新评估该结论是否成立。
- 方法：只做事实核查与分析，本轮不修改代码。所有结论附 `file:line`。
- 核心结论：**统一不必要；"不统一"在当前架构下是合理设计，但当前状态尚不是"安全的不统一"，需先补齐三项前提（见第 6 节）。**

## 1. 问题陈述

本次要回答三个问题：

1. 两端 FTS 语义统一是否必要？
2. 若不必要，"不统一"是否在工程上可行、成立条件是什么？
3. 不做统一的前提下，两端各自还能做哪些独立优化？

需要澄清一个易混淆点：**语义检索（RAG 召回）走的是 vector + fulltext(BM25) 后端（`cce-orchestrator/src/index/vector_store/`），与本文讨论的 entity FTS 是两条独立链路**。后者服务于符号查找与 autocomplete（`cce-orchestrator/src/query/coordinator.rs:395-398`），不参与答案相关性排序。这直接决定了"统一"的收益上限。

## 2. 事实基础：契约与两端实现

### 2.1 契约层已显式声明差异

`cce-storage-common/src/metadb.rs:485-498`：

```rust
/// Full-text entity search scoped to one project and epoch.
///
/// The local branch serves SQLite FTS5; the remote branch serves a
/// portable substring match over name, signature and scoped name.
/// Rows whose file path is in `excluded_files` are dropped inside the
/// branch. An empty query after sanitization returns no rows.
fn entities_search_fts(...)
```

契约注释明确写出"local = SQLite FTS5 / remote = portable substring match"，并明确"空 query 返回空行"。**语义差异是被写进契约的既定设计，不是隐藏缺陷。**

### 2.2 本地实现：完整 FTS5 能力

| 环节 | 位置 | 事实 |
|---|---|---|
| FTS5 虚表 | `cce-storage-metadb-sqlite/src/schema/indexes.rs:180-184` | `entities_fts USING fts5(name, signature, content='entities', content_rowid='id')`，**仅索引 name、signature 两列** |
| 同步触发器 | `indexes.rs:186-202` | insert/delete/update 三个触发器；update 仅在 `name`/`signature` 变化时触发（`:197`） |
| 检索 SQL | `cce-storage-metadb-sqlite/src/repo/entity_repo.rs:259-276` | `entities_fts MATCH ?1`，query **原样透传**；`ORDER BY f.rank`（相关性排序）；epoch 在 SQL 内过滤 |
| 契约转发 | `cce-storage-metadb-sqlite/src/contract.rs:421-445` | 先取 `LIMIT` 条（`:431`），再在 Rust 层按 `excluded_files` 过滤（`:437-445`） |
| 本地单测 | `entity_repo.rs:1151-1173` | 覆盖前缀 `auth*`、精确名、signature 检索、项目隔离 |

即本地支持完整 FTS5 语法：前缀、短语、布尔、字段限定、NEAR、BM25 评分排序。

### 2.3 远程实现：可移植子串合取

| 环节 | 位置 | 事实 |
|---|---|---|
| token 清洗 | `cce-storage-metadb-pg/src/files.rs:1712-1730` | `split_whitespace` → 去引号/括号（`:1717`）→ 去尾部 `*`（`:1718`）→ 去 `\ % _`（`:1719`）→ 过滤 `AND/OR/NOT/NEAR`（`:1723-1726`）→ `take(MAX_FTS_TOKENS)`（`:1728`） |
| token 上限 | `files.rs:457` | `const MAX_FTS_TOKENS: usize = 8` |
| 检索 SQL | `files.rs:485-501` | 每个 token 一个 `AND (e.name ILIKE $n OR e.signature ILIKE $n OR e.scoped_name ILIKE $n)`；参数是 `%token%`（`:506`）；`ESCAPE '\'`（`:492`）；`ORDER BY e.id`（`:501`） |
| 排除文件 | `files.rs:497-500` | `AND f.path <> $n`，**在 SQL WHERE 内**，先过滤后截断 |
| 索引现状 | `cce-storage-metadb-pg/src/schema.rs:105-136` | `entities` 表有 `project_id/name/kind/file_id` 等 btree 索引，**无任何全文或 trgm 索引**；`rank DOUBLE PRECISION NOT NULL DEFAULT 0`（`:129`）存在但在本检索中未参与排序 |

即远程是"AND 合取的子串匹配 + 按主键排序"，无相关性评分。

## 3. 上层调用场景与语法承诺

### 3.1 实际调用点（共 6 处）

| 调用点 | 场景 | 对排序/语法的依赖 |
|---|---|---|
| `cce-tools/src/find_references.rs:129-172` | 符号名解析：先查内存快照 O(1)（`:136`），miss 才回退 FTS，`limit=1`（`:145,159`） | 极低：模糊兜底，只要命中任意一个即可 |
| `cce-orchestrator/src/query/coordinator.rs:452-477` | entity search：own epoch 未命中则回退 parent（`:465-476`） | 低：两阶段兜底，非相关性排序 |
| `app/cce-server-shared/src/services/search.rs:723-735` | `run_entity_search` 委托 coordinator（`:731-732`） | 透传 |
| `app/cce-server/src/api/handlers/entity_search.rs:93-106` | HTTP `POST /api/entities/search` | rank 仅作为展示字段返回（`:154`） |
| `cce-orchestrator/src/index/storage_coordinator/entities.rs:255` | 编排层内部测试，查询词为简单 token `"run"` | 无 |
| `cce-tools/src/find_references.rs:145,156` | 同上符号解析 | 无 |

**没有任何一个调用点把 FTS 结果喂给 RAG 排序或语义相关性计算。**

### 3.2 上层却承诺了完整 FTS5 语法

| 位置 | 原文承诺 |
|---|---|
| `cce-api/src/models/entity.rs:287-291` | `/// Entity search request (FTS5)` / `/// Search query (supports FTS5 syntax)` |
| `cce-server/src/api/handlers/entity_search.rs:27-31` | `Supports FTS5 query syntax:` 下列出 prefix `auth*`、phrase `"test function"`、boolean `AND/OR/NOT`、field-specific `name:main` |
| `cce-orchestrator/src/query/coordinator.rs:402,412-417` | `supports prefix matching: auth*, phrases: "test function"`，示例含 `"\"fn test()\""` |
| `coordinator.rs:397-398,408` | "using SQLite FTS5"、"ordered by relevance" |

且这条承诺**确实触达远程分支**：handler → `search.rs:731` → `coordinator.rs:433` → `coordinator.rs:462-476` → 统一契约 `entities_search_fts`，无 local-only 短路（`has_fts5_search()` 仅判断 store 是否挂载，`coordinator.rs:480-482`）。

### 3.3 契约测试零覆盖

`cce-storage-common/tests/metadata_backend_contract.rs` 中检索 `entities_search_fts` / `search_fts` **无任何命中**。共享 suite 未对该方法做任何断言，因此上述语义漂移不会被 CI 发现；远程分支还需 `CCE_TEST_PG_URL` 才运行，进一步降低暴露概率。

## 4. 语义差异全清单

| # | 维度 | SQLite FTS5（本地） | PG ILIKE（远程） | 影响 |
|---|---|---|---|---|
| D1 | 查询语法 | 完整 FTS5 语法 | 清洗为 token 子串 | 上层承诺失配 |
| D2 | 布尔 `OR` | 并集语义 | `OR` 被过滤（`:1725`）后各 token 以 `AND` 合取（`:490`）→ **语义反转**，结果更少甚至为空 | 功能性 bug |
| D3 | 字段限定 `name:x` | 只搜 name 列 | 冒号未清洗（`:1717-1719`），变成 `ILIKE '%name:x%'` → 几乎必然 0 结果 | 功能性 bug |
| D4 | 短语 `"a b"` | 连续相邻匹配 | 拆成两个 token 做 AND → 顺序与相邻性丢失 | 召回质量差异 |
| D5 | 前缀 `auth*` | 前缀匹配 | 去 `*` 后为子串 `%auth%` → "包含"替代"前缀"，多数场景仍命中 | 大体可接受 |
| D6 | 检索列 | `name`、`signature`（`indexes.rs:181`） | `name`、`signature`、`scoped_name`（`files.rs:492-493`）→ **远程多一列** | 不对称：搜 `Class::method` 远程可命中、本地不可 |
| D7 | 排序 | `ORDER BY f.rank`（`entity_repo.rs:252,272`） | `ORDER BY e.id`（`files.rs:501`），`rank` 列闲置（`schema.rs:129`） | 结果顺序不同 |
| D8 | 排除文件 | 先 `LIMIT` 后 Rust 过滤（`contract.rs:431,437-445`）→ 返回可能少于 limit | SQL WHERE 内过滤（`files.rs:497-500`）→ 返回满 limit | 返回数量语义不同 |
| D9 | 空/非法 query | 原样交给 FTS5，非法语法报错 | sanitize 后静默返回空或部分结果 | 错误行为不同 |
| D10 | 大小写/unicode | FTS5 unicode61 分词 | `ILIKE` 的 locale 折叠 | 边缘差异 |

D2、D3、D6、D8 是**实质差异**，不能简单归入"排序不同"。

## 5. 统一必要性论证：不必要

### 5.1 收益侧：统一的收益被高估

1. **排序收益无法传递**：rank 差异（`D7`）本应是最主要的统一动机，但第 3.1 节证明没有任何下游消费相关性顺序；`EntitySearchResult.rank`（`entity_search.rs:154`）只是展示字段。
2. **语法收益有限**：真正需要高级语法（`D2/D3/D4`）的场景，在 6 个调用点里一个都没有——`find_references.rs:145` 传的是精确符号名，`coordinator.rs:463` 传的是用户搜索词，编排层测试传的是 `"run"`。语法能力主要价值在 API 文档里，而非实际链路。
3. **无性能刚需**：entity 检索不是热点路径（符号解析有 O(1) 快照优先，`:136`），本地 FTS5 与远程全表 ILIKE 在当前数据规模下都能满足。

### 5.2 成本侧：统一的代价很高

1. **schema 迁移**：PG 需引入 `tsvector` 生成列 + GIN 索引 + 存量回填（`schema.rs:105-136` 当前无任何全文索引）。
2. **分词方案未定**：本地 FTS5 默认 unicode61 对中文按字切分；PG 需 `zhparser`/`pgroonga` 等扩展，生产环境是否可用未知，且需 DBA 拍板。
3. **评分不可直接对齐**：FTS5 `bm25()` 与 PG `ts_rank()` 是不同量纲，真统一还要做分数归一化，否则 `rank` 字段仍不可比。
4. **无法本地验证**：本环境无 PG 实例，远程分支按设计跳过（`CCE_TEST_PG_URL` 未设），统一后的端到端语义只能靠 CI。
5. **接口契约需重写**：`metadb.rs:485-498` 的"portable substring match"承诺要改成统一语义，牵涉 API 文档、coordinator 文档、CLI 文档同步。

### 5.3 架构侧：分层本身支持差异共存

`MetadataStorage` 通过宏生成统一 trait 与枚举分发，其设计目标是**统一方法集与生命周期**，而非统一后端算法。契约注释已把算法差异显式化（第 2.1 节），这正是"接口统一、实现异构"的应有形态。强行统一算法等于否定该分层决策。

**结论：统一的投入产出比不成立，不应立项。**

## 6. "不统一可行"的成立条件（当前尚未满足）

"不统一"要成为有效设计，前提是**契约边界被诚实声明且两端都在边界内**。当前不满足，因为上层承诺 FTS5 语法而远程不兑现（第 3.2 节）。需补齐三项：

### 6.1 前提一：把上层承诺降级为两端共同语法（推荐）

将 API 与文档中的 "supports FTS5 syntax" 改为**最小公分母承诺**：

> 查询为空格分隔的词项；每个词项须在 `name`、`signature` 或 `scoped_name` 中子串出现（AND 合取）；词项上限 8；词项中的引号、括号、`*`、`\`、`%`、`_` 及 `AND/OR/NOT/NEAR` 不作为语法解释。结果顺序不纳入契约。

需同步修改（本轮未改）：
- `cce-api/src/models/entity.rs:287-291`
- `cce-server/src/api/handlers/entity_search.rs:25-41`
- `cce-orchestrator/src/query/coordinator.rs:395-418`
- `cce-storage-metadb-pg/src/files.rs:462-465`（SQL 注释已接近正确，微调）

代价：仅文档与注释；收益：让"不统一"在语义上自洽，消除 API 层的虚假承诺。**这是让不统一成立的最低代价路径。**

### 6.2 前提二：修补远程端的语义反转与失效项

若不改语法承诺，则至少要修 D2（`OR` 被当 AND）与 D3（`name:x` 永 0 结果）——这两项是"承诺了但结果错误"，不属于可接受的语义差异。可在 `sanitize_fts_query`（`files.rs:1712-1730`）内做，不动 schema。D4/D5 可保留为文档化差异。

### 6.3 前提三：补共享契约测试

在 `metadata_backend_contract.rs` 补 `entities_search_fts` 的最小公分母断言：
- 精确 token 命中、AND 合取、空 query 返回空、上限 8 截断；
- `excluded_files` 排除生效（先统一 D8 语义后再断言返回数量）；
- 两端均不断言顺序（契约明确不承诺）。

远程仍依赖 `CCE_TEST_PG_URL`，但至少本地分支能持续守住边界。

### 6.4 D6 的处置建议

`scoped_name` 列不对称（`indexes.rs:181` vs `files.rs:493`）建议**由本地补齐**（见 7.2），而非远程删列——远程多搜一列是超集行为，删掉会减少召回。

## 7. SQLite FTS5 端独立优化（不依赖统一）

| 编号 | 优化 | 依据 | 收益 | 代价/风险 |
|---|---|---|---|---|
| L1 | `excluded_files` 下推 SQL，先过滤后截断 | `contract.rs:431` 先 LIMIT、`:437-445` 后过滤 | 修复 D8：返回数不再少于 limit | 小；需在 SQL 拼 `NOT IN` 或 JOIN 过滤 |
| L2 | FTS5 虚表纳入 `scoped_name` 列 | `indexes.rs:181` 仅 name/signature | 消除 D6，搜 `Class::method` 可命中 | 需重建 FTS5 表 + 同步 `:190-200` 触发器；存量需 `rebuild` |
| L3 | 采用 `tokenize='porter unicode61'` | `indexes.rs:181` 未指定 tokenizer，默认 unicode61 | 英文词干召回（authenticate ↔ authentication） | 需重建索引；中文场景需另行评估 |
| L4 | MATCH 输入预校验/转义 | `entity_repo.rs:251,271` 原样透传 query | 非法 FTS5 语法不再以 500/StorageError 冒泡；配合 6.1 的最小语法承诺 | 小 |
| L5 | `ORDER BY f.rank, e.id` 加稳定 tie-breaker | `entity_repo.rs:252,272` 仅按 rank | 相同评分下分页顺序稳定 | 极小 |
| L6 | 显式声明 rank 量纲 | `EntityRecord.rank` 来自 FTS5 `bm25()`，负值越小越相关 | 避免调用方误读 | 文档 |

L1 与 L5 可立即做（无 schema 变更）；L2/L3 需索引重建，建议与存量数据迁移一并安排。

## 8. PG ILIKE 端独立优化（不依赖统一）

| 编号 | 优化 | 依据 | 收益 | 代价/风险 |
|---|---|---|---|---|
| R1 | 建 `pg_trgm` GIN 索引（name/signature/scoped_name） | `files.rs:492-493` 全表 ILIKE；`schema.rs:132-136` 无文本索引 | **本轮收益最大项**：把 `%token%` 从全表扫描转为索引辅助查询，无需改契约、无需语义变更、无需存量回填 | 需 `CREATE EXTENSION pg_trgm` + 3 个索引（DBA 授权即可） |
| R2 | 用现有 `rank` 列做次级排序 `ORDER BY e.rank DESC, e.id` | `schema.rs:129` 的 rank 已写入（`files.rs:148` 等）但 `files.rs:501` 未用 | 让结果顺序比主键序更有意义，且无需 tsvector | 需确认 rank 语义确为"实体重要性"；若不确认可只加 `e.id` tie-breaker |
| R3 | 修正 `OR`/字段限定的清洗逻辑 | `files.rs:1723-1727`、`1717-1719` | 消除 D2/D3 的功能性 bug | 小～中；与 6.2 联动 |
| R4 | 短语相邻性（可选：pg_trgm 相似度或 `<->`） | `files.rs:1714` `split_whitespace` 丢失相邻性 | 提升 D4 场景的召回质量 | 中；属于"局部增强"，不等同于统一 |
| R5 | 固化注入防护单测 | `files.rs:1719` 去 `\ % _` + `:492` `ESCAPE '\'` 已安全 | 防回归 | 极小 |
| R6 | 显式文档化 ILIKE 排序/大小写语义 | `files.rs:462-465` 注释 | 边界透明 | 极小 |

**R1 是不依赖任何统一决策、投入产出比最高的优化**：它只加索引，不改 SQL 语义、不改契约、不改 API 承诺，却能直接改善远程检索性能。

## 9. 验证方式

- 已执行：`cargo check`（6 个相关 crate）通过，无 error/warning；`cargo test -p cce-storage-common -p cce-storage-metadb-sqlite -p cce-storage-metadb-pg --tests` 全部通过（cce-storage-common lib 17 + 三份契约 2/3/2、cce-storage-metadb-pg 8 含 3 个新单测、cce-storage-metadb-sqlite 73）。
- R1（pg_trgm 索引）端到端需 PG 实例，纳入 `CCE_TEST_PG_URL` CI job，用 `EXPLAIN` 确认索引命中。
- F-01 文档降级不涉及运行期行为，可用 grep 确认无残留 "supports FTS5 syntax" 承诺。
- 工具链约束：rustc 1.93 低于 workspace 声明的 1.97，编译需 `--ignore-rust-version` + rsproxy 镜像。

## 10. 落地状态

基线：`29bbeaf8 update backend`（已 pull 用户推送的前两轮改动）。本轮在其上完成：

| 项 | 状态 | 落地位置 |
|---|---|---|
| 统一必要性与不统一可行性分析 | 已完成 | 第 5、6 节 |
| F-01 语法承诺降级 | 已落地 | `cce-api/src/models/entity.rs`、`entity_search.rs`、`coordinator.rs`、`metadb.rs:485` |
| F-02 两处语义 bug 修复 | 已落地 + 3 个纯函数单测 | `files.rs:sanitize_fts_query` / `like_pattern` |
| F-05 pg_trgm GIN 索引 | 已落地 | `schema.rs` V5_DDL + `lib.rs` V5 迁移链 |
| L1 排除路径下推 SQL | 已落地 | `entity_repo.rs:search_fts_at_epoch_excluding`、`contract.rs:421` |
| L5 排序 tie-breaker | 已落地 | `entity_repo.rs:252,272` |
| F-07 契约测试补强 | 已落地 | `metadata_backend_contract.rs`（最小语法/空查询/排除路径） |
| 测试暴露的新缺陷 | 已修复 | 本地 `MATCH ''` 抛语法错误 → 契约要求返回空（`contract.rs:429`） |
| 固化决策文档 | 已写入 | `docs/infra/storage/entity-fts-backend-parity-decision.md` |
| L2/L3、R2/R4、远程端到端 | 未落地，列入第 11 节 | 需 schema 重建或 PG 实例 |

## 11. 开放点与建议

| 编号 | 事项 | 建议 | 优先级 |
|---|---|---|---|
| F-01 | 是否把上层 FTS5 语法承诺降级为最小公分母（6.1） | **建议采纳**，这是"不统一可行"的前提；也可作为备选方案直接保留 FTS5 承诺并在远程实现翻译层（成本高，等于变相统一） | 高 |
| F-02 | D2 `OR` 语义反转、D3 字段限定失效是否修 | **建议修**，属功能性 bug，与是否统一无关 | 高 |
| F-03 | D8 排除文件的返回数量语义由哪端对齐 | 建议本地改（L1），保持远程 SQL 内过滤的高效实现 | 中 |
| F-04 | D6 `scoped_name` 不对称由哪端对齐 | 建议本地 FTS5 补列（L2），远程保持不变 | 中 |
| F-05 | 是否落地 pg_trgm GIN 索引（R1） | **建议落地**，独立于 F-01，收益最大 | 高 |
| F-06 | 是否用 `rank` 列改进远程排序（R2） | 需先确认 rank 语义；不确定时只加 `e.id` tie-breaker | 中 |
| F-07 | 共享契约测试补 `entities_search_fts`（6.3） | 建议与 F-01/F-02 一起做，守住语义边界 | 中 |
| F-08 | 若未来确有相关性排序需求 | 应扩展 vector/fulltext(BM25) 链路，而非统一 entity FTS——后者不产语义相关性 | 低 |

**总建议：放弃 FTS 统一立项；改为执行 F-01 + F-02 + F-05，并把两端独立优化（第 7、8 节）按优先级分批推进。**
