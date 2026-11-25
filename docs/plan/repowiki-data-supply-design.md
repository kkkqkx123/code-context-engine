# RepoWiki 数据供给设计（code-context-engine 作为上游）

> 术语与入口约定：WCP 导出以 `cce wiki context` / `cce wiki diff` 为唯一公开入口，HTTP 端点仅为其内部传输；MCP 的只读画像已由 `wiki_read` 更名为 `read_only` 并作为默认画像。三者的职责边界与术语统一见 `docs/plan/wiki-surface-consolidation-design.md`。

## 背景

RepoWiki 类工具的核心流水线为：目录扫描 → 构建导入依赖图 → PageRank 计算文件权重 → LLM 多轮分析生成结构化 Wiki → 增量缓存 → 导出 Markdown/HTML。其通用底层范式把工作拆成两半：**确定性的静态分析**（文件遍历、AST、依赖图、重要度排序，不需要 LLM）与**叙述性的 LLM 生成**（总览、模块文档、架构图、阅读指南），中间以缓存和溯源校验衔接。

本项目（Code Context Engine，下称 CCE）本身就是代码库的确定性理解层：tree-sitter 解析实体、关系索引构建调用/依赖图、BM25 与向量双路检索、SQLite 元数据与增量缓存。因此定位如下：

- **CCE 是上游数据与查询提供方**：向 RepoWiki 生成器供给「扫描结果、依赖图、重要度、源码访问能力」。
- **RepoWiki 生成器是下游调用方**：负责页面规划、LLM 叙述、Mermaid 渲染与最终导出；生成过程中允许**有限步数的工具调用**（grep、read file、CCE 提供的工具）回查代码。
- CCE 不生成 Wiki 文案本身。LLM 叙述归下游，确定性数据归上游，这与 repowise 的「结构化页面与 LLM 叙述分离」以及 repowiki 的「静态分析与 LLM 分批生成分离」范式一致。

设计目标：

1. 下游不重新实现扫描、依赖图、PageRank——这些 CCE 已有或易聚合。
2. 下游在生成期的回查被约束为**有限步、只读、可预算**的工具调用，杜绝无界搜索。
3. 产出的数据自带源码行级定位，支撑 repodocs 式的「每句结论可溯源到代码行」。
4. 同一数据快照贯穿一次 Wiki 生成全过程，保证可复现（对标 repo-wiki 的固定 commit 快照）。

## 能力盘点：已有与缺口

| RepoWiki 需求 | CCE 现状 | 结论 |
|---|---|---|
| 文件扫描、语言识别、过滤 | `cce-scanner` 扫描管线 + 插件 `FileFilter`，manifest 缓存含内容 hash | 已有，需打包导出 |
| 实体/符号解析 | tree-sitter 解析 20+ 语言，符号表 + 稳定符号 ID | 已有 |
| 导入依赖图（文件级） | 关系索引含依赖域边（include/import 全家桶），`graph/module` 提供单文件 imports/exports/caller_files | 已有单点查询，缺批量聚合导出 |
| 调用/继承/实现关系 | `entity_callees` / `entity_callers` / `call_chain`、类继承与实现查询 | 已有 |
| PageRank 文件权重 | `cce-codegraph::graph_metrics` 已实现 PageRank、度/介数中心性、聚类系数，编排层有 `compute_metrics` | **计算已有、未对外暴露**，需并入导出 |
| 阅读路径（start here） | 无直接产物 | 可由 PageRank + 入口点候选推导，放导出包 |
| LLM 分批生成所需证据 | 搜索结果带 file_path + 行范围 + 实体名；`fold` 可出符号骨架；文件级 NL 摘要与 `nl_docs` 导出已有 | 已有，需按 Wiki 页面粒度组织 |
| 生成期代码回查 | MCP 14 个工具（search/aggregated_search/file_search/keyword_search/关系/项目/索引/fold/health） | 已有大半；**缺 read_file 与单文件大纲** |
| 增量更新 | manifest 内容 hash、`incremental_index`、watch 热更新、关系 delta 重建 | 已有，需映射到 Wiki 页面级失效 |
| 一致性快照 | 关系快照 stamp、搜索响应带 `relation_epoch` / `relation_stale` | 已有信号，需在导出契约中固化 |

## 总体架构：双通道供给

面向下游提供两条互补通道，一次 Wiki 生成 run 两者共用同一索引快照：

```
                    ┌────────────────────────────────────────┐
                    │  code-context-engine（确定性理解层）      │
                    │                                        │
  通道 A（批量）───▶ │  Wiki Context Package 导出器              │
  一次导出          │  文件清单 / 实体目录 / 文件级依赖图         │
                    │  重要度指标 / 模块聚类 / 摘要 / 治理边      │
                    └────────────────────────────────────────┘
                    ┌────────────────────────────────────────┐
  通道 B（交互）─── │  只读工具面（MCP / HTTP），带步数预算       │
  有限步回查        │  grep 类 / read file 类 / 图与关系类       │
                    └────────────────────────────────────────┘
                                        │
                                        ▼
                    ┌────────────────────────────────────────┐
                    │  RepoWiki 生成器（下游，LLM 叙述层）      │
                    │  页面规划 → 分批生成 → 校验 → 导出/发布    │
                    └────────────────────────────────────────┘
```

- **通道 A（Wiki Context Package，WCP）**：确定性、无 LLM、可缓存的结构化数据包。覆盖 RepoWiki 流水线中全部静态分析产物，使下游的 LLM 调用「有图可依、有权重可排」，多数页面不需要交互回查。
- **通道 B（受限工具面）**：下游生成过程中按需回查，严格限步。工具以 grep、read file、CCE 查询工具三类提供（见下文映射）。

## 通道 A：Wiki Context Package（WCP）

### 形态

导出为一个自包含目录（或单个归档），以 JSON/JSONL 为主、可选 SQLite 只读副本。CLI 子命令 `cce wiki context` 是唯一公开入口，HTTP 导出端点仅作为 CLI 的内部传输通道（CLI 是纯 HTTP 客户端，经该端点访问远端或本地 server）。全部条目带稳定 ID 与源码定位（文件路径 + 行范围），排序确定（按 ID 或路径字典序），保证同快照重复导出字节级可复现。

### 包内容

| 分片 | 内容 | 来源 |
|---|---|---|
| manifest | 项目 ID/根路径、索引快照 stamp、`relation_epoch`、语言统计、文件/实体计数、配置与治理节点列表 | 项目元数据 + 扫描缓存 |
| files | 每个文件：路径、语言、行数、内容 hash、入口点候选标记、文档分类（代码/配置/文档） | 扫描器 + 分类管线 |
| entities | 每个实体：稳定符号 ID、kind、限定名、签名、所属文件与行范围 | 符号表 |
| dep_graph | **文件级依赖图**：文件为节点，imports/exports/调用方文件为边（由 `get_module_relations` 语义批量聚合），含依赖域细分（include/import 命名空间/动态导入等） | 关系索引 |
| external_deps | 外部依赖符号展示，支持两种模式：**按符号**（默认）与**按包聚合**；由配置切换，待端到端测试与输出样例人工检查后再定最终默认 | 外部依赖解析（包管理器缓存符号） |
| call_edges | 实体级调用与结构边（调用域、依赖域、结构域；含 relation 类型、domain、confidence、weight、callsite） | 关系快照 |
| metrics | 每实体/每文件的 PageRank、度中心性、介数中心性（超规模自动省略）、聚类系数 | `graph_metrics`（需新暴露） |
| clusters | 模块聚类候选：连通分量/强连通分量结果、目录聚合的模块视图 | `graph/components`、`graph/module` 语义 |
| summaries | 文件级 NL 摘要文本（及可选的 `nl_docs` 导出物相对路径） | 摘要生成 + Export 模块 |
| reading_path | 推荐阅读路径：入口点候选为起点，按 PageRank 与依赖可达性排序的文件序列 | 由以上数据确定性推导 |

要点：

1. **文件级依赖图是 RepoWiki 的核心**（对标 repowiki 的 import 图与 PageRank）。WCP 直接给出文件粒度节点与边，下游不必从实体边自行聚合。
2. **metrics 分片补齐现有缺口**：`compute_metrics` 目前只在编排层内部使用，导出时按文件聚合（文件权重 = 实体权重聚合）后写入 WCP。
3. **reading_path 是确定性推导产物**：入口点识别复用扫描阶段的入口检测（main/包入口/README 提示），PageRank 排序后输出文件序列，下游只需 LLM 润色成「Start Here」指南。入口点启发式**不做额外纠偏**：入口仅作参考起点，下游 LLM 在生成时具备自主判断能力，以调用时的实际理解为准。
4. 超大仓库按分片分页（沿用 `graph/export` 的 limit/offset 语义），manifest 中记录分片数与校验和；下游可只加载 dep_graph + metrics + summaries 先做页面规划。

### 导出配置项

| 配置 | 取值 | 默认 | 说明 |
|---|---|---|---|
| 外部依赖符号展示模式 | `symbol`（按符号）/ `package`（按包聚合） | `symbol` | 影响 external_deps 分片粒度；后续以端到端测试与输出样例人工检查结果决定最终默认 |
| 是否携带代码原文片段 | 开 / 关 | 待实际验证后定 | 开启后 entity/detail 等分片附带少量签名体或原文片段，用以压缩下游 `read_file` 步数；包体量随之增大 |

### 一致性与版本

- WCP 由**单一索引快照**导出，manifest 记录快照 stamp 与 `relation_epoch`；下游一次生成 run 钉住一个包版本，不接受半新半旧数据。
- 导出内容不含任何 LLM 生成的叙述（摘要文本除外，其本身是索引期产物且可缓存），保证确定性。

## 通道 B：受限工具调用面

下游在生成期的回查限定为有限步工具调用。三类工具的对应关系如下：

| 下游约定的工具类 | CCE 对应 | 说明 |
|---|---|---|
| grep | `keyword_search`（BM25 原文片段，含行号）、`file_search`（路径模糊） | 精确标识符/字面量回查 |
| read file | **需新增 `read_file`**（按行窗口读源码）；结构化阅读用 **需新增 `file_outline`**，或退化到 `fold` | 目前 MCP 只有无状态 `fold`（入参是原始文本），缺直接读盘工具 |
| 当前项目提供的工具 | `search` / `aggregated_search`（语义）、`entity_callees` / `entity_callers` / `call_chain`（关系）、`graph/module`、`graph/ego` 等图查询（需从 HTTP-only 升级进 MCP）、`list_projects` / `get_project` / `health` | 语义定位与关系展开 |

### 新增工具（最小集）

1. `read_file`：入参为项目内相对路径与可选行窗口（offset/limit），返回带行号的源码；强制路径越界防护与单次返回体上限。
2. `file_outline`：单文件符号骨架（类/函数签名 + 行范围），优先静态生成；`fold` 作为未知语言的降级路径。
3. `entity_detail`：按稳定符号 ID 取实体元数据（签名、位置、摘要），用于把搜索命中升级为精确引用。
4. 图只读工具**加入 MCP**：`graph_module`（文件级 imports/exports/caller_files）与受限深度的 `graph_ego`；`graph_export` 不进交互面（体量归 WCP）。暴露与否由现有 `enabled_tools` 白名单自由控制，无需新机制。

### 工具面治理

- **只读 profile**：`read_only` 画像排除全部写操作（`index_project`、`incremental_index`），生成期无法触发索引变更。
- 所有工具显式带 `project_id`（与现有约定一致）；`read_file`/`file_outline` 路径限于项目根内。
- 每次调用返回体有上限（行数/条数截断并附 `truncated` 标记，沿用现有 truncated 语义）。

## 有限步预算协议

下游「有限步数」在契约层落实为预算模型，建议作为工具响应元数据与生成器共享：

1. **三级预算**：单次工具调用上限（返回条目数）、单页面上限（如每个模块页的工具步数）、单次 Wiki run 总步数上限。超出即降级，不再返回数据。
2. **阶段化调用规划**（与 RepoWiki 四阶段生成对齐）：
   - 项目总览：以 WCP manifest + reading_path 为主，工具步数极少（可为 0）；
   - 模块文档：以 WCP dep_graph/entities/summaries 为主，按页预算内用 `file_outline` + `read_file` 验证关键声明；
   - 架构图：**确定性 Mermaid 优先**——直接由 WCP dep_graph/clusters 生成图骨架，LLM 只做标注与分层美化，理想情况下零工具步；
   - 阅读指南：reading_path + PageRank，少量 `search` 补充主题入口。
3. **run 内缓存**：同一 run 内对相同（工具名、参数、快照 stamp）的结果缓存，重复回查不计步。
4. **降级策略**：预算耗尽或工具失败时，降级为「仅用 WCP 数据 + 标注置信度」（呼应 repowise 的每段置信度评分），不得无界重试。

## 与 RepoWiki 六阶段流水线的映射

| RepoWiki 阶段 | 供给方式 |
|---|---|
| Scan 文件扫描 | WCP files + manifest（语言、入口点、分类、hash） |
| 构建导入依赖图 | WCP dep_graph（文件级）+ call_edges（实体级） |
| PageRank 排序 | WCP metrics + reading_path |
| LLM 多轮生成 | 分批证据包：每页以 WCP 对应分片为上下文，工具回查按预算协议执行 |
| 增量缓存 | 文件 hash（manifest）→ 页面级失效；`relation_epoch` 变化 → 架构图/依赖页失效 |
| 输出/发布 | 归下游（Markdown/HTML/GitHub Wiki）；CCE 仅保证引用所需的行级定位数据 |

## 增量更新与缓存

- **页面级级联**（对标 repowise）：文件 hash 变化 → 失效该文件页与其所属模块页；依赖边变化（`relation_epoch` 变化）→ 追加失效架构图与受影响模块页。级联范围由 WCP dep_graph 反向边计算，预算内完成。
- **索引先行**：Wiki 再生成前确保增量索引完成（复用 `incremental_index` / watch），WCP 按新快照重导；不直接消费 git diff，保证与索引态一致。
- **生成结果缓存归属下游**：CCE 只缓存确定性数据（manifest、WCP、工具响应），LLM 文案缓存由 RepoWiki 生成器按文件 hash 键控。

## 溯源与校验支撑

- WCP 与工具响应**统一携带**文件路径、行范围、稳定符号 ID；搜索命中天然满足（`SearchResultItem` 已含 file_path/start_line/end_line/entity_ids/stable_symbol_ids）。
- 下游行文建议采用「结论 + 文件:行范围」引用格式；校验环节（repodocs/repowiki 的 verify）用 `read_file` 按行窗口反查即可闭环，CCE 无需实现文档校验器。
- 摘要与 `nl_docs` 可作为叙述草稿来源，但最终引用以源码行为准，避免把 LLM 摘要当证据。

## 安全与治理

- 通道 B 全只读、带路径防护；通道 A 经 HTTP 端点由 CLI 调用，导出过程不触发外部 LLM 调用，不引入外呼。
- 项目隔离：所有数据与工具按 `project_id` 作用域隔离，沿用现有项目注册表模型。
- 体量控制：WCP 分片 + 工具返回体上限 + 预算协议，防止超大仓库拖垮生成过程。

## 落地演进建议

1. **先导出、后工具**：先实现 WCP 导出（HTTP 端点 + CLI），把 files/dep_graph/metrics/reading_path 打通——这一步就能支撑「无 LLM 的结构化 Wiki 页面」，也覆盖 repowiki 的主要静态分析需求。
2. **补工具缺口**：新增 `read_file`、`file_outline`、`entity_detail`，并为 `read_only` 画像补图只读工具；同步开放预算元数据。
3. **增量联动**：打通 hash 变化 → 页面级失效通知（事件或导出 diff 清单），供下游级联再生成。
4. **可选增强**：WCP 内提供确定性 Mermaid 图骨架（文件级/模块级），进一步压缩下游工具步数。

## 决策记录

原四项开放问题的结论：

1. **MCP 图工具面**：补全 `graph_module`、`graph_ego` 进 MCP。工具暴露由现有 `enabled_tools` 白名单自由控制，不再另设机制。
2. **外部依赖符号展示**：提供两种模式——按符号（`symbol`）与按包聚合（`package`），**默认按符号**。后续补充端到端测试与输出样例，经人工检查后再决定哪个模式作为最终默认。
3. **reading_path 入口点启发式**：不做额外纠偏。入口点仅作阅读路径的参考起点，下游 LLM 调用时具备自主判断能力，以生成时的实际理解为准。
4. **WCP 携带代码原文片段**：做成导出配置项，可开可关；默认取值待实际验证后再定，验证前以关闭为保守实现。

## 实施状态

落地演进的各阶段已按本文实施：

| 阶段 | 交付 |
|---|---|
| WCP 导出 | `cce-server-shared::services::wiki` 组装 manifest/files/entities/dep_graph/external_deps/metrics/clusters/reading_path；HTTP `GET /api/project/{id}/wiki/context`（`cce-cli wiki context`）；配置项 `[wiki]`（`external_deps_mode`、`include_snippets`），请求参数可覆盖并支持 `include_mermaid` |
| 工具缺口 | MCP 新增 `read_file`（带行窗口、截断标记、路径防护）、`file_outline`、`entity_detail`、`graph_module`、`graph_ego`，全部受 `enabled_tools` 白名单控制，只读面共 19 个工具 |
| 增量联动 | `POST /api/project/{id}/wiki/diff`（`cce-cli wiki diff`）：基于文件 hash 的 added/changed/removed 分类，输出受影响文件（变更文件加一跳依赖邻居）与受影响模块页清单 |
| 可选增强 | WCP 内置确定性 Mermaid 骨架（文件级与模块级），由 dep_graph 生成、按边数截断 |


