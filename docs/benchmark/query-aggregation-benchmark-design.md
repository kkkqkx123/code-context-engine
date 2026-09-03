# 查询聚合模式基准测试设计

## 1. 背景与目标

### 1.1 现状

现有 benchmark 框架（`docs/benchmark/design.md`）仅评测**单路召回**的检索质量：
- 3 种 chunking baseline（`full_pipeline`/`direct_chunking`/`text_cleaner`）
- 2 种检索器（`bge-m3` 向量相似度 / `bm25` 关键词匹配）
- 评估指标：P@5, R@5, R1（基于 source range overlap）

**缺失的覆盖**：
- ❌ 混合召回（HybridRecall：向量 + BM25 融合）
- ❌ 聚合查询（Aggregated Search：多子查询合并）
- ❌ 关系扩展（WithRelationExpansion）
- ❌ 组装模式（WithAssembly：SPSR-Graph 调用链组装）
- ❌ 摘要召回（SummaryRecall）

### 1.2 目标

扩展 benchmark 框架以支持**查询聚合模式**的效果验证：

1. **融合策略对比**：HybridRecall vs DenseRecall vs Bm25Recall
2. **权重调优**：不同 `vector_weight` / `bm25_weight` 组合的效果
3. **聚合收益**：多子查询聚合 vs 单子查询
4. **组装效果**：WithAssembly 对代码理解任务的提升
5. **关系扩展**：WithRelationExpansion 对调用链查询的帮助

---

## 2. 设计原则

### 2.1 与现有框架兼容

- 复用 `crates/cce_e2e_tests/` 的测试工具（`QueryWorkflowTest`, `TestFixture`, `RangeEvaluator`）
- 复用 `benches/scripts/` 的 Python 基准测试基础设施
- 保持现有的 `BenchmarkData` / `QueryData` 数据结构

### 2.2 增量扩展

- 不修改现有单路召回 benchmark
- 新增 `aggregation/` 目录存放聚合模式测试数据
- 新增执行策略维度的评测报告

### 2.3 可重复性

- 固定随机种子
- 固定测试查询集
- 固定融合权重配置集

---

## 3. 测试矩阵设计

### 3.1 分层设计：融合方法 × 执行模式

当前设计将"如何融合"与"如何执行"混为一谈。正确的分层应该是：

**第一层：融合方法（Fusion Method）** - 如何组合多路召回结果

| 方法 | 说明 | 配置参数 |
|------|------|----------|
| `none-vector` | 纯向量召回（基线） | - |
| `none-bm25` | 纯 BM25 召回（基线） | - |
| `minmax` | Min-Max 归一化 + 加权融合 | `vector_weight`, `bm25_weight` |
| `rrf` | Reciprocal Rank Fusion（倒数排名融合） | `k`（排名平滑因子） |

**第二层：权重预设（Weight Presets）** - 仅适用于 `minmax` 和 `rrf`

| Preset | vector_weight | bm25_weight | 说明 |
|--------|---------------|-------------|------|
| `balanced` | 0.5 | 0.5 | 等权融合（默认） |
| `vector-heavy` | 0.7 | 0.3 | 向量优先 |
| `vector-dominant` | 0.9 | 0.1 | 向量主导 |
| `bm25-heavy` | 0.3 | 0.7 | BM25 优先 |
| `bm25-dominant` | 0.1 | 0.9 | BM25 主导 |
| `vector-slight` | 0.6 | 0.4 | 向量略优 |
| `bm25-slight` | 0.4 | 0.6 | BM25 略优 |

**RRF 的 k 值预设**：

| Preset | k 值 | 说明 |
|--------|------|------|
| `rrf-standard` | 60 | 标准值（TREC 推荐） |
| `rrf-diverse` | 30 | 更重视排名差异 |
| `rrf-conservative` | 100 | 更平滑的融合 |

**第三层：执行模式（Execution Mode）** - 召回后的处理

| 模式 | 说明 | 配置 |
|------|------|------|
| `plain` | 纯检索，无后处理 | - |
| `relation` | 调用链关系扩展 | `depth=2`, `strategy=Bidirectional` |
| `assembly` | SPSR-Graph 代码组装 | `depth=1`, `strategy=Bidirectional` |
| `aggregated` | 多子查询聚合 | `sub_queries=2-3` |

### 3.2 完整测试矩阵

**基础组合**（融合方法 × 权重预设 × chunking baseline）：

| 融合方法 | 权重预设数 | 组合数 |
|----------|-----------|--------|
| `none-vector` | 1 | 1 × 3 baselines = 3 |
| `none-bm25` | 1 | 1 × 3 baselines = 3 |
| `minmax` | 7 | 7 × 3 baselines = 21 |
| `rrf` | 3 | 3 × 3 baselines = 9 |
| **小计** | - | **36 条基础评测** |

**执行模式扩展**（在最佳权重预设上应用）：

| 融合方法 | 执行模式 | 组合数 |
|----------|----------|--------|
| `minmax-balanced` | plain/relation/assembly/aggregated | 4 × 3 baselines = 12 |
| `rrf-standard` | plain/relation/assembly/aggregated | 4 × 3 baselines = 12 |
| **小计** | - | **24 条扩展评测** |

**总计**：36 + 24 = **60 条评测结果** × 2 fixtures = **120 条最终结果**

**评测范围控制**：
- 第一阶段（核心）：仅评测 `minmax` 7 权重 + `rrf` 3 k 值 + 2 单路基线 = 12 组合 × 3 baselines = **36 条**
- 第二阶段（扩展）：在最佳权重上应用 4 种执行模式 = **24 条**
- 第三阶段（跨语言）：仅 G4/G8 组 × 最佳配置 = **约 20 条**

### 3.2 查询集扩展

现有 11 个查询分为 4 组（G1-G4），主要覆盖精确符号匹配和语义匹配。需要扩展：

| 新增组 | 类型 | 示例查询 | 预期受益策略 |
|--------|------|----------|--------------|
| G5 - Call Chain | 调用链 | `find all callers of initialize()` | WithRelationExpansion |
| G6 - Code Assembly | 代码组装 | `show me the full authentication flow` | WithAssembly |
| G7 - Multi-Intent | 多意图 | `auth middleware and database connection` | Aggregated Search |
| G8 - Summary | 摘要匹配 | `high level overview of request handling` | SummaryRecall |

每组 3-5 个查询，新增约 15 个查询。

### 3.3 负样本策略

保持现有设计：联合索引目标 fixture + distractor fixture。

---

## 4. 数据结构扩展

### 4.1 BenchmarkData 扩展

```rust
// crates/cce_e2e_tests/src/bench_data.rs
pub struct BenchmarkData {
    pub chunks: Vec<ChunkData>,
    pub queries: Vec<QueryData>,
    pub schemes: Vec<SchemeStore>,
    
    // 新增：执行策略相关的元数据
    pub execution_strategies: Vec<ExecutionStrategyConfig>,
}

// 新增：执行策略配置
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub struct ExecutionStrategyConfig {
    pub strategy_name: String,  // "HybridRecall", "WithAssembly", etc.
    pub vector_weight: Option<f32>,
    pub bm25_weight: Option<f32>,
    pub expansion_depth: Option<usize>,
    pub expansion_strategy: Option<String>,  // "ForwardOnly", "BackwardOnly", "Bidirectional"
    pub enable_assembly: bool,
    pub enable_relation_expansion: bool,
}
```

### 4.2 QueryData 扩展

```rust
pub struct QueryData {
    pub id: String,
    pub text: String,
    pub query_type: QueryType,
    pub group: String,  // G1, G2, ..., G8
    
    // 新增：预期受益策略（用于分析）
    pub expected_best_strategy: Option<String>,
    
    // 现有字段
    pub relevant_ranges: Vec<SourceRange>,
    pub irrelevant_names: Vec<String>,
    pub relevance_judgments: Vec<RelevanceJudgment>,
}
```

### 4.3 存储路径

```
data/benchmark/
├── {baseline}/              # 现有：full_pipeline, direct_chunking, text_cleaner
│   └── {fixture}/
│       └── {model}/
│           └── bench_data.rkyv
└── aggregation/             # 新增：聚合模式专用
    └── {fixture}/
        └── {model}/
            ├── bench_data.rkyv       # 共享的 chunk 数据
            └── strategies.rkyv       # 执行策略配置列表
```

---

## 5. 评测流程

### 5.1 数据生成阶段

```bash
# 现有流程（不变）
cargo run --example gen_bench_data -p cce-e2e-tests

# 新增：生成聚合模式测试数据
cargo run --example gen_bench_aggregation -p cce-e2e-tests
```

**新增 example** `gen_bench_aggregation` 的任务：
1. 复用现有 chunking pipeline 生成 chunks
2. 为每个 chunk 预计算向量（BGE-M3）和 BM25 文本
3. 生成扩展查询集（G5-G8）
4. 生成 8 种执行策略配置
5. 写入 `data/benchmark/aggregation/{fixture}/{model}/`

### 5.2 评测执行阶段

```bash
# 现有评测（不变）
cargo test --test benchmark -- --nocapture

# 新增：聚合模式评测
cargo test --test benchmark_aggregation -- --nocapture
```

**新增 test** `benchmark_aggregation` 的流程：

```rust
// crates/cce_e2e_tests/tests/benchmark_aggregation.rs
#[test]
fn test_aggregation_strategies() {
    let fixtures = ["once_cell", "flask"];
    let baselines = ["full_pipeline", "direct_chunking", "text_cleaner"];
    let strategies = load_strategy_configs();  // 8 种策略
    
    for fixture in fixtures {
        for baseline in baselines {
            for strategy in &strategies {
                run_benchmark(fixture, baseline, strategy);
            }
        }
    }
}

fn run_benchmark(fixture: &str, baseline: &str, strategy: &ExecutionStrategyConfig) {
    // 1. 加载测试数据
    let bench_data = load_benchmark_data(fixture, baseline);
    
    // 2. 初始化 QueryWorkflowTest
    let mut test = QueryWorkflowTest::new(fixture, embedding_config);
    
    // 3. 对每个查询执行对应的策略
    for query in &bench_data.queries {
        let options = build_query_options(query, strategy);
        let result = test.coordinator().search(&options).await?;
        
        // 4. 评估结果（使用 RangeEvaluator）
        let metrics = RangeEvaluator::evaluate(result, &query.relevant_ranges);
        record_metrics(query.id, strategy.strategy_name, metrics);
    }
}
```

### 5.3 指标收集阶段

复用现有 `scripts/collect_metrics.py`，新增聚合模式特定指标：

```python
# scripts/collect_metrics.py
@dataclass
class AggregationMetricSnapshot:
    # 融合相关
    hybrid_fusion_latency_p50_ms: Optional[float]
    fusion_overhead_ms: Optional[float]  # 相比单路召回的额外延迟
    
    # 组装相关
    assembly_expansion_ratio: Optional[float]  # 组装后代码量 / 原始代码量
    call_chain_depth_achieved: Optional[int]
    
    # 聚合相关
    sub_queries_executed: Optional[int]
    deduplication_ratio: Optional[float]  # 去重后结果数 / 去重前
```

### 5.4 结果分析阶段

```bash
python benches/scripts/analyze_results.py --mode aggregation
```

**新增分析维度**：

| 分析类型 | 对比维度 | 输出 |
|----------|----------|------|
| 策略对比 | 8 种执行策略的 P@5/R@5 | 柱状图 |
| 权重调优 | HybridRecall 不同权重组合 | 热力图 |
| 组别分析 | G1-G8 在各策略下的表现 | 分组折线图 |
| 延迟 - 质量权衡 | 策略延迟 vs P@5 | 散点图 |

---

## 6. 评估指标扩展

### 6.1 现有指标（保留）

| 指标 | 定义 |
|------|------|
| P@5 | Precision@5 = top-5 中 relevant / 5 |
| R@5 | Recall@5 = top-5 中 relevant / total_relevant |
| R1 | top-1 是否 relevant（二元 0/1） |

### 6.2 新增指标（聚合模式特定）

| 指标 | 定义 | 适用策略 |
|------|------|----------|
| **Fusion Gain** | (Hybrid P@5 - max(Dense P@5, BM25 P@5)) / max(...) | minmax, rrf |
| **Fusion Gain@10** | 同上但用 P@10 | minmax, rrf |
| **RRF vs MinMax Gap** | rrf_score - minmax_score（同权重下） | 融合方法对比 |
| **Weight Sensitivity** | 同一融合方法在不同权重下的 P@5 标准差 | minmax, rrf |
| **Assembly Coverage** | 组装后代码行数 / 原始代码行数 | assembly |
| **Relation Precision** | 扩展的调用链节点中相关的比例 | relation |
| **Dedup Efficiency** | 1 - (去重后结果数 / 去重前结果数) | aggregated |
| **Strategy Win Rate** | 该策略在所有查询中获胜的比例 | 所有策略 |
| **NDCG@10** | 归一化折损累积增益（考虑排名位置权重） | 所有策略 |

### 6.3 综合评分

为便于跨策略比较，定义**综合评分**：

```
Composite Score = 0.6 * P@5 + 0.3 * R@5 + 0.1 * (1 - normalized_latency)
```

其中 `normalized_latency` 是该策略延迟与最慢策略延迟的比值。

---

## 7. 实现计划

### Phase 1: 数据结构扩展（1 周）

- [ ] 扩展 `BenchmarkData` 和 `QueryData` 结构
- [ ] 新增 `FusionMethod` 枚举（`Minmax`, `Rrf`, `None`）
- [ ] 新增 `ExecutionStrategyConfig` 结构（包含融合方法 + 权重 + 执行模式）
- [ ] 扩展 `gen_bench_data` example 支持聚合模式

### Phase 2: 融合算法实现（1.5 周）

- [ ] 实现 RRF 融合算法（`crates/cce_orchestrator/src/query/retrieval/post_processing/rrf.rs`）
- [ ] 扩展 `HybridFusionConfig` 支持 RRF 参数（k 值）
- [ ] 添加 RRF 单元测试（对比 minmax 的行为差异）
- [ ] 添加 RRF 集成测试（通过 `QueryWorkflowTest`）

### Phase 3: 查询集扩展（1 周）

- [ ] 设计 G5-G8 查询集（每组 3-5 个）
- [ ] 编写 relevance judgments（人工标注）
- [ ] 验证查询集覆盖度（确保每组都有正负样本）

### Phase 4: 评测框架扩展（2 周）

- [ ] 实现 `benchmark_aggregation` test
- [ ] 扩展 `QueryWorkflowTest` 支持所有融合方法和执行模式
- [ ] 实现新增评估指标（Fusion Gain, RRF vs MinMax Gap, Weight Sensitivity, NDCG@10）
- [ ] 实现权重敏感性分析工具

### Phase 5: 分析工具扩展（1 周）

- [ ] 扩展 `analyze_results.py` 支持 RRF 和权重敏感性分析
- [ ] 新增可视化图表：
  - 权重敏感性热力图
  - RRF vs MinMax 对比柱状图
  - 融合方法雷达图（按 G1-G8 分组）
- [ ] 编写分析报告模板

### Phase 6: 基线运行与调优（1.5 周）

- [ ] 运行完整基准测试（60 条结果）
- [ ] 分析权重敏感性（识别最优权重区间）
- [ ] 对比 RRF vs MinMax（按查询组分析）
- [ ] 输出推荐配置文档

**总计**：约 8 周（2 个月）

---

## 8. 预期输出

### 8.1 评测报告

每个 (baseline, strategy, fixture) 组合单独目录：

```
outputs/benchmark/aggregation/{strategy}/{baseline}/{fixture}/{model}/
├── benchmark.txt          # 测试说明
├── results.csv            # 逐 query 指标
├── group_summary.csv      # 按 G1-G8 分组汇总
└── strategy_analysis.txt  # 策略特定分析（如 Fusion Gain）
```

### 8.2 汇总报告

```
outputs/benchmark/aggregation/summary/
├── fusion_method_comparison.csv   # minmax vs rrf vs 单路
├── weight_sensitivity.csv         # 7 种权重预设的敏感性分析
├── rrf_k_sensitivity.csv          # 3 种 k 值的对比
├── execution_mode_comparison.csv  # plain/relation/assembly/aggregated
├── group_analysis.csv             # G1-G8 组别分析
└── recommendation.md              # 推荐配置文档
```

### 8.3 可视化图表

- `fusion_comparison.png`: minmax vs rrf vs 单路的 P@5/R@5 对比柱状图
- `weight_sensitivity_heatmap.png`: 权重敏感性热力图（7 权重 × 3 baselines）
- `rrf_k_comparison.png`: RRF 不同 k 值的对比
- `latency_vs_quality.png`: 延迟 - 质量权衡散点图（所有融合方法）
- `group_radar.png`: G1-G8 雷达图（每融合方法一个）
- `optimal_weight_distribution.png`: 最优权重分布直方图（按查询组）

---

## 9. 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| RRF 实现复杂 | 需要新增融合模块 | 先实现核心逻辑（O(n) 排名->分数），再逐步优化 |
| 标注成本高 | G5-G8 需要人工标注 relevance | 先标注子集（每组 2 个查询），验证流程后再扩展 |
| 评测时间长 | 60 条结果 × 15 查询 = 900 次查询 | 支持并行执行，单 fixture 评测控制在 2 小时内 |
| 结果不稳定 | 向量检索有随机性 | 固定随机种子，每次评测跑 3 次取平均 |
| Qdrant 依赖 | 向量召回需要 Qdrant 服务 | 提供 mock 模式（仅评测 BM25Recall 和 RRF 逻辑） |
| 权重空间暴增 | 7 种权重 × 60 条 = 大矩阵 | 分阶段执行，先跑核心子集，再逐步扩展 |

---

## 10. 成功标准

1. **功能完整**：融合方法 (minmax, rrf) + 权重预设 (7+3) + 执行模式 (4) 全部可评测
2. **数据可靠**：3 次重复评测的标准差 < 5%
3. **分析价值**：能够明确回答"最佳融合方法和权重是多少"
4. **文档齐全**：产出 `docs/benchmark/aggregation-guide.md` 使用指南

---

## 附录 C: 融合算法详解

### Min-Max 归一化 + 加权融合

**公式**：
```
norm_score = (raw_score - min) / (max - min)  # 如果 max == min, norm_score = 1.0
fused_score = alpha * norm_vector + beta * norm_bm25
```

**优点**：
- 保留原始分数的幅度信息
- 实现简单，O(n) 复杂度
- 支持灵活的权重配置

**缺点**：
- 单结果路径归一化为 1.0，无法区分置信度
- 对离群值敏感（一个异常高分会压缩其他分数的分布）
- 跨路径分数分布不可比（向量余弦 vs BM25 TF-IDF）

### RRF (Reciprocal Rank Fusion)

**公式**：
```
# 对每个文档 d，从两个召回列表中获取排名 rank_v(d) 和 rank_b(d)
# 如果文档只在一个列表中出现，另一个排名视为无穷大

rrf_score_v = 1.0 / (k + rank_v(d))
rrf_score_b = 1.0 / (k + rank_b(d))
fused_score = alpha * rrf_score_v + beta * rrf_score_b
```

**参数 k**：
- k=60（TREC 推荐标准值）
- k 越小，排名差异的影响越大（更重视 top 排名）
- k 越大，融合越平滑（所有排名的贡献更均匀）

**优点**：
- 不依赖分数绝对值，只用排名，规避量纲问题
- 对离群值不敏感
- 跨检索器可比性强
- 单结果不会失真（排名 n 的 RRF 分数 = 1/(k+n)）

**缺点**：
- 丢失分数幅度信息（top1=0.99 和 top1=0.51 的置信度差异被忽略）
- 排名相近的结果区分度降低
- 需要额外的排名计算开销

### 选择建议

| 场景 | 推荐融合方法 | 理由 |
|------|-------------|------|
| 向量 + BM25 分数分布差异大 | RRF | 规避量纲问题 |
| 需要保留置信度信息 | Min-Max | 保留分数幅度 |
| 单结果路径常见 | RRF | 不会强制归一化为 1.0 |
| 离线批处理 | 两者都试 | 有足够时间跑完整矩阵 |
| 在线实时检索 | Min-Max | 计算开销略低 |

---

## 附录 D: 权重敏感性分析方法

**目标**：理解权重配置对检索质量的影响，找到最佳配置。

**方法**：

1. **固定其他变量**（chunking baseline, fixture, model），遍历 7 种权重预设
2. **绘制权重 - 质量曲线**：
   - X 轴：vector_weight (0.1, 0.3, 0.4, 0.5, 0.6, 0.7, 0.9)
   - Y 轴：P@5, R@5, NDCG@10
3. **计算敏感性指标**：
   - `sensitivity = std(P@5_across_weights) / mean(P@5_across_weights)`
   - sensitivity > 0.2：高度敏感，需要仔细调优
   - sensitivity < 0.1：低敏感，默认权重即可
4. **识别最优区间**：
   - 找出 P@5 最高的 3 个权重
   - 如果最优权重集中在 vector_weight > 0.5，说明向量召回更强
   - 如果最优权重集中在 bm25_weight > 0.5，说明 BM25 召回更强

**输出**：
- 权重敏感性热力图（fixture × baseline × weight）
- 最优权重推荐表（按查询组 G1-G8 分组）

---

## 附录 A: 查询集设计示例

### G5 - Call Chain（调用链）

| ID | 查询文本 | 预期受益 | 相关实体 |
|----|----------|----------|----------|
| G5-1 | `find all functions that call initialize()` | WithRelationExpansion | `initialize()`, `main()`, `setup()` |
| G5-2 | `who uses the authenticate method` | WithRelationExpansion | `authenticate()`, `login_handler()` |
| G5-3 | `trace the request processing flow` | WithAssembly | `handle_request()`, `middleware()`, `router()` |

### G6 - Code Assembly（代码组装）

| ID | 查询文本 | 预期受益 | 相关实体 |
|----|----------|----------|----------|
| G6-1 | `show me the full authentication implementation` | WithAssembly | `auth_module/*` |
| G6-2 | `complete database connection handling code` | WithAssembly | `db_connect()`, `db_close()`, `query()` |
| G6-3 | `all code related to error handling` | WithAssembly | `error_handler()`, `logging()` |

### G7 - Multi-Intent（多意图）

| ID | 查询文本 | 子查询 1 | 子查询 2 |
|----|----------|----------|----------|
| G7-1 | `auth middleware and database connection` | `authentication middleware` | `database connection pool` |
| G7-2 | `request logging and error tracking` | `request logging` | `error tracking system` |
| G7-3 | `user session management and cleanup` | `session creation` | `session cleanup` |

### G8 - Summary（摘要匹配）

| ID | 查询文本 | 预期受益 | 相关文件 |
|----|----------|----------|----------|
| G8-1 | `high level overview of the application` | SummaryRecall | `README.md`, `ARCHITECTURE.md` |
| G8-2 | `what does this module do` | SummaryRecall | `module_summary.json` |
| G8-3 | `main components and their responsibilities` | SummaryRecall | `component_overview.md` |

---

## 附录 B: 执行策略配置示例

### 数据结构

```rust
pub enum FusionMethod {
    None,                       // 单路召回
    Minmax,                     // Min-Max 归一化 + 加权融合
    Rrf { k: usize },           // Reciprocal Rank Fusion
}

pub enum ExecutionMode {
    Plain,                                          // 纯检索
    Relation { depth: usize, strategy: String },     // 关系扩展
    Assembly { depth: usize, strategy: String },     // 代码组装
    Aggregated { sub_queries: Vec<String> },         // 多子查询聚合
}

pub struct ExecutionStrategyConfig {
    pub fusion_method: FusionMethod,
    pub vector_weight: Option<f32>,
    pub bm25_weight: Option<f32>,
    pub execution_mode: ExecutionMode,
}
```

### 配置示例

```rust
// ========== Min-Max 融合 ==========

// balanced - 等权融合
ExecutionStrategyConfig {
    fusion_method: FusionMethod::Minmax,
    vector_weight: Some(0.5),
    bm25_weight: Some(0.5),
    execution_mode: ExecutionMode::Plain,
}

// vector-heavy - 向量优先
ExecutionStrategyConfig {
    fusion_method: FusionMethod::Minmax,
    vector_weight: Some(0.7),
    bm25_weight: Some(0.3),
    execution_mode: ExecutionMode::Plain,
}

// bm25-dominant - BM25 主导
ExecutionStrategyConfig {
    fusion_method: FusionMethod::Minmax,
    vector_weight: Some(0.1),
    bm25_weight: Some(0.9),
    execution_mode: ExecutionMode::Plain,
}

// ========== RRF 融合 ==========

// rrf-standard - 标准 k 值
ExecutionStrategyConfig {
    fusion_method: FusionMethod::Rrf { k: 60 },
    vector_weight: Some(0.5),
    bm25_weight: Some(0.5),
    execution_mode: ExecutionMode::Plain,
}

// rrf-diverse - 更重视排名差异
ExecutionStrategyConfig {
    fusion_method: FusionMethod::Rrf { k: 30 },
    vector_weight: Some(0.5),
    bm25_weight: Some(0.5),
    execution_mode: ExecutionMode::Plain,
}

// ========== 执行模式扩展 ==========

// minmax-balanced + assembly
ExecutionStrategyConfig {
    fusion_method: FusionMethod::Minmax,
    vector_weight: Some(0.5),
    bm25_weight: Some(0.5),
    execution_mode: ExecutionMode::Assembly {
        depth: 1,
        strategy: "Bidirectional".to_string(),
    },
}

// rrf-standard + relation expansion
ExecutionStrategyConfig {
    fusion_method: FusionMethod::Rrf { k: 60 },
    vector_weight: Some(0.5),
    bm25_weight: Some(0.5),
    execution_mode: ExecutionMode::Relation {
        depth: 2,
        strategy: "Bidirectional".to_string(),
    },
}

// minmax-balanced + aggregated
ExecutionStrategyConfig {
    fusion_method: FusionMethod::Minmax,
    vector_weight: Some(0.5),
    bm25_weight: Some(0.5),
    execution_mode: ExecutionMode::Aggregated {
        sub_queries: vec![
            "authentication middleware".to_string(),
            "database connection".to_string(),
        ],
    },
}
```
