# 服务后端补充指标采集与问题确认（2026-11-20）

## 采集范围

在 devs 上用全局安装后的 Qdrant 1.15.5、Elasticsearch 7.17.28、PostgreSQL 10.23
完成剩余采集。构建使用 `bench` profile，服务版本标签为 `followup-0c1e6fa4`，
确认轮标签为 `followup-rate0`。外部服务进程指标由管理脚本的 `sample` 子命令每 5 秒
采样一次，与基准进程指标分开记录。

| 运行 | run_id | 文件档 | 说明 |
|---|---|---|---|
| 基线小档 | `af076ec7773f` | 500 | 四档后端完整矩阵 + 服务检索矩阵 |
| 基线中档 | `af18a1489f41` | 5,000 | 四档后端完整矩阵，用于阶段归因 |
| 限流修复后 | `b0bdcdc76dc3` | 500 | 关闭 mock provider 的令牌桶后复测 |

原始 TSV 与日志：`.bench_logs/followup-*.tsv`、`.bench_logs/rate0-*.tsv`、
`.bench_logs/bench-followup-{small,medium,rate0}.log`；采集摘要为
`.bench_logs/followup-collect-summary.log`。

## 索引阶段结果

5,000 文件档的墙钟耗时仍为 130–143 秒，但新增阶段 TSV 已经可以完整解释这部分时间，
不再需要猜测后端差异：

| 组合 | 墙钟 ms | entity_store | vector_store | batch_process | bm25_store | relation | batch_overhead |
|---|---:|---:|---:|---:|---:|---:|---:|
| local/local | 130,541 | 68,206 | 28,954 | 8,165 | 6,679 | 5,167 | 6,163 |
| qdrant/local | 140,004 | 72,758 | 30,616 | 8,399 | 6,883 | 5,153 | 6,960 |
| local/es | 142,875 | 78,306 | 30,831 | 8,439 | 5,458 | 5,507 | 7,017 |
| qdrant/es | 132,964 | 69,369 | 31,069 | 7,007 | 5,368 | 5,269 | 6,150 |

500 文件档对应阶段总量约 10–13 秒，与墙钟基本一致；四档后端之间的墙钟差异仍在
±10% 左右，远端向量/全文不是服务索引慢的主因。

## 检索结果：混合查询的 100 ms 悬崖已定位并消除

基线轮中 hybrid 冷查询呈现明显的双峰：部分请求约 2–20 ms，另一部分恰好落在
约 98–100 ms；c=4 时四档组合的 p50 集中在 98–99 ms。阶段数据表明耗时不在
Qdrant、ES 或 fusion：

| 组合（hybrid c=1 cold） | embedding ms | provider ms | overhead ms | bm25 ms | fusion ms |
|---|---:|---:|---:|---:|---:|
| local/local | 38.67 | 0.33 | 38.33 | 0.00 | 0.00 |
| qdrant/local | 12.67 | 0.67 | 12.00 | 0.00 | 0.00 |
| local/es | 1.00 | 0.33 | 0.00 | 13.33 | 0.00 |
| qdrant/es | 8.00 | 0.67 | 7.33 | 9.33 | 0.00 |

耗时几乎全部落在 `embedding_overhead_ms`，即嵌入 resilience 栈，而不是传输提供者。
根因是离线 mock provider 继承了默认 `rate_limit = 600`（10 请求/秒、burst 10）。
索引阶段耗尽令牌后，hybrid 的向量召回在固定预算上等待令牌补充，等待时间上限正好是
100 ms；vector 单独模式因为在每轮组合中最先执行、仍可消耗剩余令牌，所以没有同样表现。

修复方式是把离线测试 provider 的速率限制设为 0，使 mock 不引入人为限流；生产默认
限流保留。复测结果（`followup-rate0`）：

| 组合 | 模式 | 修复前 p50 / p95 (ms) | 修复后 p50 / p95 (ms) |
|---|---|---:|---:|
| local/local | hybrid c=1 | 20 / 99 | 3 / 3 |
| local/local | hybrid c=4 | 98 / 99 | 4 / 5 |
| qdrant/local | hybrid c=4 | 99 / 99 | 4 / 5 |
| local/es | hybrid c=4 | 98 / 99 | 9 / 10 |
| qdrant/es | hybrid c=4 | 99 / 99 | 9 / 9 |

修复后 hybrid c=1 的 embedding overhead 降为 0，provider 约 0.7–1.0 ms；
四档组合的 hybrid 冷请求稳态为 3–9 ms。该问题属于测试夹具配置缺陷，不是存储后端缺陷。

`embedding_first_ms` 在所有阶段行均为 0：启动预热已经把首次连接成本移出请求路径，
说明该字段按设计工作，后续不应再用它解释查询尾延迟。

## Elasticsearch 与本地全文的差距仍然存在

500 文件档 bm25 冷请求（修复限流后的同轮对比）：

| 组合 | c=1 p50 / p95 ms | c=4 p50 / p95 ms |
|---|---:|---:|
| local/local | 1 / 2 | 2 / 3 |
| qdrant/local | 1 / 2 | 2 / 2 |
| local/es | 10 / 12 | 9 / 9 |
| qdrant/es | 8 / 12 | 8 / 8 |

去掉限流干扰后，ES 分支仍比本地 Tantivy 慢约 5–10 倍，且差距集中在 `bm25_retrieval_ms`，
即远端往返与刷新语义。该差距不是代码缺陷，应按独立 SLA 与 JVM 堆配置管理，
不能把 ES 当作 Tantivy 的等价替换。

## 外部服务进程资源

采样窗口 2026-11-20T06:58:44Z 至 07:16:44Z，共 211 个有效样本/服务：

| 服务 | CPU 增量 | RSS 平均 | RSS 峰值 | VmHWM 峰值 |
|---|---:|---:|---:|---:|
| Qdrant | 6.84 s | 185 MB | 244 MB | 244 MB |
| Elasticsearch | 16.31 s | 1,600 MB | 1,653 MB | 1,653 MB |
| PostgreSQL | 0.18 s | 25 MB | 25 MB | 27 MB |

ES 的固定 1 GiB JVM heap 之外还有约 0.5–0.6 GiB 常驻开销；这些数值不进入基准进程 RSS，
容量规划必须单独计入。Qdrant 与 PostgreSQL 在检索阶段基本空闲，CPU 增量主要来自索引写入。

## 仍然存在的问题

### 1. 服务索引与流水线基准不可直接对比，但差额是真实的文件落盘成本

此前“服务索引 5,000 文件 125–134 s，而完整流水线 26.8 s”的差额，现已被阶段 TSV 定位：
`entity_store_ms` 为 68–78 s，而流水线基准同阶段约 5 s；`vector_store_ms` 为 29–31 s，
流水线约 7 s。两侧夹具相同但元数据后端不同：流水线使用 `in_memory_metadata_store()`，
服务路径使用文件型 SQLite（WAL + `synchronous=NORMAL`）。

代码层面，`StorageCoordinator::store_parsed_files` 对每个文件依次执行
`files_ensure`、`file_by_path`、`entities_delete_by_file`、`entities_insert_returning_ids`
以及逐 parent 的 `entities_set_parent`，每次调用各自提交事务。5,000 文件、约 6.7 万实体
因此产生数万次事务，文件型 SQLite 的提交开销无法被流水线的内存库忽略。

这是当前最明确的待优化项：需要把单文件多事务改为每批合并事务，并在同 durability 口径下
重新对比，而不是继续把差额归给 Qdrant/ES。

### 2. 生产限流与查询延迟 SLA 需要显式对齐

默认 provider 限流 600/min 在令牌耗尽时会给单次嵌入引入最长 100 ms 等待。对交互查询，
这要求把限流预算、burst 与并发预期一起纳入 SLA；否则即使存储后端全部本地化，
hybrid 仍会出现同样的周期性尾延迟。基准已关闭 mock 限流以避免该假象。

### 3. ES 的成本边界需要独立治理

ES bm25 冷请求 8–12 ms、进程常驻约 1.6 GB；在低延迟档位应优先使用本地 Tantivy。
若必须使用 ES，需要为 JVM heap、刷新间隔和首字节延迟单独设限并持续观测。

## 已确认不再成立的历史猜测

- hybrid 100 ms 悬崖不是 Qdrant/ES、fusion 初始化或首次嵌入连接造成的；
  是测试 provider 的令牌桶等待。
- 服务级四档后端索引耗时差异不显著；后端切换不是 5,000 文件档 130 s 的主因。
- PostgreSQL 服务级链路可用；契约 3/3、服务 smoke 1/1 通过，同步快照桥接的
  运行时死锁已消除。
- 插件逐文件固定开销已从 128–180 µs 降到 3–11 µs；高亮成本仍在微秒级。
