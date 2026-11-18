# 查询 API 子查询集成评审

> 本文件是问题分析与设计判断；具体改法与落地状态见同目录 `sub-query-aggregation-remediation.md`。
>
> 状态（按本文件 §5 编号）：A 分数域、B 权重悬崖、C 合并身份、D 超时降级、G 原句、I 校验、J 引用降级、K 观测、L 成功计数、M 无键候选、N 重试语义已修复；§6.8 的 MCP 缺位已补齐第 14 个工具；E（两阶段富化）与 H（缓存键）经评估后明确不做，F（`with_source` 死字段）随 E 保留现状 —— 理由与替代路径见 `sub-query-aggregation-remediation.md` 第 5 节。两文件编号不同，按条目名称对应（该文件 §2 给出映射表）。下文描述的是修复前的行为，作为问题依据保留。

## 0. 结论摘要

子查询（`POST /api/search/aggregated`）的**分层位置和执行骨架是合理的**：请求显式给出子查询，服务端共享一次 epoch 视图、有界并发、把阈值/重排/关系扩展后移到合并阶段，只在全部子查询失败时报错。这部分设计比"每条子查询各自 rerank + 各自扩展"要正确得多。

问题集中在三个层面，且都属于"能跑但语义不对"的隐性缺陷：

1. **跨子查询的分数不可比**，却被当成同一 `[0,1]` 证据域直接比较/累加。单路子查询经过 Min-Max 后头部恒为 1.0，混合子查询上限约等于融合权重（默认 0.5），默认 `max` 策略下混合子查询系统性吃亏。
2. **合并身份是"实体"而不是"代码块"**。同一 chunk 在两个子查询里可能由不同实体代表，合并后同一段代码出现两条。
3. **降级策略自相矛盾**：子查询报错可返回部分结果，超时则整单失败并丢弃已完成结果；而并发 4 + 默认 30s 预算下，10 条子查询超时是常态而非异常。

结论：**架构方向可保留，计分与合并的语义需要重做**；其余（富化重复、缓存键派生、校验分散、MCP 缺位）属于工程债。

## 1. 范围与快照

- 仓库：`kkkqkx123/code-context-engine`，评审快照 `952103cc`（`update sub-query 2`，2026-11-18）。
- 评审对象：查询 API 中"子查询"的集成方式，含 API 契约、服务层、编排层合并逻辑、缓存与降级语义。
- 不在范围：索引侧、hybrid 融合算法本身、Graph 查询。

## 2. 集成链路

"子查询"只有一条实现路径，四个调用面全部汇聚到同一个引擎入口：

| 层次 | 位置 | 职责 |
|---|---|---|
| API 契约 | `crates/app/cce-api/src/models/search.rs:209-269` | `SubQueryRequest{text, query_type, weight}`、`AggregatedSearchRequest` |
| HTTP 接入 | `crates/app/cce-server/src/api/handlers/search.rs:197-330` | 项目解析、条数上限、`query_type`→`SearchSources`、权重校验 |
| 共享服务 | `crates/app/cce-server-shared/src/services/search.rs:320-417` | 合并项目配置、拼 `original_query`、构造 `AggregatedQueryOptions` |
| 编排执行 | `crates/app/cce-orchestrator/src/query/coordinator.rs:634-880` | 归一化、并发召回、加权合并、单次 rerank/阈值/扩展、两级缓存 |

调用面：HTTP（`cce-server`）、CLI（`cce-cli/src/commands/agg_search.rs`）、桌面端（`cce-desktop/src/services/search_service.rs`）、前端（`frontend/src/lib/stores/search.svelte.ts`）。CLI 与桌面端都走 `cce-client` 打在同一个 HTTP 端点上，执行语义只有一份，这点是好的。**MCP 面缺失**（`cce-mcp/src/handler.rs:84-220` 共 13 个工具，无聚合检索）。

## 3. 实际执行语义

1. 解析请求、校验条数（上限 10，仅 HTTP 层，`handlers/search.rs:221`）与权重 `(0,1]`（`handlers/search.rs:271-279`）。
2. 共享服务合并项目配置、把子查询文本用 `" + "` 拼成 `original_query`（`services/search.rs:363-373`）。
3. 编排层取一次 epoch 视图（`coordinator.rs:639-642`），按 `(text, sources, weight)` 折叠重复子查询（`83-93`、`661-662`）。
4. 每条子查询派生独立选项：关阈值、按 `limit × N` 放大候选上限（封顶 100）、关 rerank/结果过滤/注解/扩展（`890-927`）。
5. 信号量并发 4 执行完整单查询管线（`696-714`），`join_all` 保序。
6. 加权后按统一对齐键合并，`max` 或 `noisy_or` 累积（`54-77`、`732-812`）。
7. 合并集上执行一次 rerank/过滤 → 全局阈值 → 排序截断（`824-835`）→ 一次关系扩展（`839-848`）。

## 4. 值得保留的决策

| 决策 | 证据 | 为什么合理 |
|---|---|---|
| 子查询由调用方显式给出，服务端不做 NL 拆分 | `docs/app/api/aggregated-search.md:5` | 拆解质量由上层决定，引擎不背 LLM 误差 |
| 共享同一 epoch 视图 | `coordinator.rs:639-642` | 避免 N 次 manifest 读取，也保证子查询看到同一索引快照 |
| 阈值后移、子查询脱阈召回 | `902`、`826-829` | 权重决定排序，而不是"谁先过阈值"的运气 |
| rerank/结果过滤只做一次 | `923-924`、`824` | 省 N 倍成本，且跨查询分数分布一致才可比 |
| 关系扩展只作用于最终 Top-N | `839-848` | 避免每子查询重复扩展与预算浪费 |
| 部分失败可返回成功 + 降级标记不写缓存 | `750-755`、`852`、`cache.rs:437-441` | 瞬时故障不会固化成 TTL 内的"标准答案" |
| 聚合键与单查询键不冲突、共用 single-flight | `cache.rs:178-235`、`coordinator.rs:684-691` | 两层缓存不会互相重入死锁 |

## 5. 缺陷清单

| ID | 严重度 | 问题 | 证据 |
|---|---|---|---|
| A | 高 | 跨子查询分数域不可比：单路 Min-Max 头部恒为 1.0，混合路径不归一化（上限≈融合权重） | `searcher_core.rs:267`、`518`，`279-406` 无归一化调用，`boost/normalization.rs:29-38` |
| B | 高 | 权重与全局阈值形成"悬崖"：`weight ≤ min_score` 的子查询贡献被整段丢弃 | `coordinator.rs:732-736`、`826-828`，默认 `min_score=0.25`（`cce-config/.../search.rs:454`） |
| C | 高 | 合并身份是"首实体"而非 chunk：同一代码块可重复出现 | `cce-types/src/types/alignment.rs:26`，`fusion/merger.rs:738-781`，`coordinator.rs:774` |
| D | 高 | 超时不当降级：已完成子查询结果被丢弃，与部分失败策略冲突 | `coordinator.rs:750-755` vs `869-873` |
| E | 中 | 每子查询都做全量正文富化，合并后丢弃约 90% | `coordinator.rs:921`、`searcher_core.rs:267-276`、`518-525` |
| F | 中 | `with_source` 是死字段：只进缓存键，无消费者 | `query_options.rs:185`、`cache.rs:146`、`coordinator.rs:921,954` |
| G | 中 | 聚合请求没有原句字段，rerank 收到 `"a + b"` 拼接伪查询 | `models/search.rs:224-269`、`services/search.rs:368-373`、`coordinator.rs:930-960` |
| H | 中 | 子查询缓存键含派生 `limit`，同文本不同请求形状不命中 | `cache.rs:154-170`、`coordinator.rs:903-909` |
| I | 中 | 校验三处重复、引擎层无条数上限 | `handlers/search.rs:271-279`、`coordinator.rs:653-659`、`agg_search.rs:133-175`、`handlers/search.rs:221` |
| J | 中 | 排序缺"引用降级"步骤，与单查询结果顺序不一致 | `coordinator.rs:830` vs `post_processing.rs:87-90` |
| K | 中 | 无 per-sub-query 观测：只有计数，无耗时/命中数 | `coordinator.rs:851-866`、`models/search.rs:66-86` |
| L | 低 | `succeeded_sub_queries` 语义与文档不符（空结果也计成功） | `coordinator.rs:851` vs `docs/app/api/aggregated-search.md:83` |
| M | 低 | 未命中对齐键的条目直接追加，不去重不计数 | `coordinator.rs:772-778`、`814-819` |
| N | 低 | 重试队列按子查询入队，一次聚合失败塞 N 条单查询 | `coordinator.rs:538-548` |

## 6. 重点问题详解

### 6.1 分数域：A + B

单路召回在 `enrich` 前调用 `apply_score_normalization`（BM25 `searcher_core.rs:267`、Dense `518`），默认策略 Min-Max（`cce-config/.../search.rs:506-507`），退化输入直接给满额（`normalization.rs:29-38`）。因此**任何单路子查询的第一名都是 1.0**。

混合路径 `run_hybrid_recall`（`searcher_core.rs:279-406`）不调用该归一化，分数来自路径内 Min-Max 后的加权和，默认 0.5/0.5（`cce-config/.../search.rs:309-310`）。于是混合子查询里"只命中一条路径"的结果上限约 0.5。

默认 `max` 策略（`AggregationScoreStrategy::Max`，`search.rs:253-255`）直接比较这两个量：**相同权重下，hybrid 子查询的命中永久排在单路子查询之后**。这不是阈值问题，是排序偏置。

更根本的是，Min-Max 本身依赖列表长度与分布。项目自己的分析文档已把"结果数依赖、跨查询不可比"列为该方案的缺陷（`docs/app/orchestrator/query/score-normalization-analysis.md`）。聚合把不同子查询的 Min-Max 值直接比较，恰好踩在这个已知缺陷上。`noisy_or`（`coordinator.rs:64-67`）更进一步，把序数分数当作独立概率做 `1-Π(1-s)`，缺少校准；配置注释也承认它不适合做默认（`search.rs:244-256`）。

**B 的悬崖效应**：权重在合并前直接乘在分数上（`coordinator.rs:732-736`），合并后统一 `retain(score >= min_score)`（`826-828`）。默认 `min_score = 0.25`（`search.rs:454`），因此：

- `weight ≤ 0.25` 的单路子查询：上限 `1.0 × w ≤ 0.25`，**贡献全部被阈值清空**；
- `weight ≤ 0.5` 的 hybrid 子查询：上限 `≈0.5 × w ≤ 0.25`，同样全灭。

API 允许 `(0,1]` 任意权重且文档只说"分数缩放"（`docs/app/api/aggregated-search.md:51`），没有任何关于与 `min_score` 耦合的提示。调用方付了完整检索成本，拿到零贡献，也没有任何遥测指出这件事。

### 6.2 合并身份：C

单查询内部按 chunk 分组（`fusion/merger.rs:738-781`）：多实体 chunk 折叠成一条，代表是**得分最高的实体**，其余实体记入 `entity_scores`。也就是说代表实体的 id 取决于该子查询的分数分布。

聚合合并键 `alignment_key` 取 `entity_ids.first()`（`cce-types/.../alignment.rs:26`）。若子查询 A 认为实体 E1 最强、子查询 B 认为 E2 最强，两条键 `e:E1` / `e:E2` 不同，**同一 chunk 会在最终结果里出现两次**（`id`、`content` 完全相同），`contributing_sub_queries` 各自为 1。聚合层没有任何 post-merge 的 chunk 级去重，`weak_post_key` 只在"完全没有对齐键"时才兜底（`fusion/aligner.rs:246-274`）。

连带后果：合并后 `score` 被 `combine` 改写，但 `entity_scores` 仍是代表子查询的原值，对外 `EntityScoreDto`（`models/search.rs:199-206`）与 `score` 不自洽。

### 6.3 超时与降级：D

子查询各自带一层 `timeout_ms`（`coordinator.rs:517-526`，取同一 `config.timeout_ms`），外层再包一层（`869-873`）。并发上限 4、条数上限 10 → 至少 3 轮，单条耗时超过 `timeout/3` 时整体必然超时。超时返回 `Err`，已完成子查询的候选全部丢弃，与"部分失败仍返回 200 + `failed_sub_queries`"（`750-755`、文档 `94`）的设计自相矛盾。工程上最坏情况变成：最慢的一条拖垮整单，且调用方看不到任何已完成结果。

### 6.4 富化浪费：E + F

`build_sub_query_options` 固定 `with_source: true`（`coordinator.rs:921`），而 `with_source` 在整条管线里没有消费者——只出现在字段定义（`query_options.rs:185`）、缓存键哈希（`cache.rs:146`）和两处置 true（`coordinator.rs:921,954`）。也就是说**当前无法做"只要标识不要正文"的召回**。

结果：每条子查询都会 `enrich_results`（`searcher_core.rs:267-276`、`518-525`），对最多 100 条候选批量读 chunk 记录 + 正文 + 项目根，合并后只保留 `limit`（默认 10）条。成本随 N 线性增长，绝大部分是纯浪费。

### 6.5 rerank 上下文：G

`AggregatedSearchRequest` 没有"用户原句"字段（`models/search.rs:224-269`）。服务层只能用 `" + "` 拼接子查询文本（`services/search.rs:363-373`），编排层把该串当 rerank 查询（`930-960`）。LLM reranker 收到的是 `"user authentication + login validate"` 这类伪查询——而这恰恰是全流程质量最敏感的一步。文档说"以原始请求文本作为 rerank 上下文"（`docs/plan/aggregated-query-hardening.md`），但接口上根本拿不到原始文本。

### 6.6 缓存：H

子查询走 `CacheKey::from_options_with_view`，`limit` 在键内（`cache.rs:154-170`）；而子查询的 `limit` 是 `request_limit × N` 派生并夹在 `[10,100]`（`coordinator.rs:903-909`）。同一段文本在 N=2 / N=5、limit=10 / 20 下产生互不相同的键，**子查询层缓存的复用率被请求形状绑死**，跨请求几乎不命中。

同时两层缓存都保存带正文的结果；`CacheConfig` 默认 1000 条 / 300 秒（`cache.rs:249-257`），单条聚合缓存最多 100 条候选 × 每块 ≤2000 token，内存放大不可忽略。

### 6.7 契约与校验分散：I

同一套权重规则被实现了三次（HTTP handler、编排层、CLI），错误文案几乎逐字复制；条数上限 10 只在 HTTP handler（`handlers/search.rs:221`），**引擎层没有任何上限**，任何绕开 HTTP 的调用方（MCP、内部评测、未来的 SDK）都能传 1000 条子查询，把 embedder 打满。合理的位置是 `AggregatedQueryOptions` 的构造/校验处，一处生效、按配置调整。

### 6.8 调用面缺口与误用

- MCP 是 agent 侧的主要消费面，但 13 个工具里没有聚合检索，子查询能力对 agent 不可见。
- 前端在未填子查询草稿时，用**同一句话**拆成 `bm25` + `vector` 两条（`frontend/src/lib/stores/search.svelte.ts:376-393`）。这不是多意图分解，而是拿聚合去模拟路径融合，绕过了 hybrid 融合的加权与对齐。语义上等于给系统留了第二条融合实现，且默认策略 `max` 与融合加权和不等价。

## 7. 观测与测试缺口

- 只有 5 个计数（`models/search.rs:66-86`），没有每条子查询的耗时、候选数、贡献数。调权重、定位"是哪条子查询拖慢/污染结果"都无从下手。
- `succeeded_sub_queries = executed - failed`（`coordinator.rs:851`），返回 0 条的子查询也算成功，与文档"贡献了候选列表"（`docs/app/api/aggregated-search.md:83`）不符。
- 单元测试只覆盖三个纯函数（`coordinator.rs:1263-1335`）：合并时代表如何选择、chunk 重复、权重阈值悬崖、超时降级全都无测试。集成覆盖在 `crates/app/cce-server/tests/http_storage_search.rs:224,465` 两条。
- 准入控制按路径鉴权/限流（`cce-admission`），一次聚合请求计 1 个单位，实际消耗 N 倍 embed + N 倍召回，成本模型与限流口径失真。

## 8. 改造建议

**P0（先修语义，改动小、收益最大）**

1. 统一子查询分数域：所有子查询（含 hybrid）在合并前按同一规则归一化到可比尺度；或直接换成分数无关的融合（RRF/加权倒数排名），彻底绕开 Min-Max 的跨查询不可比。
2. 消除权重与阈值的悬崖：权重改为归一化权重（对 Σw 归一，允许任意正数），或合并后再归一；同时在 API 文档中写清 `weight` 与 `min_score` 的相互作用，并对"贡献必然被阈值清空"的组合在计划阶段拒绝。
3. 合并后按 chunk 再分组（当 `fusion.dedup_by_chunk` 开启时），或把合并键改为 chunk 优先、实体次之；同时明确 `entity_scores` 在跨查询合并后是清空还是重算。

**P1（正确性与成本）**

4. 超时按部分失败降级：保留已完成候选、把未完成子查询写入 `failed_sub_queries`、标记 `cacheable=false`；或为聚合单独分配预算（每子查询 `timeout/N`）。注意这会改变 API 语义（超时也回 200）。
5. 让 `with_source` 真正生效，或新增"仅召回"模式：召回阶段只取标识+分数，合并截断后一次性富化。这一项能把聚合的 SQLite/文件读取量从 `N × 100` 降到 `limit`。
6. 新增可选 `query` 字段承载用户原句，rerank 优先用它；缺省回退到第一条子查询文本，而不是拼接串。
7. 子查询缓存键剔除派生 `limit`（改用固定 over-fetch 预算），或让子查询层只缓存轻量候选。

**P2（工程债）**

8. 校验下沉到 `AggregatedQueryOptions` 一处，条数上限进配置；消除 handler/CLI 的重复实现。
9. 排序补齐引用降级步骤，让聚合与单查询顺序规则一致。
10. 补 per-sub-query 观测：耗时、候选数、进入最终 Top-N 的数量（可以直接支撑权重调优）。
11. 明确聚合的定位：要么给 MCP 加聚合工具，要么把前端默认的"同句双路"改为直接走 hybrid。
12. 抽出 `query/aggregation/` 模块：`coordinator.rs` 已 1336 行，聚合约 250 行内联逻辑与 `post_processing::dedup_by_alignment_key`（`374-395`）概念重复但实现分叉（一个只取最大值、一个要做证据累积），合并逻辑值得独立成纯函数以便单测。

## 9. 待拍板开放点

1. **聚合的定位**：只服务"多意图分解"，还是也承接"路径融合"（前端当前默认用法）？若后者，是否直接复用 hybrid 融合而非 `max` 合并。
2. **合并算法**：保留 `max`/`noisy_or`，还是引入 RRF / 归一化加权？是否放弃"聚合分数与单查询分数同尺度"的承诺，改成"每子查询独立阈值 + 合并后只做相对排序"。
3. **权重语义**：改成归一化权重（可 >0），还是保留 `(0,1]` 并只做文档警示。
4. **超时是否降级**：接受"超时也回 200 + 部分结果"的语义变更吗。
5. **缓存分层**：接受"子查询层只缓存轻量候选"带来的 `SearchResult` 结构调整（正文与标识分离）吗。
6. **MCP 暴露**：工具数量已经 13 个，聚合是新增工具还是做成 `search` 的多意图参数。

## 10. 验证方式

- 单测：权重×阈值悬崖（`weight=0.2` 时应显式报错或明确降级）、合并后 chunk 去重、超时部分结果、`succeeded` 语义。
- 集成：构造"同一 chunk 在两条子查询里由不同实体代表命中"的夹具，断言输出 1 条且 `contributing_sub_queries=2`。
- 基准：N=1..10 × limit=10/50 的 p50/p95、embed 调用次数、SQLite 读取次数；对比富化改造前后的读放大。
- 效果对照：`aggregated(bm25+vector 同句)` vs `single hybrid` 的 recall/nDCG，用于判定前端默认用法是否等价，以及权重语义调整后的排序变化。

## 11. 代码坐标索引

| 主题 | 位置 |
|---|---|
| 子查询契约 | `crates/app/cce-api/src/models/search.rs:209-269` |
| 响应计数 | `crates/app/cce-api/src/models/search.rs:55-92` |
| HTTP 校验与映射 | `crates/app/cce-server/src/api/handlers/search.rs:197-330` |
| 服务层装配 | `crates/app/cce-server-shared/src/services/search.rs:320-417` |
| 合并与计分 | `crates/app/cce-orchestrator/src/query/coordinator.rs:41-105`、`765-819` |
| 执行主体 | `crates/app/cce-orchestrator/src/query/coordinator.rs:634-880` |
| 子查询选项派生 | `crates/app/cce-orchestrator/src/query/coordinator.rs:890-960` |
| 聚合缓存键 | `crates/app/cce-orchestrator/src/query/cache.rs:178-235` |
| 降级不缓存 | `crates/app/cce-orchestrator/src/query/cache.rs:430-441` |
| 单查询管线 | `crates/app/cce-orchestrator/src/query/searcher/searcher_core.rs:142-219`、`246-527` |
| 后处理与引用降级 | `crates/app/cce-orchestrator/src/query/searcher/post_processing.rs:25-113`、`374-395` |
| 对齐键 | `crates/core/cce-types/src/types/alignment.rs:21-38`、`.../fusion/aligner.rs:246-274` |
| chunk 分组 | `crates/app/cce-orchestrator/src/query/retrieval/primitives/fusion/merger.rs:738-781` |
| 阈值与截断 | `crates/app/cce-orchestrator/src/query/ranking/threshold_filter.rs:19-31` |
| 默认配置 | `crates/core/cce-config/src/modules/search.rs:234-321`、`450-457` |
| API 文档 | `docs/app/api/aggregated-search.md:96-104` |
| 既有加固计划 | `docs/plan/aggregated-query-hardening.md` |
