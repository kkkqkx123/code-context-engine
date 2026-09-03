# 查询类型设计重构方案

## 1. 背景与问题

### 1.1 现状

现有查询集（`crates/cce_e2e_tests/src/judgments/{once_cell,ripgrep,flask}.rs`）分为三类：

| 类型 | 查询示例 | 数量占比 |
|------|----------|----------|
| `exact` | `RegexMatcher::find_at`、`Flask::wsgi_app`、`unsync::OnceCell::new` | 10/30 |
| `semantic` | `dispatch an incoming request to its matching view function` | 20/30 |
| `cross_lang` | `懒加载的全局变量` | 仅 once_cell 遗留 |

### 1.2 问题分析

**问题一：exact 查询职责错位。** `RegexMatcher::find_at` 这类查询是纯符号查找，本质是 grep/ctags/IDE 跳转的职责，不是语义检索系统应当评测的能力。

**问题二：exact 查询名不副实，且在不同 baseline 下测的是不同能力。** full_pipeline 的 chunk 是 AST→NL 转换后的自然语言文本，原始符号串（`RegexMatcher::find_at`）并不逐字存在于语料中。现有结果已实证（`outputs/benchmark/ripgrep/per_query_results_top5.md`）：G1Q1 在 full_pipeline 下 emb P@5=1.0 而 BM25=0.0。因此：

- `raw_source` 类 baseline 下：exact 是字面匹配（BM25 必胜）；
- `full_pipeline` 下：exact 是"从自然语言重建符号"的语义任务（emb 必胜）。

同一组查询在两种 baseline 下测的是互斥的能力，exact 组的结论被 baseline 方案绑架，失去判别力。

**问题三：缺少"词面有扰动、结构可确定"的中间难度档。** 现有查询只在两个极端：符号串（词面零扰动）与纯行为描述（词面结构双扰动）。真实使用中更常见的是用户记得符号轮廓但记不准确切拼写/风格（`init` vs `initialize`、`find_at` vs `findAt`），这一档没有覆盖。

**问题四：类型安全缺陷。** `file_documentation` 是实际存在的第四种查询类型，但只是硬编码字符串（`bench_data.rs` 中 `QueryData.query_type: String`），游离于 `QueryType` 枚举与 `EvaluationScope` 过滤之外；聚合基准设计中的 G5-G8 组同样未纳入枚举。

**问题五：分布失衡。** exact 占 1/3，为词面检索器（BM25）提供虚高支撑，压低了整体基准的判别力。

## 2. 查询类型重构

### 2.1 扰动轴模型

查询与目标 chunk 之间的"距离"由两条正交扰动轴决定：

| 轴 | 含义 |
|----|------|
| 词面轴（lexical） | 标识符/词语的拼写是否与语料一致 |
| 结构轴（structural） | 查询是否以自然语言结构包装符号（`in`/`on`/`method of`），以及行为描述层面的改写 |

四档查询类型恰好覆盖四个象限：

| 类型 | 词面 | 结构 | 测什么 | 反例工具 |
|------|------|------|--------|----------|
| `qualified`（原 `exact` 改名） | 保留 | 加自然语言结构 | 标识符消歧 + 成员-类型关系理解 | grep 需正则才能回答 |
| `fuzzy`（新增） | 人为扰动 | 保留 | 对命名风格/同义词变换的鲁棒性 | grep 无法回答 |
| `semantic`（现有） | 换词 | 行为级改写 | 纯语义行为匹配 | grep 无法回答 |
| `cross_lang`（现有） | 跨语言换词 | 保留 | 跨语言对齐能力 | grep 无法回答 |

### 2.2 类型定义

`QueryType` 枚举从 3 变体扩展为 4 变体：

- `Exact` → **`Qualified`**（序列化名 `qualified`）：`find_at in RegexMatcher`、`find_at on OnceCell`、`method find_at of RegexMatcher`。保留原始符号词面（token 可逐字匹配），但以自然语言结构包装，要求系统区分"标识符"与"连接词"并理解成员归属关系。
- 新增 **`Fuzzy`**（序列化名 `fuzzy`）：词面扰动，结构保留。
- `Semantic`、`CrossLang` 维持不变。

### 2.3 fuzzy 组构造规范

**构造原则**：relevance 判断完全复用现有查询（同一批 source ranges），仅改写 query text，标注成本为零。

**扰动子类型**（每个 fuzzy 查询必须标注子类型，用于分组分析检索器对哪类偏差鲁棒）：

| 子类型 | 规则 | 示例 |
|--------|------|------|
| `naming_case` | 命名风格变换 | `find_at` → `findAt`、`get_or_init` → `getOrInit` |
| `naming_affix` | 加/去前后缀 | `find_at` → `find_at_method`、`initialize` → `initialize_cell` |
| `synonym` | 同义词替换 | `initialize` → `init`、`create` → `make`、`retrieve` → `fetch` |
| `paraphrase` | 短语级改写（保留至少一个核心标识符词面） | `get_or_init` → `fetch or create the value` |
| `abbrev_expand` | 缩写展开/收缩 | `init` → `initialization`、`config` → `configuration` |

**派生规则**：

- 每个 qualified 查询派生 1-2 个 fuzzy 变体，扰动子类型在 qualified 源查询间轮转，保证各子类型覆盖均衡（每组 3-4 个）；
- 同一源查询的多个变体不得使用同一子类型（避免重复测同一种能力）；
- 查询 ID 约定：`FZ-<源查询ID>-<子类型>`，如 `FZ-G1Q1-synonym`；
- fuzzy 查询的 `relevant_ranges` 与源查询完全一致。

**约束**：fuzzy 查询不允许与任何现有查询文本相同；派生时人工复核保证扰动后仍指向同一实体（防止同义词改写歧义漂移）。

### 2.4 数量与分布

每个 fixture：10 个 `qualified` + 15 个 `fuzzy`（10 源 × 1-2 变体）+ 20 个 `semantic` + 可选 `cross_lang`。fuzzy 与 qualified 占比合计约 45%，避免词面类查询重蹈"过半虚高"的覆辙。

## 3. 数据结构变更

- `QueryType`：新增 `Qualified`、`Fuzzy` 变体，删除 `Exact`；
- `QueryData.query_type`：由 `String` 改回枚举（修复 `file_documentation` 游离问题）；
- `RelevanceJudgment`：新增可选字段 `fuzzy_subtype: Option<String>`（仅 fuzzy 查询携带）；
- `EvaluationScope`：`core_retrieval` 包含 `qualified` + `fuzzy` + `semantic`；`all` 同步扩展。

## 4. 评估与报告变更

- 汇总报告 `aggregate_metrics_by_query_type` 自动获得新分组维度，无需改评分逻辑；
- 新增 fuzzy 子类型分析维度：`aggregate_metrics_by_fuzzy_subtype`（按 `naming_case`/`synonym`/`paraphrase` 等分组），用于回答"检索器对哪类偏差最脆弱"；
- 历史数据对比：`exact` 组数据保留在 outputs 归档中，不迁移，报告注明新旧类型定义差异。

## 5. 实施计划

| 阶段 | 内容 | 验证 |
|------|------|------|
| 1 | 枚举扩展 + 类型安全修复（QueryData/EvaluationScope/file_documentation） | `cargo clippy --all-targets --all-features` |
| 2 | 三个 fixture 的 qualified 改写（10 个/项目，人工） | 逐一核对 relevant_ranges 不变 |
| 3 | fuzzy 变体派生（15 个/项目，含子类型标注） | 抽样复核扰动后实体指向一致 |
| 4 | 重新生成 bench_data + 运行基准 | 对比新旧报告，确认 qualified/fuzzy 判别力（BM25 不应再碾压词面类查询） |

## 6. 风险与缓解

| 风险 | 缓解 |
|------|------|
| qualified 改写引入歧义（`find_at in RegexMatcher` 可能命中多个同名方法） | 优先选择成员名在语料中唯一的实体；必要时加限定词（`find_at in RegexMatcher in matcher module`） |
| fuzzy 同义改写导致实体漂移 | 改写后人工复核 + 保留原始 token 约束（至少一个核心标识符词面不变） |
| 旧报告与旧代码引用 `QueryType::Exact` | 一次性枚举重命名，全仓 grep 清理；outputs 归档数据不重建 |
