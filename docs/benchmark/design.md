# Benchmark 设计文档

## 概述

CCE 检索 benchmark 用于量化评估不同 code chunking 方案 + 不同检索器组合的检索质量。采用 3 种预处理方案 × 2 种检索器的交叉矩阵设计。

## 流程

```
cargo run --example gen_bench_data -p cce-e2e-tests    # 生成基准数据
cargo test --test benchmark -- --nocapture              # 评测 + 输出报告
```

- 生成阶段：对每个 baseline，将代码源文件切分成 chunk，通过 BGE-M3 一次批量 embedding，将 (vectors + texts) 写入 `data/benchmark/{baseline}/{fixture}/{model}/bench_data.rkyv`。
- 评测阶段：从 `.rkyv` 读取数据，对每个 scheme 分别运行两种检索器的评分逻辑，输出到 `outputs/benchmark/{baseline}/{retriever}/{fixture}/{model}/{benchmark.txt,results.csv}`。

## Baseline 矩阵

| Baseline | Chunking方式 | 文本来源 |
|---|---|---|
| full_pipeline | IndexOrchestrator (ParseCoordinator → AstToNL) | raw_code, nl_desc, hybrid 三种 scheme |
| direct_chunking | EntityChunker (ParseCoordinator → PreprocessingPipeline → GroupChunker) | 实体组 raw source（不做 NL 转换） |
| text_cleaner | 同上 EntityChunker，传 `cleaned=true` | 实体组 source 经 strip_non_doc_comments 处理 |

3 baselines × 2 retrievers = 6 条评测结果。

## 检索器

| Retriever | 评分方法 | 输入 |
|---|---|---|
| bge-m3 | cosine_similarity | 预计算 chunk_vectors / query_vectors |
| bm25 | BM25Okapi (k1=1.5, b=0.75) | chunk_texts / query_texts（简单 word 分词） |

每个 `bench_data.rkyv` 文件同时存储两种检索器的数据，无需额外索引文件。

## 数据结构

```
BenchmarkData
├── chunks: Vec<ChunkData>         // 所有 chunk 元信息
│   ├── chunk_id, entity_name, file_path, raw_code, nl_text
├── queries: Vec<QueryData>        // 查询集
│   ├── id, text, query_type, relevant_names, irrelevant_names
└── schemes: Vec<SchemeStore>      // 每个方案一个 store
    ├── name, dimension
    ├── chunk_vectors: Vec<f32>    // 扁平 chunk 嵌入
    ├── query_vectors: Vec<f32>    // 扁平 query 嵌入
    └── chunk_texts, query_texts: Vec<String>  // BM25 用
```

序列化格式：rkyv（零拷⻉反序列化）。

## 查询设计

11 个查询分 4 组，覆盖不同难度和场景：

| 组 | 类型 | 示例 |
|---|---|---|
| G1 - Exact symbol | 精确名 | `OnceCell::get_or_init` |
| G2 - Semantic init | 语义匹配 | `initialize a value only once` |
| G3 - Lazy eval | 语义匹配 | `lazy evaluation delayed computation` |
| G4 - Cross-lang | 跨语言 | `懒加载的全局变量` |

每组独立统计（GroupMetrics），可分析 baseline/retriever 在不同场景下的表现差异。

## 评估指标

| 指标 | 定义 |
|---|---|
| P@5 | Precision@5 = top-5 中 relevant / 5 |
| R@5 | Recall@5 = top-5 中 relevant / total_relevant |
| R1 | top-1 是否 relevant（二元 0/1） |

Relevance 判断：基于 entity_name 匹配（支持无歧义的前缀/后缀匹配）。不在 relevant_names 也不在 irrelevant_names 的 chunk 视为 negative。

## 负样本策略

索引联合目标 fixture（once_cell）和 distractor fixture（sorting/matrix 工具函数）。生成时两个 fixture 各自独立索引后合并：

- once_cell chunks → 正样本域
- distractor chunks → 负样本池（其 entity_name 写入各 query 的 irrelevant_names）

评测时 chunk pool 包含两者最接近真实场景——无关代码不应被召回。

## 输出

每个 (baseline, retriever, fixture, model) 组合单独目录：

```
outputs/benchmark/{baseline}/{retriever}/{fixture}/{model}/
├── benchmark.txt    # 测试用例描述 + 评估方法说明
└── results.csv      # 逐 query 指标 + 按组汇总 + 全局汇总
```

### Design Decisions

- EntityChunker 使用与 full_pipeline 相同的 ParseCoordinator + PreprocessingPipeline + GroupChunker，跳过 NL 转换。产物 GroupConversions 中 `combined_source` 既是 embedding_text 也是 bm25_text。
- ChunkingConfig max_tokens=8192 确保实体边界即 chunk 边界，不作进一步切分。
- full_pipeline 产出 3 个 SchemeStore（raw_code/nl_desc/hybrid），entity-based 各产出 1 个。
- BM25 评分在 evaluator 中即时计算，不依赖 tantivy 索引。
- 数据存储路径不含时间戳/UUID，`bench_data.rkyv` 固定文件名，直接覆盖。
