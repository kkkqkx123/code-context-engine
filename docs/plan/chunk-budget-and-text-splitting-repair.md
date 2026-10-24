# 分块预算与文本切分修复方案

本方案只描述目标、约束和改造点，不包含实现代码。结论以当前工作区代码为准。

## 已确认的失败

复现命令：

```text
RUST_BACKTRACE=1 ./target/debug/examples/gen_bench_media_server
```

失败位置：

```text
crates/parser/cce-parser/src/ast_to_nl/chunker/chunker.rs:268
glossary injection must respect the chunk budget
```

失败现场的数值：

- 配置硬上限 `max_tokens = 512`。
- `glossary_reserve_ratio = 0.06`，保留量 `ceil(512 * 0.06) = 31`。
- 切分器按缩减后的上限 `481` 工作。
- 切分得到的正文正好是 `481` token。
- `ChunkBuilder` 在切分之后追加 identity 行，约 `27–28` token，得到约 `509`。
- 术语注入再加两个展开（`track fragment box`、`track fragment run box`），约 `16` token，得到约 `525`。
- 注入自身的增量预算只检查了 `16 <= 31`，因此认为成功。
- 最后用原始硬上限检查 `525 > 512`，debug 构建触发断言；release 构建只记录 warn 并保留超限块。

触发文件与术语表：

- `crates/app/cce-e2e-tests/fixtures/cpp/review/media-server/.cce/plugins/media_vocabulary.lua`
- 失败实体为 `mov_read_tfra`，术语 `traf`、`trun`。

这不是断言过严，而是预算模型自相矛盾：切分时假设“正文不超过 `max - reserve`，保留量就完整留给注入”，但切分之后又追加了未计预算的 identity，保留量先被装饰吃掉了。

## 根因链

1. 文件带术语表时，`effective_config` 在切分前把硬上限全局缩减为 `max - reserve`。
2. 切分器按缩减后的上限产出正文，正文可以顶满这个上限。
3. `ChunkBuilder` 在切分和合并之后才追加 identity、续块 identity、topic 行等装饰文本；header 路径的续块头也在切分之后合成。
4. 注入使用“已花费 + 本次增量 <= reserve”的增量预算，没有检查注入后的完整文本。
5. 最终没有统一的硬上限闸门；唯一的检查是 `debug_assert`，release 行为与 debug 不一致。
6. `VocabularyStats.overflow` 把不变量破坏降级成一个计数，监控和调用方可以继续使用超限文本。

因此，只提高 reserve、只缩小全局 `max_tokens`、只删除断言、或只在最后截断文本，都会把问题转移到别处，不能建立可靠的不变量。

## 统一不变量

对每一条最终写入存储的 chunk，必须满足以下条件。路径 `path` 只使用一种度量单位，所有计算共享同一个函数。

- `H`：配置的硬上限。Embedding 为 token 数，BM25 为索引分词后的 term 数；`0` 表示不限制。
- `R`：文件带术语表时，为该路径规划的保留量；不带术语表时为 `0`。
- `T = H - R`：注入前的上限。切分、装饰、合并都以它为目标。
- `D`：该 chunk 家族全部追加装饰文本的实际开销，使用与 `H` 相同的度量。
- `B = max(1, T - D)`：正文可用的预算。
- 注入预算不是 `R`，而是 `H - cost(注入前完整文本)`；每次候选展开都必须对“候选完整文本”重新度量。

必须成立的性质：

1. 任何 `chunk.text` 的最终成本不大于 `H`。
2. 任何注入前的完整文本成本不大于 `T`。
3. 任何装饰文本在进入正文预算之前已经被度量；禁止在限额检查之后追加未计预算的文本。
4. 合并只依据实际拼接后的完整文本成本，不依据两边成本之和。
5. 注入只依据候选完整文本的绝对成本，不依据增量近似。
6. 超限是确定性错误，不是 warn、不是计数、不是静默截断。
7. 同一路径的切分、装饰、合并、注入、最终校验只使用一个度量函数。
8. 所有字节偏移在切片前必须是字符边界；release 与 debug 使用同一套校验。

`R` 是规划目标，不是注入许可。若某个 chunk 的装饰较小，注入可以使用超过 `R` 的剩余空间；只要最终不超过 `H`。若装饰较大，正文必须更小，而不是让注入挤掉保留量。

## 缺陷清单

| 位置 | 现象 | 风险 | 修复方向 |
|---|---|---|---|
| `chunker.rs::inject_vocabulary`、`pipeline.rs::inject_terminology` | 注入只用增量预算，最后用 `debug_assert` 检查全文 | 装饰吃掉 reserve 后必然超限；debug 崩溃，release 写入超限块 | 按候选全文绝对成本判定；删除“断言即正确性”的做法 |
| `chunk_builder.rs` 的 `from_segments`、`from_single_text`、`from_unsplit` | identity、续块 identity、topic 行在切分/合并之后追加 | 追加文本完全不受预算约束 | 先计算装饰并确定正文预算，再切分；最终统一闸门 |
| `cce-config` 的 `with_glossary_reserve`、`reduced_limit` | 全局缩减 `max_tokens` / `max_bm25_terms`；下限用 `overlap + 1` 兜底 | 缩减量被装饰消耗；小上限时预算塌缩；下限可能高于缩减后的上限 | 删除该机制，改为显式预算对象 |
| `splitter.rs::is_within_oversize_tolerance` | 单实体允许到缩减上限的 `1.25` 倍 | 直接违反硬上限；缩减后可达约 `601`，超过 `512` | 删除 tolerance；超限必须继续切 |
| `splitter.rs::split_range_at` 的 `boundaries.len() < 2` | 只有 1 个边界时被当作“无边界数据”下降 | 单实体边界携带的 entity id 丢失；tolerance 分支被绕过 | 区分 0 个边界与 1 个“整段边界”；下降时继承 id |
| `splitter.rs::merge_small_segments` 与 `boundary::can_merge` | 用 `prev_cost + next_cost` 近似；实际拼接没有分隔符 | 四舍五入或分词边界变化会让合并后实际成本超出上限 | 用实际拼接文本度量；内层与外层合并明确各自分隔符 |
| `splitter.rs::boundaries_to_segments`、`split_range_at` | 覆盖、连续性、字符边界只在 `debug_assert` 中检查 | release 可能静默丢内容，或在切片处 panic | 改为 release 校验并返回确定性错误 |
| `strategies/nested_groups.rs` | 对 `to_lowercase()` 结果做 `find`，把偏移直接用于原文 | 非 ASCII 大小写折叠会改变字节长度，导致错位或切片 panic | 使用返回原文偏移的大小写不敏感查找 |
| `chunk_builder.rs::qualify_name_in_text` | 非完整标识符匹配时用 `pos + 1` 前进 | 多字节字符中间不是字符边界，切片 panic | 按一个完整字符前进 |
| `strategies/lines.rs::force_split_line` | 用 `limit * 6` 字符作为切分长度 | CJK 或密集符号文本的实际成本远超预算 | 按度量逐步收缩，保证每段不超限 |
| `header.rs` 的 `compact_header`、`truncate_at_sentence_end` | `header_limit` 按 token/word 计算，却用 `chars().take(...)` 兜底 | 字符数不等于预算单位，仍可能超限 | 所有截断按度量进行，并落在字符边界 |
| `header_chunk.rs` 的 header 预算与成员分组 | header、续块、合成头、成员名没有统一计入预算；加法成本忽略 `\n\n`；`below_min` 可以压过预算 | 超限，或正文被挤到没有空间 | 统一装饰预算；用实际拼接文本度量；`min` 不凌驾硬上限 |
| `chunker.rs::chunk_single_path` | `needs_split` 只看原始 text | 未切分块追加 identity 后超限 | 先算装饰预算，再决定是否切分以及切分上限 |
| `document/common/token_split.rs` | estimator 选点后不复测；与 parser 重复实现；搜索窗口可能落到 chunk 起点之前 | 超限、单字符碎片、两套逻辑漂移 | 收敛到共享切分原语；切分后复测并 clamp 到起点之后 |
| `document/common/chunker.rs` | BM25 用 `split_whitespace` 计数，CJK 整段可能只算 1 个词；`split_bm25_by_words` 重新 join 丢失原始空白 | CJK 的 BM25 块实际无界；文本保真度下降 | BM25 使用与索引一致的分词计数；保留原文边界 |
| `pipeline.rs` 的注入循环 | 与 parser 的注入循环重复 | 同一个 bug 出现在两处，修复容易漏 | 抽到 `cce-plugin` 的共享 chunk 注入入口 |
| `try_plugin_chunk`、`inject_into_output` | 插件产出的 chunk 没有预算校验；超限仍可注入和入库；插件错误自动回退 | 插件可以绕过硬上限 | 校验插件 chunk；超限显式报错，不自动回退 |
| `ChunkedResult::full_text_with_overlap` | 直接拼接 overlap，没有预算；当前无消费者 | 未来一旦用于 provider 输入就会超限 | 删除；overlap 只作为元数据 |
| `VocabularyStats.overflow` 与 orchestrator 指标 | 把不变量破坏降级为计数 | 监控掩盖问题，超限文本继续流动 | 删除字段和指标；最终闸门报错 |
| `segment_limit.rs`、`finalize_group_path_chunks` | 复切之后仍可能被装饰推高；finalize 不做最终检查 | 超限块从这些路径漏出 | 由统一最终闸门兜底，不在局部重复造阈值 |

## 预算模型

### 预算对象

在 `cce-config` 中增加一个按“路径 + 文件是否有术语表”构造的预算对象，至少包含：

- 硬上限 `H`。
- 保留目标 `R`。
- 注入前上限 `T`。
- 该 chunk 家族的装饰开销 `D`。
- 正文上限 `B`。
- 合并上限 `T`。
- 该路径的度量单位。

预算对象由 group、path 和 chunk 角色共同确定。BM25 与 Embedding 各自持有自己的预算，不再用一个被全局缩减的 `ChunkingConfig` 同时服务两条路径。

`TextSplitter`、`HeaderHelper`、`segment_limit`、合并判定都接收预算对象，不再通过克隆配置并改写 `max_tokens` / `max_bm25_terms` 来传递预算。

### 装饰先行

“装饰”指在切分输入之外，随后追加到 chunk 头部的结构性文本：

- 非 header 路径：首块 identity 行、续块 identity（签名、限定名、kind）、续块 topic 行。
- header 路径：切分之后追加的首块 identity、续块 identity、topic 行、合成续块头（成员名列表）。
- 其他在正文之外追加的结构性文本。

header 的首块/续块文本本身是切分输入的一部分，不重复计入追加装饰 `D`；它通过“合并文本按 `T` 切分”自然占用预算。成员分组时使用实际拼接文本的成本，包含 `\n\n` 分隔符。

处理顺序必须是“先构造并度量追加装饰，再给正文分配预算”，不是“先切正文，再追加装饰”。

具体规则：

- `D` 取该 chunk 家族所有角色中的最大追加装饰成本；保守估计优于超限。
- 追加装饰总量上限为 `T / 2`。必填的 identity 行可以例外，但不得超过 `T - 1`。
- 超出追加装饰上限时按以下顺序压缩：合成头改用首个成员名加数量；topic 行从尾部删除；签名截断到度量边界。header 文本在合并文本预算不足时单独按度量压缩。
- 压缩后仍无法给正文留下至少 1 个预算单位时，返回确定性错误，不生成空正文或超限正文。
- 装饰压缩只删除重复的结构性信息，不截断实体正文；正文永远不静默截断。

### 正文切分

- 非 header 路径：正文上限为 `B = T - D`。
- header 路径：合并文本的切分上限为 `T - D`，其中 `D` 是该块角色后续追加的装饰；header 自身已经包含在合并文本中。
- 终端 token/word 切分必须按实际度量复测每一步；选点后如果仍超限，继续向字符边界收缩。
- 单个字符仍无法满足预算，说明配置对该内容不可满足，返回确定性错误，不产出超限块。
- 所有策略产出的边界必须连续、无重叠、完整覆盖原文、落在字符边界；违反者是策略 bug，release 也返回错误。

### 合并

- 内层合并（splitter 内部）按实际拼接方式度量；如果拼接不带分隔符，就按不带分隔符的文本度量。
- 外层跨 group 合并按实际 `a.text + "\n\n" + b.text` 度量。
- 合并上限是 `T`，不是 `H`。合并后的块仍要给注入保留至少 `R` 的空间。
- 不允许用 `prev_cost + next_cost` 代替实际拼接成本。
- 合并后如果最终闸门发现超过 `T` 或 `H`，撤销该次合并或返回错误；不能保留超限结果。

### 注入

- `InjectionBudget` 只包含：最大条目数、该路径的绝对上限 `H`、度量单位。
- 每条候选展开都构造候选完整文本并度量；超过 `H` 的条目标记为 `truncated`，不写入。
- 不再维护“已花费 + 增量”的近似预算。
- 注入点必须是字符边界；不是字符边界时跳过该锚点并计入未注入，不做字节切片。
- 注入后重新计算 `token_count` 和 BM25 的 term 计数，使用同一度量函数。
- 删除 `VocabularyStats.overflow`。注入未放入的术语只体现在 `truncated`；超限由最终闸门报错。
- 术语注入的共享循环放入 `cce-plugin`，parser 和 document 只负责构造策略并调用，不再各写一份。

### 最终闸门

在 parser 和 document 的所有公开返回路径上，统一执行一次最终校验：

- 逐块计算实际成本，超过 `H` 立即返回永久性切分错误。
- 错误消息包含 group、chunk id、path、实际值、`H`、`R`、`D`，便于定位。
- 同步校正 `token_count`、`bm25_word_count`，避免元数据与文本不一致。
- 最终闸门是 release 代码，不依赖 `debug_assert`。
- 不允许“warn 后继续”“计数后继续”“截断到上限后继续”。

## 模块改造

### `cce-text`

- 提供唯一的 `ChunkUnit` 与 `cost`：Embedding 使用现有 token 估计器，BM25 使用与索引一致的 `MixedTokenizer` term 数。
- 提供共享的终端预算切分原语，parser 和 document 只提供各自的边界偏好，不再各维护一套 token 切分。
- 提供大小写不敏感查找，返回原文的字节范围，不返回 lowercase 文本的偏移。
- 提供按度量截断的辅助函数，所有截断落在字符边界。
- 不新增平行分词器；复用现有 `MixedTokenizer` / `segment`。

### `cce-config`

- 增加预算对象和构造方法；删除 `with_glossary_reserve`、`reduced_limit`、`exceeds_limit` 这类会诱导调用者拿错阈值的接口。
- 配置校验：
  - 保留量必须小于硬上限。
  - `min_*` 不得高于对应的 `max_*`。
  - `overlap_*` 必须小于对应上限。
  - `cross_group_merge_threshold` 为 0 或不超过硬上限；使用时再与 `T` 取小。
- BM25 的预算单位改为 term 数后，字段名同步改为 `max_bm25_terms`、`min_chunk_bm25_terms`、`overlap_bm25_terms`，不保留旧字段别名。默认值按基准重新标定。
- `0` 表示不限制只保留为内部语义；用户配置仍要求大于 0。

### `cce-plugin`

- `InjectionBudget` 改为绝对全文上限，注入逻辑按候选完整文本判定。
- 增加共享的 chunk 注入入口，处理 `ExpandedTerms`、`max_entries`、entity/chunk scope、计数更新。
- 删除 `VocabularyStats.overflow`，更新所有使用方。
- 注入度量复用 `cce-text`，不复制 token 估计或分词规则。
- 不修改 native 插件 ABI；`cce_plugin.h`、`declare_plugin!`、host loader 和 SDK 符号表都不需要改。插件 chunk 的预算约束由 host 校验，属于契约而非 ABI。

### `cce-parser`

- 删除 `effective_config` 的全局缩减语义；按 group + path 构造预算。
- `ChunkInfrastructure` 或 `ChunkInput` 携带该路径的预算和 splitter，BM25 与 Embedding 不共享同一个缩减配置。
- `ChunkBuilder` 的装饰文本统一由装饰计划生成；`D` 在切分前已知。
- `from_segments`、`from_single_text`、`from_unsplit` 的 `token_count` / `word_count` 统一使用共享度量，不再一处 `split_whitespace`、一处 `estimate_tokens`。
- `HeaderHelper` 使用预算对象；header 压缩与成员分组使用合并文本预算，续块头、topic 行、identity 计入追加装饰 `D`。
- `TextSplitter` 删除 1.25 倍 tolerance；修正单边界下降；终端切分按度量复测。
- `boundaries_to_segments` 与范围检查改为 release 校验并返回错误。
- 合并判定使用实际拼接文本；内层与外层分别明确分隔符。
- `chunk_group` 与 `chunk_groups` 两条公开入口都在注入之后执行最终闸门；插件 chunk 也走同一闸门。
- 插件 chunk 超限时返回错误，不自动回退到内置 chunker；插件执行失败的其他既有回退语义可以保留。
- 删除 `ChunkedResult::full_text_with_overlap` 的使用假设；overlap 只作为元数据，不进入 provider 输入。

### `cce-document`

- `two_tier_chunking` 接收预算或“是否有术语表”的显式参数；Embedding 和 BM25 各自按预算切分。
- 删除 `common/token_split.rs` 中与 parser 重复的终端切分逻辑，改用 `cce-text` 的共享原语；保留文档段落/句子的边界偏好。
- `find_good_split_point` 的搜索窗口 clamp 到 chunk 起点之后，任何候选不得落到起点之前。
- BM25 使用与索引一致的分词计数；`split_bm25_by_words` 不再用空白分词决定 CJK 的预算。
- `pipeline.rs` 的注入循环改为调用 `cce-plugin` 的共享入口；所有返回分支都经过最终闸门。
- CSV、JSON、YAML、XML、plain text 和插件文档管线都通过 `two_tier_chunking` 或同一闸门；`csv_chunker` 等分批判定改用预算对象，自定义 chunker 不得直接把未校验文本交给存储。
- 删除重复的 debug 断言检查；错误路径与 parser 保持一致。

### `cce-orchestrator`

- 删除 `VocabularyStats.overflow` 对应的指标与聚合。
- 确认预算错误走永久性 `CodeSplitting` 分类，不被当作瞬态错误重试。
- 不修改存储 schema；不保留旧索引兼容。BM25 度量变化后需要重建索引，这是开发阶段的预期行为。

### `cce-types`

- 删除无消费者的 `full_text_with_overlap`，避免未来把 overlap 拼进 provider 输入。
- 明确 `token_count` 的路径语义：Embedding 为 token，BM25 为 term。
- 如现有 `ParseError` 分类不足以表达预算不可满足，可增加专用变体；否则复用 `CodeSplitting` 并在消息中携带完整上下文。

### 文档

- 更新 `docs/archive/utf8-string-splitting.md`，补充“大小写不敏感查找必须返回原文偏移”“任何字节切片前必须验证字符边界”的约定。
- 更新 `crates/app/cce-plugin-sdk/docs/guide.md` 的 chunk 契约说明：插件产出的 chunk 必须满足配置上限；不新增 ABI 符号，因此符号表和方法表无需变更。
- 同步配置说明和示例中的 BM25 字段改名。

## 修复后的关键流程

1. 文件级确定是否有术语表，从而确定每条路径的 `R`。
2. 对每个 group + path 计算追加装饰计划：首块/续块 identity、topic、合成头，得到实际装饰文本和 `D`；header 文本归入合并文本预算，不重复计入 `D`。
3. 若 `D` 超过装饰上限，按固定顺序压缩；压缩后仍无法留出正文预算则报错。
4. 非 header 路径以 `B = T - D` 切分正文；header 路径以 `T - D` 切分合并文本，`D` 只包含切分后追加的装饰。
5. 组装 chunk：装饰 + 正文；此时完整文本不得超过 `T`。
6. 合并：只接受实际拼接后不超过 `T` 的合并。
7. 注入：按候选完整文本不超过 `H` 判定；未放入的术语计入 `truncated`。
8. 最终闸门：逐块检查不超过 `H`，校正计数；超限返回永久性错误。
9. overlap 只写入元数据，不改变 `chunk.text`，不进入 provider 输入。

## Unicode 与边界安全

- 所有来自策略的 `start_byte` / `end_byte` 在切片前必须通过字符边界检查；release 也检查。
- 大小写不敏感查找不能用 lowercase 文本的字节偏移直接映射原文；必须返回原文偏移和原文长度。
- `qualify_name_in_text` 在非完整标识符匹配时按一个完整字符前进，不能按一个字节前进。
- header、identity、topic 的截断按度量进行，并落在字符边界；禁止用字符数代替 token/term 数。
- 终端切分在字符边界上收缩；单字符仍超限时返回错误。
- 合并、注入、overlap 的分隔符都按实际文本度量；不能用“加一个空格”或“加两个换行”的心算代替。
- 增加固定种子的多语言属性测试：ASCII、CJK、组合字符、emoji、RTL 混合文本，验证覆盖完整、边界连续、每段不超限、切片不 panic。

## 错误与降级

- 正文超限、装饰无法压缩、单字符仍超限：返回永久性 `CodeSplitting` 错误，文件处理失败。禁止写超限文本，禁止静默丢弃正文。
- 插件返回超限 chunk：返回错误，不自动回退到内置 chunker；插件执行失败的其他错误可以保留现有回退语义。
- 术语展开放不下：只计入 `truncated`，不报错；这是预算内的正常降级。
- 装饰压缩：只删除重复的结构性文本，不删除正文；不设置 `truncated` 标志。
- 不再有 `overflow` 计数，也不再有“warn 后继续”的路径。
- debug 与 release 行为一致；`debug_assert` 只能作为额外断言，不能承担正确性。

## 测试计划

### 单元与集成

- 复现 panic 的注入用例：注入前文本接近 `T`，展开会越过 `H`；断言最终不超限、未放入的条目计入 `truncated`、debug 不 panic。
- 未切分块追加 identity 后仍不超限。
- 续块 identity、签名、topic 行、合成头计入追加装饰预算；header 文本参与合并文本预算；装饰压缩后仍不超限。
- 装饰本身无法放入 `T` 时返回确定性错误，不产生空正文或超限正文。
- 单实体超过 `1.25` 倍上限时继续切分；单实体边界下降后 entity id 不丢失。
- 合并用例：两边成本之和在上限内，但实际拼接后超限时不得合并。
- `force_split_line` 对 CJK、emoji、密集符号文本每段都不超限。
- `find_good_split_point` 在短 chunk 中不得返回起点之前的围栏位置。
- 非 ASCII 大小写折叠（例如 `İ`）下 nested group 查找不 panic、偏移正确。
- `qualify_name_in_text` 在多字节名称前有伪匹配时不 panic。
- 文档 Embedding 与 BM25 的注入都按绝对全文上限判定。
- CJK 文档的 BM25 按索引 term 数切分，不再整段算一个词。
- 插件 chunk 超限返回错误，且不自动回退。
- BM25 度量与 `MixedTokenizer` 的 term 数一致。
- 固定种子多语言属性测试：覆盖、连续性、字符边界、预算四项同时成立。

### 回归

- 运行 `gen_bench_media_server`，确认不再在 `chunker.rs:268` panic。
- 对生成的 chunk 做一次全量抽查：所有 Embedding chunk 不超过 `max_tokens`，所有 BM25 chunk 不超过 `max_bm25_terms`。
- 抽查失败实体 `mov_read_tfra` 的术语展开：`traf` / `trun` 要么完整放入，要么明确计入 `truncated`，不能超限。

### 验证命令

按“少编译、集中验证”的原则执行：

```text
cargo fmt
cargo clippy --all-targets --all-features
cargo test -p cce-config -p cce-plugin -p cce-parser -p cce-document --lib
cargo run --example gen_bench_media_server
```

如果属性测试或端到端回归位于其他 crate，只追加对应的定向测试，不做全工作区重复构建。最终闸门是普通 release 代码，不需要额外的 release 构建来验证；debug 测试走的是同一条判定。

## 实施顺序

1. 在 `cce-text` 建立唯一度量与共享终端切分原语，并同步 `cce-config` 的预算对象和校验。
2. 改造 `cce-plugin` 的绝对注入预算与共享 chunk 注入入口，删除 `overflow`。
3. 改造 parser：装饰计划、per-path 预算、header 预算、最终闸门、插件 chunk 校验。
4. 修正 splitter：删除 tolerance、修正单边界下降、实际拼接度量、release 校验。
5. 修正 Unicode 偏移与所有按字符数截断的路径。
6. 改造 document：共享切分、BM25 度量、注入循环、最终闸门。
7. 清理 overlap 死接口与 orchestrator 指标，更新相关文档。
8. 补齐单元、属性、回归测试，最后执行上面的集中验证命令。

## 明确不做

- 不通过提高 `glossary_reserve_ratio` 或全局缩小 `max_tokens` 来掩盖问题。
- 不删除断言了事，也不把 `debug_assert` 换成 `warn`。
- 不在最终阶段静默截断、丢弃或跳过超限 chunk。
- 不保留 `overflow` 计数作为“已知问题”出口。
- 不新增“v2 chunker”或第二条平行切分路径；只修改现有模块。
- 不为旧配置字段、旧索引或旧插件行为做兼容。
- 不修改 native 插件 ABI；插件 chunk 的预算约束是 host 侧契约。
- 不为属性测试引入新的随机数依赖，使用固定种子即可。
