# 存储后端剩余开放点细化设计与落地（第四轮）

## 0 文档信息

| 项 | 值 |
| --- | --- |
| 主题 | 第三轮 `storage-error-protocol-and-openpoints-decision.md` 第 11 节遗留四开放点 O-A'~O-D' 的细化设计与代码落地 |
| 仓库 | `kkkqkx123/code-context-engine`，递归拉取含全部子模块 |
| 环境 | Rust 1.93.0（workspace 声明 1.97，用 `--ignore-rust-version`）；ES/PG/Qdrant 外部服务未启动 |
| 约束 | 遵循 `AGENTS.md`：代码与配置用英文、文档用简体中文、禁 `unwrap`、少用 `dyn`、代码注释不得引用计划编号 |
| 与既有文档关系 | 承接第三轮 O-A'~O-D'；本轮落地 O-A'/O-C'/O-D'，O-B' 原保留独立立项；第五轮重新评估后决定放弃统一，见 `fts-unification-necessity-and-independent-optimization.md` |
| 文档约定 | 每条结论给出 `路径:行号`；diff 仅示意要点 |

## 1 背景与目标

第三轮把四类问题留作后续：远程 fulltext 真正延迟发布（O-A'）、统一两端实体 FTS 语义（O-B'）、补 generation/snapshot 状态机契约测试（O-C'）、调用方改用窄域 trait（O-D'）。本轮逐项给出可执行设计，并按"能否本地验证 + 改动风险"决定落地范围：

| 开放点 | 决策 | 判定依据 |
| --- | --- | --- |
| O-D' | **落地** | 生产调用面仅 3 处，宏的 blanket impl 保证调用方零改动，编译可验证 |
| O-C' | **落地（无 FK 依赖的状态机路径）** | generation/candidate 两端方法集对称，共享 suite 本地可跑 |
| O-A' | **落地（真延迟发布 + 失败补偿）** | 修复"远程 commit 后 bulk 不 refresh 导致不可见"的真实缺陷；缓冲状态机纯内存可单测 |
| O-B' | **第五轮推翻结论：统一不必要，不统一可行** | 原判"需 tsvector 迁移 + 独立立项"被第五轮核查推翻：无下游消费 rank，真正问题是上层 FTS5 语法承诺失配；改走"降级承诺 + 修两处语义 bug + pg_trgm 索引"，详见第五轮文档第 5、6、8、11 节 |

## 2 O-D' 调用方改用窄域 trait（已落地）

### 2.1 事实依据

- 宏 `metadata_storage_contract!` 为每个域生成窄 trait，并以 blanket impl `impl<T: MetadataStorage> $trait_name for T` 让所有后端自动实现（`cce-storage-common/src/metadb/contract.rs:26-49`）。
- 生产代码中把组合 `MetadataStorage` 作为泛型约束的仅 `cce-tools/src/store.rs` 三处（全仓检索 `impl MetadataStorage` 确认）。
- `get_chunk_records`/`resolve_chunk_records` 只用 `chunks_by_ids`（属 `MetadataContentStore` 域，`metadb.rs:319`）；`resolve_project_root` 只用 `project_record`（属 `MetadataProjectStore` 域，`metadb.rs:57`）。

### 2.2 改动

```diff
-use cce_storage_common::MetadataStorage;
-use cce_storage_common::metadb::{ChunkRecord, MetadataBackendStore};
+use cce_storage_common::metadb::{
+    ChunkRecord, MetadataBackendStore, MetadataContentStore, MetadataProjectStore,
+};
-pub async fn get_chunk_records(store: &impl MetadataStorage, ...) 
+pub async fn get_chunk_records(store: &impl MetadataContentStore, ...)
-async fn resolve_chunk_records(store: &impl MetadataStorage, ...)
+async fn resolve_chunk_records(store: &impl MetadataContentStore, ...)
-pub async fn resolve_project_root(store: &impl MetadataStorage, ...)
+pub async fn resolve_project_root(store: &impl MetadataProjectStore, ...)
```

### 2.3 安全性

调用方（`keyword_search.rs:178,180`、`entity_mapper.rs:48,70`、`common.rs:23`）传入的是 `ToolMetadataStore` 或 `MetadataStore` 具体类型，它们是 `MetadataStorage` 实现者，经 blanket impl 自动满足对应窄 trait，故调用侧零改动。收益：这三个函数现在只暴露其真实依赖，未来新增元数据方法不会再无谓扩大其编译期约束面。

## 3 O-C' generation/candidate 状态机契约测试（已落地）

### 3.1 事实依据

- `overrides_upsert_batch`/`overrides_clear`/`candidate_begin`/`candidate_clear_epoch` 两端均实现：SQLite `contract.rs:1705,1722,1840,1868`；PG `lib.rs:970,980,1024,1035`（委托 `project.rs:771,801`、`generation.rs:444,491`）。
- 这些路径不依赖真实 FK row id（entity 需要 file_id 才能插入，属 local-only），可在共享 suite 对两端对称运行。

### 3.2 改动（`tests/metadata_backend_contract.rs`）

在 overrides 段之后插入独立 epoch（12/13）的状态机往返：

```diff
+    store.overrides_upsert_batch(project, 12, &[
+        ("gen/a.rs".into(), OverrideDisposition::Replaced),
+        ("gen/b.rs".into(), OverrideDisposition::Deleted),
+    ]).await.expect("batch upsert overrides");
+    assert_eq!(store.overrides_for_generation(project, 12).await?.len(), 2);
+    assert_eq!(store.overrides_clear(project, 12).await?, 2);
+    assert!(store.overrides_for_generation(project, 12).await?.is_empty());
+    store.candidate_begin(project, 13, &format!("{tag}-candidate"), Some(3))
+        .await.expect("begin candidate");
+    let _ = store.candidate_clear_epoch(project, 13).await?;
```

用新 epoch 避免干扰既有断言（epoch 3/4 的 manifest 断言、epoch 7/8/9 的 local-only 断言）。candidate 继承 active 的 epoch-3 manifest，符合契约（`metadb.rs:283-299`）。

### 3.3 未覆盖项（保留）

`generation_copy_file`/`generation_materialize`/`generation_gc_plan`/`generation_gc_apply`/`candidate_delete_file_rows`/`candidate_prepare_channel` 需要真实 file/entity row id 或复杂前置生成链；共享 suite 的契约方法不暴露 row id，构造成本高，列为后续按域分批补。

## 4 O-A' 远程 fulltext 真正延迟发布（已落地）

### 4.1 缺陷事实（本轮新发现，构成落地理由）

- 远程 `commit_staged` 原先仅 drain 计数、不写 ES、不 refresh（`cce-storage-bm25/src/contract.rs:156-159` 旧实现）；stage 阶段已 `batch_index` 写入但 bulk 默认不 force refresh（`elasticsearch/write.rs:104-107` 明确要求最后调 `flush`）。
- 编排层 `commit_staged_bm25` 调 `commit_staged` 后直接写派生元数据、推进 work unit（`-orchestrator/.../storage_coordinator/bm25.rs:170-196`），全程无 `flush()` 调用（全仓检索确认）。
- 结论：远程在 commit 后、下一次 ES 自动 refresh（默认 1s）前，新文档对 search/count 不可见，与本地 `commit_staged` 内部 `reload_reader` 的即时可见语义不符（`cce-storage-bm25/src/client.rs:315-336`）。这是跨后端语义缺口，不只是抽象不对称。

### 4.2 设计：内存缓冲 + 提交即写 + 写后 refresh + 失败补偿

- 新增 `StagingBuffer`（`elasticsearch.rs:76-114`）：按 index_name 分组的 `HashMap<String, Vec<FulltextDocument>>`，提供 `stage`/`take`/`restore`/`pending`。刻意与 HTTP 传输解耦，使状态机可脱离 ES 单测。
- `ElasticsearchClient.staged_documents` 由 `Arc<AtomicUsize>` 改为 `Arc<Mutex<StagingBuffer>>`（`elasticsearch.rs:124-128`）。
- `stage_documents` 只调 `buffer_stage` 把文档放入内存，返回计数，不触网（`contract.rs:146-153`、`elasticsearch.rs:184-196`）。
- `commit_staged`：`buffer_take` 取整批 → `batch_index` 写入 → 成功则 `flush()`（refresh，保证即时可见）→ 失败则 `buffer_restore` 把整批放回队首（`contract.rs:155-177`）。`batch_index` 自带部分前缀回滚（`write.rs:36-48`），故 restore 整批是幂等无损的。
- `finish_writes` 远程保持 no-op（无本地 merge 可等待）。

### 4.3 契约与测试

- 契约注释改为"两端均延迟发布，stage 必须与 commit 配对，默认体为第三方实现保留立即发布"（`fulltext.rs:159-166`）。
- 共享 suite 新增"commit 前不可见 + commit 后可检索"断言（`tests/fulltext_backend_contract.rs:395-440`）。
- 三个 StagingBuffer 纯内存单测（`elasticsearch.rs` tests）：延迟到 take、失败批次放回队首、index 间隔离。

### 4.4 残余风险

- 远程 end-to-end 行为（batch_index + refresh 的真实可见性、失败回滚与 ES 交互）仍需 `CCE_TEST_ES_URL` 下的契约测试验证；本环境无 ES，远程分支按设计跳过，仅缓冲状态机被本地单测覆盖。
- 提交阶段的 `flush` 是一次额外 refresh；若未来写入批次极密，可评估合并多次 commit 的 refresh（当前编排层按 generation 提交，频率可控）。

## 5 O-B' 统一两端实体检索语义（细化设计，未落地；第五轮已否决该方向）

### 5.1 差异事实

| 分支 | 实现 | 检索方式 | 排序 |
| --- | --- | --- | --- |
| 本地 | `entity_repo.rs:259-276` | SQLite FTS5 `entities_fts MATCH` | `ORDER BY f.rank`（BM25 相关性） |
| 远程 | `metadb-pg/src/files.rs:466-519` | 每 token 一个 `ILIKE '%token%'` 的 AND 合取 | `ORDER BY e.id`（插入序） |

更正事实：`sanitize_fts_query` 仅存在于 PG 端（`files.rs:1712-1730`，调用点 `files.rs:474`），本地 `entity_repo.rs:251,271` 将 query 原样透传给 FTS5 MATCH，两端并不共用清洗逻辑。PG 未建全文索引，`entities` 表仅有 btree 索引（`schema.rs:132-136`）。同一查询在两端返回集合不同（FTS5 词法匹配 vs 子串匹配）、顺序不同（相关性 vs id）。第五轮进一步确认无下游消费 rank，故第 5.2 节的统一方案已否决，改见 `fts-unification-necessity-and-independent-optimization.md`。

### 5.2 细化设计（供拍板）

1. **PG schema 迁移**：在 `schema.rs` 的建表/迁移序列中为 `entities` 增加生成列与 GIN 索引：
   - `ALTER TABLE entities ADD COLUMN search_tsv tsvector GENERATED ALWAYS AS (to_tsvector('simple', coalesce(name,'') || ' ' || coalesce(signature,'') || ' ' || coalesce(scoped_name,''))) STORED;`
   - `CREATE INDEX idx_entities_search_tsv ON entities USING GIN (search_tsv);`
   - 需处理存量数据回填与迁移版本（`ensure_migrated` 幂等性）。
2. **检索改写**（`files.rs:466-519`）：用 `websearch_to_tsquery('simple', $q)` 或把 tokens 以 `&` 连接成 `tsquery`，`WHERE search_tsv @@ $q`，排序改 `ORDER BY ts_rank(search_tsv, $q) DESC, e.id`。
3. **locale 选择**：`'simple'` 配置不含中文分词；若语料含中文需评估 `zhcfg` 或 r，`simple` 仅保证与 FTS5 unicode61 大体接近，不保证逐 token 等价。
4. **契约对齐点**：把 `metadb.rs:484-497` 的"远程为 ILIKE 子串"注释改为"两端为全文词法检索"，并明确排序从 id 变为相关性。
5. **验证门槛**：需在真实 PG 上跑存量/新增数据、中文/标识符/短语用例，并接受排序变化对上层召回的影响；本环境无 PG 服务，无法验证。

### 5.3 为何本轮不改代码

- 涉及 schema 迁移（有状态、需幂等与回滚方案）与查询排序语义变更（影响召回质量），风险与影响面远超本轮其余三项。
- 需要 DBA（迁移与索引成本）与算法（相关性排序与中文分词）双方拍板，属独立立项；继续沿用既有 O-04 决策，不强行统一。

## 6 修复后的一致性复核

| 抽象 | 本轮改动 | 对称性变化 |
| --- | --- | --- |
| FulltextStorage | 远程 stage 由"立即写"改为"缓冲"；commit 由"报数"改为"写+refresh+失败补偿" | 两端 staging 语义由第三轮的"仅计数对齐"升级为"可见性语义对齐" |
| MetadataStorage | 调用方 3 处改依赖窄 trait；共享 suite 补 generation/candidate 状态机 | 不改变 trait 方法集；契约覆盖提升 |
| VectorStorage | 无 | 不变 |

## 7 验证计划与结果

| 项 | 命令 | 结果 |
| --- | --- | --- |
| 针对性编译 | `cargo check --ignore-rust-version -p cce-tools -p cce-storage-bm25 -p cce-storage-common -p cce-orchestrator -p cce-types` | 通过（`Finished dev profile`，无 error/warning） |
| bm25 单测 | `cargo test --ignore-rust-version -p cce-storage-bm25 --tests` | 25 passed（含 3 个 StagingBuffer 状态机测试），0 failed |
| 三份契约测试 | `cargo test --ignore-rust-version -p cce-storage-common --tests` | lib 17 + fulltext 2 + metadata 3 + vector 2，全部 passed |
| 工具层回归 | `cargo test --ignore-rust-version -p cce-tools --tests` | 64 passed，0 failed（验证窄域 trait 改动无回归） |
| 环境说明 | — | rustc 1.93.0 低于 workspace 声明的 1.97，用 `--ignore-rust-version`；依赖经 rsproxy 镜像下载 |

新增的 3 个 StagingBuffer 单测与 metadata 状态机断言、fulltext"commit 前不可见"断言均已实际运行通过。远程/远程 PG 与 ES 分支因未设 `CCE_TEST_ES_URL`/`CCE_TEST_PG_URL` 按设计跳过，外部服务语义仍需 CI 验证。

## 8 风险与残余

- O-A'：`StagingBuffer` 只在单进程内存，客户端重建（如故障重启）会丢失未 commit 的缓冲批次——与本地 writer 未 commit 数据在重启后丢失的语义一致，可接受。远程 commit 的 refresh 成本与失败补偿的真实行为需 ES 环境验证。
- O-C'：generation  deeper 状态机（copy/materialize/gc）与 snapshot 激活链仍未纳入共享 suite，跨后端等价主要靠本地分支与代码审阅。
- O-D'：窄 trait 依赖收窄后，若某调用方未来需要更多方法，需显式扩展其约束——这是期望的编译期提醒，不是缺陷。
- O-B'：第五轮决策变更——放弃统一，差异保留为既定设计；但需先降级上层 FTS5 语法承诺并修远程 `OR`/字段限定两处 bug，详见 `fts-unification-necessity-and-independent-optimization.md` 第 6、11 节。

## 9 落地状态汇总

| 编号 | 状态 | 落地文件 |
| --- | --- | --- |
| O-D' | 已落地 | `crates/app/cce-tools/src/store.rs` |
| O-C' | 已落地（部分） | `crates/infra/cce-storage-common/tests/metadata_backend_contract.rs` |
| O-A' | 已落地 + 单测 | `crates/infra/cce-storage-bm25/src/elasticsearch.rs`、`src/contract.rs`、`crates/infra/cce-storage-common/src/fulltext.rs`、`tests/fulltext_backend_contract.rs` |
| O-B' | 第五轮重新决策：放弃统一（原"保留独立立项"作废） | 无代码改动；第五轮仅出分析与决策文档 |

## 10 测试覆盖新增

- fulltext 契约：stage→(commit 前不可见)→commit→(commit 后可检索) 完整延迟发布往返。
- bm25：`staging_buffer_defers_until_take`、`staging_buffer_restore_puts_failed_batch_at_head`、`staging_buffer_isolates_indexes`。
- metadata：`overrides_upsert_batch` + `overrides_clear` 往返、`candidate_begin` + `candidate_clear_epoch` 状态机。

## 11 后续开放点

| 编号 | 事项 | 建议 |
| --- | --- | --- |
| O-B' | ~~统一两端实体 FTS 语义~~（第五轮否决该方向） | 不统一可行；改为 F-01 降级上层语法承诺 + F-02 修 `OR`/字段限定 bug + F-05 建 pg_trgm GIN 索引，详见第五轮文档第 6、8、11 节 |
| O-C'' | 补 generation copy/materialize/gc 与 snapshot 激活链契约测试 | 需暴露或绕过 FK row id，按域分批补；远程纳入独立 CI job |
| O-A'' | 远程 commit 的 refresh 成本优化 | 若写入批次极密，评估多次 commit 合并一次 refresh；当前频率可控，暂不动 |
| O-E' | 远程三分支契约测试改为独立 CI job | 配置 `CCE_TEST_ES_URL`/`CCE_TEST_PG_URL`/`CCE_TEST_QDRANT_URL`，把外部服务语义纳入常态验证 |
