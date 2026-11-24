# Benchmark 与检索结果的 group 级对齐设计

本文档整理 chunk 检索基准（benchmark）与生产检索在 group 级别对齐的设计思想。对应的问题定义见仓库根目录的 `docs/plan/filled-range-benchmark-design.md`，实现落在 `cce-orchestrator` 的结果物化链路与 `cce-e2e-tests` 的评估链路中。

## 1. 背景与问题

生产检索的 chunk 只是 group 的一个 fragment。group 是一个方法、类或实体的完整语义单元；chunker 会把 group 拆成多个 fragment，每个 fragment 只覆盖 group 的部分行。生产检索虽然按 alignment key 去重，但返回给调用方的仍然是 fragment 的行范围。

这种不一致导致基准测试出现三个偏差：

1. **召回偏低**：一个方法被拆成多个 fragment 时，top-k 必须拿到全部相关 fragment 才能覆盖目标，`R_s` 偏低。
2. **整组膨胀不可用**：如果把每个 fragment 的 range 膨胀成整个 group，召回会恢复，但 precision 会塌陷到接近 0，因为所有 fragment 彼此完全覆盖。
3. **仅填平空白行无效**：只在块内部填平空白行（gap-filled）不会改变目标覆盖，因为目标从不落在缺口行上，无法反映真实 API。

因此评估与生产必须使用同一粒度：生产返回完整方法/实体，基准按 group 级别评测。

## 2. 设计目标

- 生产检索返回完整方法/实体，而不是 fragment。
- 基准按 group 级别评测：同一 group 的多个 fragment 折叠为一个结果，使用 group span 评估覆盖。
- 保留精确 `source_ranges` 作为 chunk 质量诊断，不重新引入整组声明。
- 不新增存储字段；`ChunkRecord` 已保存 `segment_id`，物化时利用它计算 group span。

## 3. 生产检索：group span 物化

### 3.1 物化逻辑

`cce-orchestrator` 的 `entity_mapper::materialize` 负责把 chunk 记录转换为最终 `SearchResult`。改动后，它不再使用 chunk 自己的 fragment span 作为返回范围，而是计算同一 `segment_id` 下所有 chunk 的最小 `start_line` 与最大 `end_line`，作为 group span。

具体行为：

- 从 `chunk_records` 中筛选 `file_path` 与 `segment_id` 都与当前 chunk 相同的记录。
- 对这些记录的 `start_line` / `end_line` 取最小值与最大值，得到 `(group_start, group_end)`。
- `SearchResult.start_line` / `end_line` 使用 group span，而不是 fragment span。
- 读取源码时也使用 group span，因此返回的 body 是完整方法/实体。
- `dedup_by_alignment_key` 仍按 `segment_id` / entity 完成去重，因此同一 group 只保留一个结果。

### 3.2 行号边界

chunk 记录存储的是 tree-sitter 的 0-based 行号。`materialize` 是唯一把 0-based 转换为 1-based 的边界位置：group span 先在 0-based 空间内计算，再通过 `to_one_based_line` 转换为展示用行号。下游不再做行号转换。

### 3.3 精确元数据保留

精确 fragment 元数据继续保留在 `ChunkRecord` / `ChunkData.source_ranges` 中，用于 chunk 质量诊断。生产 API 只暴露 group span，不暴露 fragment 级声明；诊断与报告仍可使用精确范围。

## 4. 基准评测：GroupFilled 视图

### 4.1 RangeMode

`cce-e2e-tests` 的 `judgments::evaluate` 新增 `RangeMode`：

- `RangeMode::Precise`：使用 chunk 原始 `source_ranges`，作为 chunk 质量诊断。
- `RangeMode::GroupFilled`：使用同 `segment_id` 下所有 chunk 的 group span，作为 API 对齐评测。

`RangeMode::suffix` 返回 `""` 或 `"_group"`，用于 retriever 名称后缀。

### 4.2 group span 计算

`group_span_map` 对一组 chunk 计算每个 `segment_id` 的最小 `start_line` 与最大 `end_line`：

- 忽略 `start_line == 0` 的未知范围 chunk。
- 按 `segment_id` 分组，取最小 start 与最大 end。
- 只保留 `max >= min` 的 span。

这与生产 `materialize` 的 group span 来源一致。

### 4.3 chunk 视图转换

`group_filled_chunk` 克隆 chunk，并把其 `source_ranges` 替换为单个 `ChunkSourceRange { start_line: mn, end_line: mx }`。其他字段（分数、文件路径、segment_id、entity_ids 等）保持不变，因此排序、过滤、top-k、target 定义都不变。

### 4.4 group 级扫描去重

在 `evaluate_variant_query` 中，当 `group_level == true` 时，扫描 ranked list 时按 `segment_id` 去重：同一 `segment_id` 的后续 chunk 被跳过，只推进一次。空 `segment_id` 的 chunk 不参与去重，避免把无归属 chunk 错误折叠。

这与生产 `dedup_by_alignment_key` 的键语义一致：代码 chunk 优先使用 `e:{entity_id}`，文档/plain-text chunk 使用 `s:{segment_id}`，无归属时回退到 `c:{chunk_id}`。

### 4.5 双视图运行

`benchmark_runner::run_standard_benchmark` 对每个 baseline 依次运行 `RangeMode::Precise` 与 `RangeMode::GroupFilled`，并把结果追加到同一组结果集合中。因此一次 benchmark 同时输出两套指标。

## 5. 指标与报告

- retriever 名称追加 `_group` 后缀：`emb_group`、`BM25_group`。
- 同时输出 precise 与 group 两套指标，便于对比 chunk 质量与 API 对齐效果。
- `RangeMode::Precise` 保留为 chunk 质量诊断，反映 fragment 级覆盖。
- `RangeMode::GroupFilled` 反映生产 API 返回完整实体后的覆盖能力。

## 6. 一致性保证

- group span 的来源必须是同一 `segment_id` 下 chunk 的 `start_line` / `end_line`，与生产物化逻辑一致。
- 去重键必须与生产 `dedup_by_alignment_key` 一致：优先 entity，其次 segment，最后 chunk。
- 行号统一为 0-based 存储、1-based 展示；生产在 `materialize` 转换，基准通过 `line_range_opt` 得到 1-based 值。
- 排序、过滤、top-k、target 定义不变，只改变评估输入的 range 与去重。
- 精确 `source_ranges` 保留为 chunk 质量诊断，不重新引入整组声明。

## 7. 验证

- 单测：group span 必须覆盖该 group 下所有 chunk 的 fragment span。
- 单测：group-level 扫描中同一 `segment_id` 只推进一次。
- 集成：重新生成 Flask / once_cell 基线，确认 `_group` 指标输出。
- 回归：生产返回的 `start_line` / `end_line` 必须等于 benchmark group span。

## 8. 使用方式

标准 benchmark 入口 `run_standard_benchmark(project_name, judgments, scope)` 已封装双视图运行与报告写出。各语言 `benchmark_{target}` 示例已简化为调用该入口，不再重复编排评估流程。

运行前仍需先生成 `bench_data.rkyv`（`gen_bench_{target}`），再运行对应 benchmark 示例。

## 9. 边界与注意事项

- group span 是完整实体的导航范围，可能包含 fragment 间空白行；它用于 API 对齐评测，不替代精确 `source_ranges` 的 chunk 质量诊断。
- 精确 `source_ranges` 仍反映 chunk 实际覆盖的源码行，用于定位 chunker 问题。
- `GroupFilled` 只改变评估输入与去重，不改变检索排序或 top-k 定义。
- 若 chunk 缺少 `segment_id`，group span 与去重都不会折叠它，避免误合并无归属 chunk。
