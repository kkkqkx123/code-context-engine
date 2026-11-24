# outputs目录输出文件分析

本文档分析 `outputs` 目录中各类输出文件的生成来源和用途。所有示例均为**人工审查用途**，
不参与自动断言（断言在 `tests/` 下）；示例列表以 `Cargo.toml` 的 `[[example]]` 条目为准。

## 目录结构概览

```
outputs/
├── scenarios/                               # 场景/审查类输出（按语言分子目录）
│   ├── {lang}/                              # bash/c/cpp/csharp/dart/go/java/javascript/
│   │   │                                    # lua/php/python/rust/scala/typescript
│   │   ├── summary/{fixture}/               # NL 文档导出（Markdown）
│   │   │   ├── SUMMARY.md
│   │   │   └── <按源目录组织的 *.md>
│   │   ├── chunks/{fixture}/{emb,bm25}/     # 分块验证输出（emb / bm25 双路径）
│   │   │   ├── SUMMARY.txt
│   │   │   └── <按源目录组织的 *.txt>
│   │   ├── structured/{fixture}/            # 符号表 + 关系 + 类型推断结构化报告
│   │   │   ├── SUMMARY.md
│   │   │   ├── <按源目录组织的 *.txt>       # 每文件报告
│   │   │   └── <dir>.dir.txt                # 每目录概览
│   │   └── annotation/{project}/            # 关系标注审查（assembly 改名而来）
│   │       ├── index.md
│   │       └── {query_id}.md                # 原始召回 vs 标注后对照
│   │   ├── query_review/{project}/          # 真实检索链路审查（BM25/混合 + 关系扩展标注）
│   │   │   ├── run_manifest.txt
│   │   │   ├── index.md
│   │   │   ├── {query_id}.md               # 命中表 + 命中明细 + 标注内容
│   │   │   ├── aggregated_demo.md           # 聚合查询演示（`*_aggregated` 示例）
│   │   │   └── aggregation_metrics.md       # 按查询类别/策略/权重的聚合评测
│   │   └── relation_review/{project}/       # 关系图可视化导出（调用链 + ego 图）
│   │       ├── run_manifest.txt
│   │       ├── index.md
│   │       ├── entity/{seed}.md             # 逐种子：callee/caller/前后向链/ego 图
│   │       └── graph/{seed}.json            # 机器可读 ego 图快照
│   ├── rust/alignment/                      # Hybrid 跨路径对齐审查报告
│   │   ├── run_manifest.txt
│   │   ├── alignment_summary.md
│   │   └── per_query_alignment.md
│   ├── java/summary/springboot-minimal-demo/  # 旧版导出（仅 summary，无 chunks/structured）
│   └── multi_language/                      # 多语言索引结果 JSON 报告
├── index/                                   # 索引工作流输出（回归测试产生）
├── benchmark/                               # 基准测试输出（按项目分子目录）
│   ├── cross_language/{project}/            # 独立跨语言评测（不并入主基准）
│   │   ├── cross_language_metrics.md
│   │   └── run_manifest.txt
│   ├── {project}/                           # once_cell/ripgrep/flask/express/gin/
│   │   │                                    # jackson_core/mediatr
│   │   ├── aggregate_metrics_top{k}.md
│   │   ├── aggregate_metrics_by_query_type_top{k}.md
│   │   ├── per_query_results_top{k}.md
│   │   ├── relevance_top5.md
│   │   ├── test_diagnostics.md
│   │   ├── run_manifest.txt
│   │   ├── evaluation_scope.md
│   │   ├── retrieval_method/                # 召回方式基准（emb/bm25/hybrid 融合）
│   │   │   ├── run_manifest.txt
│   │   │   ├── alignment_coverage.md
│   │   │   ├── aggregate_top{k}.md
│   │   │   ├── aggregate_by_query_type_top{k}.md
│   │   │   ├── per_query_top{k}.md
│   │   │   ├── fusion_gain_by_query_type_top{k}.md
│   │   │   ├── weight_sensitivity.md
│   │   │   ├── rrf_k_sensitivity.md
│   │   │   └── relevance_top5.md
│   │   ├── aggregation_enhance/             # 聚合增强（summary/relation boost）离线基准
│   │   │   ├── run_manifest.txt
│   │   │   ├── aggregate_top{k}.md
│   │   │   ├── aggregate_by_query_type_top{k}.md
│   │   │   ├── enhance_gain_by_query_type_top{k}.md
│   │   │   ├── per_query_top{k}.md
│   │   │   └── relevance_top5.md
│   │   └── rerank_{variant}/                # 重排基准（仅 once_cell；每个融合变体独立目录）
│   │       ├── run_manifest.txt
│   │       ├── aggregate_top{k}.md
│   │       ├── aggregate_by_query_type_top{k}.md
│   │       ├── per_query_top{k}.md
│   │       ├── rerank_gain_by_query_type_top{k}.md
│   │       ├── latency_cost.md
│   │       └── relevance_top5.md
└── demo/                                    # 文档分块演示
    └── documents/{file_name}_{type}/        # chunk_XXXX.txt + metadata.csv
```

说明：

- `{k}` 取 5/10/20/30/50。
- 旧版目录 `relation/`、`tools/`、`debug/`、`benchmark/{project}/bm25_parameter_sweep/`、
  `benchmark/{project}/rerank_*/`（once_cell 之外）为历史遗留或未运行时不存在；
  `dump_document_chunks` 的输出为 `demo/documents/`（非旧文档所述 `demo/documents/` 下的
  `SUMMARY.md` 结构）。
- `scenarios/{lang}/` 下的三个标准子目录（summary / chunks / structured）由统一的
  review 导出框架（`src/review_export.rs`）生成，任何语言的 `export_{lang}` 示例输出结构一致。
- `query_review/{project}_bm25/` 为 Qdrant 不可用时的 BM25-only 单路输出；
  `query_review/{project}/` 默认目录只承载 hybrid 结果。

## 输出文件生成来源分析

### 1. scenarios/ 目录

#### 1.1 各语言审查导出（summary / chunks / structured 三件套）

**生成命令（按语言）：**

```bash
cargo run --example export_rs -p cce-e2e-tests    # rust: once_cell/ripgrep/index_sidecar/re_export/relation_demo/relation_diamond
cargo run --example export_py -p cce-e2e-tests    # python: flask/index_sidecar/re_export/wildcard
cargo run --example export_java -p cce-e2e-tests  # java: index_sidecar/springboot-minimal-demo/jackson-core
cargo run --example export_go -p cce-e2e-tests    # go: gin
cargo run --example export_ts / export_js / export_cs / export_kt / export_php \
  / export_rb / export_lua / export_dart / export_scala / export_c / export_cpp \
  / export_sh -p cce-e2e-tests                    # 其余语言，各自 review fixture
```

**来源文件：** `examples/{lang}/export_{lang}.rs`（统一走 `src/review_export.rs` 的
`ReviewExportJob` / `export_jobs` 框架）

**生成逻辑（每个 fixture 一次解析，三种输出共享同一 `ParsedFile` 快照）：**
1. 使用 `FixtureSpec::{lang}_{fixture}()` 加载 review fixture
2. 扫描、解析、分组后导出 Markdown NL 文档（summary）
3. 按 Embedding / BM25 双路径生成分块验证文档（chunks）
4. 通过 `StructuredOutputWriter`（`src/structured_output/`）生成符号表 + 关系 +
   类型推断报告（structured），按源目录层级组织，含 `SUMMARY.md` 与 `<dir>.dir.txt`

**输出内容（每个 fixture）：**
- `summary/{fixture}/SUMMARY.md` + `src/**/*.md` — NL 文档
- `chunks/{fixture}/{emb,bm25}/SUMMARY.txt` + `src/**/*.txt` — 分块验证
- `structured/{fixture}/SUMMARY.md` + `src/**/*.txt` + `src.dir.txt` — 结构化报告

**分块验证格式（每个文件 `.txt`）：**

```
{文件名}
total chunks: {chunk总数}
---

=== CHUNK 1/{总数} (lines {起始行}-{结束行}) ===
chunk_id: {id} | group: {group_id} | kind: {实体类型}
fragment: {n}/{总数} | split_reason: {split_reason}
title: {bm25_title} | tokens: {token_count}
keywords: [kw1, kw2, ...]
related: {group_id} ({relation_type}), ...
prev_overlap: {n} tokens from {chunk_id}
next_overlap: {n} tokens from {chunk_id}

{chunk 的 NL 描述文本}
```

**验证要点：**
- `split_reason`: 检查分割原因是否合理 (member_boundary / hard_limit / sentence_boundary 等)
- `fragment`: 检查分片编号是否连续 (1/3, 2/3, 3/3)
- `original_entity_id`: 同一实体的分片应共享相同 ID
- `prev_overlap` / `next_overlap`: 检查相邻 chunk 的重叠区域是否一致
- `related_groups`: 检查前驱/后继/调用关系是否正确

#### 1.2 类型推断可视化（`scenarios/{lang}/summary|structured/{case}/`）

**生成命令：**
```bash
cargo run --example export_type_inference -p cce-e2e-tests
```

**来源文件：** `examples/type_inference/export_type_inference.rs`

**生成逻辑：** 扫描所有语言的 `type_inference` fixture，经 `FileProcessor` 解析后，
NL 文档走 `DirectExporter`，结构化报告走 `StructuredOutputWriter`（含独立
`TYPE_INFERENCE.md`）。回归断言在 `tests/regression/type_inference/`，不读本输出。

#### 1.3 关系标注审查（`scenarios/{lang}/annotation/{project}/`）

**生成命令：**
```bash
cargo run --example annotation_oncecell -p cce-e2e-tests
cargo run --example annotation_ripgrep -p cce-e2e-tests
cargo run --example annotation_flask -p cce-e2e-tests
cargo run --example annotation_gin -p cce-e2e-tests
cargo run --example annotation_express -p cce-e2e-tests
cargo run --example annotation_spring_boot -p cce-e2e-tests
cargo run --example annotation_mediatr -p cce-e2e-tests
```

**来源文件：** `examples/{lang}/annotation_{project}.rs`（express 在 `examples/javascript/`、
gin 在 `examples/go/`、spring_boot 在 `examples/java/`、mediatr 在 `examples/csharp/`）；
核心逻辑 `src/annotation_review.rs`（旧名 `assembly_review`，已于近期改名）

**生成逻辑：**
- 只消费 `data/benchmark/full_pipeline/{project}/bge-m3/bench_data.rkyv`（其余 baseline
  无实体结构无调用边，无标注意义）
- 离线重放 embedding 召回（余弦，无 Qdrant 无网络），逐 query 生成自包含 Markdown
- 每个命中展示"原始 top-K（对照组）"与"经 `RelationAnnotator::annotate_single` 标注后"
  对照，附 `AnnotationMetadata`
- 开启 expansion 时经调用边侧车做一跳扩展（先 callees 后 callers），受
  `ReviewFilterOptions` 路径策略与扩展预算约束

**输出内容：**
- `index.md` - 逐 query 的 top-1 命中汇总表
- `{query_id}.md` - 原始召回 vs 标注后内容对照（含 `FZ-` 变体查询）

#### 1.4 Hybrid 对齐审查报告（`scenarios/rust/alignment/`）

**生成命令：**
```bash
cargo run --example alignment_report -p cce-e2e-tests
```

**来源文件：** `examples/rust/alignment_report.rs`

**生成逻辑：**
- 加载 `fixtures/rust/basic` fixture，使用确定性 mock 嵌入服务（`src/mock_embedding_server.rs`），无需 LLM API
- 使用嵌入式本地向量引擎执行 vector/BM25/hybrid 三路查询，manifest 记录后端与 mock 嵌入来源
- 对齐键为 `cce_types::alignment_key`；三项比率由生产 `FusionAlignmentStats` 计算，汇总行按跨 query 计数合并
- 使用 `OutputManager`（Scenarios / rust / alignment）写入

**输出内容：**
- `run_manifest.txt` - 运行清单（fixture、本地向量后端、mock 嵌入、对齐键口径、三项指标定义、查询列表）
- `alignment_summary.md` - 逐 query 与汇总的匹配率、单路键占比、不可对齐占比，以及 hybrid 键包含关系
- `per_query_alignment.md` - 逐 query 的 vector/bm25/hybrid 对齐键明细

#### 1.5 索引结果快照（`scenarios/{rust,multi_language}/`）

**生成命令：**
```bash
cargo run --example rust_basic -p cce-e2e-tests          # 输出 scenarios/rust/presentation/basic/
cargo run --example multi_language_basic -p cce-e2e-tests # 输出 scenarios/multi_language/
```

**来源文件：** `examples/rust/basic.rs`、`examples/multi_language/basic.rs`

**生成逻辑：** 通过 `IndexOrchestrator` 执行完整索引工作流，`OutputReporter` +
`OutputSerializer::json()` 生成索引结果 JSON（文件数、实体数、关系数、向量数等统计）。

**输出内容：**
- `rust_basic_complete.json` - Rust basic 索引结果
- `multi_language_project.json` / `python_basic.json` / `java_basic.json` - 多语言场景结果

#### 1.6 文档格式分块与摘要导出（`scenarios/documents/`）

**生成命令：**
```bash
cargo run --example export_doc -p cce-e2e-tests     # 分块
cargo run --example summary_doc -p cce-e2e-tests    # 文件级摘要
```

**来源文件：** `examples/rust/export_doc.rs`、`examples/rust/summary_doc.rs`

**生成逻辑：** 将 `fixtures/documents/` 下各格式（JSON/XML/YAML/TOML 等）经文档流水线
（regex / serde / quick-xml 解析，非 tree-sitter）处理后导出。

**输出内容：**
- `scenarios/documents/chunks/{format}/{file_name}.txt` - 分块文本
- `scenarios/documents/summary/{format}/{file_name}.md` - 文件级 DocSummary

#### 1.7 真实检索链路审查（`scenarios/{lang}/query_review/{project}/`）

**生成命令：**
```bash
cargo run --example query_review_oncecell -p cce-e2e-tests
cargo run --example query_review_flask -p cce-e2e-tests
```

**来源文件：** `examples/{rust,python}/query_review_{oncecell,flask}.rs`；
核心逻辑 `src/query_review.rs`（判定集定义在 `src/judgments/{project}.rs`），
聚合评测在 `src/query_aggregation.rs`

**生成逻辑：** 经 `QueryWorkflowTest` 做真实索引（BM25 + SQLite 元数据 + 内存关系图，
无 Qdrant 无网络）后逐判定查询；每个命中用内容状态机物化正文（超限降级为引用），
经 `RelationAnnotator::annotate_single` 做关系扩展标注（命中行范围反查关系快照取
callee/caller，前后向各最多 3 个），同一对齐键去重后渲染。聚合评测复用同一索引与
单查询结果，另取阈值放开且 limit 放大的子查询候选构造合并前基线，再调用生产
`search_aggregated` 跑策略/权重矩阵。

**目录后缀规则：** Qdrant 可用时落默认目录（hybrid）；不可用时降级为 BM25-only
并改写 `query_review/{project}_bm25/` 后缀目录，manifest 的 `capability` 行声明
该配置下语义（G2）、模糊（FZ）、跨语言（G4）结论不具代表性。默认目录只承载
hybrid 输出。

**输出内容：**
- `run_manifest.txt` - 运行清单（索引文件/实体/关系数、outcome/errors、请求来源与
  执行策略、融合权重、缓存命中情况）
- `index.md` - 逐 query 汇总表
- `{query_id}.md` - 命中表（含对齐键、boost 原因列）+ 逐命中明细（内容状态、
  标注内容与扩展节点）
- `aggregated_demo.md` - 聚合查询演示（仅 `*_aggregated` 示例输出）
- `aggregation_metrics.md` - 聚合评测：同类型/混合类型子查询对 × `noisy_or`/`max` × 三组权重；含 union recall、merge_delta、去重率、一致率、失败数与增益

#### 1.8 关系图可视化导出（`scenarios/{lang}/relation_review/{project}/`）

**生成命令：**
```bash
cargo run --example relation_review_oncecell -p cce-e2e-tests
cargo run --example relation_review_flask -p cce-e2e-tests
```

**来源文件：** `examples/{rust,python}/relation_review_{oncecell,flask}.rs`；
核心逻辑 `src/relation_review.rs`（种子来自 `src/judgments/{project}.rs` 的强期望
行范围与 hub 名）

**生成逻辑：** 同 query_review 的真实索引链路，但只消费关系图：种子经快照行范围
查找与 `get_function_ids_by_name` 解析，对每种子跑生产关系查询（callee/caller、
前后向调用链、ego 图）并渲染。关系查询无需向量后端，全程离线。

**输出内容：**
- `run_manifest.txt` - 运行清单（种子数、索引实体/关系数、项目图节点/边数、未命中 hub；`scope` 行声明测试路径是否排除）
- `index.md` - 逐种子汇总表（callee/caller/前后向链/ego 节点边数）
- `entity/{seed}.md` - 逐种子的调用链、非调用引用（Incoming references）与 ego 图明细
- `graph/{seed}.json` - 机器可读 ego 图快照

**口径说明：**
- ego 边数为调用点去重后的边，不等于索引关系数；同一调用点的重复捕获（方法/回调、直接/静态）在提取层合并为一条。
- 前后向链与 ego 默认只含生产文件，测试路径被排除（`CCE_RELATION_REVIEW_INCLUDE_TESTS=1` 可切回含测试视图）。
- 前后向链只遍历内部实体，外部被调仅出现在 Callees 与 ego 中；`Incoming references` 节展示表驱动分发等非调用引用边，此类边不计入 Callers。
- Callees 表的 `callsite` 列为调用点文件与 1-based 行号，per-callsite 重复行以此区分。

### 2. benchmark/ 目录

所有 benchmark 均直接复用 `data/benchmark/{baseline}/{project}/bge-m3/bench_data.rkyv`
（预计算向量 + BM25 文本 + 查询），离线评测，无 LLM API 调用（rerank 生成除外）。

#### 2.1 基础检索基准（`benchmark/{project}/`）

**生成命令：**
```bash
cargo run --example benchmark_oncecell -p cce-e2e-tests
cargo run --example benchmark_ripgrep -p cce-e2e-tests
cargo run --example benchmark_flask -p cce-e2e-tests
cargo run --example benchmark_express / benchmark_gin / benchmark_jackson_core \
  / benchmark_spring_boot / benchmark_mediatr -p cce-e2e-tests
```

**来源文件：** `examples/javascript/benchmark_express.rs`、`examples/go/benchmark_gin.rs`、
`examples/csharp/benchmark_mediatr.rs`、`examples/java/benchmark_{jackson_core,spring_boot}.rs`；
核心逻辑 `src/judgments/{evaluate,report}.rs`（判定集定义在 `src/judgments/{project}.rs`）

**生成逻辑：** 对 embedding 变体和 BM25 变体计算 Precision@K、Recall@K、MRR@K、F1@K。

**输出内容：**
- `aggregate_metrics_top{k}.md` / `aggregate_metrics_by_query_type_top{k}.md`
- `per_query_results_top{k}.md` / `relevance_top5.md`
- `test_diagnostics.md` / `run_manifest.txt` / `evaluation_scope.md`

#### 2.2 召回方式基准（`benchmark/{once_cell,ripgrep,flask}/retrieval_method/`）

**生成命令：**
```bash
cargo run --example benchmark_retrieval_oncecell -p cce-e2e-tests
cargo run --example benchmark_retrieval_ripgrep -p cce-e2e-tests
cargo run --example benchmark_retrieval_flask -p cce-e2e-tests
```

**来源文件：** `examples/{lang}/benchmark_retrieval_{project}.rs`；
核心逻辑 `src/retrieval_method/{fusion,aggregation,benchmark,report}.rs`

**生成逻辑：**
- 评测方法矩阵：
  - `emb`、`bm25`：单路原始块排序（每块一次）
  - `minmax-*`：7 组 vector/bm25 权重，实体级对齐
  - `minmax-0.5-{vec,bm25,strict}`：单路开关的三个变体（仅保留向量独有键 / 仅保留 BM25
    独有键 / 严格交集），两条路径都命中的键不受开关影响
  - `rrf-*`（k=30/60/100）与 `rrf-60-strict`
  - `agg-max` / `agg-noisy-or`：多来源聚合（两条召回路径充当两个子查询结果列表），
    键与计分公式直接复用生产实现
- 跨路径对齐键去重后评测，保证 hybrid 与单路口径一致
- 复用 range-based P/R/F1 计算，另加首次命中排位、同 range 重复占位、fusion_gain、权重敏感性指标

**输出内容：**
- `run_manifest.txt` / `alignment_coverage.md`（含每路不可对齐块数与占比）
- `aggregate_top{k}.md` / `aggregate_by_query_type_top{k}.md` / `per_query_top{k}.md`
- `fusion_gain_by_query_type_top{k}.md` / `weight_sensitivity.md` / `rrf_k_sensitivity.md`
- `single_path_policy.md`：单路开关四种策略的 F1 对照（同权重，只差开关）
- `aggregation_strategy.md`：`agg-max` 与 `agg-noisy-or` 的 F1 对照，附多来源重叠率
- `fusion_score_baseline.md`：融合分分布基线（条目数、路径覆盖拆分、达到理论上限的占比、
  均值/最大值）——排序不变而分数尺度变化时，只有这张表会动
- `relevance_top5.md`

#### 2.3 重排基准（`benchmark/once_cell/rerank_{variant}/`）

**生成命令（两步）：**
```bash
cargo run --example gen_rerank_oncecell -p cce-e2e-tests       # 调用真实重排模型，生成分数旁路文件
cargo run --example benchmark_rerank_oncecell -p cce-e2e-tests # 纯离线评分（默认使用 config 融合）
# 离线融合变体（旁路只存纯重排分，无需重新调用模型）:
cargo run --example benchmark_rerank_oncecell -p cce-e2e-tests -- rerank_only
cargo run --example benchmark_rerank_oncecell -p cce-e2e-tests -- multiplicative
cargo run --example benchmark_rerank_oncecell -p cce-e2e-tests -- linear_weighted --normalize-initial
```

**来源文件：** `examples/rust/gen_rerank_oncecell.rs`、`examples/rust/benchmark_rerank_oncecell.rs`；
核心逻辑 `src/rerank_benchmark.rs` + `src/rerank_benchmark/report.rs`

**生成逻辑：**
- 复用 `bench_data.rkyv` + `rerank_*.rkyv` 旁路（FNV-1a 源哈希校验一致性，过期直接报错）
- 评测矩阵：`emb`/`bm25`/`minmax-0.5` × 对照/重排后；候选为各召回排序头部固定 50 个
- 候选文本两种来源：`emb-text`（向量切块文本）与 `raw-code`（真实源码切片），
  每种文本来源产生独立变体目录
- 变体目录名示例：`rerank_rerank_only_emb-text/`、
  `rerank_linear-weighted-alpha0.7_raw-code_norm-init/`
- 更换融合策略只需重跑评分阶段

**输出内容（每个变体目录）：**
- `run_manifest.txt` / `aggregate_top{k}.md` / `aggregate_by_query_type_top{k}.md`
- `per_query_top{k}.md` / `rerank_gain_by_query_type_top{k}.md`
- `latency_cost.md` / `relevance_top5.md`

#### 2.4 跨语言基准（`benchmark/cross_language/{project}/`）

**生成命令：**

```bash
cargo run --example cross_language_oncecell -p cce-e2e-tests
```

**来源文件：** `examples/rust/cross_language_oncecell.rs`；核心逻辑 `src/cross_language.rs`。

**生成逻辑：** 对每条非英文判定查询，按判定行范围重叠度匹配一条英文查询；两者共用同一批预计算 bge-m3 向量，分别评估 dense 召回。英文结果代表语料检索质量，`language_gap = English F1 - 非英文 F1` 代表语言对齐成本。BM25 结果仅为诊断，不参与结论。

**输出内容：**
- `cross_language_metrics.md` - 逐配对指标与 top-10 汇总
- `run_manifest.txt` - 配对规则与排除范围

**边界：** 主 benchmark、`benchmark_retrieval_*`、`benchmark_agg_*` 均使用 `CoreRetrieval`，排除 `cross_lang`；跨语言结论只看本目录。

#### 2.5 聚合增强基准（`benchmark/{project}/aggregation_enhance/`）

**生成命令：**
```bash
cargo run --example benchmark_agg_oncecell -p cce-e2e-tests
cargo run --example benchmark_agg_ripgrep -p cce-e2e-tests
cargo run --example benchmark_agg_flask -p cce-e2e-tests
```

**来源文件：** `examples/rust/benchmark_agg_{oncecell,ripgrep,flask}.rs`；
核心逻辑 `src/aggregation_enhance/{enhance,benchmark,report}.rs`

**生成逻辑：**
- 只消费 `full_pipeline` 数据集，比较普通 base 排序 vs 离线 summary/relation boost 后排序
- summary 信号镜像生产侧 `cce_orchestrator::query::boost` 公式（文件向量均值池化 +
  逐 query 余弦 + 阈值归一化）；relation 信号为离线代理（文件内聚实体图 + BFS 扩展 +
  `1/sqrt(hops)` 衰减）
- 聚合逻辑镜像生产 `apply_boosts`（分源上限 + 全局 `max_addition`，乘法加成）

**输出内容：**
- `run_manifest.txt`
- `aggregate_top{k}.md` / `aggregate_by_query_type_top{k}.md`
- `enhance_gain_by_query_type_top{k}.md` - boost 相对 base 的提升
- `per_query_top{k}.md` / `relevance_top5.md`

### 3. demo/ 目录

#### 3.1 文档分块演示（`demo/documents/`）

**生成命令：**
```bash
cargo run --example dump_document_chunks -p cce-e2e-tests
```

**来源文件：** `examples/rust/dump_document_chunks.rs`

**输出内容：**
- `{file_name}_{type}/chunk_XXXX.txt` - 每个分块的文本
- `{file_name}_{type}/metadata.csv` - 分块元数据

### 4. index/ 目录

**生成来源：** 回归测试（`tests/regression/e2e_outputs.rs` 等）产生的索引工作流输出。

### 5. 临时/诊断示例

部分示例不产出标准 outputs 目录，仅供临时诊断：

- `debug_convert` / `dbg_check` / `dbg_issue_tmp` / `dbg_meta` - 转换/解析诊断
- `zz_diag_rerank` - 转储单个 query 的重排精确输入
- `dump_retrieval` - 转储每条查询的检索 ranked 结果到 `benchmark/results/` 下
  （路径由代码内 `ranked_dir` 决定）

## 核心生成模块说明

### OutputManager / OutputBuilder (`src/output_manager.rs`)

输出管理器，负责管理输出目录结构和文件创建。

**关键功能：**
- `OutputCategory` 枚举定义输出类别：Index、Query、Scenarios、HotUpdate、Tools
- `OutputBuilder` 链式设置 category / language / scenario 后 `build()` 出 `OutputManager`
- `write()` 方法写入文件内容；默认基础目录为 crate 根 `outputs/`

### OutputReporter (`src/output_reporter.rs`)

高级报告生成器。

**关键功能：**
- `report_index_result()` - 生成索引结果报告
- `write_custom()` - 写入自定义内容
- 支持元数据（描述、类别、标签）

### OutputSerializer (`src/output_serializer.rs`)

序列化器，支持 JSON 和 Markdown 格式。

**关键功能：**
- `serialize_index_result()` - 序列化索引结果
- `SerializableIndexResult` - 可序列化的索引结果结构
- `json()` / `markdown()` - 快速构造序列化器

### ReviewExportJob (`src/review_export.rs`)

各语言 `export_{lang}` 示例的统一导出框架：单次解析产出 summary / chunks / structured
三种输出，保证三路审查基于同一解析快照。

### StructuredOutputWriter (`src/structured_output/`)

结构化报告渲染框架：符号表（按 kind 域分组）、关系（按文件归位）、类型推断，
含层级目录概览（`<dir>.dir.txt`）。

### annotation_review (`src/annotation_review.rs`)

关系标注审查导出：离线 embedding 召回重放 + `RelationAnnotator` 标注对照。
（旧名 `assembly_review` / `assembly_{project}` 示例，已统一改名为 `annotation_*`。）

### judgments / retrieval_method / rerank_benchmark / aggregation_enhance

四套基准的核心逻辑模块，分别对应上文 2.1–2.4 四类基准输出；示例仅做 fixture
与判定集（`src/judgments/{project}.rs`）的装配。

### DirectExporter / NlDocumentExporter (`cce_orchestrator::export`)

NL 文档导出器：`export_groups()` 导出实体组 Markdown；`NlDocumentExporter` 导出
文件级摘要文档。

### FileProcessor (`cce_orchestrator::index`)

文件处理器，负责完整流水线处理。

**关键功能：**
- `process_file_complete()` - 处理单个文件的完整流水线
- 支持 Embedding 和 BM25 两种路径

## 基准数据生成 (`data/benchmark/`)

**生成命令：**
```bash
cargo run --example gen_bench_oncecell -p cce-e2e-tests
cargo run --example gen_bench_ripgrep -p cce-e2e-tests
cargo run --example gen_bench_flask -p cce-e2e-tests
cargo run --example gen_bench_express / gen_bench_gin / gen_bench_jackson_core \
  / gen_bench_spring_boot / gen_bench_mediatr -p cce-e2e-tests
```

**来源文件：** `examples/{lang}/gen_bench_{project}.rs`

**生成逻辑：**
1. 加载目标代码库和 distractor 代码库的源文件
2. 执行三种分块基线：full_pipeline、direct_chunking、full_pipeline_raw_source
3. 对所有分块文本和查询文本进行批量嵌入（如配置了 API key；无 key 时仅生成 chunk）
4. 将结果序列化为 `rkyv` 格式

**输出内容：**
- `data/benchmark/{baseline}/{project}/bge-m3/bench_data.rkyv`
- `data/benchmark/{baseline}/{project}/bge-m3/rerank_{model}_{text_source}_depth{depth}.rkyv`
  - 重排分数旁路文件（由 `gen_rerank_oncecell` 生成，评分阶段只读）

## 生成命令汇总

| 输出目录 | 生成命令 | 来源文件 | 是否依赖向量化 |
|---------|---------|---------|---------------|
| scenarios/{lang}/summary\|chunks\|structured/ | `cargo run --example export_{lang}` | examples/{lang}/export_{lang}.rs | 否（直接调用文件流水线） |
| scenarios/{lang}/summary\|structured/{type_inference case}/ | `cargo run --example export_type_inference` | examples/type_inference/export_type_inference.rs | 否 |
| scenarios/{lang}/annotation/{project}/ | `cargo run --example annotation_{project}` | examples/{lang}/annotation_{project}.rs | 否（离线余弦重放） |
| scenarios/rust/alignment/ | `cargo run --example alignment_report` | examples/rust/alignment_report.rs | 否（mock 嵌入；hybrid 路需 Qdrant） |
| scenarios/rust/presentation/basic/ | `cargo run --example rust_basic` | examples/rust/basic.rs | 是（完整索引） |
| scenarios/multi_language/ | `cargo run --example multi_language_basic` | examples/multi_language/basic.rs | 是（完整索引） |
| scenarios/documents/chunks/ | `cargo run --example export_doc` | examples/rust/export_doc.rs | 否 |
| scenarios/documents/summary/ | `cargo run --example summary_doc` | examples/rust/summary_doc.rs | 否 |
| benchmark/{project}/ | `cargo run --example benchmark_{project}` | examples/{lang}/benchmark_{project}.rs | 否（使用预计算向量，CoreRetrieval） |
| benchmark/cross_language/{project}/ | `cargo run --example cross_language_{project}` | examples/{lang}/cross_language_{project}.rs | 否（独立跨语言赛道） |
| benchmark/{once_cell,ripgrep,flask}/retrieval_method/ | `cargo run --example benchmark_retrieval_{project}` | examples/{lang}/benchmark_retrieval_{project}.rs | 否（使用预计算向量与文本） |
| benchmark/once_cell/rerank_{variant}/ | `cargo run --example benchmark_rerank_oncecell [-- fusion] [--normalize-initial]` | examples/rust/benchmark_rerank_oncecell.rs | 否（离线融合旁路分数） |
| data/benchmark/.../rerank_*.rkyv | `cargo run --example gen_rerank_oncecell` | examples/rust/gen_rerank_oncecell.rs | **是**（调用真实重排模型） |
| benchmark/{project}/aggregation_enhance/ | `cargo run --example benchmark_agg_{project}` | examples/{lang}/benchmark_agg_{project}.rs | 否（离线 boost 模拟） |
| demo/documents/ | `cargo run --example dump_document_chunks` | examples/rust/dump_document_chunks.rs | 否 |
| data/benchmark/ (各 project) | `cargo run --example gen_bench_{project}` | examples/{lang}/gen_bench_{project}.rs | **可选**（无 API key 时仅生成 chunk） |

## 注意事项

1. **不提交到版本控制**: outputs目录已被 `.gitignore` 忽略
2. **人工审查用途**: 这些输出仅用于人工审查，不参与自动断言（见 `examples/README.md`）
3. **重新生成**: 修改核心逻辑后，应重新运行对应示例生成输出文件
4. **对比审查**: 使用 `git diff` 观察输出变化是否符合预期
5. **示例命名**: `examples/{dir}/{file}.rs` 注册为 `{dir}_{file}` 风格的示例名，
   具体以 `Cargo.toml` 的 `[[example]]` 条目为准
