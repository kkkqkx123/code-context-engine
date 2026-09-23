# Assembly 人工审查导出设计方案

## 1. 背景与动机

SPSR-Graph 组装模块（`cce-orchestrator/src/query/assembly/`）自 e792206 起与搜索管线断开：排序阶段的关系增强（`RelationBoost`、`WithRelationExpansion`）经确认对召回质量无益而删除，但**结果组装阶段的关系/结构增强**（纯文本增强，不影响排名）从未做过质量验证，属于"未证伪"而非"已证伪"。

当前该模块处于休眠状态（模块级 `#![allow(dead_code)]`，见 `query/assembly.rs` 的 "Status (dormant)" 注释）。在决定"重新集成"还是"彻底删除"之前，需要一个**人工审查通道**：对真实查询生成"原始结果 vs 组装后结果"的并排输出，供人工评估组装的文本增益是否值得其复杂度。

本方案不引入任何生产代码改动，仅在 `cce-e2e-tests` 新增离线 example。

## 2. 数据现状

### 2.1 输入数据

复用 `gen_bench_flask.rs` 等生成脚本产出的 rkyv 序列化数据：

```
data/benchmark/{baseline}/{fixture}/bge-m3/bench_data.rkyv
```

`BenchmarkData` 中与本方案相关的字段：

| 字段 | 用途 |
|------|------|
| `queries: Vec<QueryData>` | 查询集（id、text、query_type） |
| `query_texts` + `embedding.query_vectors` | 查询向量，用于离线召回 |
| `embedding.chunks: Vec<ChunkData>` | chunk 元数据（`file_path`、`start_line`、`end_line`、`entity_ids`、`segment_id`） |
| `embedding.texts` / `vectors` | chunk 文本与向量（1024 维，bge-m3） |
| `bm25.*` + `bm25_documents` | BM25 路径数据（本方案不使用，见 2.2） |

### 2.2 只针对 full_pipeline

三个 baseline 中只处理 `full_pipeline`：

- `full_pipeline_raw_source` 的 chunk 文本已是原始源码切片，组装（语义单元提取 + 结构拼接）对其无意义；
- `direct_chunking` 是直接分块对照组，同理排除；
- 现有 `outputs/scenarios/` 目录下的 chunk 导出（`review_export.rs`）也只服务 full_pipeline 场景，与本方案口径一致。

### 2.3 召回方式

不引入 LLM、不启动 Qdrant/SQLite 服务。对每个查询：

1. **Emb 单路召回**：查询向量与 chunk 向量做余弦相似度（复用 `bench_data::cosine_similarity`），取 top-K（默认 K=10，可参数化）；
2. **BM25 单路召回**（可选开关）：复用 `infra::scorer` 的离线打分器与 `bm25_documents`；
3. **Hybrid 融合**（可选开关）：按 `entity_ids` 对齐的 min-max 归一化加权融合，口径对齐 `docs/plan/retrieval_method_benchmark_design.md`，保证审查结果与生产排名语义一致。

首版只实现 Emb 单路（最简单、确定性最高），Hybrid 作为后续扩展。

## 3. 输出设计

### 3.1 目录结构

效仿 `outputs/scenarios/<lang>/chunks/{fixture}/` 的组织方式，在 chunks 同级新增 `assembly` 子目录：

```
outputs/scenarios/{lang}/
├── chunks/{fixture}/          # 现有：chunk 分段导出
│   ├── emb/
│   └── bm25/
└── assembly/{fixture}/        # 新增：逐查询组装审查
    └── {baseline}/            # 固定为 full_pipeline（预留扩展位）
        ├── raw/               # 原始召回结果（对照组）
        │   └── {query_id}.md
        └── assembled/         # 组装增强后结果
            └── {query_id}.md
```

- `{query_id}` 直接取 `QueryData.id`（与 judgments 中的标注 id 一致，方便对照相关性标注审查）；
- `raw/` 与 `assembled/` 一一对应同名文件，便于 diff 与并排阅读；
- 语言与 fixture 维度沿用 scenarios 的既有惯例（`python/flask` 等）。

### 3.2 单查询输出格式

每个 `{query_id}.md` 是自包含的人工审查单元：

```markdown
# Query: {query_id}

- **Query**: {query_text}
- **Type**: {qualified|fuzzy|semantic|cross_lang}
- **Mode**: emb_top10            （emb | bm25 | hybrid）

## Results (top-K)

### 1. {entity_name}  score={score:.4f}

- file: `{file_path}:{start_line}-{end_line}`
- entity_ids: [...], segment_id: {segment_id}

#### Content (raw)

<原始 chunk 文本（score 截断线以下的结果只出现在列表，不给正文）>

#### Content (assembled)          ← 仅 assembled/ 目录包含

<SPSRGraphAssembler 输出的 assembled_content，附 AssemblyMetadata
（expanded、truncation 等元信息），若 expanded=false 则标注
"not expanded"，正文与 raw 相同>
```

设计要点：

- **单文件单查询**：审查者按 id 对照 `judgments/{project}.rs` 中的 `relevant_ranges`，逐查询判断组装是否让相关实体的上下文更完整、是否引入噪音（如扩入无关的相邻函数）；
- **raw 与 assembled 分目录而非同一文件**：两者内容可能很长，同文件会淹没对照；分目录支持 `diff -r` 与逐目录浏览；
- **metadata 透传**：`AssembledResult.metadata`（是否扩展开、截断策略等）写入结果块，审查者可区分"组装无效果"与"组装被配置关闭"；
- 顶层生成一个 `index.md`（查询清单 + 各查询 top-1 命中的文件），作为审查入口。

### 3.3 组装调用方式

直接实例化 `SPSRGraphAssembler`，绕过已删除的搜索管线入口：

```rust
use cce_orchestrator::query::assembly::{SPSRGraphAssembler, SearchResultInput, SPSRGraphConfig};

let assembler = SPSRGraphAssembler::new(SPSRGraphConfig::default());
for hit in top_k_hits {
    let input = SearchResultInput {
        id: hit.chunk_id.clone(),
        entity_id: hit.entity_ids.first().map(|id| cce_types::EntityId(*id)),
        name: hit.entity_name.clone(),
        kind: /* chunk.language 推导或留空 */,
        file_path: hit.file_path.clone(),
        start_line: hit.start_line as u32,
        end_line: hit.end_line as u32,
        content: hit_text.clone(),      // benchmark 中的 chunk 文本
        score: hit.score,
    };
    let assembled = assembler.assemble_single(input).await?;
}
```

`SearchResultInput` 的字段与 `ChunkData` 一一对应，无需任何数据结构改动。

**内容来源的两种模式**（example 参数切换，默认前者）：

1. `--content chunk`：用 benchmark 中的 chunk 文本（`extract_unit_from_content` 路径），完全离线、可复现；
2. `--content source`：用 `FixtureSpec::root_path()` + `file_path` 读取真实源文件（`extract_unit` 路径），语义单元提取更完整，但依赖 fixtures 目录存在。

### 3.4 Example 入口

新增 `examples/python/assembly_review_flask.rs`，风格对齐 `gen_bench_flask.rs`：

```rust
use cce_e2e_tests::FixtureSpec;
use cce_e2e_tests::assembly_review::{AssemblyReviewConfig, run_assembly_review};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_assembly_review(AssemblyReviewConfig {
        project: "flask",
        baseline: "full_pipeline",
        language: "python",
        spec: FixtureSpec::python_flask(),
        top_k: 10,
        content_mode: ContentMode::Chunk,
        recall: RecallMode::Emb,
    })
    .await
}
```

核心逻辑放在 `src/assembly_review.rs`（新模块）而非 example 内：

- 加载 rkyv（复用 `bench_data::load_benchmark_data` + `BenchmarkPaths::data_dir`）；
- 离线召回打分；
- 逐查询调用 assembler 并渲染 markdown；
- 写 `outputs/scenarios/{lang}/assembly/{fixture}/{baseline}/{raw|assembled}/{query_id}.md` 与 `index.md`。

先落地 `flask`（python）一条链路验证格式，后续按需复制到其他语言目录（与 gen_bench_* / benchmark_* 系列的复制惯例一致）。

## 4. 与既有设施的关系

| 设施 | 关系 |
|------|------|
| `outputs/scenarios/` 目录与 `review_export.rs` | 复用目录惯例；不改动 `review_export.rs` 本身，assembly 导出是独立 job |
| `BenchmarkPaths`（`judgments/evaluate.rs`） | 复用 `data_dir()` 定位 rkyv；输出路径由新模块自管（scenarios 树，非 benchmark 树） |
| `docs/plan/retrieval_method_benchmark_design.md` | 召回口径（余弦/min-max/entity 对齐）保持一致；本方案不做质量评分，只做输出导出 |
| 生产搜索管线 | 零耦合。assembler 以库形式直接调用，不恢复 `ExecutionStrategy::WithAssembly` |

## 5. 验收与后续处理

### 5.1 验收标准

1. `cargo run --example assembly_review_flask -p cce-e2e-tests` 在已有 `data/benchmark/full_pipeline/flask/bge-m3/bench_data.rkyv` 前提下离线完成（无网络、无 Qdrant）；
2. `outputs/scenarios/python/assembly/flask/full_pipeline/` 下 raw 与 assembled 目录文件一一对应，每个文件可独立阅读；
3. `expanded=true` 的结果能观察到与 raw 不同的内容（结构标记、扩展单元），`expanded=false` 的结果与 raw 一致并明确标注。

### 5.2 人工审查流程

1. 打开 `index.md`，按 query_type 抽样（每类至少 2 条，优先 semantic 与 fuzzy——组装对模糊命中的上下文补偿价值最可疑）；
2. 对照 `judgments` 的 `relevant_ranges`，逐条回答：组装后的内容是否比 raw 更利于回答该查询？
3. 结论记录在本文件 §5.3。

### 5.3 审查结论（待填写）

> 审查完成后在此记录：每类 query_type 的增益判断、典型样例路径、以及最终处置建议。

### 5.4 处置出口（二选一，均需删除 `#![allow(dead_code)]`）

- **结论为增益不足**：删除 `query/assembly/` 整个模块及本 example，生产搜索管线不受影响（当前已断开）；
- **结论为有增益**：以 post-processing 阶段 + config 门控的形式重新集成（ranking 之后按 top-N 组装，不触碰排序），同时恢复配置读取路径并清除 dormant 注解。

## 6. 明确不做

- 不恢复 `ExecutionStrategy::WithAssembly`、`Searcher.assembly_handler` 等管线集成点（清理已完成，等审查结论再动）；
- 不做自动质量评分（与既有 benchmark 的指标体系正交，人工审查先行）；
- 不处理 `full_pipeline_raw_source` 与 `direct_chunking`；
- 不在本方案内引入关系元数据增强（callers/callees 附加）——那是独立的 post-processing 功能，待 assembly 审查有结论后另行设计。
