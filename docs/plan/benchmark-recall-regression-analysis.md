# Benchmark 召回全面退化分析与正确设计

## 1. 背景

自 `ea980bc` 以来，benchmark 中的召回几乎退化为随机。问题不局限于单个模块，而是 **解析 → 分组 → chunk 元数据 → 存储对齐 → benchmark 评估** 的整条链路同时发生语义漂移。召回退化不是评估噪声，而是 chunk 的文本、实体归属、source range、对齐键同时失去了稳定含义。

## 2. 根因分析

### 2.1 解析与 chunk 层

#### 2.1.1 无归属实体的 chunk 被清空 source range

`source_coverage_for_entity_ids` 在 `entity_ids` 为空时，原先会保留 group span 作为可评估 range；当前改为返回空 `ranges`。  
benchmark 的 range evaluator 只消费 `source_ranges`，空 range 的 chunk 永远无法覆盖目标。因此一旦 chunking 过程中出现归属实体被过滤的情况，这些 chunk 就变成评估不可见。

#### 2.1.2 header 被从 `content_entity_ids` 中排除

当前逻辑将 header 从 `content_entity_ids` 中过滤掉。  
header 自身的 chunk 也会变成空实体列表，导致：

- storage 层无法建立 entity → vector 映射；
- benchmark `ChunkData.entity_ids` 为空；
- 对齐键退化为 segment/chunk 级别。

这会把同一组内的多个 chunk 折叠成同一个对齐键，召回粒度丢失。

#### 2.1.3 NL 边界偏移假设脆弱

`compute_nl_entity_boundaries` 按文本长度拼接偏移，并在成员之间固定增加分隔符长度。  
但 continuation、合成 head 截断、空成员跳过等路径可能破坏这个不变量。一旦偏移错位，实体归属就会漂移到错误 chunk，或完全无归属。

#### 2.1.4 调用目标名裁剪过深

`call_target_name` 裁掉 `->` / `.` 左侧接收者，只保留被调用名。  
这会丢失限定调用信息，削弱 topic anchor、调用链文本和 BM25 精确匹配的区分度。

### 2.2 分组层

#### 2.2.1 `SmallFragmentMerger` 纳入 `FunctionWithMembers`

当前 `is_merge_candidate` 允许 `FunctionWithMembers` 参与合并，但 `create_merged_group` 在合并时跳过其内部成员。  
结果是局部符号的 source span 和文本从合并组中消失，chunk 覆盖不完整。

#### 2.2.2 merged group 失去 owner 语义

多处逻辑把 `MergedFragments` 视为“无 owner”：

- `resolve_owner` 对 merged group 返回空；
- BM25 标题不再用 group 名称限定成员；
- continuation head 改为文件 scope 或成员名列表。

这会削弱 `Owner.member` 这类限定共现，BM25 精确匹配和向量语义锚点都被削弱。

#### 2.2.3 合并策略不稳定

proximity、parent scope、test boundary 等条件被大量重写，导致 chunk 粒度在不同文件和语言之间不稳定，召回评估的可比性下降。

### 2.3 benchmark 基础设施

#### 2.3.1 对齐键退化

`alignment_key` 只取 `entity_ids.first()`。  
当 `entity_ids` 为空时，回退到 `segment_id`。  
如果多个 chunk 共享同一个 group/segment id，它们在 fusion 中会被折叠成一个键，召回评估看到的是被“去重”后的伪排名，而不是真实 chunk 排名。

#### 2.3.2 source range 为空导致评估失效

如 2.1.1 所述，空 `source_ranges` 的 chunk 在 range evaluator 中贡献为 0，目标无法被 recovered。即使检索排序正确，评估也看不到命中。

#### 2.3.3 fixture 插件自动加载改变文本分布

`file_processor_for` 现在会自动加载 fixture 的 `.cce/plugins`。  
带 vocabulary 插件的 fixture 会注入额外术语，导致不同 fixture 的文本分布不一致，benchmark 结果不再可比。

#### 2.3.4 chunk 级去重放大漂移

单路径现在只保留 chunk index 的首次出现。  
如果上游 `entity_ids` 已经漂移，这个去重会把真正相关的 chunk 排挤出 top-k，放大随机性。

## 3. 正确设计

### 3.1 source coverage 的正确语义

source range 应表达 chunk **实际覆盖的源码行**，而不是归属实体的导航信息。

- 有归属实体时：使用这些实体的 span，并合并相邻范围；
- 无归属实体时：不应直接清空 range，而应保留 chunk 对应的 group 局部 span；
- group span 只能作为 navigation span，不能作为精确 range。

这样 range evaluator 才能正确判断 chunk 是否覆盖目标。

### 3.2 content_entity_ids 的正确语义

`content_entity_ids` 应表达 chunk **实际承载内容的实体**。

- header 的 chunk 必须包含 header 实体本身；
- 其他成员 chunk 不应错误地把 header 当作内容；
- 无归属实体时，应回退到该 chunk 对应的局部实体，而不是空列表。

只有这样，storage 层的 entity mapping、benchmark 的对齐键、fusion 的 per-entity expansion 才能稳定工作。

### 3.3 NL 边界计算的正确设计

NL 边界应从 **实际拼接文本** 推导，而不是从假设的固定偏移推导。

- header 与 member 之间的真实分隔符应作为边界计算的唯一依据；
- 空成员跳过时，偏移应按实际文本推进；
- 合成 head 截断后，边界应基于截断后的文本重新计算；
- 应加入 debug 断言，确保 `boundary.end_byte <= combined_text.len()`。

### 3.4 分组合并的正确设计

合并器的职责是减少碎片，而不是改变实体归属。

- `FunctionWithMembers` 不应被合并，或合并时必须保留其内部成员 spans；
- merged group 的标题和 continuation head 应保留 owner 或文件 scope，不能退化成通用描述；
- 合并条件应基于 source span、parent scope、test boundary 等稳定不变量，而不是基于文本长度或名称。

### 3.5 对齐键的正确设计

对齐键应表达 chunk 的 **语义归属**，而不是物理 chunk id。

- code chunk：优先使用 `e:{entity_id}`；
- document/plain-text chunk：使用 `s:{segment_id}`；
- 无归属时：使用 `c:{chunk_id}`，避免多个 chunk 被折叠成同一个键。

这样 fusion 才能正确进行 per-entity expansion 和 per-key 选择。

### 3.6 benchmark 基础设施的正确设计

- source range 为空时，不应直接评估为 0，而应使用 group 局部 span 作为 fallback；
- 对齐键为空时，应回退到 chunk id，而不是 segment id；
- fixture 插件加载应可配置，benchmark 模式下应使用固定 registry；
- 单路径去重应基于 chunk id，而不是 entity id，避免多实体 chunk 被错误折叠。

## 4. 修复路线

1. 恢复 source range 的最小可用性；
2. 保证 header 实体在自身 chunk 的 `content_entity_ids` 中保留；
3. 校验 NL 边界偏移与拼接文本一致；
4. 合并器不对 `FunctionWithMembers` 合并，或合并时保留成员 spans；
5. 恢复 merged group 的 owner / 限定名；
6. benchmark 中 entity_ids 为空时回退到 chunk id；
7. 固定 benchmark 的插件集。

## 5. 验证

- 单元测试：source coverage、NL 边界、content_entity_ids、对齐键；
- 集成测试：benchmark recall 不应退化为随机；
- 回归测试：merged group、FunctionWithMembers、空实体 chunk 的 source range；
- 评估：range evaluator 能正确恢复目标。 
