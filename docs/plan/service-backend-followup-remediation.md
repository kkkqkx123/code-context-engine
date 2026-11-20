# 服务后端基准后续修复方案

## 范围与现状核对

本方案对应 `docs/issue/service-backend-benchmark-2026-11-20.md` 的八项发现。
以当前 `main` 为准逐项核对：

1. Qdrant 1.15 兼容：`ok` / `completed` 双状态、结构化 `min_should`、
   关键字 payload 的目录前缀客户端二次过滤均已落地并有单测，属于已完成项。
2. Elasticsearch 冷查询慢：属于后端固有往返成本，不能通过改写检索口径伪装成
   Tantivy 等价替换。本轮补齐阶段归因、统一启动参数与 SLA 说明。
3. hybrid 约 100 ms 悬崖：上一轮已确认在嵌入传输内部。本轮把首次连接耗时和
   嵌入缓存命中从传输层一路带到响应 DTO，并在服务启动时预建连接。
4. 服务索引 5,000 文件缺阶段数据：`IndexResult.stages` 尚未进入服务响应和
   服务基准 TSV。本轮补齐 DTO、handler 映射和独立阶段 TSV。
5. PostgreSQL 服务级 metadata：代码已完成确定类型后端分发、契约补齐与配置放开；
   需要在 devs 启动实例并执行契约 + 服务 smoke 验证，而不是再次引入转发层。
6. 插件逐文件固定开销：Lua 每次 hook 都新建 OS 线程并走通道等待。本轮改为
   在调用线程内执行，由 Lua 指令钩子按截止时间中断，并把脚本耗时与调度耗时拆开。
7. 高亮观测：高亮仍留在独立 harness，不并入 BM25 检索；服务响应补充渲染阶段耗时，
   使展示成本与检索成本可分离。
8. 资源指标边界：外部服务进程指标由管理脚本单独采集；基准进程继续使用 `VmHWM`
   并在文档中声明其语义。

## 修改顺序

### 第一步：嵌入传输与查询阶段

- `EmbeddingTiming` 增加“首次传输调用耗时”，由传输层记录，经过 handler 汇总。
- `CachedEmbedder` 区分真实远程调用与缓存/单飞命中，并把命中信息记入本次诊断数据。
- `QueryStageTimings` 与 API DTO 增加首次嵌入耗时、嵌入缓存命中数；检索矩阵 TSV
  同步增加列。
- 引擎装配阶段对嵌入端点做一次非致命预热，把连接建立移出首个用户请求；
  预热失败只告警，不改变引擎启动结果。

### 第二步：索引阶段透出

- `IndexResponse` 携带 `IndexStageTimingsDto`，handler 从 `IndexResult.stages` 映射。
- 服务远程后端矩阵新增索引阶段 TSV，记录全部批次阶段与调用次数，
  使 500 / 5,000 文件差额可以分摊到具体阶段。

### 第三步：插件执行模型

- Lua 插件删除每次 hook 的线程创建与同步通道；deadline 写入保护钩子，
  指令钩子超时后映射为 `PluginError::Timeout`，内存上限仍然生效。
- VM 归还池时移除钩子，避免过期 deadline 影响下一次调用。
- 插件基准 TSV 增加脚本执行与调度耗时两列，native 插件仍保留 FFI 的独立线程超时。

### 第四步：服务响应与高亮边界

- 搜索响应增加响应渲染耗时，仅覆盖 handler 内构造 DTO 的部分。
- 高亮 harness 保持独立，记录标记数量；不把文件读取成本计入检索阶段。

### 第五步：远端 metadata 同步桥接

- PostgreSQL 连接池连接驱动统一跑在专用运行时上，避免快照同步读取阻塞调用方运行时
  导致连接维护任务饿死；同步桥接由普通线程进入该运行时，不再依赖
  `block_in_place` 的运行时类型限制。
- 该修复在契约之外补上了服务级 smoke 的实际覆盖。

### 第六步：devs 服务管理

- 服务安装到 `/opt/cce-services`，二进制链接到 `/usr/local/bin`，数据在
  `/var/lib/cce-services`，日志在 `/var/log/cce-services`；单一脚本补齐 PostgreSQL：
  本地解包二进制、`initdb`、独立 data dir、PID 启停。
- 新增 `env` 输出 `CCE_TEST_*_URL` 等测试变量；新增 `metrics` 从 `/proc` 采集
  外部进程 CPU、RSS、峰值 RSS。
- 状态命令同时检查进程与 HTTP / `pg_isready` 健康状态；缓存保留，运行数据按
  `RUN_ID` 隔离。

## 验证结果与证据

devs 上最终一次合并验证（`.bench_logs/final-followup-summary.log`）：

- `cargo check --workspace --all-targets --all-features`：通过。
- Qdrant / Elasticsearch / PostgreSQL 三项契约测试：分别 2/2、2/2、3/3 通过。
- `cce-plugin-runtime` 88 项单测通过，包含超时、超时后 VM 复用与内存上限。
- PostgreSQL 服务级 smoke 通过，覆盖项目创建、索引、检索、热更新读路径、
  检查点往返与图导出。
- `plugin_overhead` 重跑：四类逐文件 hook 从约 128–180 µs/次降到 3–11 µs/次，
  其中脚本执行约 6 µs、调度约 0.2 µs；查询改写从约 142 µs 降到约 3 µs。
- `highlight_overhead` 重跑：单次 6.54 µs，其中读文件 4.51 µs、截取 0.67 µs、
  标记 1.36 µs。
- 原始结果见 `.bench_logs/plugin_overhead_followup.tsv` 与
  `.bench_logs/highlight_overhead_followup.tsv`。

服务矩阵已用全局安装服务完整重跑：500 与 5,000 文件档四档组合均错误数为 0，
索引阶段 TSV 与检索阶段 TSV 均已产出。复测又定位到 hybrid 约 100 ms 悬崖的真实来源：
离线 mock provider 继承默认 600/min 令牌桶，索引耗尽令牌后等待补充；关闭 mock 限流后
hybrid 冷请求从 p50/p95 98–99 ms 降到 3–9 ms。完整结论见
`docs/analysis/service-backend-followup-metrics-2026-11-20.md`。
