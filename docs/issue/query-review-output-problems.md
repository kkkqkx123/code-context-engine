# query_review 输出问题清单（once_cell / flask）

检查时间：2026-10-09。输出为当日 09:12–09:15 UTC 现场生成，与当前源码一致，非陈旧残留。

检查范围：

- `outputs/scenarios/rust/query_review/once_cell/`（39 题 + `index.md` + `aggregated_demo.md` + `run_manifest.txt`）
- `outputs/scenarios/python/query_review/flask/`（45 题 + `index.md` + `aggregated_demo.md` + `run_manifest.txt`）
- 对照基准：同 fixture 的 `chunks/`（行区间与正文齐全）、`structured/`（`SUMMARY.md` 显示 294 entities / 709 relations）、fixture 源码、judgment 定义、设计文档 `docs/plan/query-visualization-output-design.md`

先说结论：judgment 与 fixture 均正常（题目数与输出文件数吻合，抽查的期望区间如 `src/lib.rs:480-482` 在 fixture 中真实存在，`chunks/` 输出行号与正文齐全），问题集中在**查询链路的富化阶段完全失效**，以及由此连锁导致的标注、关系、去重等环节的偏差。共 12 项，按严重度排序。

---

## 1. BM25 命中全部缺失正文、行号与 kind（最严重，785 个命中全中）

现象：两个项目全部 785 个命中 `file` 字段均为 `xxx:0-0`，`#### Content` 代码块全部为空，`kind` 为空（表格 entity 列与详情标题中表现为 kind 缺失，如 `|  unsync |`、`### 1.  \`unsync\``）。抽查 `G1Q1.md` 10 个命中 10 个空正文；含 `:0-0` 的文件 84 个，覆盖全部 query 文件。

与之对照：同 fixture 的 `chunks/once_cell/bm25/src/lib.rs.txt` 行区间（如 `lines 1-373`）与正文齐全，说明索引数据本身正常。

根因链（已定位到代码）：

- `crates/app/cce-orchestrator/src/query/retrieval/strategies/bm25.rs` 召回时有意只填 BM25 索引字段，`content` 留空、`start_line/end_line` 置 0、`kind` 置空，等待后处理富化。
- 富化依赖 `entity_mapper.rs` 的 `materialize` 按 `result.id` 查 SQLite `chunks` 表（`get_chunk_records_from_store`），查不到时原样返回。
- 索引侧 `storage_coordinator/bm25.rs` 的 `store_bm25_batched` 虽会持久化 BM25 chunk 记录，但经由 `store_chunk_records`，而该函数在 `metadata_store` 为空时**静默跳过**（`vector.rs`）。
- `crates/app/cce-e2e-tests/src/query_test.rs` 的 `index()` 只装配了 checkpoint 用 SQLite、BM25 客户端、Qdrant 与 embedder，**从未给索引编排器装配 `metadata_store`**，因此 chunk 记录从未写入查询侧后来查询的那个内存 SQLite。

也就是说，`cce-orchestrator` 已有回归测试 `test_bm25_results_are_enriched_from_sqlite` 保障生产路径，但 e2e harness 走了一条没有元存储的索引路径，把同一回归以 100% 命中率复现了出来。

影响：期望区间对照完全不可做（`Expected ranges` 有标尺但命中无行号）；第 3 项标注失败、第 4 项关系为空都部分源于此。

修复方向：给 `QueryWorkflowTest::index()` 的索引编排器装配与查询侧同一个内存 SQLite 的元存储；同时建议给 `store_chunk_records` 加上与 `store_bm25_batched` 缺客户端时同等的显式报错，而不是静默跳过。

## 2. `content_state: Full` 与空正文自相矛盾，且零降级记录

现象：785 个命中全部标记 `content_state: Full`、`truncated: false`，`Reference` 降级出现 0 次。`materialize` 在查不到 chunk 记录时直接返回，保留了召回阶段预设的 `Full` 状态。

这比单纯缺正文更坏：审查者无法区分“正文完整”与“富化失败”。设计文档要求正文超预算时保留截断标记与降级原因、不静默丢弃，而现状是富化缺失时连标记都没有。

修复方向：`materialize` 在记录缺失且正文为空时应降级为引用态（如新增 `DowngradeReason` 或复用 `FileMissing` 语义）并扣分，而不是保留 `Full`。

## 3. 标注对照 100% 失败，且即使成功也永远看不到关系扩展

现象：有命中的 82 个 query 文件逐命中全部为 `(annotation failed; see raw content above)`，成功 0 次。

两层原因：

- 直接原因：第 1 项导致 `content` 为空、`start_line/end_line` 为 0，而 `annotation/extractor.rs` 明确拒绝 0 行号，`annotate_single` 在第一步提取主单元即报错。
- 结构性原因：`query_review.rs` 的 `render_query_page` 调用 `annotate_single` 时 forward/backward 固定传空向量。`annotate_single` 的扩展完全依赖调用方传入的已解析单元，因此即使富化修复，`expanded` 也恒为 false，`expanded_nodes` 恒为 0，设计文档要求的“使标注无效果与标注被关闭可区分”无从谈起，审查者永远看不到关系扩展行为。

修复方向：先修第 1 项；再让 query job 经 `RelationSearcher` 为每个命中解析 ego 调用/被调用单元后传入 annotator（或明确在文档中声明本导出不覆盖扩展语义）。

## 4. `run_manifest.txt` 显示 `relations=0`，同 fixture 结构化输出为 709

现象：两份 manifest 均为 `relations=0`（`entities` 294/3775 与结构化输出一致，仅关系为 0）。而 `structured/once_cell/SUMMARY.md` 对同一 fixture 给出 Relations total 709（entity 563 + file-level 146）。

`QueryWorkflowTest` 已传 `build_relations: true`，批处理中 `builder.add_file_symbols` 也与元存储无关，说明符号已喂入但最终 `resolved_relation_count()` 为 0。确切断点尚未定位（`build_and_publish_relations` 的重建与解析环节需单步跟查；另注意到 review fixture 目录仅有 `src/` 而无构建清单文件，可能与构建配置扫描/依赖加载的交互有关）。

影响：关系查询审查（`relation_review` 种子 ego 图、调用链）在此链路上无数据可用；第 3 项的扩展语义同样无数据可用。

修复方向：以 `structured` 链路为参照对比单步跟查 `build_and_publish_relations`，确认是符号注册、解析还是快照发布环节丢数据；`relation_review` 输出暂缓验收，以 `relations>0` 为前置门槛。

## 5. 同实体重复命中挤占 Top-K，且与聚合路径的去重行为不一致

现象：

- once_cell `G1Q1.md` / `G1Q2.md` 前 5 名全是同一实体 `e:155`（不同 BM25 子 chunk：`group_155_bm25_1/_0/_4/_2/_3`），Top-10 实际只覆盖约 6 个不同实体。
- flask `G1Q1.md` 第 1、2 名分数**完全相同**（original 44.3148），仅 chunk id 不同（`group_6856_bm25_101` vs `_103`），属逐字重复。
- 但 `aggregated_demo.md` 中 `e:155` 只出现 1 次：`search_aggregated` 按对齐键去重了，而 `search` 未去重。两入口行为不一致。

设计文档要求聚合演示“验证权重合并与按对齐键去重”，现状是聚合去重、单查不去，去重语义没有统一。

修复方向：在 `search` 与 `search_aggregated` 之间统一对齐键去重语义（至少明确哪一层负责：融合层 dedup file references 目前只处理引用态，不处理 Full 态重复）。

## 6. 名义默认配置、实际 BM25-only：语义/模糊/跨语言题只测了关键词一路

现象：两份 manifest 均为 `sources: bm25`、`qdrant_available: no`、`vectors=0`，全部命中 `vector: 0.0000`、`boosted: false`。设计文档要求默认跑 hybrid（vector+bm25 融合 + summary boost 门控 + annotation 开 + rerank 关），单路对照才用后缀目录（如 `query_review/{project}_bm25`）。现状是默认目录名承载了单路结果。

后果：

- 语义题（G2）、模糊题（FZ）的“鲁棒性”结论在此输出中只能反映 BM25 关键词匹配，体现不出向量语义。
- 跨语言题（G4，如中文“懒加载的全局变量”）全部 0 命中——机制上符合预期（中文词无法命中英文 BM25 索引），但跨语言能力在审查输出中完全未被行使。

这不是代码缺陷，是覆盖率与命名方法学问题。修复方向：有 Qdrant 的环境补跑 hybrid 默认输出；无向量环境时输出目录改用 `_bm25` 后缀并在 manifest 中注明可审查的能力边界（G2/FZ/G4 在此配置下不具代表性）。

## 7. 渲染器与设计文档的字段偏差

`query_review.rs` 渲染缺设计文档点名的字段：

- 文件头缺实际执行的 intent / execution strategy、融合权重与算法、缓存命中信息（现有只有 Sources/Total/耗时/Config 行）。
- Top-K 汇总表缺 boost reason 独立列（只在详情区出现，且现状本就无 boost 命中，列缺失不易察觉）。
- manifest `expectations: src/judgments/{project}.rs` 为未插值的字面占位符。
- 来源命名不一致：配置侧 `sources: bm25`，结果侧 `sources: bm25_recall`。

均为渲染层小改，建议随第 1 项修复一并补齐。

## 8. 源码注释含中文，违反英文注释规范

`crates/app/cce-e2e-tests/src/query_review.rs` 有 3 处中文“对照”（模块文档、配置字段、函数文档）。项目规范要求代码文件一律英文。另 `relation_review.rs` 需同步排查。

## 9. `relation_review` 输出完全缺失

`outputs/scenarios/` 下不存在任何 `relation_review` 目录；设计文档要求的逐种子 Markdown、Mermaid 图、机器可读 JSON 快照均无。结合第 4 项（`relations=0`），即使现在运行也只能产出空图，属于被阻塞状态，暂不计为渲染器缺陷，但需在第 4 项解决后补跑验收。

## 10. `outputs_guide.md` 未收录新输出

`crates/app/cce-e2e-tests/docs/outputs_guide.md` 全文无 `query_review` / `relation_review` 字样，新目录的布局与阅读方法无文档说明。建议在格式定稿后补一节（目录树、index 入口、manifest 字段含义、BM25-only 环境的解读注意事项）。

---

## 非问题（已验证正常的部分）

- 题目与文件对应：once_cell 39 题、flask 45 题，输出文件数与 judgment 集合一致，无遗漏、无多余。
- 期望区间有效：抽查 `G1Q1` 期望 `src/lib.rs:480-482`，fixture 该行确为 `pub const fn new()`，标尺可信。
- 语料链路健康：`chunks/` 与 `structured/` 输出的行号、正文、实体统计均正常，缺陷 confined 在查询富化路径，不在索引解析。
- 聚合演示基本可用：双子查询权重合并与按对齐键去重在 `aggregated_demo.md` 中表现符合设计（9 条合并、无重复键），只是同样受第 1 项影响（无正文、无行号、标注缺失）。
