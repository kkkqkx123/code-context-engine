# query_review 输出与真实 API 调用输出的差异分析

本文档对比 `outputs/scenarios/{lang}/query_review/{project}/` 下的查询审查报告（如
`flask_bm25/FZ-G1Q1-naming_affix.md`）与通过 REST API（`/search` / `/search/aggregated`，
见 `crates/app/cce-server/src/api/handlers/search.rs`）实际拿到的 JSON 输出之间的差异，
说明哪些字段是报告独有的展示性元数据，以及各类 content 的拼接方式。

报告生成代码：`crates/app/cce-e2e-tests/src/query_review.rs`。
核心数据结构：orchestrator 的 `SearchResult`（`crates/app/cce-orchestrator/src/query/types/search_result.rs`）
与 API 的 `SearchResultItem`（`crates/app/cce-api/src/models/search.rs`）。

## 一、报告独有、API 不返回的内容（仅展示用元数据）

报告直接读取 orchestrator 内部 `SearchResult` 的全部字段；API 层在
`convert_orchestrator_result` 中只挑选少量字段，其余全部丢弃。下表左侧为报告展示、
右侧为 API 是否保留：

| 报告字段（来源） | API `SearchResultItem` 中的对应 |
|---|---|
| `id`（chunk ID，如 `group_209_bm25_103`） | 不返回 |
| `entity_ids` / `segment_id` | 仅返回 `entity_ids`（转 i64），`segment_id` 丢弃 |
| alignment key（`e:536` 等，`alignment_key()` 计算） | 不返回（调用方可由 entity_ids 自行推导） |
| `original` / `vector` / `bm25` 分项分数 | 不返回，只返回融合后的 `score` |
| `boosted` / `boost reason` | 不返回 |
| `content_state` | 保留（映射为 `ReferenceOverLimit` 等枚举名） |
| `truncated`（索引期截断标记） | 不返回 |
| `category`（test/config/normal） | 不返回 |
| `metadata`（HashMap 附加元数据） | 不返回 |
| `pattern_info`（JSON 模式检测信息） | 不返回 |
| `kind` / `name`（实体类型与名称） | 仅 `kind` 变为可选 `entity_type`；`name` 丢弃（`entity_names` 恒为空） |

报告页面头部同样多为展示性内容，API 层不存在或不同：

- `Type` / `Fuzzy subtype` / `Intent` / `Fusion`（权重与算法）/ `Capability`：来自评测
  judgment 与测试配置，不是服务端产物。
- `Config:` 一行（limit/min_score/top_k 等）：来自测试用 `SearchConfig`，真实请求中
  这些是请求参数或项目配置，响应不回显。
- `Expected ranges` 与 `Coverage` / `in_expected` 列：来自人工标注
  （`judgments/`），是评测期望值，真实调用中不存在。
- `cache: hit/miss`：内部缓存状态，API 不暴露。
- `executed as`（内部执行策略）；API 侧对应信息只有 `sources_used` 列表。
- `elapsed_ms`：报告用 orchestrator 内部计时，API 用 handler 侧墙钟时间（含结果
  转换开销），两者数值通常不同。

API 响应反过来有、报告中没有的顶层字段：`success`、`relation_epoch`、
`relation_stale`、`failed_sub_queries`（报告中仅在非空时以一行展示，聚合演示页除外）。

## 二、content 的来源与拼接差异

### 1. 原始 body（`#### Content`）

报告的 `#### Content` 就是 `SearchResult.content` 原样输出（索引期存储的 chunk 文本，
含类内缩进）。API 中对应 `code_chunk` 字段——**但有本质区别**：

- 生产链路中，若项目配置 `search.annotation.enable_annotation = true`
  （默认 false），`post_processing::annotate_results` 会把 top-N 结果的
  `content` **原地替换**为标注后的文本（`result.content = annotated.annotated_content`），
  即 API 的 `code_chunk` 可能已经是拼接产物，而不是纯源码 body。
- 报告则把两者分开呈现：`#### Content` 永远是未标注的原始 body，
  `#### Annotated content` 单独展示标注结果，从不覆盖。

### 2. 标注内容（`#### Annotated content`）的拼接方式

两者都走 `RelationAnnotator::annotate_single` + `StructureConcatenator`，拼接顺序为：

1. 主单元（hit body），包在 `<unit name="..." lines="...">` 片段标签中；
2. 扩展单元按 score 择优、按（文件, 行号）结构化排序追加，每个单元一个片段标签，
   关系方向与类型编码为 `rel` 属性（`rel="calls"` / `rel="called by:call.method"`）；
3. 超预算单元降级为自闭合引用标签（`<reference path lines reason/>`），缺失文件同样降级；
4. 超出总量预算时追加自闭合 `<omitted units="N" tokens="~T"/>` 截断说明；
5. 跨文件结果用 `<file path="...">` 分组，整体包在 `<annotation>` 根标签中。

但参数与输入存在关键差异：

- **扩展单元来源**：报告的 `expansion_units()` 通过 relation searcher 真实解析
  callee/caller（call 域、每方向最多 3 个、seed 截断 4 个），并从 fixture 源码
  读取片段；而 REST 生产路径（`post_processing.rs`）调用
  `annotate_single(input, Vec::new(), Vec::new())`，**扩展列表恒为空**——即真实
  API 输出的标注内容只含主单元片段，不会出现 `rel="calls"`、`rel="called by"`
  等扩展片段标签。
- **参考窗口**：报告的扩展单元只保留定义行 ±1 行的 3 行窗口
  （`reference_window`），且起始行号做了 +1 的展示层换算；这是审查专用的最小化
  展示，生产路径不存在该逻辑。
- **omit_primary_body**：报告固定 `omit_primary_body(true)`——主 body 不重复渲染，
  因此第 1、3、5 号命中出现"Annotated content 只有扩展片段"的形态（`expanded: false`
  时直接写 "(no expansion units; see Content above)"）；生产默认值为 false，若
  启用标注，`code_chunk` 会是"片段标签 + 完整 body"的完整拼接。
- **annotated 主 body 的缩进**：报告 `Content` 中的前导缩进来自索引期原文；
  标注路径从 `content` 重新提取单元，两者共用同一坐标系（`search_result_input`
  不做行号换算），不会出现行号偏移。

### 3. 引用型 content（Reference 状态）

`content_state` 为 Reference 系列时，`content`/`code_chunk` 不是源码，而是
`reference_content()` 生成的自闭合 XML 引用标签（path/lines/reason/tokens 全部
编码为属性）。报告与 API 行为一致，但报告会照常渲染该标签并跳过标注
（`annotation skipped: reference ...`）。

## 三、小结

- 报告 = orchestrator 全量内部字段 + 评测期望标注 + 人工增强的 relation 扩展演示，
  面向人工审查；是真实输出的**超集演示**。
- API = 精简 DTO（`file_path`、`code_chunk`、行号、`entity_ids`、`score`、`sources`、
  `content_state`）+ 少量顶层元数据（`total`、`elapsed_ms`、`sources_used`、
  `relation_epoch` 等）；分项分数、boost、segment 对齐键、category/pattern 等均不外泄。
- content 差异要点：生产 API 在启用标注时把标注文本就地写回 `code_chunk` 且不带
  扩展单元；报告则始终并列展示"原始 body"与"带扩展标记的标注文本"两份内容。

## 四、标注内容中的位置信息与标记形式分析（历史设计讨论，已落地为 XML）

> 本节为迁移前的设计讨论记录，保留作为决策依据。讨论中的 `// [...]` comment
> 形态已被替换，现行为见本节末尾与第 2 节的 XML 形态描述。

针对标注内容（`#### Annotated content` 及生产链路写回 `code_chunk` 的标注文本）中
的关系引用片段，讨论两个设计问题：文件路径 / 行范围是否应前置于片段，
以及 `// [...]` 注释形式是否应改为 XML 包围。

### 1. 迁移前现状（comment 形态，已被替换）

迁移前每个扩展片段由三部分拼接：

```text
// [file] src/flask/app.py
// [calls:call.method] handle_exception (src/flask/app.py:896-898)

    def handle_exception(self, ctx: AppContext, e: Exception) -> Response:
        """Handle an exception that did not have an error handler
```

迁移后形态（`concatenator.rs` 的 `render_segment`）：

```text
<annotation>
<file path="src/flask/app.py">
<unit rel="calls:call.method" name="handle_exception" lines="896-898" excerpt="true">
    def handle_exception(self, ctx: AppContext, e: Exception) -> Response:
        """Handle an exception that did not have an error handler
</unit>
</file>
</annotation>
```

- `<file path>`：仅跨文件结果输出，同一文件的片段共用一个分组标签；
- `<unit rel name lines>`：携带关系方向与类型、符号名、1-based 行范围；
- 片段 body：来自 `reference_window` 截取的 3 行窗口（报告，带 `excerpt="true"`）
  或完整 body（生产）。

行范围语义断层已修正：报告侧窗口片段标记中的起止行是窗口的真实渲染行
（`window_start+1` / `window_end+1`），且携带 `excerpt="true"` 属性标明裁剪语义，
模型不会把该范围误当作完整定义范围。

### 2. 路径 / 行范围是否应直接填充到片段前方

**结论：应该，且大部分已满足，需要修正的是"范围与内容的对应性"。**

支持前置的理由：

1. LLM 消费场景中，片段的价值 = 代码 + 精确锚点。锚点与代码分离（例如放在
   JSON 元数据字段里）时，模型需要在两段上下文间跳转对齐，引用幻觉率上升；
   内联前置让"这段代码是什么、在哪"自包含。
2. 现有设计已内联：关系标记含 `(file:start-end)`，引用行含路径与范围。方向正确。
3. 片段级（而非文件级）前置路径是必要的：一个标注结果可跨多个文件
   （`file_count > 1`），仅靠首个 `// [file]` 标记无法定位后续片段。

需要修正的点：

- **裁剪窗口应标真实渲染范围**：`snippet_unit` 窗口化后应把标记中的
  `start-end` 改为窗口的实际起止行（`window_start+1` / `window_end+1`），
  或在标记中注明 `excerpt`（如 `(file:896-898, excerpt of def at 896-943)`），
  否则模型可能把标记范围当作完整定义范围引用。
- **`// [file]` 标记可考虑下沉到片段级**：目前同一文件多片段共用一个文件标记，
  依赖"行号是文件绝对坐标"来保持可定位性；这成立，但若未来片段改为相对行号，
  就必须逐片段前置路径。保持文件绝对行号 + 文件级标记是当前更省 token 的选择。

### 3. `// [...]` 注释形式是否误导，是否应改用 XML 包围

**`// [...]` 的风险是真实存在的，但程度取决于片段语言：**

1. **注释误读风险**：`// [calls:call.method] handle_exception (...)` 在 C 系语言
   （Rust/C/Java/JS/Go）里是合法行注释，模型可能将其当作代码作者写的注释；
   但在 Python 中 `//` 不是注释——它出现在 Python 片段前就成了裸露的非法语法
   文本。项目支持多语言，统一 `//` 前缀在 Python/Lua 等语言中"看起来像代码"
   比看起来像注释更糟。这是当前方案最实际的缺陷。
2. **边界模糊风险**：标记行与代码之间仅靠空行分隔，没有闭合语义。当片段本身
   含有以 `// [` 开头的注释（C 系代码完全可能），或片段被降级为引用行时，
   模型需要靠启发式区分"标记"与"内容"。对比之下 XML 包围有显式开闭边界。
3. **注入/污染风险**：片段内容不受约束，恶意或巧合的源码可以伪造
   `// [called by] ...` 行（提示注入面）。XML 标签包围同样不能根治注入
   （内容里也能写 `</code>`），但配合转义可以显著收窄。

**元数据类标记（关系标记、文件标记、引用行、omitted 统计）已完整迁移为 XML 风格**
（无配置开关，`comment` 风格已移除），实际形态：

  ```text
  <annotation>
  <file path="src/flask/app.py">
  <unit rel="calls:call.method" name="handle_exception" lines="896-898" excerpt="true">
  ...片段代码原样...
  </unit>
  </file>
  </annotation>
  ```

  XML 优势：a) 标签语法与所有主流编程语言的代码正交，任何语言下都不会被误读为
  注释或代码；b) 开闭标签给出显式边界，截断/降级状态可以放进属性
  （`<reference path="..." lines="..." reason="..." tokens="..."/>` /
  `excerpt="true"`）；c) 与项目已有的 MCP/结构化输出生态
  天然对齐，便于下游解析。单文件结果省略 `<file>` 层；截断说明渲染为自闭合
  `<omitted units="N" tokens="~T"/>`；主段合并的行间隙在 XML 风格下渲染为
  `<gap lines="N" range="s-e"/>`。
- **代码内容本身不要 XML 转义**：转义会破坏可读性与 token 效率，且代码不是
  文本字段，直接原样置于开闭标签之间即可；边界防混淆靠闭合标签而非转义。
- **结构建议三层包围**：整个标注结果一层（对应 `AnnotatedResult`）、每个文件一层、
  每个片段（含扩展片段与降级引用）一层。这同时解决了"主单元与扩展单元边界"
  和"多文件分组"两个当前的隐性约定。
- **迁移注意**：标记格式是提示词契约。`RelationAnnotationConfig` 曾提供
  `marker_style: comment | xml` 开关做双轨过渡，现已完成完整迁移并移除该开关，
  全部输出统一为 XML 形态。

### 4. 小结

- 路径与行范围前置到片段是正确方向且现状已基本实现；需要补的是裁剪窗口的
  "真实渲染范围 vs 定义范围"标注，避免行号误导。
- `// [...]` 注释形式在多语言场景（尤其 Python）下确实产生"非注释文本混入代码"
  的误导，且缺少显式边界；建议元数据层改用 XML 标签包围（文件层 + 片段层 +
  整体层），代码内容原样保留不转义，并通过配置开关平滑迁移。
