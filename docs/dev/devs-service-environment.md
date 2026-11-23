# devs 外部服务现状与测试环境配置

## 当前状态

devs（`ssh devs`，用户 `itc`）上三个外部服务已安装为系统级布局，按 PID 文件按需启停，
不注册为 systemd 服务。旧的 `/data/kkkqkx/cce/services/` 已清理，仅保留基准用的 native 插件；
运行时数据只写入 `/var/lib/cce-services/`，压缩包缓存只在 `/opt/cce-services/cache/`。

| 服务 | 版本 | 地址 | 安装位置 | 运行数据 |
|---|---|---|---|---|
| Qdrant | 1.15.5 | `http://127.0.0.1:16333` | `/opt/cce-services/qdrant/bin/qdrant`，`/usr/local/bin/qdrant` | `/var/lib/cce-services/qdrant/<RUN_ID>` |
| Elasticsearch | 7.17.28 | `http://127.0.0.1:19200` | `/opt/cce-services/elasticsearch/`，`/usr/local/bin/elasticsearch` | `/var/lib/cce-services/elasticsearch/<RUN_ID>` |
| PostgreSQL | 10.23 | `postgres://postgres@127.0.0.1:15432/postgres` | `/opt/cce-services/postgres/usr/lib/postgresql/10/bin/`，`psql`/`pg_ctl`/`initdb`/`pg_isready` 链接到 `/usr/local/bin` | `/var/lib/cce-services/postgres/data` |

压缩包缓存位于 `/opt/cce-services/cache/`，日志位于 `/var/log/cce-services/<service>/`。
测试执行、数据隔离与清理的统一规范见 `docs/spec/testing.md`。安装目录由 `itc` 持有，因此普通用户即可启停；`qdrant --version`、
`elasticsearch -V`、`psql --version` 等命令在任意目录都可用（ES 包装脚本会设置
`ES_JAVA_HOME` 指向随包 JDK，避免误用系统的 Java 8）。管理脚本位于仓库 `scripts/devs-benchmark-services.sh`，
同步到 devs 后也复制一份到 `/data/kkkqkx/cce/scripts/`。脚本只按 PID 文件停止自己启动的进程，
不使用全局 `pkill`。

## 配置命令

```bash
cd /data/kkkqkx/cce/repo
bash scripts/devs-benchmark-services.sh run all -- cargo bench -p cce-server --bench service_remote_backend_matrix
bash scripts/devs-benchmark-services.sh status all
```

`run` 是基准的唯一推荐入口：先停止脚本上次启动的进程，若端口仍被外部进程占用则直接失败；
启动选中服务后执行命令，退出时（包括命令失败或中断）按 PID 停止服务，并删除本次运行
产生的 `RUN_ID` 数据目录。`run` 同时导出 `CCE_TEST_*` 地址与 cargo PATH，无需再 `eval env`。

`up`、`down`、`status`、`metrics`、`clean` 支持 `qdrant`、`elasticsearch`、`postgres`、
`all` 四个目标，用于手工排障。`up` 不再复用已健康的服务，避免基准结束后残留无人管理的进程；
`clean` 会先停止自己启动的进程，若服务仍被外部进程占用则拒绝删数据。安装目录和压缩包缓存
始终保留。

`metrics` 从 `/proc` 读取外部进程的 CPU ticks、RSS 与 `VmHWM`，与基准进程自身的
资源指标分开记录。
Qdrant 与 Elasticsearch 每次运行通过 `RUN_ID` 生成独立数据目录；未设置时取当前 Unix 秒。
PostgreSQL 使用单一全局 cluster，测试通过唯一项目名和数据库对象隔离。
Elasticsearch 固定 1 GiB JVM heap，不计入基准进程 RSS。

如需重新布置安装目录，先 `down`，再由有 sudo 权限的用户创建 `/opt/cce-services`、
`/var/lib/cce-services`、`/var/log/cce-services` 并交给 `itc`，然后从缓存复制二进制；
正常测试无需 sudo。

## 验证命令

在 devs 的 repo 目录下：

```bash
bash scripts/devs-benchmark-services.sh run qdrant -- \
  cargo test -p cce-storage-common --test vector_backend_contract
bash scripts/devs-benchmark-services.sh run elasticsearch -- \
  cargo test -p cce-storage-common --test fulltext_backend_contract
bash scripts/devs-benchmark-services.sh run postgres -- \
  cargo test -p cce-storage-common --test metadata_backend_contract
bash scripts/devs-benchmark-services.sh run postgres -- \
  cargo test -p cce-server --test metadata_backend_smoke
bash scripts/devs-benchmark-services.sh run all -- \
  cargo bench -p cce-server --bench service_remote_backend_matrix
export PATH="$HOME/.cargo/bin:$PATH"
cargo bench -p cce-server --bench plugin_overhead
cargo bench -p cce-server --bench highlight_overhead
```

契约为组件级语义验证；`metadata_backend_smoke` 才覆盖服务级远端 metadata。
当前三项契约分别 2/2、2/2、3/3 通过，服务级 smoke 1/1 通过。
服务矩阵会同时写出索引阶段 TSV 与检索阶段 TSV，用于拆分 500 / 5,000 文件档位差异。

## 注意事项

- Elasticsearch 冷查询与本地 Tantivy 不等价：500 文件档 c=1 冷查询约 14 ms 对 1 ms，
  需要在部署侧单独设定首字节 SLA 与 JVM 堆大小，不能把 ES 当作透明替换。
- Qdrant 集合名与 ES 索引名由 `run_id` 和组合名生成，跨后端复用项目会产生混合世代数据；
  服务矩阵在每档结束时删除本档集合与索引，契约测试也各自清理远端对象。
- 基准的临时工程目录与测试临时目录均由作用域守卫回收；`/data` 上的 CCE 目录主要是
  `repo/target` 构建缓存，与外部服务运行数据不在同一块盘。
- PostgreSQL 远端 metadata 已放开配置，但远端 fulltext 同开仍需
  `database.allow_nonstandard_backends = true` 的高级开关。
- devs 缺少桌面端链接库，桌面端不参与本轮验证。
