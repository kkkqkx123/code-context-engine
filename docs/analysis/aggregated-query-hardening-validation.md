# 聚合查询加固验证与分析

## 目标

验证聚合查询加固改造后的执行语义、缓存行为、可观测字段和检索效果，并据此确定 `[search.aggregation].score_strategy` 的默认值。

验证覆盖以下生产路径：

- `POST /api/search/aggregated` 对应的 `SearchService::run_aggregated`
- `QueryCoordinator::search_aggregated`
- 聚合缓存、子查询缓存与关系快照版本
- 合并后的单次 rerank、阈值、截断与关系扩展

## 验证环境

| 项目 | 值 |
|---|---|
| 构建 | workspace debug profile |
| 向量后端 | 嵌入式 simvec |
| 全文后端 | 本地 Tantivy |
| 嵌入模型 | 确定性 mock embedder |
| 性能 fixture | 小型 Rust fixture，BM25-only |
| 效果 fixture | `once_cell`、`flask` query review 数据集 |
| 策略基准数据 | `ripgrep` retrieval-method 离线结果 |

绝对延迟与吞吐受 debug 构建和小型 fixture 限制，仅用于横向比较，不代表生产容量。

## 执行项

| 类型 | 命令 | 结果 |
|---|---|---|
| 单元测试 | `cargo test -p cce-orchestrator --lib` | 611 passed |
| 端到端测试 | `cargo test -p cce-e2e-tests --test e2e test_multi_query_aggregation_workflow` | 1 passed |
| 性能测试 | `cargo run -p cce-e2e-tests --example aggregated_search_perf` | 成功 |
| 效果测试 | `cargo run -p cce-e2e-tests --example query_review_oncecell` | 成功 |
| 效果测试 | `cargo run -p cce-e2e-tests --example query_review_flask` | 成功 |
| 策略基准 | `cargo run -p cce-e2e-tests --example benchmark_agg_ripgrep` | 成功 |

性能结果输出：

- `crates/app/cce-e2e-tests/outputs/benchmark/aggregated_search_perf.tsv`

效果结果输出：

- `crates/app/cce-e2e-tests/outputs/scenarios/rust/query_review/once_cell/aggregation_metrics.md`
- `crates/app/cce-e2e-tests/outputs/scenarios/python/query_review/flask/aggregation_metrics.md`

策略参考输出：

- `crates/app/cce-e2e-tests/outputs/benchmark/ripgrep/retrieval_method/aggregation_strategy.md`

## 性能结果

### cold 路径

| 子查询数 | 并发 | p50 | p95 | 吞吐 | 实际执行子查询数 |
|---:|---:|---:|---:|---:|---:|
| 1 | 1 | 3 ms | 3 ms | 333/s | 1 |
| 1 | 16 | 4 ms | 5 ms | 941/s | 16 |
| 2 | 1 | 3 ms | 3 ms | 250/s | 2 |
| 2 | 16 | 6 ms | 7 ms | 696/s | 32 |
| 4 | 1 | 9 ms | 9 ms | 111/s | 4 |
| 4 | 16 | 10 ms | 13 ms | 457/s | 64 |
| 8 | 1 | 7 ms | 7 ms | 143/s | 8 |
| 8 | 16 | 10 ms | 11 ms | 410/s | 128 |

cold 路径中 `executed_sub_queries` 严格等于子查询数乘以并发数，子查询缓存命中数为 0。说明去重和缓存没有误吞请求，fan-out 成本真实发生。

### warm 路径

| 子查询数 | 并发 | p50 | 吞吐 | 执行子查询数 | 聚合缓存命中请求数 |
|---:|---:|---:|---:|---:|---:|
| 1 | 1 | 1 ms | 1000/s | 0 | 1 |
| 1 | 16 | 1 ms | 3200/s | 0 | 16 |
| 2 | 16 | 1 ms | 5333/s | 0 | 16 |
| 4 | 16 | 0 ms | 5333/s | 0 | 16 |
| 8 | 16 | 1 ms | 4000/s | 0 | 16 |

warm 路径全部由聚合缓存直接返回，不再触发任何子查询。吞吐相对 cold 提升约一个数量级。

### 性能分析

1. cold 成本主要来自子查询 fan-out。并发 16 下，子查询数从 1 增至 8，吞吐从约 941/s 降至 410/s。
2. 并发对 cold 有帮助但存在上限；8 子查询时并发 16 的 p95 已到 11 ms，继续增加并发会先增加排队和后端压力。
3. 聚合缓存收益明确：命中后 `executed_sub_queries = 0`，且 `from_cache = true`，不会再进入子查询层。
4. 当前 hard-coded 子查询并发为 4。该值能防止后端被打满，但缺少按后端能力调优的配置入口；后续应考虑配置化。

## 基准自身修正

首次运行发现性能 example 的三个口径问题，已修正后重新采集：

1. 原实现用 4 个基础词生成 8 个子查询，重复项被去重，导致 8 子查询只执行 4 次；现改为 8 个独立词。
2. 原 warm 阶段没有预热，单并发测量实际仍是 cold；现在 warm 阶段先执行一次相同请求再计时。
3. 原 cold 文本只带 `request_index`，跨并发档复用，后续 case 会命中前面 case 的缓存；现在 cold token 包含子查询数和并发数。

修正后 cold 的 `cache_hits_sum` 全为 0，warm 的 `executed_sum` 全为 0，数据才具备可比性。

## 效果结果

### flask

| 类别 | 最佳子查询 F1 | union recall | Noisy-OR merged recall | Max merged recall | 结论 |
|---|---:|---:|---:|---:|---|
| `same:qualified` | 0.1667 | 0.5000 | 1.0000 | 0.5000 | Noisy-OR 明显恢复互补目标 |
| `same:fuzzy` | 0.0000 | 0.0000 | 1.0000 | 0.0000 | Noisy-OR 从零召回拉起目标 |
| `same:semantic` | 0.0000 | 0.0000 | 0.0000 | 0.0000 | 两种策略均无增益 |
| 混合类别 | 0.1538 以下 | 0.3333 以下 | 基本持平 | 基本持平 | 未观察到稳定增益 |

### once_cell

| 类别 | 最佳子查询 F1 | union recall | Noisy-OR merged recall | Max merged recall | 结论 |
|---|---:|---:|---:|---:|---|
| `mixed:qualified+semantic` | 0.1739 | 0.6667 | 1.0000 | 0.6667 | Noisy-OR 有明显收益 |
| `same:fuzzy` | 0.1818 | 1.0000 | 1.0000 | 1.0000 | 已到上限，无差异 |
| `same:qualified` | 0.1818 | 1.0000 | 1.0000 | 1.0000 | 已到上限，无差异 |
| `same:semantic` | 0.1053 | 0.0000 | 0.0000 | 0.0000 | 两种策略均劣于最佳子查询 |
| cross-language | 0.0000 | 0.0000 | 0.0000 | 0.0000 | diagnostic，不参与默认策略结论 |

### 策略表

`aggregation_strategy.md` 当前 9 组对比中：

- Noisy-OR 正向：2 组
- 持平：1 组
- Noisy-OR 负向：6 组

负向主要集中在 semantic 类；正向主要集中在 fuzzy 或词面互补类。

## 可观测字段验证

效果报告的新增列在实际输出中表现符合设计：

- `wall_ms` 与 `reported_ms` 一致，说明 `elapsed_ms` 已改为当前调用墙钟耗时，不再返回缓存条目里的历史耗时。
- 聚合缓存命中时 `executed = 0`，且 `cache = hit`。
- 子查询缓存命中而聚合缓存未命中时，`cache_hits = 2` 且 `cache = miss`，两个缓存层级能够区分。
- 所有报告的 `failed = 0`，没有部分失败污染聚合结果。

## 配置决策

实测数据中，Noisy-OR 只在互补的词面/模糊子查询上稳定获益；在 semantic-only 场景经常无增益甚至劣于最佳子查询。策略表的 9 组对比中，Noisy-OR 负向多于正向。因此继续把它作为全局默认会让多数请求承担不必要的排序放大风险。

默认策略调整为：

```toml
[search.aggregation]
score_strategy = "max"
```

Noisy-OR 仍保留为可选策略，供调用方在确认子查询互补后显式开启。

涉及修改：

- `crates/core/cce-config/src/modules/search.rs`：枚举默认值改为 `Max`
- `deploy/configs/config.example.toml`：示例配置改为 `max` 并说明适用边界
- `docs/app/api/aggregated-search.md`：API 文档同步默认策略
- `docs/app/orchestrator/query/hybrid-fusion-design.md`：融合设计文档同步默认策略并修正权重说明

## 结论

1. 聚合查询加固的核心行为已在实际运行中验证：字段语义清晰、缓存分层正确、部分失败不会进入聚合缓存。
2. 冷路径的主要成本是子查询数量，热路径由聚合缓存承担，收益显著。
3. Noisy-OR 是条件性优化，不是稳定全局最优；默认改为 Max 更符合当前数据。
4. 后续若需要进一步提升效果，应基于 query intent 或子查询相似度动态选择策略，而不是继续寻找单一全局策略。
5. 子查询并发数仍需配置化，并在更大规模项目和真实 embedding 后端上补充测量。
