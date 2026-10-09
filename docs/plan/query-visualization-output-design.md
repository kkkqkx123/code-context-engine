# 完整查询可视化输出设计方案

## 1. 背景与目标

当前查询链路（`cce-orchestrator/src/query/`）已具备语义检索、混合融合、boost、标注、重排、关系与图查询等完整能力，但缺少“对特定查询输出完整实际结果”的人工审查通道。现有 `outputs/` 下的 benchmark 输出全是聚合指标（P/R/MRR），`annotation` 输出是离线余弦重放（不经过真实融合、boost、重排），都无法回答“这次查询到底返回了什么、排得对不对、关系图长什么样”。

本方案目标：在 `cce-e2e-tests` 内新增两条只读审查导出，全部走真实查询管线，输出自包含 Markdown 供人工审查：

- 语义查询完整结果：复用 `judgments` 的题目（id、文本、类型、期望区间）作为查询集与期望对照，把真实 `QueryResult` 的每个命中完整落盘。
- 关系查询图输出：对同一语料的代表实体输出调用关系与子图落盘。

本方案不改动任何生产代码，仅在 `cce-e2e-tests` 新增离线 example 与渲染模块。

## 2. 总体决策

### 2.1 走真实管线，不做离线重放

语义部分必须经过 `QueryCoordinator -> Searcher -> 召回 -> 融合 -> 富化 -> boost -> 重排 -> 阈值 -> 标注` 全链路，relation 部分必须经过 `RelationSearcher / GraphService` 真实快照。`annotation_review.rs` 与 `retrieval_method/` 的离线余弦打分逻辑不复用为召回源，只复用其报告排版思路。这样审查到的才是线上行为，融合退化、boost 误加、标注截断等问题才可见。

### 2.2 语料与题目复用，数据目录只做对照

- 语料：复用 `fixtures/{lang}/review/{project}/` 下的真实项目（与 `gen_bench_*` 同一份语料），经 `FixtureSpec` 加载后用 `QueryWorkflowTest` 做全量索引。索引是手段不是目的，索引结果本身不落盘。
- 题目与期望：复用 `src/judgments/{project}.rs` 的 `RelevanceJudgment`（query id、query_text、query_type、relevant_ranges）作为查询集与期望对照。每份输出文件头都回链期望区间，审查者逐条判断命中是否落在期望区间内。
- `data/benchmark/.../bench_data.rkyv` 不作为召回数据源（真实管线自己检索），只在需要时作为辅助对照（例如核对题目 id 集合是否与 judgments 一致）。不新增任何 data 文件，不回写 fixtures。

### 2.3 离线可跑，无 LLM 依赖

向量侧使用确定性 mock 嵌入服务（与 `alignment_report` 相同做法），BM25、SQLite 使用本地临时实例。重排默认关闭（只验证排序与阈值逻辑）；向量后端不可用时降级为 BM25-only 并在 manifest 中明确记录，不静默缺失。

### 2.4 输出位置：scenarios 树，与 annotation 并列

benchmark 树放聚合指标，不适合放逐 query 完整正文。新输出放在人工审查类的 scenarios 树下，与现有 `summary / chunks / structured / annotation / alignment` 并列：

```text
outputs/scenarios/{lang}/query_review/{project}/
outputs/scenarios/{lang}/relation_review/{project}/
```

经由 `OutputManager`（`OutputCategory::Scenarios`）统一管理，天然获得按语言分子目录、自动建目录、overwrite 语义。`outputs/` 已被 `.gitignore` 忽略，仅供人工审查，不参与自动断言。

## 3. 语义查询完整结果输出

### 3.1 查询矩阵

每个 judgment 题目默认跑一次生产默认配置（hybrid，vector+bm25 融合 + summary boost 门控 + annotation 开启 + rerank 关闭）。首版不做 emb/bm25/hybrid 全矩阵（那是 `retrieval_method` 基准的事），保持“一个题目一份完整实际结果”。如需对照单路行为，通过 example 参数切换 `SearchSources` 重跑一次即可，输出目录用后缀区分（例如 `query_review/{project}_bm25`）。

聚合查询（`search_aggregated`）首版只用一个演示用例覆盖：取同一 project 的两个同类型题目派生两个子查询并验证权重合并与按对齐键去重，不对全部题目做笛卡尔积。

### 3.2 单查询文件格式

每个 `{query_id}.md` 是自包含审查单元，节结构如下：

- 文件头：query id、query_text、query_type、实际执行的 intent 与 execution strategy、权重与融合算法、是否命中缓存、耗时、 sources、limit 与 min_score 配置。
- 期望对照：该题目的 `relevant_ranges` 列表（文件与行区间），审查者以此为标尺。
- Top-K 汇总表：每行一个命中，列为排名、统一分、原始分、向量分、BM25 分、来源、是否被 boost、对齐键、文件与行区间、实体 kind 与 name。分数列保留四位小数，boost 命中的 boost reason 单列展示。
- 逐命中详情：每个命中的完整元数据（id、entity_ids、segment_id、content_state、截断标记、pattern 与 category 透传字段）与完整正文。正文超预算时保留截断标记与降级原因，不静默丢弃。
- 标注对照：每个命中并排展示标注前原文与 `RelationAnnotator` 标注后文本，外加是否扩展开、扩展节点数、截断信息，使“标注无效果”与“标注被关闭”可区分。

顶层另写 `index.md`（逐 query 的 top-1 命中汇总表，作为审查入口）与 `run_manifest.txt`（fixture、题目数、嵌入方式、后端可用性、阈值与 top_k、过滤策略）。

### 3.3 渲染信息来源

渲染所需字段全部来自真实返回类型，不做二次计算：`QueryResult` 提供 items、total、elapsed、sources；`SearchResult` 提供 score、original_score、vector_score、bm25_score、sources、entity_ids、segment_id、file_path、起止行、kind、name、content、is_boosted、boost_reason、metadata、content_state；对齐键调用生产 `alignment_key` 函数；标注对照调用生产 `RelationAnnotator::annotate_single` 的返回体。中间阶段可观测但不落盘的量（改写后 query、embed 缓存命中、融合统计、glob 丢弃数）摘要写入文件头，不展开为独立文件。

## 4. 关系查询图输出

### 4.1 种子实体选择

关系输出需要先确定“从哪些实体查起”。种子确定性地来自同一套 judgments：取每题首个 Strong 期望区间，按文件行号在符号表中定位宿主实体；再加上每个 project 的少量枢纽实体（例如 once_cell 的 `OnceCell::new/get` 类实体，由各 example 以常量给出）。种子列表写入 `run_manifest.txt`，保证可复现。期望区间定位不到实体的种子直接跳过并在 index 中标记原因，不报错中断。

### 4.2 每个种子的输出

每个种子输出一个 Markdown 文件，包含调用者、被调用者、单向调用链（双向各一）、以该实体为中心的 ego 子图。子图部分包含节点表（实体名、kind、文件与行）、边表（起点、终点、关系类型）与一个 Mermaid 流程图代码块（Markdown 预览可直接渲染，节点标签做转义）。另附一份同名 JSON（节点与边的机器可读快照，供二次处理）。调用链深度与分页复用生产 `RelationQueryOptions` 默认值，图遍历复用生产 `GraphFilter` 默认值。

顶层同样写 `index.md`（种子清单与每种子的出入度汇总）与 `run_manifest.txt`（fixture、种子来源、深度与 limit 配置）。

## 5. 涉及文件清单

### 5.1 计划新建的文件（实现侧）

| 新建文件 | 职责 |
|---|---|
| `crates/app/cce-e2e-tests/src/query_review.rs` | 语义完整结果导出的统一 job：题目加载、真实索引与查询编排、逐 query Markdown 与 index、manifest 渲染 |
| `crates/app/cce-e2e-tests/src/relation_review.rs` | 关系图导出的统一 job：种子解析、关系与图查询编排、逐种子 Markdown、Mermaid 与 JSON 渲染 |
| `crates/app/cce-e2e-tests/examples/rust/query_review_oncecell.rs` | once_cell 语义导出 thin wrapper（首批落地之一） |
| `crates/app/cce-e2e-tests/examples/python/query_review_flask.rs` | flask 语义导出 thin wrapper（首批落地之一） |
| `crates/app/cce-e2e-tests/examples/rust/relation_review_oncecell.rs` | once_cell 关系导出 thin wrapper（首批落地之一） |
| `crates/app/cce-e2e-tests/examples/python/relation_review_flask.rs` | flask 关系导出 thin wrapper（首批落地之一） |
| `crates/app/cce-e2e-tests/Cargo.toml`（追加条目） | 为上述四个 example 新增 `[[example]]` 注册 |
| `docs/plan/query-visualization-output-design.md` | 本设计文档 |

后续推广到 ripgrep、gin、express、spring_boot、jackson_core、mediatr 时，每个 project 各加一对 thin wrapper example（命名沿用 `query_review_{project}` / `relation_review_{project}` 惯例，与现有 `annotation_*` 系列一致），共用上述两个 job 模块，不再新增核心逻辑。首版只落地 once_cell 与 flask 两条链路，格式定稿后再复制。

### 5.2 复用但不修改的文件（输入与能力侧）

语料与题目：

| 文件 | 用途 |
|---|---|
| `crates/app/cce-e2e-tests/fixtures/rust/review/once_cell/` 等各语言 review 语料 | 索引语料唯一来源，经 `FixtureSpec` 加载 |
| `crates/app/cce-e2e-tests/src/judgments/once_cell.rs`、`ripgrep.rs`、`flask.rs`、`gin.rs`、`express.rs`、`spring_boot.rs`、`jackson_core.rs`、`mediatr.rs` | 题目与期望区间来源（id、query_text、query_type、relevant_ranges） |
| `crates/app/cce-e2e-tests/src/bench_data.rs` | 只复用 `QueryType`、`RelevanceJudgment`、`SourceRange` 类型定义，不读 rkyv 向量 |
| `crates/app/cce-e2e-tests/src/judgments/evaluate.rs` | 只复用题目组织惯例；`BenchmarkPaths` 仅用于辅助核对题目集合 |

测试基础设施：

| 文件 | 用途 |
|---|---|
| `crates/app/cce-e2e-tests/src/fixture.rs` | `FixtureSpec` / `TestFixture` 语料加载 |
| `crates/app/cce-e2e-tests/src/query_test.rs` | `QueryWorkflowTest` 真实索引与查询编排（含 embedder、sources、config 装配） |
| `crates/app/cce-e2e-tests/src/mock_embedding_server.rs` | 确定性 mock 嵌入服务，实现离线可跑 |
| `crates/app/cce-e2e-tests/src/output_manager.rs` | `OutputManager` / `OutputBuilder` / `OutputCategory::Scenarios` 输出目录管理 |
| `crates/app/cce-e2e-tests/src/review_filter.rs` | 路径过滤策略，与现有审查导出共用语义 |
| `crates/app/cce-e2e-tests/src/structured_output/` | 种子实体定位时参考符号表渲染口径 |

生产查询能力（只调用，不修改）：

| 文件 | 用途 |
|---|---|
| `crates/app/cce-orchestrator/src/query/coordinator.rs` | `QueryCoordinator` 统一查询入口与聚合查询 |
| `crates/app/cce-orchestrator/src/query/searcher/` | 真实检索执行流 |
| `crates/app/cce-orchestrator/src/query/retrieval/` | 召回策略与融合（含对齐键口径） |
| `crates/app/cce-orchestrator/src/query/boost.rs`、`boost/` | boost 数值与原因透出 |
| `crates/app/cce-orchestrator/src/query/annotation/` | `RelationAnnotator` 标注前后对照 |
| `crates/app/cce-orchestrator/src/query/ranking/` | 排序与阈值行为的实际体现 |
| `crates/app/cce-orchestrator/src/query/types/` | `QueryResult`、`SearchResult`、`SearchConfig`、`QueryOptions` 渲染字段来源 |
| `crates/app/cce-orchestrator/src/query/relation_searcher.rs` | callees、callers、调用链、继承等关系查询 |
| `crates/app/cce-orchestrator/src/query/graph/` | ego、path、subgraph、components、export 图查询 |

排版参考（只参考，不修改）：

| 文件 | 参考点 |
|---|---|
| `crates/app/cce-e2e-tests/src/annotation_review.rs` | 逐 query 自包含 Markdown 与 index 对照排版 |
| `crates/app/cce-e2e-tests/src/retrieval_method/report.rs` | manifest 与汇总表的写法 |
| `crates/app/cce-e2e-tests/examples/rust/alignment_report.rs` | mock 嵌入 + 真实管线 + Qdrant 探测降级的范例 |
| `crates/app/cce-e2e-tests/examples/rust/dump_retrieval.rs` | 按 query 转储 ranked 结果的范例 |
| `crates/app/cce-e2e-tests/docs/outputs_guide.md` | 输出目录惯例，本文档落地后需同步增补新目录小节 |

### 5.3 计划输出的文件（生成侧，均在 `.gitignore` 内）

语义部分（以 once_cell 为例）：

```text
outputs/scenarios/rust/query_review/once_cell/
├── run_manifest.txt
├── index.md
├── G1Q1.md
├── G1Q2.md
└── ...
```

关系部分（以 once_cell 为例）：

```text
outputs/scenarios/rust/relation_review/once_cell/
├── run_manifest.txt
├── index.md
├── entity/{seed_slug}.md
└── graph/{seed_slug}.json
```

flask 的布局相同，只是顶层语言目录为 `python`。`data/` 目录不新增文件，`fixtures/` 目录不回写。

## 6. 人工审查流程

1. 运行对应 example（例如语义 once_cell 与关系 once_cell），确认 `run_manifest.txt` 中的后端模式符合预期（全量模式应为 hybrid 可用；降级为 BM25-only 时只做单路审查，不与全量结果混读）。
2. 打开 `query_review` 的 `index.md`，按 query_type 抽样（每类至少两条，优先 semantic 与 fuzzy），逐条打开 `{query_id}.md`，对照文件头的期望区间回答：top-K 是否命中期望实体、排序是否合理、boost 与标注是否引入噪音。
3. 打开 `relation_review` 的 `index.md`，抽查枢纽实体的调用链与 ego 图：边是否缺失、方向是否正确、Mermaid 图与边表是否一致。
4. 结论记录在审查意见中；输出文件本身不提交版本控制，改动核心逻辑后重新运行对应 example 并用文件 diff 观察变化是否符合预期。

## 7. 验收标准

1. 在已有 fixtures 前提下，四个首批 example 均可离线完成，无需真实 LLM key；Qdrant 不可用时降级路径在 manifest 中有明确记录。
2. 每个 `{query_id}.md` 可独立阅读：含期望区间、分数四列、完整正文、标注对照；`index.md` 可作为入口跳转到各 query 文件。
3. 每个种子实体文件可独立阅读：含调用双向结果、调用链、节点边表、Mermaid 图；同名 JSON 可被机器解析。
4. 新增模块不修改生产代码与现有 benchmark 口径；`cargo clippy --all-targets --all-features` 与 `cargo fmt` 通过。

## 8. 明确不做

- 不做聚合指标评分（P/R/MRR 仍归 benchmark 体系，本方案只做完整结果呈现）。
- 不做 emb/bm25/hybrid 全矩阵对比（归 `retrieval_method` 基准，本方案默认 hybrid 单次全量呈现）。
- 不处理 `direct_chunking` 与 `full_pipeline_raw_source` 语料变体，只处理真实索引语料。
- 不覆盖 MCP、Gateway、CLI 透出层（它们是生产查询能力的薄封装，行为由本方案的输出覆盖）。
- 不默认开启在线 LLM 重排；重排模型的效果评估沿用现有 rerank 基准的旁路机制。
