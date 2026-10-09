# query_review 输出复核问题清单（once_cell / flask）

检查时间：2026-10-09。输出为当日现场生成，与当前源码一致。

检查范围：

- `outputs/scenarios/rust/query_review/once_cell/`（39 题 + `index.md` + `aggregated_demo.md` + `run_manifest.txt`）
- `outputs/scenarios/python/query_review/flask/`（45 题 + 同上前缀文件）
- 对照基准：同 fixture 的 `structured/`、`chunks/`、`relation_review/`，fixture 源码，judgment 定义，以及 `src/query_review.rs`、`cce-orchestrator` 的 annotator / concatenator / resolver 源码

本文取代早期的 `query-review-output-problems.md`。该文记录的多数缺陷已修复（命中富化恢复、`Reference(OverLimit)` 降级标记正常、标注不再全量失败、`relation_review` 输出齐备、同实体重复命中已消除、`outputs_guide.md` 已收录新目录、中文注释已清理）。下文仅列当前仍存在的问题，共 10 项，按严重度排序。

---

## 1. `Annotated content` 把整个符号体展开，冗余到不可读（最严重）

现象：扩展单元携带的是被引符号的**完整源码**，而不是最小化引用。

- `src/query_review.rs` 的 `snippet_unit()` 用 `entity.span.start_position.row .. end_position.row` 之间**全部行**作为 `ExpandedUnit.code`。期望行为是只给最小化引用：符号所在行，或符号行前后各 1 行。
- once_cell `G1Q1.md` 命中 1 是 `pub struct OnceCell`，`#### Content` 25 行；`#### Annotated content` 先原样复读这 25 行，再追加 3 个单行 trait impl 和一个 96 行的 impl 块全文，标注段落达到 339 行，是正文的 13.6 倍。
- flask `G1Q1.md` 命中 1 是 `wsgi_app`（51 行），扩展把 `full_dispatch_request`（28 行）、`request_context`（15 行）全文塞入。
- once_cell `G2Q6.md` 命中 3 是 `field cell`，`#### Content` 仅 2 行，标注里追加了 `impl Lazy` 38 行全文。

第二层冗余：`AnnotatedResult.annotated_content` 必然以 primary 全文开头（`from_primary` 用 `unit.code` 初始化），于是同一份代码在同一文件里出现两份。once_cell `G1Q1.md` 全篇 1227 行，就是这种"正文一遍、标注一遍、标注再加扩展"的结构。

规模：once_cell 39 个查询文件合计 38515 行，flask 45 个合计 48863 行，单文件普遍 1000–1900 行，人工复核成本极高，与"让审查者快速比对命中与期望"的目标相反。

修复方向：

- 扩展单元降为最小引用（符号定义行前后各 1 行），完整正文继续由 `#### Content` 承担；
- 或者在渲染层改为增量模式，只输出扩展部分并显式标注 primary 见上。

另需一并处理：`expansion_units()` 对 struct / field / module 这类**非函数实体**同样取 `get_callees` / `get_callers`，把"包含 / 实现"渲染成"调用"。once_cell `G2Q6.md` 中 `field cell` 的 `// [called by]` 挂的是 `impl RefUnwindSafe for Lazy`，语义上是错的。

## 2. 关系增强未按关系域过滤，结构域与依赖域关系被当成调用

现象：

- once_cell 全部查询合计 932 个扩展标记，其中 205 个指向"起止行相同"的单行 impl / 声明。`// [called by] Sync (src/imp_std.rs:32-32)`、`Send (…:33-33)`、`RefUnwindSafe (…:35-35)` 这类 trait impl 噪声，在 `G1Q1.md` 占 12/29，`G1Q3.md` 占 11/26。
- 这些边在 `relation_review/once_cell/entity/OnceCell-1e.md` 中被明确记录为 `impl_association` 与 `trait_bound`，属于结构域，不是调用。
- flask 侧同类问题更明显：`// [calls] App (src/flask/app.py:44-44)` 出现 60 次，而第 44 行是 `from .sansio.app import App`，属于导入；`// [called by] Blueprint (src/flask/blueprints.py:18-128)` 出现 36 次，第 18 行是 `class Blueprint(SansioBlueprint)`，属于类声明 / 继承。标签写成 `calls` / `called by`，与实际关系域不符。

根因链（已定位到代码）：

- `query/relation_searcher.rs` 的 `get_callees` / `get_callers` 落到 `query.rs` 的 `get_callees_by_entity` / `get_callers_by_entity`，返回的是**全部关系域**的已解析关系，没有限定调用域。
- `src/query_review.rs` 的 `snippet_unit()` 构造 `ExpandedUnit` 时既不设置 `relation_type`，标签也硬编码为 `"calls"` / `"called by"`。
- `ExpandedUnit::is_call_domain()` 在 `relation_type == None` 时返回 `true`，于是 annotator 里 `allow_structural_edges=false` 这一默认过滤完全失效。
- `annotator.rs` 的注释称 stdlib / external 过滤是"调用方已过滤之后的防御第二层"，但调用方一侧实际没有任何过滤。

关于"是否应把 stdlib 过滤纳入关系增强"的结论：**不应，二者不是同一个问题。**

- 索引期 `filter_stdlib_calls`（`cce-codegraph/src/index/resolver.rs:466-475`）只丢弃**未解析到本地实体**且判定为 stdlib 的外部调用；该处注释明确写了"内部已解析的被调用者即使名字像标准库也保留"。
- `Sync` / `Send` / `RefUnwindSafe` 的 impl 在 once_cell 仓库内已解析为本地实体，`is_external` 为 false，stdlib 分支根本不进入。若按名字把它们并入 stdlib 过滤，会误删本地代码。
- 正确修法是在调用方（`expasion_units` 或 relation_searcher 的扩展包装）按 `relation_type.is_call()` 过滤，只保留调用域；`ResolvedRelation` 已携带 `relation_type`，改动点明确。
- 若仍要保留 stdlib 语义，应定义为"外部未解析 + stdlib 类别"，并在构造 `ExpandedUnit` 时由调用方调用 `.with_stdlib()`；annotator 只做兜底。现有代码结构已是这个意图（`with_stdlib` / `with_external` builder 存在），只是两侧都没接上。
- 附带问题：`RelationConfig.filter_stdlib_calls`（索引期）与 `RelationAnnotationConfig.filter_stdlib`（标注期）是同名不同层的两个开关，语义重叠易误读，建议改名区分，例如索引期叫 `filter_external_stdlib_calls`。

## 3. BM25-only 环境下语义结论不成立，默认目录名仍误导

两份 manifest 均为 `qdrant_available: no`、`sources: bm25`、`vectors=0`，全部命中 `vector: 0.0000`、`boosted: false`。

- flask 20 道语义题（G2）与 16 道模糊题（FZ）、once_cell 11 道语义题与 16 道模糊题，结论只反映关键词匹配，体现不出向量语义。
- once_cell 4 道跨语言题（G4，中文查询）全部 0 命中，跨语言能力在此输出中完全未被行使。
- 输出目录仍是 `query_review/{project}`，无后缀；manifest 虽有 `capability` 行，但目录名与 `index.md` 首页都没有标注。

建议：无向量环境时目录改用 `_bm25` 后缀，`index.md` 顶部同样标注能力边界。

## 4. 命中了 `tests/`，而对照基准（structured / chunks）被人为排除，无法交叉验证

- `query_review` manifest：flask `files=83 entities=3775`；`structured/flask/SUMMARY.md`：`Files 35、Entities 1637`；`chunks/flask/{emb,bm25}` 各 33 个文件。
- 差距来自 `examples/python/export_py.rs` 对 flask 使用了 `ReviewFilterOptions::default().with_exclude_tests(true)`，且 `include_patterns: &["*.py"]`——即导出报告时排除测试文件、只收 Python 文件，而索引是全量的。
- 后果：`G2Q2.md` 命中 `tests/test_basic.py:955-966` 与 `tests/test_testing.py:250-266`、`G2Q1.md` 命中 `tests/test_basic.py` 等，在 structured / chunks / relation_review 里都没有对应符号表可核对。
- 三层输出的索引范围不一致，而三份输出都没有说明这一点。

建议：manifest 增加索引范围字段（include / exclude 模式），`index.md` 说明与 `structured` 输出的范围差异。

## 5. `kind` 与 `name` 元数据失真

- flask 的 `class wsgi_app`、`class __call__`、`class run`：把方法标成了 class（`structured/flask/src/flask/app.py.txt` 明确记为 `method`）。
- once_cell 的 `inherent_impl new`：实体本身是 `impl<T> OnceCell<T>` 块，`name` 取了组内首个成员 `new`，与实体身份不符。
- 影响：命中汇总表的 entity 列与详情标题不可信，无法按类型筛选或做统计。

## 6. 命中行号比 fixture 实际行号少 1

- flask `G1Q1.md` 命中 `src/flask/app.py:1565-1615`；fixture 与 structured、judgment 三方一致为 `1566-1616`。
- flask `G2Q1.md` 命中 `run` 为 `src/flask/app.py:631-752`；实际 `def run(` 在 632 行，structured 为 `L632-L753`。
- once_cell `G1Q1.md` 命中 `src/lib.rs:400-423`，而 `pub struct OnceCell` 在 L421-L424，起点 400 是空行，范围含前导空白。

影响：`Expected ranges` 与命中区间无法直接比对，标尺失效。需要核对富化阶段把树 span 的 0-based 行号换算为 1-based 时的偏移，以及 group span 是否吞掉了前导空白。

## 7. 缺"期望 vs 命中"的对照判定

每个 `{query_id}.md` 给出 `Expected ranges` 与 `Hits (top-K)` 两张表，但没有把二者对齐：没有"命中是否落在期望区间"的判定列，也没有按相关等级（strong / related）汇总。

- 例如 once_cell `G1Q1` 期望 `src/lib.rs:480-482`（`pub const fn new()`），Top-10 无一落在该区间，页面无任何提示，审查者只能目测。
- 仓库已有 `src/range_evaluator.rs` 实现区间判定逻辑，可直接复用，在命中表加一列 `in_expected` 并在页首给一行汇总。

## 8. 跨语言题全 0 命中无任何解释

once_cell `G4Q1`–`G4Q4` 全部 0 命中、耗时 0–1ms，页面只有 `(none)`。没有"中文查询分词无法命中英文 BM25 索引"的说明，审查者会误判为索引缺失或 bug。建议在零命中页补一行机制说明。

## 9. 命名与渲染细节偏差

- 来源命名不一致：manifest 与 Config 行写 `sources: bm25`，结果行写 `result sources: bm25_recall`。
- manifest 的 `expectations: src/judgments/once_cell.rs relevant_ranges` 是未插值的字面占位符。
- 关系计数口径不一致且未标注：once_cell manifest `relations=563`，`structured/SUMMARY.md` 为 709（563 entity + 146 file-level）；flask manifest `relations=3308`，structured 为 1481（差异原因见第 4 项）。
- `aggregated_demo.md` 现已按对齐键去重（once_cell `e:314` 只出现一次），与单查行为一致，此项已无问题。

## 10. 扩展渲染占用全部预算，导致正文被截断

`anannotated_length` 限制为 8000 token，而 `truncated: false` 在全部命中上成立——因为超大正文先被降级为 `Reference(OverLimit)` 引用行。观察到的副作用是：能进正文的都是中等体量符号，扩展单元却可以带进多个百行级 impl 块，挤占的正是本该给其它命中的一致性。once_cell `G1Q1.md` 命中 3 为 96 行 impl 块，其后仍追加了 4 个扩展单元。建议把"每扩展单元最小引用化"（第 1 项）与"按域过滤"（第 2 项）一并实施后，再复核预算是否仍需要引用降级。

---

## 已验证正常的部分

- 题目与文件一一对应：once_cell 39 题、flask 45 题，输出文件数与 judgment 集合一致，无遗漏、无多余。
- 索引解析链路健康：`structured/`、`chunks/` 的行号、正文、实体统计完整准确（对照时注意第 4 项的范围差异）。
- 富化链路已恢复：命中带正文、行号与 kind，`content_state: Reference(OverLimit)` 降级标记与"read the file range on demand"提示均正常。
- `relation_review/` 输出齐备：逐种子 callee / caller / 前向链 / 后向链、ego 图表格、Mermaid 图、机器可读 JSON 快照均有；once_cell 9 个种子、flask 8 个种子。
- 同实体重复命中已修复：once_cell `G1Q1.md` 的 10 个命中对应 10 个不同对齐键；`aggregated_demo.md` 与单查去重行为一致。
- `docs/outputs_guide.md` 已收录 `query_review` 与 `relation_review` 的目录结构与阅读方法。
- `src/query_review.rs` 已无中文注释。
