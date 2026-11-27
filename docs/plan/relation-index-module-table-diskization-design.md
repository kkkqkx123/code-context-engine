# 关系构建期模块表磁盘化与有界符号视图设计

## 背景

当前全量构建已经把关系实体、关系边和文件级局部符号表放进操作本地临时 SQLite。medium 档关系阶段增量降到 324,500 KiB，达到 500 MiB 上界；large 档峰值从 9,103,100 KiB 降到 4,631,348 KiB，下降 49.1%，但关系阶段增量仍为 3,244,720 KiB。剩余常驻结构是 `PackageSymbolTable` 中的模块导出/导入/重导出表，以及项目级简单名称、类型与重载索引。

同时，large 档暴露出明显的非线性耗时。最新 stage TSV 中 `relation_ms` 为 33,010,512 ms，medium 为 250,290 ms；文件数只增加 10 倍，关系阶段却增加约 132 倍。这个回归不能继续用“large 档较慢”解释，必须在模块表磁盘化时一并处理。

本文档只描述剩余任务的架构与验收口径；medium 档是主要验证环境，large 档仅做最终冒烟。前置指标与性能定位见 `docs/issue/relation-index-module-table-metrics-medium.md`。

## 性能问题定位

合成夹具跨文件重复使用 `fn_00000`、`fn_00001` 等相同函数名。每个文件约十几个实体，50,000 个文件会产生大量同名符号。当前两处结构因此成为二次复杂度的来源：

- `CrossFilePropagator` 的 `name_index`、`name_to_ids`、`param_name_index` 把同名实体全部放进一个向量。跨文件传播调用 `resolve_single_call_binding` 时，会按函数名取候选并逐个比较 arity、参数形状与返回类型；热门名称的候选数随文件数线性增长。
- `ProjectSymbolTable.simple_name_index` 同样按名称聚合候选，解析时排序并逐个检查。模块表磁盘化后如果仍保留无界候选向量，名称解析仍会随热门名称的重复次数线性放大。

新增的细分指标又暴露出一个独立的线性但高系数问题：`relation_publish_ms` 占 medium relation 阶段的 60%–75%，其中 `relation_publish_store_ms` 约 172,540–188,799 ms，而 `relation_state_mark_ms` 只有约 680 ms。`ServerRelationSnapshotPublisher::publish` 在激活分支构造了一次 `RelationSnapshotIndex::from_store`，随后又无条件构造第二次；同一 epoch 的快照被完整读取并装载两遍。该重复装载已改为复用第一次构造的 `Arc`，无需改变发布语义。

artifact 写出、SQLite 行写入和局部表反序列化都是按实体或按文件的线性开销，不能解释 medium 到 large 的百倍放大。阶段数据也支持这一判断：`symbol_propagation_ms` 从 36,703 ms 增至 7,274,302 ms，增幅远大于 `relation_symbols_ms` 的 13 倍。

结论：存在两个真实性能问题：发布路径重复物化整份快照，以及同名候选的无界二次扫描。前者已修复；后者必须在模块表磁盘化时一并解决，不能靠磁盘 I/O 优化掩盖。

## 总体架构

### 定义 `BuildSymbolView`

在全量构建路径引入有界查询契约，替代直接传入 `&ProjectSymbolTable`：

- 按文件路径读取模块记录：语言、包、模块路径、命名空间、导出、导入、重导出、通配符导入、类型成员贡献。
- 按名称读取导出候选、导入绑定、重导出绑定；候选必须带模块路径与稳定排序键。
- 查询局部作用域、类型成员、跨文件返回类型、参数类型与重载集合。
- 返回 `Arc` 或借用副本，工作集由 LRU 控制；视图不暴露完整模块表容器。

增量构建、单元测试和已发布查询继续使用内存实现，二者共享同一查询契约，不保留第二套解析算法。

### 临时库结构

在现有 `relation-build-store.sqlite3` 中增加以下表，均由同一操作持有，结束时随 spool 删除：

- `modules`：文件路径、包 ID、模块路径、命名空间前缀、序列化模块记录。
- `module_exports`：名称、文件路径、可见性、稳定符号键；按名称建索引。
- `module_imports` / `module_reexports`：本地名、源文件路径、目标模块或符号、链深度。
- `module_types`：类型限定名、简单名、成员名、所属文件、实体 ID；按名称和 arity 建索引。
- `name_candidates`：名称、arity、候选实体 ID、稳定排序键、来源；按 `(name, arity)` 建索引并限制单桶大小。

文件级局部符号表继续使用 `local_symbols` 表，不合并回模块记录，避免单条记录过大。

### 有界工作集

视图内部维护两个独立 LRU：

- 模块工作集：默认 128 个模块，约 96 MiB 软上限。
- 名称候选工作集：默认 4,096 个 `(name, arity)` 桶，约 64 MiB 软上限。

超限时不静默截断语义：候选桶超过上限后按稳定排序键保留前 N 个，并把被裁剪的候选数计入指标；若裁剪数非零，构建 manifest 失败并要求显式调参。这个失败语义比生成不完整关系图更安全。

### 写入流程

1. `SymbolBuildSession::add_file` 生成模块记录和局部表记录，写入临时库；不再向常驻 `PackageSymbolTable.add_module`。
2. 项目级全局索引改为流式聚合：只保留 `file_to_package`、`module_path -> file`、命名空间索引和聚合后的简单名称候选键。
3. 跨文件传播阶段按文件安装当前局部表；类型传播只查询视图中的按名称、按 arity 索引，不再扫描无界候选向量。
4. 关系解析阶段由 `BuildSymbolView` 提供模块导出、导入、重导出、类型成员和跨文件类型；调用者不接触具体 `ProjectSymbolTable`。
5. artifact 写出路径不变，仍只读取关系构建库中的实体/关系行。

### 性能控制

- 同名候选按 `(name, arity)` 分桶，跨文件传播只读取同 arity 候选，避免对全部同名实体逐个判断。
- 候选向量排序键稳定，裁剪结果可复现；裁剪数非零直接失败，不采用“取前几个继续跑”的静默降级。
- `CrossFilePropagator` 的 parquet/param 查询改为视图接口，保留元数据和 arity，去掉重复扫描同名全集的回退路径。
- medium 档必须先满足耗时上界，再考虑 large 冒烟；不再用 large 长时间运行作为主要反馈循环。

## 一致性约束

- 稳定符号键、规范行顺序、fingerprint 输入和关系计数必须与当前实现一致。
- 模块记录必须保留可见性、导入别名、重导出链深度、通配符来源和类型成员贡献；任何被裁剪的候选都必须导致构建失败。
- 增量路径继续走内存 `ProjectSymbolTable`，不因为全量视图改造改变热更新结果。
- 临时库写入失败、视图工作集超限、候选裁剪非零、文件计数不一致均使 operation manifest 失败且不激活 generation。

## 指标采集

已在 stage TSV 中增加以下独立计时列，用于定位阶段回归：

- `relation_symbol_register_ms`
- `symbol_propagation_ms`
- `relation_register_ms`
- `relation_resolve_ms`
- `relation_artifact_ms`
- `relation_publish_ms`
- `relation_publish_store_ms`
- `relation_state_mark_ms`
- `finalize_index_ms`

medium 档每轮采集：

1. `full_pipeline_perf`：峰值 RSS、关系阶段增量、上述阶段耗时、实体/关系/向量计数。
2. `relation_query_matrix`：冷/热 p95、错误数、分片目录条目和目录字节。
3. fingerprint 一致性：磁盘构建 artifact 与参考 canonical artifact 的双指纹和四类行数。
4. 新增视图指标：模块工作集峰值条目/字节、名称候选工作集峰值条目/字节、候选裁剪数。这些值写入 operation manifest，超限直接失败。

## 验收口径（medium 为主）

- 峰值 RSS 相对 1,173,636 KiB 下降 50% 以上。
- `rss_after_relation - rss_after_batches` 低于 500 MiB。
- `relation_ms` 不高于当前 medium 250,290 ms 的 2 倍，即 500,580 ms。
- `symbol_propagation_ms` 不高于当前 medium 73,406 ms 的 2 倍，即 146,812 ms。新增字段首次采集后以首次稳定值更新上限。
- 冷查询 p95 不高于 146/168 ms 的 2 倍，热查询低于 1 ms，错误数为 0。
- fingerprint、规范行内容、关系计数、查询结果与 delta 合并结果一致。

large 档只作为最终冒烟：验证峰值不再回到数 GiB 以上、关系计数与 medium 比例一致、无候选裁剪失败。由于单次 large 运行耗时过长，不再用它作为迭代反馈。

## 风险与控制

- 视图查询次数增加：按文件分组预载当前模块，索引覆盖所有热点查询列；查询次数进入 manifest。
- 工作集抖动：模块与名称候选使用独立 LRU，避免热门名称挤掉当前文件模块。
- 语义偏差：保留内存路径做单测对照；同一 fixture 同时跑内存与磁盘视图，比较解析结果和 artifact。
- 磁盘占用：模块记录只存符号与路由信息，不存源码和 NL 文本；临时库随操作删除。
