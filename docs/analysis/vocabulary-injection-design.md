# 词表注入改造方案：预算前置、双路径嵌入与首次出现渲染

- 范围：`Vocabulary` 能力的**索引侧注入方式**（BM25 / Embedding 分别怎么嵌）+ **预算时序**（注入 vs 切分）
- 前置结论：词表是加法能力（Additive），不替换标识符；本方案只改"在哪注入、注入成什么形状、什么时候算预算"，不改匹配语义

---

## 0. 决策摘要

| # | 决策 | 内容 | 依据 |
|---|---|---|---|
| D1 | 注入位置 | 从"全流程末尾追加一行"前移到**分块阶段、重叠之前** | §2 现状证伪：末尾追加是唯一在预算评估后写 `chunk.text` 的逻辑 |
| D2 | 预算保证方式 | 切分时使用**预留头部空间（reserve）的影子配置**，注入后按真实上限终检 | §6，避免"注入后再切"的二次切分与括号被切断 |
| D3 | Embedding 渲染 | 首次出现处**就地**插入 ` (expansion)`，标识符原样保留 | 自然语言习惯、向量在符号附近获得语义 |
| D4 | BM25 渲染 | 首次出现**所在行的行尾**追加 `[token = expansion; ...]`，不改写行内结构 | BM25 是词袋，行尾与词内对召回等价；内联会破坏已启用的**短语查询**（`retrieval.rs:311-316`）与签名相邻性 |
| D5 | 注入粒度 | 默认"每 chunk 首次出现"；提供 `glossary_scope = chunk | entity` 开关 | 检索单元是 chunk；每实体首次会牺牲后段 chunk 的释义 |
| D6 | 单 chunk 上限 | `max_entries_per_chunk = 4` + 字符预算，超限截断并计数 | 现有只有"单条 200 字符"（`types/plugin.rs:413`），没有总量约束 |
| D7 | 导出 | 保留内联释义（不再保留尾部整行 `Glossary:`）；`strip_index_context` 继续只剥 sidecar | 导出有两条路径都消费 chunk 文本，见 §8.4 |
| D8 | 版本 | `PIPELINE_VERSION` 保持 `"1"`，不 bump | 开发阶段不提供向后兼容（仓库 `AGENTS.md` 约束：开发期不升版本、统一为 1）；输出形状变化由调用方重建索引消化，不做缓存迁移 |

---

## 1. 事实基线（代码位置，逐条可核；已按本次落地更新）

| 事实 | 位置 |
|---|---|
| 词表合并/匹配唯一实现 | `crates/core/cce-plugin/src/vocabulary.rs`（合并、`FileVocabulary`、`CompiledScope`）+ `crates/core/cce-plugin/src/vocabulary/injection.rs`（`inject`、`TerminologyIndex`、`ExpandedTerms`、`InjectionStyle/Budget`） |
| 代码索引注入 | `crates/parser/cce-parser/src/ast_to_nl/chunker/chunker.rs`：`effective_config`（~188）、`inject_vocabulary`（~211）、`chunk_group`（~292，先注入后重叠）、`chunk_groups_builtin`（~494，合并→注入→重叠）、`chunk_group_with_conversions`（~389，单组入口同样先注入后重叠） |
| 文档索引注入 | `crates/parser/cce-document/src/pipeline.rs`：`process_with_plugins_degraded_flag`（~360，切分走 `effective` 影子配置、注入走原始 `config`，见 §8.3），`inject_terminology`（~473） |
| 切分预算判定 | `crates/core/cce-config/src/modules/ast_to_nl.rs:253`（`exceeds_limit`），默认 `max_tokens=512`、`max_bm25_words=150`；预留 `glossary_reserve`（~273）、影子配置 `with_glossary_reserve`（~292，overlap 尺寸不缩减） |
| 单段强制合规 | `chunker/segment_limit.rs`（`enforce_segment_max_limit` 保证每段 ≤ 上限） |
| 跨组合并由上限保护 | `chunker/boundary.rs`（`merge_limits`），`merge.rs` |
| 重叠不写 `text`，只写 `prev_overlap` | `chunker/overlap.rs:217`（通用）、`273`（Emb 专用），2× 上限告警在同文件尾部 |
| 分块文本计数 | `chunker/boundary.rs`（`cost`）；注入后重算 `token_count` / `bm25_word_count`（`chunker.rs:253-263`，文档侧 `pipeline.rs:525-536`） |
| BM25 字段与权重 | `crates/infra/cce-storage-bm25/src/schema.rs`；查询权重 `retrieval.rs:278-280`（title 2.0 / content 1.0 / keywords 2.0） |
| BM25 短语查询确实启用 | `retrieval.rs:311-316`（`extract_phrases` → `build_phrase_query`），位置索引见 `schema.rs` |
| 导出消费 chunk 文本并做"剥离" | `crates/app/cce-orchestrator/src/export/nl_exporter.rs:293`（`prepare_export_chunks`）；剥离函数 `export/path_utils.rs:86`（sidecar/terminology 分界见注释）；直接导出 `export/direct_generator.rs:121`（`with_terminology`，同形状内联渲染） |
| 实体级释义字段 | `crates/core/cce-types/src/types/ast_to_nl/result.rs:139-150`（`glossary: Vec<VocabularyMatch>`，转换阶段只携带、切分阶段才渲染，见 `index_enrichment.rs:83-103`） |
| 流水线版本 | `crates/parser/cce-parser/src/ast_to_nl.rs:59`（`PIPELINE_VERSION = "1"`，本次不 bump，见 D8） |

---

## 2. 现状核查：预算评估之后还有谁在写 `chunk.text`？

对 `chunker` / `cce-document` / `orchestrator` 全量排查 `chunk.text` 的写入点，结论如下：

| 逻辑 | 发生时机 | 是否改 `chunk.text` | 是否在预算评估后 |
|---|---|---|---|
| topic anchor 插入（`insert_after_first_line`） | 转换阶段（切分前） | 改 `ConversionResult.embedding_text/bm25_text` | **否**（切分时才算预算） |
| 索引附加正文（`append_index_context_raw_separate`） | 转换阶段 | 同上 | **否** |
| header 重复拼装 | 切分内部，拼完即 `enforce_segment_max_limit` | 是 | **否** |
| overlap | 切分后 | **否**（写 `prev_overlap` 侧字段） | 是，但按设计放行（ratio 校验 + 2× 告警） |
| **词表追加一行** | 切分 → 合并 → 重叠之后 | **是**（`push('\n')` + `push_str`） | **是，且无任何检查** ← 唯一违反 |
| 导出前 `strip_index_context` | 导出侧 | 是（只在导出副本） | 与索引预算无关 |

**结论：用户判断成立。**改造前的 `chunk.text` 只应在"切分—合并"阶段被写入；词表是唯一在预算之后往正文里加字的东西，且它的长度既不受 `max_bm25_words` 也不受 `max_tokens` 约束。overlap 是**侧字段**，不构成同类问题（但要在文档里把它标注为"唯一允许的预算后增量"）。

---

## 3. 设计不变量

改造后必须同时成立：

- **I1（预算）**：任一 `chunk.text`（含注入）满足 `!config.exceeds_limit(text, path)`。
- **I2（无预算后写入）**：`chunk.text` 的写入只发生在切分阶段；切分之后只允许写侧字段（`prev_overlap`）。
- **I3（原文保真）**：注入不得删除/替换任何原始字符，标识符原文逐字保留。
- **I4（幂等与无级联）**：注入一次完成，不扫描已注入文本；同一输入必然产出同一输出（缓存与指纹依赖）。
- **I5（零成本开关）**：无词表 provider 时，输出与改造前**逐字节一致**。
- **I6（计数一致）**：`token_count` / `metadata.bm25_word_count` 与注入后文本一致（已有测试 `cce-document/src/common/chunker.rs:663-665` 就在守这条）。

---

## 4. 方案总览

```
                        ┌────────────── 切分前 ──────────────┐
Converter ──► conversion.bm25_text / embedding_text
                        │
            ┌───────────▼───────────┐
            │ 影子配置（reserve）    │  max_bm25_words - R_words
            │ effective ChunkingConfig│  max_tokens   - R_tokens
            └───────────┬───────────┘
                        │  切分 / 单段强制合规 / 跨组合并
                        ▼
              raw chunks（每段 ≤ 预留后上限）
                        │
            ┌───────────▼───────────┐
            │ 词表注入（本方案核心） │  BM25: 行尾段；EMB: 内联括号
            │ 每 chunk 首次出现      │  单 chunk ≤ 4 条
            └───────────┬───────────┘
                        │  重算 token_count / bm25_word_count
                        │  终检：exceeds_limit(text, path) 必须为 false
                        ▼
                 overlap（侧字段）──► 写入索引
```

关键点：**预留在切分前，注入在切分后但仍在"最终尺寸确认"之前**。这样既保住"每 chunk 首次出现"（这必须知道 chunk 边界）又满足"注入参与预算"（注入量被预留额度硬约束，注入后只做终检、不做二次切分）。

> 为什么不"注入后再切"：括号 `(decoding time to sample box)` 可能被切在中间，产生半截释义；且会引入二次切分的不确定性（chunk_id 漂移）。

---

## 5. BM25 路径设计

### 5.1 渲染

在**首次出现所在的标识符 run 的行尾**追加一段紧凑术语块：

```
原始： fn parse_stts(sample_count: u32) -> Vec<u32>
注入： fn parse_stts(sample_count: u32) -> Vec<u32>  [stts = decoding time to sample box]
```

- 一行内命中多个术语 → 合并成一段：`[stts = ...; ctts = ...]`
- 同一 chunk 内同一 token 只注入一次（首次出现行）
- 括号用 `[ ]` 与正文的 `( )` 区分，便于人工与脚本识别

### 5.2 为什么不是"内联括号"

BM25 已启用**短语查询**（`retrieval.rs:311-316`；位置索引 `schema.rs:54,63`）。若在标识符后紧插 `(expansion)`：

- `parse_stts` 与其后的 `sample_count` 之间被插入 5-6 个 token → 任何跨越该点的引号短语（`"parse_stts sample_count"`）失配；
- 相反会**制造**新的相邻关系：`time to sample` 变成可命中的短语 → 误召回。

行尾段保留全部原有相邻关系，同时把 expansion 词注入 content 字段（权重 1.0），召回收益不变。

### 5.3 为什么不是 keywords 字段

`keywords` 字段查询权重 2.0（`retrieval.rs:280`），且被 `field_doc_freq`（`452`、`524`）与查询侧"零文档频扩展"逻辑共用。把猜测性释义塞进 keywords：

- 会把释义抬到比实体名更高的权重档（2.0 vs content 1.0），排序易被"释义撞词"主导；
- 会改变 `doc_freq` 分布，间接影响查询侧扩展判定（`retrieval.rs:518-529`），属于跨模块隐式耦合。

保留为**实验开关**（见 §7.3），不作为默认。

---

## 6. Embedding 路径设计

### 6.1 渲染

在首次出现的标识符**之后就地**插入半角括号释义，标识符原样：

```
原始： parse_stts reads the sample table and returns timing.
注入： parse_stts (decoding time to sample box) reads the sample table and returns timing.
```

- 一个标识符 run 内命中多个术语（如 `parse_stts_sdp`）→ 一段括号内用 `; ` 合并：`parse_stts_sdp (decoding time to sample box; session description protocol)`
- 锚点必须是**整个标识符 run 的结尾**，不是分段结尾。反例：`sttsCount` 的分段 `stts` 结束于 `Count` 之前，若按分段注入会得到 `stts (…)Count`，把标识符劈开。
- 括号前补一个半角空格（前字符已是空白则不补），保证与正文分词不粘连。

### 6.2 为什么内联

- 向量模型对"符号周边上下文"敏感：释义紧贴符号时，`stts` 的向量才真正带上"时间-采样"语义；放在段尾则退化为"这段文字里同时出现了这两组词"。
- Embedding 文本不参与 BM25 短语查询、不参与签名展示，插入括号没有结构风险。
- 合并后的文本仍是可读的英文句，符合自然语言习惯。

---

## 7. 预算模型

### 7.1 预留额度

新增配置（`crates/core/cce-config/src/modules/ast_to_nl.rs` 的 `ChunkingConfig`）：

```toml
[ast_to_nl.chunking]
# 词表注入预留：按路径上限的比例预留，0 表示不预留（不推荐）
glossary_reserve_ratio = 0.06      # 默认 6%
# 单 chunk 最多注入几条术语
glossary_max_entries   = 4         # 默认 4
# 注入锚点粒度：chunk | entity
glossary_scope         = "chunk"   # 默认 chunk
```

- `R_bm25 = ceil(max_bm25_words * ratio)` → 150 × 6% ≈ **9 词**
- `R_emb  = ceil(max_tokens * ratio)` → 512 × 6% ≈ **31 token**
- 仅当该文件存在非空词表（`registry.vocabulary_table(file, lang).is_some()` 且非空）时才启用预留，否则**完全走原路径**（保证 I5）。

### 7.2 注入即受限

单 chunk 的注入预算 = `min(glossary_max_entries, 预留额度内可容纳的条数)`：

- 逐条累加渲染长度（BM25 按 `split_whitespace` 词数，EMB 按 `estimate_tokens`），一旦超出预留额度即停止，并记 `vocabulary_injection_truncated` 计数；
- 4 条 × 每条 expansion ≤ 200 字符（`types/plugin.rs:413`）实际可远超 9 词 → **必须按额度截断**，不能只按条数。取 `min(条数, 额度)`。

### 7.3 终检与指标

注入完成后：

```rust
debug_assert!(
    !config.exceeds_limit(&chunk.text, chunk.path),
    "glossary injection must respect the chunk budget (I1)"
);
```

- release 下不 panic，改为 `warn!` + 计数 `vocabulary_budget_overflow_total`（应恒为 0，出现即说明预留比例配小了）；
- 指标：`vocabulary_entries_injected_total` / `vocabulary_entries_truncated_total` / `vocabulary_files_with_table_total`。

### 7.4 `glossary_scope = entity` 的实际语义（已落地，非转换前注入）

 entity 粒度**不是**"转换阶段先注入再切分"，而是"切分后注入 + 跨 chunk 去重"：`inject` 每次调用只扫描当前 chunk 文本（无级联，I4），但调用方持有一个跨 chunk 的 `ExpandedTerms`，`chunk_scope` 逐 chunk 清空、`entity_scope` 按 `source_group_id` 切换时才清空（代码侧 `chunker.rs:221-232`，文档侧 `pipeline.rs:492-503`，文档无实体阶段故按 `source_group_id` 近似）。

 效果：同一术语在同一实体的后段 chunk 里不再解释，索引更紧凑；代价是后段 chunk 独立被检出时无释义。预算模型与 chunk 粒度完全一致（同一预留、同一终检），不存在"注入后再切"的半截括号问题。原方案中"转换后切分前做一次"的描述已作废。

---

## 8. 实现改动清单

### 8.1 `cce-plugin`：注入 API（已落地，以实现为准）

```rust
// crates/core/cce-plugin/src/vocabulary/injection.rs（实际签名）

pub enum InjectionStyle { Embedding, Bm25 }

pub struct InjectionBudget { pub max_entries: usize, pub max_cost: usize }
// max_cost 单位：BM25=词数，Embedding=token 数；调用方用 glossary_reserve(path) 换算。

pub struct Injection { pub text: String, pub injected: usize, pub truncated: usize }

pub fn inject(
    text: &str,
    index: &TerminologyIndex,      // 由 FileVocabulary::matches() 一次建成，match_key 去重
    style: InjectionStyle,
    budget: &InjectionBudget,
    expanded: &mut ExpandedTerms,  // 跨 chunk 去重状态（entity 粒度用，chunk 粒度逐 chunk 清空）
) -> Injection
```

要点（与初版方案的差异已对齐实现）：

1. 不再设 `VocabularyTable::get`：查找走 `TerminologyIndex::get(match_key)`，每文件一次建成，每 chunk 只扫标识符 run。
2. 位置信息来自 `identifier_runs(text) -> Vec<TokenRun { end, keys }>`：`identifier_tokens` 已改为其薄封装；锚点是 run 尾（`sttsCount` 不劈标识符），BM25 侧再按 `line_ranges` 收敛为"行尾一个锚点"。
3. 插入按锚点出现序一次组装（`apply` 单遍拷贝），不做字节降序补丁；`mentioned` 保证每 chunk 首次，`expanded` 保证 entity 粒度跨 chunk 只一次，且已注入文本不再参与匹配（I4）。
4. 预算按整块渲染长度计量（`fitting_prefix` 整块试算，`measure` 按风格取词数/token 数），到顶即停并返回 `truncated`；同一 run 多命中合并为一段（`; ` 分隔），同行多命中（BM25）合并为一段。

### 8.2 `chunker`：影子配置 + 注入钩子（已落地；本次补单组入口）

`chunker/chunker.rs` 实际形状：`effective_config()`（有表才 `with_glossary_reserve()`，否则克隆原配置，保证 I5）、`inject_vocabulary(&mut chunks, &FileVocabulary)`（风格按 `chunk.path` 选择，`reserve = self.config.glossary_reserve(path)` 取**原始**上限、`budget.max_cost` 即此值；注入后重算 `token_count` / `bm25_word_count`，终检用**原始** `self.config.exceeds_limit`）、`inject_into_output()`（`Chunk` 插件覆盖路径的统一注入点）。

调用顺序（均为"合并 → 注入 → 重叠 → 关系/已在内部处理"）：

| 位置 | 现状（已落地） |
|---|---|
| `chunk_group` | 切分（影子）→ 注入 → 重叠（仅 Emb 切片）→ 关系 |
| `chunk_groups_builtin` | 切分（影子）→ 跨组合并 → 注入（Emb/BM25 各一次）→ 重叠（仅 Emb） |
| `chunk_group_with_conversions` | 切分（影子）→ 注入 → 重叠（仅 Emb 切片）；此前缺注入与重叠，本次已补（否则单组直调路径静默丢失术语，违反 I1/I2 的"所有入口一致"） |
| `Chunk` 插件覆盖 | 插件产出 chunks → `inject_into_output` 统一注入（插件文本同样受预留约束；终检同上） |

- `chunk_single_path` 的 `needs_split` 读到影子配置，切分即带预留；**不改** `TextSplitter` 签名（`ChunkInfrastructure.config` 传入影子实例即可）。
- `ChunkInfrastructure` 仍持有 `&ChunkingConfig`，指向影子实例；`OverlapManager` 仍用原始配置（overlap 尺寸不缩减，`with_glossary_reserve` 注释已明确）。

### 8.3 文档管线：同一套钩子（已落地；本次修预算基准 bug）

`crates/parser/cce-document/src/pipeline.rs`：

- 入口 `process_with_plugins_degraded_flag` 构造一次影子配置 `effective`（有表才 `with_glossary_reserve()`，否则克隆原配置，保证 I5），切分（`process_with_plugin_list` / `process_with_degraded_flag`）走 `&effective`；
- **注入必须走原始 `config`**：`inject_terminology(&mut chunks, vocab, config)`。此前三处调用误传 `&effective`，导致 `glossary_reserve` 被二次打折（Emb 约少 2 token）且终检 `exceeds_limit` 对的是缩减后上限——任何正常使用预留的 chunk 都会被误判 `overflow`。本次已改为传原始 `config`，与代码侧 `self.config` 基准一致；
- 注入后重算 `token_count`（Emb 用 `estimate_tokens`，BM25 用词数）与 `meta.bm25_word_count`（有值才重算），终检 `debug_assert! + warn! + overflow` 计数；
- 文档无语言概念，`vocabulary_for(file, None)` 按文件模式筛选；`glossary_scope=entity` 时按 `source_group_id` 近似分组（文档无实体阶段，见 §7.4）。

### 8.4 导出侧（已落地；本次补 SDK 残留）

导出有**两条**消费路径，已统一为内联形状：

| 路径 | 数据来源 | 现状（已落地） |
|---|---|---|
| `nl_exporter` | 存储里的 Embedding chunk 文本 | 注入已是内联括号，直接带释义；`strip_index_context` 只剥 sidecar（**不**剥术语，`path_utils.rs:76-88` 注释已明确分界） |
| `direct_generator` | `ConversionResult.embedding_text` + `glossary: Vec<VocabularyMatch>` | `with_terminology`（`:121`）对文本做一次 `InjectionStyle::Embedding` 内联注入（上限 `MAX_EXPORTED_TERMS=8`，`max_cost=usize::MAX`，面向人读不做预算），已无尾部整行 |

本次另修三处残留"尾部一行 `Glossary:`"表述（行为早已是内联，文档未跟上）：`cce-plugin-sdk/docs/guide.md:207`、`cce_plugin.h:432-434`、`cce-plugin/src/lib.rs:740-741`，统一改为"Embedding 内联括号 / BM25 行尾块"。

### 8.5 版本与文档

`PIPELINE_VERSION` 保持 `"1"`（`ast_to_nl.rs:59`），不 bump：开发阶段不提供向后兼容、当前也无已发布的旧数据需要失效，版本统一为 1（仓库约束）。

`docs/user-guide/plugin/plugin-capabilities.md:431-549` 已是新形状（BM25 行尾块 / Embedding 内联括号 / 每 chunk 首次 / 预留预算 / 导出保留），本次无需再改；`cce-plugin-sdk` 侧三处残留见 §8.4，已同步。

---

## 9. 备选方案与否决理由

| 方案 | 做法 | 判定 |
|---|---|---|
| A 现状 | 末尾追加整行 | **否决**：预算后写入（I2 违反）、DF 稀释、每条都重复整行 |
| B 全路径内联 | BM25 也内联括号 | **仅作可选**：破坏短语相邻性（§5.2），收益不明 |
| C 本方案 | EMB 内联 + BM25 行尾段 + 预留预算 | **推荐** |
| D keywords 字段 | 释义只进 BM25 keywords | **推迟**：权重 2.0 + 与 `field_doc_freq` 耦合，需 A/B 数据 |
| E sidecar 字段 | 新增 `glossary_terms` 字段，写入时合成 | **否决**：与 overlap 同构但改动跨存储/写入/导出三层，收益不抵成本 |
| F 查询侧扩展 | 复用 `QueryRewrite` 做缩写→全称 | **部分采纳**：BM25 侧长期可替代，EMB 侧无效（向量不吃查询扩展），可作为后续实验 |
| G 替换文本 | 用 expansion 覆盖 identifier | **否决**：精确符号召回不可逆损坏（I3） |
| H 每实体首次 | `glossary_scope=entity` | **保留开关**，默认 chunk |

---

## 10. 测试计划

**注入单元（`cce-plugin`）**

1. 首次出现才注入；第二次出现不再注入；
2. `sttsCount` → 括号落在 `sttsCount` 之后（run 结束），不在 `stts` 之后；
3. `parse_stts_sdp` 命中两条 → 一段括号内 `; ` 合并；
4. 预算截断：额度只够 1 条时不注入第 2 条，`truncated > 0`；
5. 中文/UTF-8：多字节偏移插入不 panic、不切坏字符；
6. 无级联：expansion 文本本身含表内 token 时不二次注入；
7. BM25 行渲染：同一行两条命中合并；行尾定位正确（含 `\r\n`）。

**预算（`cce-parser` / `cce-document`）**

8. 有词表时：每个 chunk `!exceeds_limit(text, path)`（属性测试，跑 1000 组随机文本）；
9. 无词表时：`chunk.text` 与改造前**逐字节一致**（I5 回归金样本）；
10. 注入后 `token_count` / `bm25_word_count` 与文本一致（复用现有断言）；
11. 预留生效：同一文件在 6% 预留下 chunk 数只增不超 6%/min_chunk 的预期；
12. overlap 仍在注入之后应用，`prev_overlap` 偏移指向注入后文本。

**端到端**

13. 示例插件 `media_vocabulary.lua`：一个含 `stts`/`ctts` 的 C 文件，导出文档中出现内联释义且无 `Glossary:` 整行；
14. 关掉插件 → 索引与导出与基线一致。

---

**待拍板**

| # | 开放点 | 选项 | 倾向 |
|---|---|---|---|
| 1 | BM25 到底用行尾段还是内联括号 | 行尾（本方案）/ 内联（用户初版想法） | 行尾：短语查询已启用，内联有实证风险 |
| 2 | 预留比例默认值 | 4% / 6% / 8% | 6%，并用 §10.11 的实测调 |
| 3 | 单 chunk 条目上限 | 3 / 4 / 6 | 4（一行 4 条 × 平均 5 词 ≈ 20 词，已接近 9 词额度的边界，需实测收敛） |
| 4 | 粒度默认 chunk 还是 entity | chunk / entity | chunk |
| 5 | 导出是否保留释义 | 保留（内联）/ 剥离 | 保留：导出面向人读，"不必查缩写"是能力初衷 |
| 6 | `glossary` 字段是否改结构化 | 保持 `Option<String>` / 改 `Vec<VocabularyMatch>` | **已定：结构化**（`result.rs:150` 为 `Vec<VocabularyMatch>`，转换阶段携带、切分/导出各渲染一次；`with_glossary/set_glossary` 同步） |
| 7 | BM25 keywords 实验 | 做 / 不做 | 先不做，留 A/B 开关位 |

---

## 11. 本次评审结论与修改方向（新增）

总体判定：D1–D7 合理并已落地，D8 原"1→2"不合理（违反开发期版本约束），已改为保持 `"1"`。

设计合理性逐条复核：

- D1（注入前移到切分后、重叠前）：成立。`chunk.text` 写入点已收敛为切分阶段，重叠只写 `prev_overlap` 侧字段（`overlap.rs`），I2 成立。
- D2（影子预留 + 终检、不做注入后再切）：成立。避免括号被切断与 `chunk_id` 漂移；预留 6%（Emb 31 token / BM25 9 词）配合 `fitting_prefix` 整块试算，I1 由"有效上限切分 + 预算内注入 + 原始上限终检"三段保证。拒绝"注入后再切"正确。
- D3（Embedding 内联）：成立。向量对符号邻域敏感，内联是唯一把释义放进符号上下文的形状；不碰 BM25 短语与签名展示。
- D4（BM25 行尾块）：成立。词袋召回等价、相邻性保留；内联会破坏 `extract_phrases` 短语（`retrieval.rs:311-316`）且制造"expansion 内部相邻"误召回；keywords 字段权重 2.0 与 `field_doc_freq` 耦合，不应进 keywords（D 否决正确）。
- D5（默认 chunk）：成立。检索单元是 chunk；entity 粒度已保留为开关（实现为跨 chunk `ExpandedTerms` 去重，见 §7.4 修正）。
- D6（4 条 + 额度双约束）：成立。单条 200 字符上界下，4 条仍可远超 9 词预留，必须取 `min(条数, 额度)`；`truncated/overflow` 计数完整。
- D7（导出保留）：成立。两条导出路径已同形（`nl_exporter` 直接带内联、`direct_generator::with_terminology` 同形状重渲染），`strip_index_context` 只剥 sidecar。

本次发现并已修复的三个实现偏差：

1. 文档管线预算基准错误（bug）：`inject_terminology` 误用影子配置做 `glossary_reserve` 与 `exceeds_limit`，导致预留被二次打折、终检恒报 `overflow`。已改为切分走 `effective`、注入走原始 `config`（`pipeline.rs:393-410`）。
2. 单组入口缺注入与重叠（遗漏）：`GroupChunker::chunk_group_with_conversions` 直调 `header_chunk` 即返回，术语静默丢失。已补"注入→Emb 重叠"，与 `chunk_group` / `chunk_groups_builtin` 一致（`chunker.rs:389-425`）。
3. 文档残留（过期表述）：`§1/§8.1/§7.4` 行号与 API 已过期，按实现重写；SDK 三处"glossary line"已改为双形状；`§8.5` 版本改为不 bump；待拍板 #6 标记已定。
