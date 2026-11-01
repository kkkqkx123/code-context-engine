# BM25（Tantivy）路径过滤能力现状

> 描述 `crates/infra/cce-storage-bm25/` 的当前实现。本文档的前身是一份"能力缺口分析与扩展方案"，其缺口清单（schema 索引 file_path、检索接口携带过滤条件、召回侧 epoch/排除过滤）如今已基本落地；原提议的 PathFilter→BooleanQuery 转换方案未按原样实现，由上层策略内的 GlobFilter 与 `FulltextSearchOptions` 过滤承担。本文按现状重写。

---

## 1. Canonical Schema

**文件**：`crates/infra/cce-storage-bm25/src/schema.rs`

`IndexSchema` 为 **15 字段 canonical schema**：

| 字段 | 类型 | 用途 |
|---|---|---|
| `document_id` | STRING\|STORED | 文档唯一 ID |
| `title` | TEXT(mixed)\|STORED | 标题，加权 2.0 |
| `content` | TEXT(mixed)，仅索引不存储 | 正文，加权 1.0 |
| `keywords` | TEXT(mixed)\|STORED | 关键词，加权 2.0 |
| `chunk_id` | STRING\|STORED | 关联 SQLite chunk 行 |
| `file_path` | **STRING\|STORED** | 规范化项目相对路径，整值词条进入倒排索引，结果自带路径 |
| `project_id` | STRING\|STORED | 原生多租户隔离 |
| `epoch` | i64，STORED+INDEXED | 数据 epoch（世代视图过滤） |
| `entity_id` | TEXT，STORED | 逗号分隔实体 ID CSV |
| `segment_id` | TEXT，STORED | 文档/纯文本块的对齐键（hybrid 融合） |
| `test` / `generated` / `dependency` | u64，STORED+INDEXED+FAST | 内容类型标记（下推排除） |
| `category` | u64，STORED+INDEXED+FAST | 文件类别 |

文本字段统一使用 `mixed` tokenizer（`cce-text` 的 `MixedTokenizer`，支持 tantivy feature）。`file_path` 为 STRING（不分词）使精确路径 TermQuery 成为可能；结果不再依赖 SQLite 回填路径。

---

## 2. 检索接口

**文件**：`crates/infra/cce-storage-bm25/src/retrieval.rs`、`client.rs`

统一读路径为 `Bm25Retrieval::search(manager, schema, query_text, options: &FulltextSearchOptions)`。`FulltextSearchOptions`（定义于 `crates/infra/cce-storage-common/src/fulltext.rs`）携带全部过滤与排序参数：`limit/offset`、`field_weights`、`project_id`、`epochs`、`excluded_files`、`exclude_test/generated/dependency`、`include_categories/exclude_categories`、`term_operator`。`Bm25Client::search()`（`client.rs` 约 375 行）持锁读索引后直接委托给 `Bm25Retrieval`，查询构建由检索层持有，保证远程分支可复现同一过滤语义。

### 检索层过滤组合（Tantivy BooleanQuery）

`retrieval.rs` 中过滤以原生 Tantivy 查询组合进最终查询：

- **epoch 视图过滤**（约 66–92 行）：对 `options.epochs` 中每个 epoch 构造 `TermQuery`，组合为 `BooleanQuery`（多 epoch 时 `minimum_number_should_match = 1`），以 `Must` 与全文查询求交。
- **被覆盖文件排除**（约 95–128 行）：继承视图（多 epoch）且 `excluded_files` 非空时，构造「父 epoch 命中 ∧ 文件路径 ∈ excluded_files」的子查询并以 `MustNot` 排除，父代被覆盖行对查询不可见。
- **内容类型 / 类别排除**：基于索引的 `test/generated/dependency/category` fast 字段下推。
- **项目隔离**：`project_id` 精确 TermQuery。

---

## 3. 与原扩展方案的对照

原文档提议的 `PathFilter` → Tantivy `BooleanQuery` 转换（directory_prefix RangeQuery + glob→RegexQuery 组合）**未按原样实现**。现状分工：

| 原方案条目 | 现状 |
|---|---|
| Schema 增加 `file_path: STRING\|STORED`（P0） | ✅ 已实现（schema.rs 约 80 行） |
| `search()` 接受过滤参数（P1） | ✅ 以 `FulltextSearchOptions` 形式实现，参数更完整（epoch/类别/内容类型） |
| `build_path_filter_query()`（P1） | ❌ 未实现；用户 Glob 过滤由**上层策略**（`crates/app/cce-orchestrator/src/query/retrieval/strategies/bm25.rs` 的 `apply_bm25_strategy_filters`）在召回侧执行，配合超取回填 |
| `directory_prefix` 索引内过滤（P2） | 以查询时前缀检查替代（BM25 索引无前缀字段，`directory_prefix_allows`），语义与向量侧斜杠边界前缀一致 |
| 重建索引使 file_path 生效（P2） | 已随 canonical schema 迁移完成 |

即：**引擎内过滤承担结构性、可索引的条件（epoch、项目、内容类型、类别）；模式类条件（Glob、目录前缀）由编排层策略过滤承担**。原 P1/P2 路线图被现有机制取代，不再维护。

---

## 4. 后端抽象与查询构建特性

### FulltextStorage 契约与 Elasticsearch 分支

**文件**：`crates/infra/cce-storage-bm25/src/contract.rs`

`FulltextStorage`（`cce-storage-common::fulltext`）是后端中立契约：`Bm25Client`（`backend_name = "local"`）与 `ElasticsearchClient`（`"remote"`）都实现它，编排层（`Bm25Strategy`）只面对契约，本地/远程共享一个读路径。行为差异（refresh 时机、短语近似）记录在 `ElasticsearchClient` 上。

### 分词与查询项展开

- **MixedTokenizer**（`crates/core/cce-text/src/tokenizer.rs`）：代码感知分词（符号拆分、camelCase 分解、多语言），`Bm25TextCleaner` 只负责 tokenizer 无法处理的文本噪声。
- **whole/split token 展开**（`retrieval.rs` 的 `expand_query_tokens`，约 577 行）：按位置分组每个查询 token 的 whole（完整）与 split（拆分）变体，以文档频率决定权重——whole 存在时拆分项降权，仅拆分项存在时提升，均不存在时以半权重发出；停用词与零文档频率项被剔除。
- **字段加权与 BoostQuery**：title/content/keywords 项以 `BoostQuery` 按配置权重提升；约束型子句（如 epoch/项目过滤需保持匹配但不贡献分数）用 `BoostQuery` 零 boost 实现，避免污染 BM25 分数（`no_score`，retrieval.rs 约 35 行）。
- **短语查询**：引号段构造短语查询，应用与普通词相同的停用词与零文档频率展开；多词算子由 `term_operator`（`or` 默认 / `and`）控制，引号短语始终优先。

---

*关联文档：[path-filtering-pipeline.md](path-filtering-pipeline.md)、[hybrid-fusion-design.md](hybrid-fusion-design.md)*
