# 性能基准测试

本目录包含 Code Context Engine 的性能基准测试套件。

## 目录结构

```
benches/
├── README.md                    # 本文件
├── configs/                     # 测试配置文件
│   ├── small_project.toml       # 小项目配置示例
│   ├── medium_project.toml      # 中项目配置示例
│   └── large_project.toml       # 大项目配置示例
├── fixtures/                    # 测试项目fixtures
│   ├── small/                   # 小型测试项目 (~100文件)
│   │   ├── README.md            # 填充说明
│   │   └── .cce/
│   │       └── config.toml      # 项目级配置
│   ├── medium/                  # 中型测试项目 (~1000文件)
│   │   ├── README.md
│   │   └── .cce/
│   │       └── config.toml
│   └── large/                   # 大型测试项目 (~10000+文件)
│       ├── README.md
│       └── .cce/
│           └── config.toml
├── scripts/                     # Python测试脚本
│   ├── run_benchmark.py         # 主测试脚本
│   ├── collect_metrics.py       # Metrics采集脚本
│   ├── analyze_results.py       # 结果分析脚本
│   └── generate_report.py       # 报告生成脚本
└── results/                     # 测试结果输出 (已加入.gitignore)
    ├── raw/                     # 原始metrics数据
    ├── processed/               # 处理后数据
    └── reports/                 # 生成的报告
```

## 快速开始

### 1. 准备环境

```bash
# 构建可执行文件
cargo build --release --bin cce
cd cce-cli && cargo build --release
cd ..

# 复制到 bin 目录
mkdir -p bin
cp target/release/cce bin/
cp cce-cli/target/release/cce-cli bin/

# 安装Python依赖
pip install requests
```

### 2. 填充 Fixtures

按照各 fixture 目录中的 README.md 说明,从开源项目复制代码。

**当前状态**: ⏳ Fixtures 为空壳,需要填充真实代码

- [ ] `fixtures/small/` - 待填充 (~100文件)
- [ ] `fixtures/medium/` - 待填充 (~1000文件)
- [ ] `fixtures/large/` - 待填充 (~10000+文件)

### 3. 运行测试

```bash
# 索引性能测试
python benches/scripts/run_benchmark.py \
  --fixture small \
  --scenario index \
  --iterations 5

# 查询性能测试
python benches/scripts/run_benchmark.py \
  --fixture medium \
  --scenario query \
  --iterations 3

# 分析结果
python benches/scripts/analyze_results.py \
  --input "benches/results/raw/*.json" \
  --output benches/results/reports/report.md
```

## 测试场景

### Index (索引性能)

测量从零开始完整索引项目的性能。

**关键指标**:
- 总索引时间
- 文件扫描速度
- 向量插入吞吐量
- 内存使用峰值

### Query (查询性能)

测量不同类型查询的响应时间。

**关键指标**:
- 搜索延迟 (P50/P95/P99)
- 不同查询类型对比 (vector/bm25/hybrid)
- 并发查询吞吐量

### Hot Update (热更新性能)

测量文件变更后的增量索引性能。

**关键指标**:
- 增量更新时间
- 缓存命中率
- 事件处理延迟

## 配置文件

### 全局配置 (bin/.cce/config.toml)

从项目根目录的 `config.example.toml` 复制,并根据测试需求调整。

### 项目配置 (fixtures/*/.cce/config.toml)

每个 fixture 可以有独立的项目级配置,覆盖全局设置。

## Metrics 采集

系统通过 `/api/metrics/json` 和 `/api/metrics/prometheus` 端点提供性能指标。

**主要指标类别**:
- 索引相关: `indexing_duration_ms`, `files_scanned_total`, etc.
- 查询相关: `search_latency_ms`, `vector_search_queries_total`, etc.
- 存储相关: `qdrant_upsert_latency_ms`, `bm25_search_latency_ms`, etc.
- 资源使用: `memory_bytes`, `cpu_usage_percent`, etc.

详细指标列表见: [performance_benchmark_analysis.md](../../docs/tests/performance_benchmark_analysis.md)

## 结果分析

测试结果保存在 `results/` 目录:

- `raw/`: 原始 JSON 数据
- `processed/`: 处理后的统计数据
- `reports/`: Markdown 格式的报告

使用 `analyze_results.py` 自动生成报告,包含:
- 平均值、中位数、P95、P99
- 标准差和变异系数
- 关键 metrics 摘要

## CI/CD 集成

基准测试已集成到 GitHub Actions:

- **触发条件**: 
  - Push 到 main 分支
  - 每天凌晨 2 点定时运行
  
- **执行内容**:
  - 构建可执行文件
  - 运行 small fixture 索引测试
  - 生成并上传报告

配置文件: `.github/workflows/benchmark.yml`

## 性能回归检测

建立基线数据库,自动检测性能退化:

```bash
# 检查当前结果与基线的差异
python benches/scripts/check_regression.py \
  --current benches/results/raw/latest.json \
  --baseline benches/results/baseline.json
```

如果性能下降超过 10%,会在 CI 中标记为失败。

## 维护指南

### 更新 Fixtures

建议每季度更新一次 fixtures,保持代表性:

1. 选择新的开源项目或更新现有项目版本
2. 确保包含多种语言和实体类型
3. 验证所有文件可正常解析
4. 更新 README.md 中的元数据

### 调整测试场景

根据架构变化添加或删除测试场景:

1. 在 `run_benchmark.py` 中添加新的 scenario 方法
2. 定义对应的 CLI 命令序列
3. 指定需要采集的 metrics
4. 更新文档

### 优化测试速度

如果测试运行过慢:

- 减少 iterations 数量
- 使用更小的 fixture
- 降低并发度
- 调整 batch size 配置

## 常见问题

### Q: 为什么使用端到端测试而不是单元测试?

A: 性能基准测试需要反映真实场景,端到端测试能捕捉组件间交互的影响,而单元测试只能测量孤立组件的性能。

### Q: 如何选择合适的 fixture 规模?

A: 
- 开发阶段: 使用 small fixture 快速验证
- PR 检查: 使用 medium fixture 平衡速度和准确性
- 发布前: 使用 large fixture 全面评估

### Q: Metrics 数据不准确怎么办?

A: 
- 确保服务器完全启动后再开始测试
- 每次测试前清除之前的索引数据
- 增加 iterations 数量取平均值
- 检查系统负载是否稳定

### Q: 如何对比不同版本的性能?

A: 
- 使用相同的 fixture 和配置
- 运行相同次数的 iterations
- 使用 `analyze_results.py` 生成对比报告
- 关注 P95/P99 而非仅平均值

## 参考资料

- [性能基准测试设计方案](../../docs/tests/performance_benchmark_analysis.md)
- [Metrics 模块文档](../../src/metrics/README.md)
- [CCE CLI 使用指南](../../cce-cli/README.md)

## 贡献指南

欢迎提交新的测试场景、优化脚本或改进 fixtures。提交前请:

1. 在本地运行测试验证
2. 更新相关文档
3. 确保不破坏现有测试
4. 提供性能对比数据 (如有优化)
