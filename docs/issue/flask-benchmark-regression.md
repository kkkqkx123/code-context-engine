# Flask 基准召回分析

问题编号：ISS-2026-10-26-001  
状态：分析中  
影响范围：Python Flask 基准的 chunk 输出与 range 评估  
确认依据：`outputs/scenarios/python/chunks/flask` 中的实际 chunk 输出、`outputs/benchmark/flask` 中的 aggregate 指标

## 1. 基准前提：基线不得使用词表

基准基线必须统一去除词表，以反映真实检索水平。词表应作为独立基线单独评估。当前暂无词表基线测试计划，因此基线生成不得加载任何 fixture 插件或 vocabulary。

- Flask fixture 本身没有 `.cce/plugins` 注册表，当前 Flask 基线未加载词表。
- `file_processor_for` 默认会自动加载 fixture 插件；现已改为默认禁用，仅在显式设置 `CCE_BENCH_ENABLE_PLUGINS=1` 时加载，避免基线混入 vocabulary。

## 2. chunk 输出确认的事实

### 2.1 大 group 被过度分裂

Flask 的 `Flask` 类（`src/flask/app.py:109-1625`）被拆成大量 chunk：

- embedding 路径：`group_81` 产生 48 个 chunk。
- BM25 路径：`group_81` 产生 72 个 fragment，`group_510` 也产生 72 个 fragment。
- `helpers.send_file`（`helpers.py:417-540`）被拆成 7 个 chunk，全部 source range 为 `417-540`。

这些 chunk 的 source range 仍为整函数/整类范围，这是当前 source coverage 设计的必然结果；基线也采用类似方案。问题不在于 source range 本身，而在于同一个范围被拆成过多 chunk，导致 top-k 中大量 chunk 覆盖相同目标范围。

### 2.2 chunk 之间存在大块重叠

chunk 输出中频繁出现 `prev_overlap`，重叠量通常为 50-100 tokens。例如 `group_5757_emb_0` 到 `group_5757_emb_6` 之间每个 chunk 都带有 60-95 tokens 的前序重叠。这使相邻 chunk 共享大量文本，进一步增加 top-k 中的冗余覆盖。

### 2.3 MemberBoundary 与 SentenceBoundary 产生双重覆盖

- `MemberBoundary` 将一个类拆成多个方法级 chunk，同时保留整类范围的 chunk，导致同一个类在 top-k 中同时出现方法级与整类级覆盖。
- `SentenceBoundary` 将一个长函数拆成多个句子级 chunk，每个 chunk 仍声称覆盖整个函数范围，导致同一函数在 top-k 中出现多次且全部覆盖相同目标。

### 2.4 空实体归属

当一个 chunk 只承载 group header（或 header 加裸字段）时，`content_entity_ids` 会把 header 过滤掉，最终变成空列表。随后代码回退到 `group.all_entity_ids()`，导致该 chunk 被归属到整个 group 的所有实体。这种空归属 chunk 会错误地进入多实体对齐与 BM25 标题生成，造成对齐键与实体归属失真。

这是真正的问题，应正面修复：header chunk 必须保留 header 实体本身作为 content entity，不能在过滤后留下空列表再回退到全组实体。

### 2.5 测试状态 unknown

大量 chunk 的测试状态为 `unknown`。但这不一定表示它们是测试文件。测试判定应只把明确符合测试模式的文件标记为 test；`unknown` 一律视为非测试文件。因此 high unknown count 本身不是召回退化的根因，评估时应按此规则处理。

## 3. 指标表现

BM25 路径的召回率有所上升，整体检索表现并不退化，因此 BM25 不应被列为回归问题。range 评估中的 precision 下降主要来自 chunk 过度分裂与重叠，而不是 BM25 检索质量本身。

embedding 路径的 precision 下降同样主要来自 top-k 中的冗余 chunk：同一个函数/类被拆成多个 chunk，全部覆盖相同目标范围，只有第一个计为贡献，其余全部计为冗余。

## 4. 问题清单

1. 大 group 过度分裂：`Flask` 类被拆成 48-72 个 chunk，`send_file` 被拆成 7 个 chunk。
2. chunk 之间大块重叠：`prev_overlap` 通常 50-100 tokens，增加冗余覆盖。
3. MemberBoundary 双重覆盖：类被拆成方法级 chunk，同时保留整类范围 chunk。
4. SentenceBoundary 过度切分：长函数被拆成多个句子级 chunk，全部覆盖相同范围。
5. 空实体归属：header-only chunk 的 `content_entity_ids` 为空并回退到全组实体，造成对齐与归属失真。
6. 评估冗余：range evaluator 中，同一目标范围的多个 chunk 只有第一个计为贡献，其余计为冗余，导致 precision 下降。

## 5. 修复方向

1. 减少过度分裂：提高最小 chunk 阈值，或限制单个 group 的 fragment 数量；避免对代码实体使用 SentenceBoundary 式切分。
2. 减少重叠：降低 `prev_overlap`，或使相邻 chunk 不共享大段文本。
3. 避免双重覆盖：同一 group 不应同时产出整类范围 chunk 与大量方法级 chunk；应在分组或合并阶段去重。
4. 修复空实体归属：header chunk 必须保留 header 实体作为 content entity；若过滤后为空，应回退到 header 实体，而不是全组实体。
5. 调整评估口径：保持 source range 的整组语义不变；通过减少 fragment 数量与重叠来改善 precision，而不是改变 range 语义。
6. 测试判定：将 `unknown` 统一视为非测试文件，只明确标记测试模式文件为 test。

## 6. 验证建议

- 重新生成 Flask chunk 输出，确认 `group_81` 与 `group_510` 的 fragment 数量显著下降。
- 确认 `send_file` 不再被拆成 7 个重叠 chunk。
- 确认 header-only chunk 不再回退到全组实体。
- 重新运行 benchmark，确认 range precision 不再因冗余 chunk 而异常偏低。
