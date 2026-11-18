# 子查询聚合：问题清单、决策与修改方案

> 评审依据快照 `952103cc`。问题分析见同目录 `sub-query-aggregation-design-review.md`，本文件承担"改什么、怎么改、哪些明确不改、为什么"。
>
> 迭代记录：第一轮落地 A/B/C/F/H/I/K；第二轮（本文件当前状态）对剩余七项逐一决策，落地 D/J/L/M 与 N 的 MCP 面，E/G 与 N 的前端定位决定不做并给出替代路径。

## 0. 摘要

聚合检索的分层与骨架保持不动：调用方显式给出子查询、共享 epoch 视图、有界并发、阈值/重排/扩展后移、两级缓存。

14 条问题里 **12 条落地、2 条明确不做**：

| 类别 | 条目 | 结论 |
|---|---|---|
| 让结果变对 | A 分数刻度、C 合并身份、L 无键候选 | 已修复 |
| 让成本不白付 | B 权重悬崖、H 计划校验、D 超时降级、M 重试语义 | 已修复 |
| 让调用方能判断 | F 原句、I 引用降级、K 成功计数、J 逐子查询观测 | 已修复 |
| 让入口完整 | N 的 MCP 面 | 已修复（第 14 个工具） |
| 结构性改造 | E 两阶段富化、G 缓存键 | **不做**，理由与替代路径见第 5 节 |
| 产品定位 | N 的前端用法 | **不改代码**，转待拍板第 9.1 条 |

第二轮的核心是两件事：**把"超时=整单失败"改成"超时=部分结果 + 逐子查询状态"**，以及**把每条子查询的召回质量变成可读的数据**（`sub_query_stats`）。这两件事绑定落地，因为超时本身必须靠观测字段才能被看见。

## 1. 现状一句话

每条子查询跑完整单查询管线后，按"对齐键"把候选用 `max`/`noisy_or` 合并；分数在合并前被乘以权重，合并后统一过一次阈值。问题全部出在"分数怎么对齐""合并以什么为身份""失败/超时怎么收场"这三步。

## 2. 完整问题清单

| ID | 级别 | 问题 | 证据（评审快照） | 状态 |
|---|---|---|---|---|
| A | 高 | 跨子查询分数刻度不一致：单路子查询被 Min-Max 拉成头部 1.0，混合子查询保留加权和（头部≈0.5），`max` 合并按路径配置而非相关性排序 | `searcher_core.rs:267`、`518`、`279-406`，`normalization.rs:29-38` | 已修 |
| B | 高 | 权重与全局阈值形成悬崖：`weight ≤ min_score` 的子查询贡献必然被丢弃，仍付完整成本且无提示 | `coordinator.rs:732-736`、`826-828`（旧码），默认 `min_score=0.25` | 已修 |
| C | 高 | 合并身份是"首实体"：同一 chunk 在两条子查询里由不同实体代表时，结果里出现两条相同代码 | `cce-types/.../alignment.rs:26`、`merger.rs:738-781` | 已修 |
| D | 高 | 超时整单失败并丢弃已完成结果，与"部分失败降级"策略冲突；并发 4 下 10 条子查询超时是常态 | `coordinator.rs:869-873`（旧码）vs `750-755` | 已修 |
| E | 中 | 每条子查询都做全量正文富化，合并后丢弃约九成；`with_source` 为死字段，无法只召回 | `coordinator.rs:921`、`query_options.rs:185`、`cache.rs:146` | 不做（第 5.1 节） |
| F | 中 | 聚合请求无原句字段，rerank 上下文退化为 `"a + b"` 拼接串 | `models/search.rs:224-269`、`services/search.rs:363-373` | 已修 |
| G | 中 | 子查询缓存键含派生 `limit`，同文本不同请求形状互不命中；两层都缓存整块正文 | `cache.rs:154-170`、`cache.rs:249-257` | 不做（第 5.2 节） |
| H | 中 | 权重/条数校验分散在 HTTP、编排、CLI 三处；引擎层无任何条数上限 | `handlers/search.rs:221,271-279`、`agg_search.rs:133-175` | 已修 |
| I | 中 | 合并后缺"引用降级"排序，聚合与单查询对同分结果顺序不一致 | `coordinator.rs:830`（旧码）vs `post_processing.rs:87-90` | 已修 |
| J | 中 | 无 per-sub-query 观测（耗时、候选数、贡献数），权重与预算无从调优 | `coordinator.rs:851-866`（旧码） | 已修 |
| K | 低 | `succeeded_sub_queries` 把空结果子查询计为成功，与文档语义不符 | `coordinator.rs:851`（旧码） | 已修 |
| L | 低 | 无对齐键的候选直接追加，不去重、不计数 | `coordinator.rs:772-778`（旧码） | 已修 |
| M | 低 | 重试队列按子查询入队，一次聚合失败塞 N 条单查询重试 | `coordinator.rs:538-548`（旧码） | 已修 |
| N | 低 | MCP 面没有聚合工具（13 个工具）；前端用同一句话拆双路，等于用聚合模拟路径融合 | `cce-mcp/src/handler.rs`、`search.svelte.ts:376-393` | MCP 已修；前端不改码 |

编号说明：本文件的字母按**修复批次**排列，与 `sub-query-aggregation-design-review.md` §5 的编号不同，两文件按条目名称对应：

| 本文件 | 评审文档 | 条目 |
|---|---|---|
| A / B / C / D / E | A / B / C / D / E+F | 分数刻度 / 权重悬崖 / 合并身份 / 超时降级 / 富化浪费（含 `with_source` 死字段） |
| F / G / H / I / J | G / H / I / J / K | 原句字段 / 缓存键 / 校验分散 / 引用降级 / 逐子查询观测 |
| K / L / M / N | L / M / N / §6.8 | 成功计数 / 无键候选 / 重试语义 / 调用面缺口（MCP、前端） |

## 3. 第二轮修改方案总览

| ID | 方案要点 | 落点 |
|---|---|---|
| D | 子查询各自受"请求截止时间 − 后置预留"约束，到期判 `timed_out`，其余子查询照常合并返回 | `coordinator.rs:721-731`、`aggregation.rs:148` |
| J | 响应新增 `sub_query_stats`（状态/候选数/贡献数/耗时/原因），单查询与整单缓存命中为空数组 | `query_result.rs:61-107`、`models/search.rs:94-158`、`services/search.rs:189` |
| L | 无身份候选回退到"文件 + 行范围"作最后一级合并键 | `aggregation.rs:120-141` |
| M | 子查询走不写重试队列的执行入口 | `coordinator.rs:521-534` |
| N | MCP 新增 `aggregated_search`（第 14 个工具） | `cce-mcp/src/handler.rs:98`、`handler/search.rs:283` |

## 4. 第二轮修改详述

### 4.1 超时降级为部分结果（D）

**动机**：旧实现用 `tokio::time::timeout(timeout_ms, get_or_compute(...))` 包住整单。一旦到期，future 被 drop，已经跑完的子查询结果随闭包一起消失，返回一个 `Timeout`。而"某条子查询报错"却能返回部分结果 —— 同样是"部分不可用"，两种收场方式相反。默认预算 30 s、并发 4 的情况下，10 条子查询里只要有一条慢，整单归零。

**改动**：

1. 请求开始时算出绝对截止时间 `deadline = now + timeout_ms`。
2. 子查询截止时间 `sub_query_deadline = deadline − post_processing_reserve_ms(timeout_ms)`，预留 = `clamp(budget/4, 25 ms, 4000 ms)`。
3. 每条子查询用 `tokio::time::timeout_at(sub_query_deadline, ...)` 单独约束；到期的那条记为 `timed_out`，文本进入 `failed_sub_queries`，其余子查询的结果照常合并、重排、返回 `success: true`。
4. 整单仍受请求级 `timeout_ms` 兜底，作为最后一道防线。

**为什么预留是"预算的四分之一、上限 4 秒"**：预留要保护的是合并后的 rerank/阈值/扩展，其中 rerank 的候选上限是 `rerank.max_candidates`（默认 50），是唯一的数量级开销。预留偏小 → rerank 被截断，整单仍然超时，等于没改；预留偏大 → 召回提前收尾，候选少一点，但答案仍有效。两种误判的代价不对称，所以按"宁可少召回，不可整单丢"取偏大的值。

**为什么不做"共享收集器"**：另一条实现路径是把子查询结果写进共享累加器，让请求级超时后仍能捞出已完成部分。但那样必须把执行移出 `get_or_compute` 闭包，`cacheable`/`from_cache`/single-flight 槽位释放三处语义都要重写，回归面远大于收益。当前方案在闭包内部就把超时收敛掉，缓存语义保持原样。

**可见性**：`timed_out` 与 `failed` 都进 `failed_sub_queries`，因此这类响应 `cacheable = false`，不会被写入聚合缓存 —— 一次超时不会变成整个 TTL 的答案。

**已知边界**：若 rerank 本身耗时超过预留，整单仍会在请求级超时处失败。这属于"后置阶段超时"，与子查询超时是两件事，当前不额外区分（见第 9.4 节）。

### 4.2 逐子查询观测（J）

**动机**：旧响应只有 `failed_sub_queries`（文本）和三个计数。调用方看得到"有几条成功"，看不到"哪条白跑了"：一条召回了 100 条候选却全部在合并时被吃掉，和一条压根没命中的子查询，在旧响应里完全一样。权重、子查询数量、预算三者的调优因此只能靠猜。

**改动**：`QueryResult` 与响应体新增 `sub_query_stats`：

| 字段 | 含义 |
|---|---|
| `index` / `text` | 位置与实际执行的文本（去重后仍按调用方顺序） |
| `status` | `succeeded` / `empty` / `failed` / `timed_out` |
| `candidates` | 对齐、合并前的召回条数 |
| `contributed` | 最终列表（合并 + 阈值 + 数量限制之后）里由它贡献的条数 |
| `elapsed_ms` | 该子查询的墙钟耗时 |
| `error` | `failed` / `timed_out` 的原因 |

**`contributed` 为什么在最终裁剪之后统计**：在合并处统计只能得到"进入过候选池"，那对每条子查询都恒等于 `candidates`，没有信息量。真正的信号是"它贡献的东西最后留下了几条"—— 这需要拿最终 `merged` 逐条回推合并键，再从 `contributors` 反查归属。代价是 O(最终条数 × 键查找)，最终条数受 `limit` 约束（≤ 100），可忽略。

**整单缓存命中时清空**：`executed_sub_queries` 等计数在缓存命中时归零，理由是"这次请求什么都没跑"。`sub_query_stats` 属于产生缓存条目的那一次运行，留着会与计数自相矛盾（说有 3 条成功，又说 0 条执行），因此一并清空。

### 4.3 无键候选回退到位置键（L）

**动机**：没有对齐键、也没有正文的候选（插件块、异常块、正文被裁掉的块）在旧实现里直接 `unkeyed.push`，既不参与去重也不计入 `contributing_sub_queries`。它们数量很少，但同一段代码从两条子查询各来一次就会重复出现。

**改动**：合并键派生加一级回退 —— 文件路径 + 起止行（要求 `end_line > start_line`）作为 `l:{path}:{start}-{end}`。没有真实行范围的候选仍然保持无键、直接追加，避免把同一文件里所有无范围候选压成一条。

**取舍**：位置键只在"既无身份又无正文"时生效，与单查询路径的 `weak_post_key`（路径 + 正文哈希）互补，两者都不引入跨 chunk 推断。

### 4.4 子查询不写重试队列（M）

**动机**：聚合走的是 `search_with_view`，与普通查询共用一个"可重试错误入队"分支。于是一次聚合失败会把 N 条子查询当成 N 条独立查询排队；服务恢复后它们被逐条重放，写进的是单查询缓存，而调用方要的是合并结果 —— 成本付了，语义没还原。

**改动**：拆出 `search_sub_query_with_view`（`coordinator.rs:521`），与 `search_with_view` 共用 `search_with_view_inner`，只把"是否入队"设为 false。

**为什么整单也不入队**：聚合的部分结果本身就是有效答复；重放一次完整聚合需要保存子查询列表、权重、过滤器与关系快照，属于新契约，不在本轮范围。

### 4.5 MCP 聚合工具（N 的 MCP 面）

**动机**：MCP 面 13 个工具里有单查询检索、文件检索、关键词检索，唯独没有聚合检索 —— 而"分解后合并"恰恰是 agent 侧最常见的使用方式，缺位会把 agent 逼回"连发 N 次 search 再自己拼"。

**改动**：新增 `aggregated_search` 工具（第 14 个），接入同一 `SearchService::run_aggregated`：参数含 `query`（原句）、`sub_queries[{text, query_type, weight}]` 与全套过滤/重排/扩展开关，返回体与 HTTP 一致（含 `sub_query_stats`）。工具总数断言由 13 改为 14，允许列表机制不变。

## 5. 明确不做的两项与理由

### 5.1 E 两阶段富化：不做（当前形态下会伤去重）

**收益**：现在每条子查询都会按 `limit × 子查询数`（上限 100）批量取一次正文，合并后保留约 `limit` 条，读放大接近 N 倍。

**为什么不能直接做**：把富化推到合并之后，意味着子查询阶段的结果是"无正文"的。而跨子查询去重依赖的身份（`entity_ids`、`segment_id`）**正是富化阶段从 chunk 记录回填的**（`aligner.rs:100-110` 的 `segment_id` 回填与 `is_alignable` 判定）。去掉富化，跨查询合并键会退化到 chunk-id 兜底，而 chunk-id 在向量路径与 BM25 路径上常常不同（`FusionAlignmentStats::unalignable` 就是为这件事存在的），结果是"同一段代码出现两次"这个刚修好的问题（C）反过来恶化。

**替代路径**（下一轮方向，已明确）：把身份从"富化副产品"改成"召回时携带" —— 召回阶段直接带 `entity_ids` / `segment_id`，富化只负责正文。那时才能安全地把子查询阶段的 `with_source` 置 false，并在合并后的窗口（`max(limit × 3, rerank.max_candidates)`）上做一次富化。

### 5.2 G 缓存键：不做（必须与条目容量一起设计）

**现状**：子查询缓存键包含派生出的 `limit`（`limit × 子查询数`，clamp 到 `[10, 100]`）与整份配置指纹，所以"同样两条子查询、`limit=5` 与 `limit=10`"是两条独立缓存，几乎不可能复用。

**为什么不直接剔除 `limit`**：缓存条目存的是截断后的列表。若键里不区分 `limit`，先来的 `limit=5` 请求会把只有 5 条的条目写进缓存，后来的 `limit=10` 请求命中它，就只拿到 5 条 —— 静默截断比不复用更糟。

**替代路径**：键里不写 `limit`，条目一律按该子查询的上限容量（`MAX_SUBQUERY_LIMIT`）存，命中后按请求 `limit` 截断返回。这需要同时改动条目语义、`from_cache` 的计数口径与测试基线，属独立改造。

### 5.3 N 的前端定位：不改代码

前端在聚合模式下把一句话拆成两条子查询（原文 + 关键词变体），本质是"用聚合模拟路径融合"。这属于产品定位问题（聚合是"调用方已分解"还是"服务端替你分解"），不是缺陷修复能决定的，因此只升级类型定义（`schema.d.ts` 增加 `sub_query_stats`），不改调用方式。

## 6. 行为变化与兼容性

| 变化 | 影响面 | 说明 |
|---|---|---|
| 子查询超时返回部分结果 | 聚合 API/CLI/MCP | 由"错误 + 空"变为"`success: true` + `failed_sub_queries` + `timed_out` 状态"；这类结果不写缓存 |
| 新增 `sub_query_stats` | 响应体（附加字段）、前端类型 | 旧客户端忽略即可；整单缓存命中时为空数组 |
| 无键候选按位置合并 | 聚合响应条目数 | 同一文件同一行范围不再重复返回 |
| 子查询失败不再入重试队列 | 服务内部 | 重试队列只保留整条查询；聚合的部分失败不再产生"孤儿重试" |
| MCP 工具数 13 → 14 | MCP 客户端 | 允许列表若显式枚举工具名，需加入 `aggregated_search` |

## 7. 验收标准

1. 同一批子查询在混合路径与单路路径下，头部候选的合并分数都能达到 `weight` 量级，不再出现"单路恒高"。（A，单测 `alignment_puts_every_path_head_on_one_ceiling`）
2. `max` 策略下 `weight=0.2`、`min_score=0.25` 的请求在服务层返回 400，不做任何检索。（B，集成用例）
3. 构造"同一 chunk、不同实体代表"的两条子查询，合并结果只保留一条，`contributing_sub_queries=2`。（C）
4. 传入 `query` 时，rerank 收到的上下文等于该字段；未传时回退为拼接串。（F）
5. 聚合与单查询对同分"完整正文 / 引用"给出相同顺序。（I）
6. 产出空列表的子查询不计入 `succeeded_sub_queries`。（K）
7. **一条子查询超时、其余成功时返回部分结果，`failed_sub_queries` 含该条文本，`sub_query_stats` 对应项为 `timed_out`。**（D）
8. **单条子查询成功时 `contributed` 等于最终返回条数，`candidates` 为召回条数。**（J，集成用例 `aggregated_bm25_search_succeeds_on_indexed_project`）
9. `cargo clippy --all-targets` 与 `cargo fmt --check` 通过，聚合单测与 MCP 工具数断言通过。（已覆盖）

其中第 7 条是唯一没有自动化用例的验收项：要造出"一条慢、一条快"的子查询需要可注入延迟的 embedder stub，当前测试环境（`crates/app/cce-server/tests/common.rs`）只有固定返回的本地 LLM stub。已在第 9.4 节列为待定。

## 8. 风险与回滚

- 预留切走一部分召回预算：极端情况下（预算紧、子查询多）召回会比以前早收尾，候选变少。回滚只需把 `post_processing_reserve_ms` 调小或返回 0（此时等价于旧行为，但不再保护后置阶段）。
- `sub_query_stats` 增加响应体积：每条约 100 字节，条数上限 16，可忽略。
- 位置键合并可能把"同文件同范围的两次不同召回"合成一条 —— 这正是期望行为；若下游依赖重复条目做计票，应改用 `contributing_sub_queries`。
- 子查询不再入重试队列：服务短暂不可用时聚合请求不会自动补跑，由调用方重试（语义更清晰）。

## 9. 待拍板开放点

1. **聚合定位**：是否包含"路径融合"（前端当前默认用法），若包含是否应直接复用 hybrid 融合而非两条子查询。
2. **是否引入 RRF / 加权倒数排名**替代 `max` / `noisy_or`，彻底摆脱分数刻度问题。
3. **权重是否允许大于 1**（相对份额语义），需同步重定义阈值。
4. **后置阶段超时是否单独区分**：当前 rerank 超出预留仍会整单失败；是否改为"返回未重排的合并结果 + `degraded` 标记"，以及第 7 节第 7 条的自动化用例是否随之一并补。
5. **引擎条数上限取值**（当前 16）与是否需要配置化。
6. **E 的身份前置改造**是否排入下一轮（召回阶段直接携带 `entity_ids` / `segment_id`）。
7. **MCP 允许列表**默认是否自动包含新工具，还是要求显式枚举（影响升级体验）。

## 10. 落地状态

| 项 | 文件 | 状态 |
|---|---|---|
| 聚合辅助（缩放、计划校验、合并键含位置回退、后置预留） | `crates/app/cce-orchestrator/src/query/aggregation.rs` | 已完成，含 10 个单测 |
| 逐子查询统计类型 | `crates/app/cce-orchestrator/src/query/types/query_result.rs` | 已完成 |
| 协调器：子查询截止时间、状态归集、归属统计、重试抑制 | `crates/app/cce-orchestrator/src/query/coordinator.rs` | 已完成 |
| 引用降级可见性 | `crates/app/cce-orchestrator/src/query/searcher/post_processing.rs` | 已完成 |
| 契约：`query`、`sub_query_stats`、`SubQueryStatDto` | `crates/app/cce-api/src/models/search.rs` | 已完成 |
| 服务层装配与 DTO 转换 | `crates/app/cce-server-shared/src/services/search.rs` | 已完成 |
| HTTP 透传 | `crates/app/cce-server/src/api/handlers/search.rs` | 已完成 |
| MCP 聚合工具 | `crates/app/cce-mcp/src/handler.rs`、`handler/search.rs` | 已完成 |
| 前端类型与 `query` 透传 | `frontend/src/lib/api/schema.d.ts`、`stores/search.svelte.ts` | 已完成（未改调用方式，见 5.3） |
| 文档 | `docs/app/api/aggregated-search.md`、`docs/app/server/mcp-api.md` | 已完成 |
| E 两阶段富化、G 缓存键、前端定位 | — | 明确不做，见第 5 节 |

验证记录：

- `cargo check --workspace --all-targets`：通过（含 11 个递归子模块）。
- `cargo clippy -p cce-orchestrator -p cce-api -p cce-server-shared -p cce-server -p cce-mcp -p cce-cli --all-targets`：无新增告警（仅 `cce-parser` 一条既有 `question_mark`）。
- `cargo fmt --all --check`：通过。
- `cargo test -p cce-orchestrator --lib query::`：347 项通过；其中 `query::aggregation` 10 项（新增"位置回退键""后置预留"两项）。
- `cargo test -p cce-mcp --lib`：14 项通过（含工具数 14 的断言与聚合参数反序列化用例）。
- `cargo test -p cce-server --test http_storage_search aggregated`：4 项通过，其中 `aggregated_bm25_search_succeeds_on_indexed_project` 额外断言 `sub_query_stats` 的条数、文本、状态、候选数与贡献数。

## 11. 代码坐标

| 主题 | 位置 |
|---|---|
| 聚合辅助（合并键 / 后置预留） | `crates/app/cce-orchestrator/src/query/aggregation.rs:120`、`:148` |
| 子查询统计类型 | `crates/app/cce-orchestrator/src/query/types/query_result.rs:61`（状态）、`:90`（统计） |
| 聚合主流程入口 | `crates/app/cce-orchestrator/src/query/coordinator.rs:665` |
| 截止时间与后置预留 | `crates/app/cce-orchestrator/src/query/coordinator.rs:721-731` |
| 子查询执行入口（不入队） | `crates/app/cce-orchestrator/src/query/coordinator.rs:521` |
| 状态归集 / 归属统计 | `crates/app/cce-orchestrator/src/query/coordinator.rs:764`、`:931` |
| 响应契约 | `crates/app/cce-api/src/models/search.rs:94-158` |
| 服务层 DTO 转换 | `crates/app/cce-server-shared/src/services/search.rs:189` |
| MCP 工具 | `crates/app/cce-mcp/src/handler.rs:98`、`crates/app/cce-mcp/src/handler/search.rs:283` |
| API 文档 | `docs/app/api/aggregated-search.md` |
| MCP 文档 | `docs/app/server/mcp-api.md` |
