# 实体全文检索（entities_search_fts）后端语义决策

## 1. 目的与适用范围

本文档固化 `entities_search_fts` 在 SQLite 与 PostgreSQL 两端的语义关系，作为长期有效的架构决策，**防止后续按"统一两端 FTS 语义"重复立项**。

适用范围：元数据契约中的实体名称/签名检索，服务于符号查找与 autocomplete。不适用于向量语义检索与 BM25 全文检索召回链路（那是另外两个存储后端，与本文无关）。

## 2. 固化结论

**两端 FTS 语义不统一是既定设计，禁止以"对齐两端 FTS 语义"为目标立项。**

- 契约层只承诺一组两端都能兑现的最小语法，具体算法由各后端自行实现。
- 本地 SQLite 可保留 FTS5 增强能力（前缀、短语、布尔、字段限定、BM25 排序）；远端 PostgreSQL 使用可移植的子串合取匹配。
- 两端的结果集合、排序、评分不可比，**调用方不得依赖任何一端的增强语义**。

## 3. 契约边界（最小公分母语法）

`cce-storage-common/src/metadb.rs:496` 的契约注释为权威定义：

- 查询是**空格分隔的子串词项**，词项之间为 **AND 合取**；
- 匹配列为 `name`、`signature`、`scoped_name`；
- 词项上限 **8**（`cce-storage-metadb-pg/src/files.rs:457`，`MAX_FTS_TOKENS`）；
- 空查询返回空结果；
- `excluded_files` 在 `LIMIT` **之前**过滤，保证分页数量语义一致；
- **结果顺序由后端定义，不纳入契约**。

FTS5 专有语法（尾部 `*`、`"短语"`、`AND/OR/NOT/NEAR`、`name:field`）属于本地后端增强，两端均不承诺。上层 API、CLI、coordinator 文档已统一降级为该边界：

- `cce-api/src/models/entity.rs:287`
- `cce-server/src/api/handlers/entity_search.rs:25`
- `cce-orchestrator/src/query/coordinator.rs:395`
- `cce-storage-common/src/metadb.rs:485`

## 4. 两端实现事实

| 环节 | 本地（SQLite） | 远端（PostgreSQL） |
| --- | --- | --- |
| 索引 | `entities_fts` FTS5 虚表 + 三个同步触发器（`schema/indexes.rs:180-202`） | `pg_trgm` GIN 三列索引（`metadb-pg/src/schema.rs:35`，V5 增量迁移） |
| 检索 | `entities_fts MATCH ?1`，`ORDER BY f.rank, e.id`（`repo/entity_repo.rs:259,272`） | 每词项一个 `ILIKE '%token%' ESCAPE '\'` 的 AND 合取，`ORDER BY e.id`（`files.rs:466,492,501`） |
| 词项处理 | query 原样交给 FTS5 | `sanitize_fts_query`（`files.rs:1717`）+ `like_pattern`（`files.rs:1752`） |
| 排除路径 | SQL 内 `NOT IN`，`search_fts_at_epoch_excluding`（`repo/entity_repo.rs:284`） | SQL WHERE 内 `f.path <>`（`files.rs:497`） |

## 5. 决策依据（摘要）

完整论证见 `docs/plan/fts-unification-necessity-and-independent-optimization.md`。核心三点：

1. **无下游消费相关性**：6 个调用点中符号解析走 O(1) 快照优先、FTS 仅作 miss 兜底且 `limit=1`；entity search 用于 autocomplete；`rank` 仅为响应展示字段。排序差异无法传递到任何质量指标。
2. **统一成本高、收益无法验证**：PG 需 `tsvector` 生成列 + GIN + 存量回填，中文分词方案未定，`bm25()` 与 `ts_rank()` 量纲不同，且本环境无 PG 实例。
3. **分层设计支持异构**：`MetadataStorage` 宏生成统一方法集与枚举分发，目标是接口一致而非算法一致；契约注释显式声明差异正是应有形态。

## 6. 不统一成立的前提与已落地保障

"不统一"成立的前提是**契约边界被诚实声明且两端都在边界内**。以下为保障该前提的已落地改动：

| 编号 | 保障 | 落地位置 |
| --- | --- | --- |
| G-01 | 上层语法承诺降级为最小公分母 | API 模型、handler、coordinator、契约注释（见第 3 节） |
| G-02 | 修复远端字段限定失效：`name:x` 不再变成 `ILIKE '%name:x%'` 恒 0 结果，而是剥离前缀后检索 | `files.rs:1717` |
| G-03 | 修复远端通配符误删：不再删除 `_`/`%`，改为绑定时转义，使 `my_func` 等代码符号可命中 | `files.rs:1717,1752` |
| G-04 | 本地排除路径下推 SQL，消除"先 LIMIT 后过滤导致返回数不足" | `repo/entity_repo.rs:284`、`contract.rs:421` |
| G-05 | 本地排序加 `e.id` tie-breaker，保证相同评分下顺序稳定 | `repo/entity_repo.rs:252,272` |
| G-06 | 公共契约测试覆盖最小语法、空查询、排除路径 | `tests/metadata_backend_contract.rs` |
| G-07 | pg_trgm 索引使远端子串匹配不必全表扫描（纯索引，不改语义） | `schema.rs:35`、`lib.rs` V5 迁移链 |
| G-08 | 空查询语义对齐：本地 FTS5 对 `MATCH ''` 原抛语法错误，现返回空结果，与远端空 token 路径一致 | `metadb-sqlite/src/contract.rs:421` |

## 7. 允许的独立优化（不违反本决策）

以下优化**各自独立、无需另一端配合**，可以立项：

- 本地：FTS5 tokenizer 调优（如 porter 词干）、虚表补 `scoped_name` 列、查询输入预校验。
- 远端：pg_trgm 索引参数调优、利用现有 `entities.rank` 列做次级排序、按数据规模调整 `MAX_FTS_TOKENS`。
- 若未来确有相关性排序需求，应扩展 vector / BM25 fulltext 链路，**而不是统一 entity FTS**。

## 8. 禁止事项

- 禁止提交"统一两端 entity FTS 语义""为 PG 引入 tsvector 以对齐 FTS5"之类的立项提案。
- 禁止在 API 文档、CLI 帮助、coordinator 注释中重新承诺 FTS5 专有语法可跨后端使用。
- 禁止让任何调用方依赖 `rank` 的具体值或结果顺序做业务判断。
- 若确需改变本决策，须先推翻本文档，并给出"下游确实消费相关性"的调用链证据，而非仅以"两端不一致"为由。

## 9. 验证方式

- 本地：`cargo test -p cce-storage-common --tests` 与 `-p cce-storage-metadb-sqlite --tests`，覆盖 G-04/G-05/G-06。
- 远端词项处理：`cargo test -p cce-storage-metadb-pg --tests`，覆盖 G-02/G-03 的纯函数单测。
- PG 端到端（含 G-07 索引命中）需 `CCE_TEST_PG_URL` 指向真实实例，用 `EXPLAIN` 确认；本地环境无法验证。
- 工具链约束：rustc 1.93 低于 workspace 声明的 1.97，编译需 `--ignore-rust-version` 并配置 rsproxy 镜像。

## 10. 相关文档

- `docs/plan/fts-unification-necessity-and-independent-optimization.md`（完整分析与优化清单）
- `docs/plan/storage-openpoints-detailed-design.md`（第四轮 O-B'，已标注被本决策取代）
- `docs/infra/storage/backend-selection-and-constraints.md`（存储后端选择与约束）
