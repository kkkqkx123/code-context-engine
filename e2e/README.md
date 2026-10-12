# 端到端功能与性能测试

本目录是端到端测试的实际落点,脚本经超文本接口驱动真实服务。微基准归各归属 crate 的 `benches` 目录,不在此处。

## 目录结构

```
e2e/
├── README.md               # 本文件
├── config.toml             # 服务启动配置示例
├── fixtures/               # 规模分档的示例数据
│   ├── once_cell/          # 小规模真实项目(约 27 文件)
│   ├── flask/              # 中规模真实项目(约 235 文件)
│   ├── small/              # 待填充(目标约 100 文件)
│   └── medium/             # 待填充(目标约 1000 文件)
├── scripts/
│   ├── main.py             # 入口: functional / benchmark / all
│   ├── test_runner.py      # 功能场景与规模分档执行
│   ├── service_manager.py  # 服务启停
│   ├── collect_metrics.py  # 指标采集
│   └── analyze_results.py  # 统计与报告(含 P95/P99)
└── results/                # 本地测试输出(已忽略，仓内示例仅作格式参考)
    ├── raw/                # 原始指标数据
    ├── processed/          # 统计后数据
    └── reports/            # 最终报告
```

## 快速开始

```bash
pip install requests
python e2e/scripts/main.py functional --fixture once_cell
python e2e/scripts/main.py benchmark --fixture flask --iterations 3
python e2e/scripts/main.py all --fixture once_cell --iterations 5
```

`--fixture` 可选 `once_cell`(小)或 `flask`(中),`--project` 缺省时与 `--fixture` 同名,用于结果目录划分。

## 测试场景

功能模式覆盖一次全量索引、一次幂等重索引、向量与全文与混合三类检索、无结果检索、存储状态与索引统计。性能模式在功能全集之外逐轮记录索引与混合检索耗时及前后指标快照,由 `analyze_results.py` 给出最小值、最大值、均值、中位数、P95、P99 与标准差。

## 规模分档

| 分档 | 数据 | 用途 |
| ---- | ---- | ---- |
| 小 | `fixtures/once_cell` | 默认冒烟与本地快速验证 |
| 中 | `fixtures/flask` | 索引与检索放大对照 |
| 大 | 待补 | 终局回放阈值跳变等悬崖行为,暂无数据 |

`small` 与 `medium` 目录目前仅为占位,目标分别为约 100 与约 1000 文件,补齐前不要引用。

## 基线与回归

`results/` 本地运行输出被版本控制忽略,不做趋势追踪,仓内示例仅作格式参考。需要回归判定时,将某次 `reports/<fixture>/final_report.json` 复制为基线存档(产物归档或文档附录),人工对比索引与检索分位延迟,退化超阈即转优化任务。服务等级目标待中大规模数据补齐后制定。

## 维护指南

- 新增场景只改 `test_runner.py`,新增规模只改 `FIXTURES` 映射与本说明的表格。
- 功能阈值按小规模标定,中规模只要求更大下界,不收紧断言。
- 运行过慢时减少 `--iterations`,不要改默认配置。
