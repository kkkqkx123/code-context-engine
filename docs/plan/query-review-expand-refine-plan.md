# query_review 输出精化与行号口径修复方案

对应问题清单：`docs/issue/query-review-expand-and-relation-noise.md`（10 项）。本文只写改什么、为什么、按什么顺序与如何验收，不贴完整代码。

## 1. 根因结论

**扩展冗余与关系域噪声同源。** `src/query_review.rs` 的 `expansion_units` / `snippet_unit` 两个函数决定了扩展的全部行为，两处都缺少约束：

- `snippet_unit` 用 `entity.span` 的起止行取**全部源码**作为扩展单元正文，没有最小化引用概念，于是一个 96 行的 impl 块会被整体塞进标注区。
- `expansion_units` 走 `coordinator.get_callees` / `get_callers`，二者分别落到 `query.rs` 的 `get_callees_by_entity` / `get_callers_by_entity`，返回**全部关系域**的已解析关系。编排器侧本身已有带域过滤的公开路径（`get_callees_paginated` / `get_callers_paginated`，内部经 `filter_callees` / `filter_callers`），harness 没有用。

标注器的兜底过滤因此全部落空：`snippet_unit` 从不设置 `relation_type`，而 `ExpandedUnit::is_call_domain()` 在 `relation_type` 为 `None` 时按兼容旧输入返回真，`allow_structural_edges=false` 这一默认约束完全不起作用；`is_stdlib` / `is_external` 同样从未被设置。

**stdlib 过滤不应扩展到关系增强。** `cce-codegraph/src/index/resolver.rs` 的 stdlib 分支只丢弃未解析到本地实体且判定为 stdlib 的外部调用，内部已解析的被调用者即使名字像标准库也保留。`Sync` / `Send` / `RefUnwindSafe` 的 impl 在仓库内已解析为本地实体，属于本地结构域关系，按名字过滤会误删本地代码。正确做法是按**关系域**过滤，而不是按名字。

**命中行号少 1 是口径混用，不是数据错误。** chunk 记录的行字段存的是 tree-sitter 的 0 基行号（写入方 `storage_coordinator/mapping.rs:119-120`，读取方 `source_reader.rs` 与 `tools/common.rs:40-47` 的取值逻辑一致按 0 基下标取行，三处自洽）。所有面向用户的呈现面——`reference_content` 生成的引用行、query_review 的命中区间、judgment 的期望区间、`structured` 导出的 `L480-L482`——都按 1 基行号书写。分界线在 `entity_mapper.rs:298-299`：它把 0 基行号直接赋给 `SearchResult.start_line / end_line`，类型上没有任何约定，下游一律当 1 基用。

该口径混用还掩盖了一个真问题：`entity_mapper.rs:279` 用 `start_line == 0 && end_line == 0` 判定"命中无区间"，但 0 基下首行 chunk 的行号本来就是 0，合法区间与哨兵值撞车。改为 1 基后该判定重新变可靠。

**kind / name 失真不在本次范围。** flask 的方法被标成 `class`、once_cell 的 impl 块以组内首个成员命名，属于分组与实体身份的呈现策略问题，需要单独设计，不在本方案内。

## 2. 修改项（按依赖顺序）

### A. 扩展单元改为最小化引用

位置：`crates/app/cce-e2e-tests/src/query_review.rs` 的 `snippet_unit`。

- 正文不再取符号完整 span。取符号定义行（`span.start_position.row + 1`）及其前后各 1 行，共 3 行；文件不足时向内侧收拢，最短可为 1 行。
- 行范围按收拢后的实际范围计算，与正文严格一致，避免"范围 3 行、正文 1 行"这类新的不一致。
- 扩展单元的语义变为"定位用引用"，完整正文继续由命中自身的 `#### Content` 承担。这一项直接决定标注区能否从 300 行量级降到几十行。

配套：`Annotated content` 区不再原样复读主正文。当前 `AnnotatedResult` 必然以 primary 全文开头，导致同一份代码在同一文件出现两次。改法是在渲染层把标注内容与原始正文做一次前缀剥离，只渲染增量部分，并在标注行注明"主正文见上"；若一次都没扩展，输出一行"无扩展"而不是复读。

### B. 关系增强限定调用域

位置：同文件 `expansion_units`。

- 前向改用 `coordinator.get_callees_paginated`，后向改用 `get_callers_paginated`，两者内部都会经 `filter_callees` / `filter_callers` 施加域过滤。
- 传入 `RelationQueryOptions`，只设三个字段：`relation_domains` 为 `["call"]`、`include_external` 为 false、`limit` 为既有每方向上限。其余保持默认，不动目录前缀与排除项，保持 harness 行为可预期。
- 这一步一次性消除三类噪声：结构域关系（impl association、trait bound、继承）、依赖域关系（import / use）、外部未解析目标。once_cell 单行 trait impl 噪声与 flask 的 import、类声明噪声同属这三类。
- 标签 `calls` / `called by` 在过滤后即为真实语义，保留不改。

补充：`filter_callers` 在需要判断关系域时会读该 caller 的出边，前向同理。每命中 3 个种子、每方向各 3 个单元，开销有界，不影响导出耗时量级。

### C. 标注区预算与降级一致性

位置：同文件渲染逻辑。

- A、B 落地后重新核对单文件行数。若仍超标，把每方向上限从 3 降为 2，并优先保前向（调用去向对复核更有意义）。
- 确认 `content_state: Reference(OverLimit)` 的降级标记与 `read the file range on demand` 提示在最小引用化之后依然成立，不因正文变短而丢失。

### D. 命中行号改为 1 基

位置：`crates/app/cce-orchestrator/src/query/retrieval/post_processing/entity_mapper.rs`。

- 从 chunk 记录取值后，赋给 `SearchResult` 的行号统一 +1（0 基行号转 1 基行号），并加注释说明 chunk 记录存 0 基、本类型对外为 1 基。
- 读源码仍传 chunk 记录里的原始 0 基值，`read_source_lines_cached` 与其调用方语义不变，不做兼容叠加。
- `start_line == 0 && end_line == 0` 的哨兵判定随之恢复可靠性，无需改动判断表达式本身。
- 影响面只有对外呈现面：引用行内容、query_review 命中区间、API 返回给调用方的行号。存储与读取链路不变，无需重索引。

### E. 期望区间对照判定

位置：同文件 `render_query_page`。

- 命中表增加一列，标出该命中是否落在 judgment 的期望区间内（任一区间命中即算）。判定逻辑复用 `src/range_evaluator.rs`，不另写一套。
- 页首在 `Expected ranges` 之后加一行汇总：Top-K 中落入期望区间的命中数。once_cell `G1Q1` 这种期望区间完全未覆盖的情形将直接可见，不再依赖人工目测。

### F. 零命中解释与渲染细节

位置：同文件。

- 查询零命中时，除了 `(none)` 输出一行机制说明：跨语言查询在当前 BM25-only 配置下无法命中英文分词索引，属配置边界而非索引缺失。once_cell 4 道 G4 题当前完全无解释。
- 来源命名统一：manifest 与结果行用同一套名字，不再一处 `bm25`、一处 `bm25_recall`。
- manifest 的 `expectations` 行插值真实 judgment 模块名，去掉未替换的字面占位。
- manifest 的关系计数与 `structured/SUMMARY.md` 口径对齐：分别列出实体关系与文件级关系两个数，或至少标注当前数字取的是哪一口径，避免 once_cell 563 与 709、flask 3308 与 1481 这类无法解释的差距。

### G. BM25-only 环境的目录命名

位置：`examples/{rust,python}/query_review_*.rs` 与 `run_query_review`。

- Qdrant 不可达时，输出目录加 `_bm25` 后缀，`index.md` 顶部同样标注能力边界。manifest 已有 capability 行，但目录名与首页仍按混合检索呈现，会误导阅读。

### H. 输出范围说明

位置：同文件 manifest 与 `index.md`。

- 补一行索引范围说明。query_review 走全量索引（flask 83 文件 / 3775 实体），而 `structured` 与 `chunks` 对 flask 使用了排除测试且只收 Python 文件（35 文件 / 1637 实体），命中测试文件的查询无法在后两者中核对。该差异目前在三份输出里都没有说明。

## 3. 验证计划（三轮）

第一轮覆盖 D（生产改动）：`cargo test -p cce-orchestrator`，重点看实体映射与内容引用相关用例；配 `cargo clippy -p cce-orchestrator --all-targets`。本轮失败即停，不进入后续。

第二轮覆盖 A、B、C、E、F、G、H（harness 改动）：`cargo clippy -p cce-e2e-tests --all-targets`，保证编译与 lint 干净。

第三轮重跑四个 review 示例，按问题清单逐项复核：标注区不再出现完整 impl 块、扩展标记中不再出现 trait impl 与 import、命中行号与 fixture 实际行号一致、期望区间判定列生效、G4 页有机制说明、manifest 无字面占位。每轮失败即停，不进入下一轮。
