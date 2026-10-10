# 关系标注标记格式重构方案（XML 包围 + 行号修正）

背景：`crates/app/cce-e2e-tests/docs/query_review_vs_api.md` 第四节分析了标注内容中
关系引用片段的两个设计缺陷——裁剪窗口行号与实际渲染内容不对应，以及 `// [...]`
注释形式标记在多语言场景下（Python/Lua 等无 `//` 注释的语言）与代码混淆、缺少显式
边界、存在伪造注入面。本方案将改进落地为后续任务，只写改什么、为什么、按什么顺序，
不贴完整代码。

## 1. 目标形态

标注文本（生产写回 `code_chunk` 的文本，以及 e2e 报告的 `#### Annotated content`）
由注释式行标记改为三层 XML 包围，元数据全部进标签属性，代码内容原样保留：

- 整体层：整个标注结果一个根标签，携带结果级属性（结果 id、content_state）；
- 文件层：按文件分组一层标签，携带文件路径，取代现有 `// [file] <path>` 标记；
- 片段层：每个片段（主单元、扩展单元、降级引用）一个标签，携带关系方向与类型
  （`rel="calls:call.method"`）、符号名、位置范围，以及状态属性
  （`state="reference"` 表示降级引用、`excerpt="true"` 表示窗口裁剪片段）。

- 代码内容不做 XML 转义，直接置于开闭标签之间；边界防混淆依赖闭合标签。
- 降级引用行（`reference_content` 的输出）改渲染为片段层标签 + 标签内单行引用
  文本，或直接将位置与原因编码为标签属性；二选一在实施时定，以 token 开销小者为准。
- `// [omitted] N unit(s) (~T tokens)` 截断说明改为根标签的自闭合子标签。

## 2. 修改项（按依赖顺序）

### A. 行号真实性修正（独立可先行，无论 B 是否实施都应做）

位置：`crates/app/cce-e2e-tests/src/query_review.rs` 的 `snippet_unit` /
`reference_window`，以及 e2e 报告的关系标记渲染。

- 扩展片段标记中的起止行改为窗口的真实渲染行（`window_start+1` / `window_end+1`），
  不再使用实体定义的名义范围。
- 若保留名义范围（便于定位完整定义），在标记中显式注明 excerpt 语义
  （如 `excerpt of def at 896-943`），二者取一，禁止无声错位。
- 生产链路无窗口裁剪（扩展单元即完整 body 或引用行），不受此项影响，但实施 B 时
  片段层标签需带 `excerpt` 属性位以统一形态。

### B. 标记风格配置开关

位置：`crates/core/cce-config/src/modules/search.rs` 的 `RelationAnnotationConfig`。

- 新增 `marker_style` 字段，枚举 `comment`（现状）与 `xml`（目标形态），默认
  `comment`，保证存量行为不变。
- 标记渲染集中点：`crates/app/cce-orchestrator/src/query/annotation/concatenator.rs`
  的 `relation_marker`、`render_segment`、`format_file_marker`，以及
  `crates/app/cce-orchestrator/src/query/types/content_reference.rs` 的
  `reference_content` / `file_level_reference`。按开关分派两种渲染，选择逻辑
  收敛在 concatenator 一处，不在渲染函数间散落分支。

### C. XML 渲染实现

位置：同 B 的渲染集中点，新增 XML 风格渲染路径。

- 三层包围的标签命名统一使用固定前缀（如 `cce:`），避免与用户代码中出现概率高的
  通用标签名（`<file>`、`<code>`）撞名。
- 片段按现有结构化排序输出不变（primary 先、扩展按文件与行号）；文件层标签仅在
  跨文件时体现分组，单文件结果可省略文件层以省 token。
- 预算选择（`standalone_cost` / `selection_cost`）按新标签的真实渲染文本计费，
  确保预算估算与实际输出一致。

### D. 生产链路接入与报告对齐

- e2e `query_review` 的 `RelationAnnotator` 构造处切换为 `marker_style: xml`，
  作为新形态的参照实现与回归样本。
- 生产默认值保持 `comment` 不变；切换到 `xml` 作为默认值属于独立决策，待下游
  消费方（MCP、插件、e2e 断言）验证后再单独执行，不在本方案范围内。
- `docs/plan` 外的提示词类文档若引用了 `// [calls:...]` 样例，随本次一并更新。

### E. 测试与文档

- concatenator 单测补 XML 风格用例：三层包围、单文件省略文件层、降级引用形态、
  omitted 标签、excerpt 属性。
- 现有 `comment` 风格用例全部保留，断言默认行为不变。
- `crates/app/cce-e2e-tests/docs/query_review_vs_api.md` 与
  `docs/plan` 本文档在实施完成后回填实际标签形态。

## 3. 非目标

- 不改变扩展单元的解析来源与数量上限（报告侧每方向 3 个、call 域过滤等维持现状）。
- 不改变生产链路"扩展单元恒为空"的现状（是否给 REST 路径接通真实扩展是独立议题）。
- 不做代码内容的转义或净化，注入面收敛依赖标签边界与下游提示词约束。
