# Flask 基准召回退化分析

问题编号：ISS-2026-10-26-001  
状态：分析中  
影响范围：Python Flask 基准与 AST 代码分块链路  
对比对象：`crates/app/cce-e2e-tests/outputs/benchmark/flask`（当前） vs `ref/flask`（历史基线）  
确认依据：`outputs/scenarios/python/chunks/flask` 中的实际 chunk 输出、当前/历史 aggregate 指标、`init` 提交的分块配置

## 1. 基准前提：基线不得使用词表

基准基线必须统一去除词表，以反映真实检索水平。词表应作为独立基线单独评估，当前暂无测试计划，因此基线生成不得加载任何 fixture 插件或 vocabulary。

- Flask fixture 本身没有 `.cce/plugins` 注册表，当前 Flask 基线基本未加载词表。
- 但 `file_processor_for` 默认会自动加载 fixture 插件；只有设置 `CCE_BENCH_DISABLE_PLUGINS=1` 才跳过。这使得 media-server 等带 vocabulary 的 fixture 基线混入了词表注入。
- 结论：基准生成应默认禁用插件自动加载，仅在显式启用时才加载 vocabulary，避免基线与真实水平混淆。

## 2. 数据规模差异

| 基线 | 路径 | 历史块数 | 当前块数 |
|---|---|---:|---:|
| full_pipeline | embedding | 623 | 1159 |
| full_pipeline | BM25 | 1167 | 2705 |
| full_pipeline_raw_source | embedding | 623 | 1159 |
| full_pipeline_raw_source | BM25 | 1167 | 2705 |
| direct_chunking | embedding | 738 | 789 |
| direct_chunking | BM25 | 506 | 2645 |

BM25 路径块数膨胀最严重（direct_chunking 从 506 到 2645，约 5.2 倍）。测试状态 `unknown` 比例也明显上升（embedding 约 62%，BM25 约 74%），导致 `no_test` / `impl_only` 过滤不可靠。

## 3. 指标退化（top@5）

| 变体 | 基线 | 路径 | R_a（历史->当前） | P_a（历史->当前） | F1_a（历史->当前） |
|---|---|---|---|---|---|
| all | full_pipeline | BM25 | 0.6630 -> 0.7000 | 0.3556 -> 0.0267 | 0.4388 -> 0.0434 |
| all | full_pipeline | embedding | 0.9185 -> 0.8333 | 0.2756 -> 0.0622 | 0.4071 -> 0.1026 |
| all | full_pipeline_raw_source | BM25 | 0.6630 -> 0.6444 | 0.4044 -> 0.0133 | 0.4745 -> 0.0222 |
| all | full_pipeline_raw_source | embedding | 0.6741 -> 0.5556 | 0.2489 -> 0.0533 | 0.3391 -> 0.0878 |
| all | direct_chunking | BM25 | 0.5333 -> 0.6222 | 0.1644 -> 0.0133 | 0.2430 -> 0.0201 |
| all | direct_chunking | embedding | 0.8519 -> 0.7778 | 0.2267 -> 0.0622 | 0.3497 -> 0.1026 |
| no_test | full_pipeline | BM25 | 0.6852 -> 0.7111 | 0.4267 -> 0.0222 | 0.5036 -> 0.0360 |
| no_test | full_pipeline | embedding | 0.9370 -> 0.8444 | 0.2889 -> 0.0622 | 0.4249 -> 0.1026 |
| no_test | full_pipeline_raw_source | BM25 | 0.7074 -> 0.6556 | 0.4711 -> 0.0133 | 0.5390 -> 0.0212 |
| no_test | full_pipeline_raw_source | embedding | 0.8074 -> 0.7000 | 0.3022 -> 0.0622 | 0.4139 -> 0.1026 |

`related` 层退化更严重：历史基线的 `R_r` 普遍为 0.5-0.98，当前基本为 0.00-0.06。top@10/20/30/50 下趋势一致：召回率基本持平或略降，精度下降一个数量级。

## 4. 从 chunk 输出确认的问题

### 4.1 source range 被整组 span 膨胀

Flask 的 `Flask` 类位于 `src/flask/app.py:109-1625`。当前 chunk 输出中，方法级 chunk 仍声称覆盖整个类范围：

- `group_81_emb_0`：文本仅为 `Flask.__init_subclass__`，但 source range 为 `109-1625`。
- `group_81_emb_6`：文本仅为 `Flask.send_static_file`，但 source range 为 `109-1625`。
- `group_81_emb_24`：文本仅为 `Flask.handle_exception`，但 source range 为 `109-1625`。
- `group_510` 产生 72 个 fragment，其中 `group_510_bm25_0` 的 source range 为 `109-1625`，而文本只是 `__init_subclass__` 的一部分。

后果：一个方法级 chunk 在 range evaluator 中被视为覆盖整个类内所有目标。top-k 中的多个 chunk 都覆盖同一目标范围，只有第一个计为贡献，其余全部计为冗余，导致 precision 崩塌，同时 recall 看起来正常。

### 4.2 单个 group 被过度分裂

`group_81`（Flask 类）在 embedding 路径产生 48 个 chunk，在 BM25 路径产生 72 个 fragment；`group_510` 也产生 72 个 fragment。这些 fragment 的 source range 仍多为方法级或整类级，造成大量重叠覆盖。块数量膨胀与冗余覆盖直接对应第 2 节的块数膨胀。

### 4.3 块粒度与阈值变化

与 `init` 提交相比，当前分块配置发生了以下变化：

- BM25 长度单位从 `words` 变为 `terms`，min 阈值从 80 词降到 45 项，更容易产生小块。
- 新增 glossary reserve（默认 6%）与 glossary max entries（默认 4），切分与合并需要为术语注入预留空间，进一步增加切分次数。
- 引入多层预算（hard / pre-injection / body / decoration），切分、头部成员分组、段上限、跨组合并各自使用不同上限解释；任一环节不一致都会导致块被拆得更细或整文件被丢弃。
- NL 转换增加了身份行、continuation 标识、合成块头、成员列表等装饰文本，若切分时未扣除这些装饰，装配后文本超限，进而触发更多切分或文件级失败。

这些变化与 chunk 输出中的过度分裂现象一致。

### 4.4 测试状态判定退化

当前 `unknown` 测试状态比例很高，说明 `TestInfo` 的 AST/路径规则未覆盖 Flask 的测试布局。`no_test` 与 `impl_only` 变体因此无法正确剔除测试代码，评估结果不可信。

## 5. 问题清单

1. source range 被整组 span 膨胀：方法级 chunk 声称覆盖整个类，造成冗余覆盖与 precision 崩塌。
2. 单个 group 过度分裂：Flask 类被拆成 48-72 个 fragment，块数膨胀。
3. 块粒度阈值变化：min BM25 阈值降低、glossary reserve 引入、多层预算不一致，导致更细粒度切分。
4. NL 装饰未在切分时扣除：身份行、continuation、合成块头等装饰导致文本超限与额外切分。
5. 测试状态大量 `unknown`：测试过滤变体不可靠。
6. 对齐键退化：空实体归属的代码块回退到 segment 键，多个块被折叠，fusion 与评估失真。
7. 合并组丢失 owner 语义：`MergedFragments` 与 `FunctionWithMembers` 合并丢失成员 span 与限定名，削弱 BM25 精确匹配。
8. 调用目标名裁剪过深：丢失限定调用信息，削弱 topic anchor 与调用链。
9. fixture 插件自动加载：vocabulary 注入改变文本分布，基线不可比。

## 6. 改进方向

1. 修正 source coverage：chunk 的 source range 必须表达其实际覆盖的源码行；方法级 chunk 只能覆盖该方法范围，不得回退到整类/整组 span。
2. 统一 chunking 预算：切分、头部成员分组、段上限、跨组合并全部使用同一预算解释；切分时必须扣除装饰与词表预留。
3. 降低过度分裂：提高 min chunk 阈值或限制 fragment 数量，避免单个类产生数十个重叠 fragment。
4. 修正 NL 边界：从实际拼接文本推导边界，对 `boundary.end_byte <= combined_text.len()` 做断言。
5. 修复测试状态判定：完善 `TestInfo` 规则，降低 `unknown` 比例。
6. 修正对齐键：代码块无实体归属时使用 `c:{chunk_id}`，不回退到 segment 键。
7. 修正合并器：保留 owner 与成员 span，合并条件基于稳定不变量。
8. 稳定基线：默认禁用 fixture 插件自动加载，词表作为独立基线。

## 7. 验证建议

- 重新生成 Flask 基准，确认 `group_81` / `group_510` 不再产生整类范围的方法级 chunk。
- 确认 BM25/embedding 块数回落，`unknown` 测试状态比例下降。
- 确认 `related` 层召回恢复，precision 不再较历史基线下降一个数量级。
- 确认基线生成在 `CCE_BENCH_DISABLE_PLUGINS=1` 或默认禁用插件时一致。
