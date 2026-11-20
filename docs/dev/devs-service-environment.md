# devs 外部服务现状与测试环境配置

## 当前状态

devs（`ssh devs`，用户 `itc`）上三个外部服务已安装为系统级布局，按 PID 文件按需启停，
不注册为 systemd 服务。旧的 `/data/kkkqkx/cce/services/` 只是下载缓存来源，不再是运行时位置。

| 服务 | 版本 | 地址 | 安装位置 | 运行数据 |
|---|---|---|---|---|
| Qdrant | 1.15.5 | `http://127.0.0.1:16333` | `/opt/cce-services/qdrant/bin/qdrant`，`/usr/local/bin/qdrant` | `/var/lib/cce-services/qdrant/<RUN_ID>` |
| Elasticsearch | 7.17.28 | `http://127.0.0.1:19200` | `/opt/cce-services/elasticsearch/`，`/usr/local/bin/elasticsearch` | `/var/lib/cce-services/elasticsearch/<RUN_ID>` |
| PostgreSQL | 10.23 | `postgres://postgres@127.0.0.1:15432/postgres` | `/opt/cce-services/postgres/usr/lib/postgresql/10/bin/`，`psql`/`pg_ctl`/`initdb`/`pg_isready` 链接到 `/usr/local/bin` | `/var/lib/cce-services/postgres/data` |

压缩包缓存位于 `/opt/cce-services/cache/`，日志位于 `/var/log/cce-services/<service>/`。
安装目录由 `itc` 持有，因此普通用户即可启停；`qdrant --version`、
`elasticsearch -V`、`psql --version` 等命令在任意目录都可用（ES 包装脚本会设置
`ES_JAVA_HOME` 指向随包 JDK，避免误用系统的 Java 8）。管理脚本位于仓库 `scripts/devs-benchmark-services.sh`，
同步到 devs 后也复制一份到 `/data/kkkqkx/cce/scripts/`。脚本只按 PID 文件停止自己启动的进程，
不使用全局 `pkill`。

## 配置命令

```bash
cd /data/kkkqkx/cce/repo
bash scripts/devs-benchmark-services.sh up
eval "$(bash scripts/devs-benchmark-services.sh env)"
bash scripts/devs-benchmark-services.sh status
bash scripts/devs-benchmark-services.sh metrics
```

`up` 支持 `qdrant`、`elasticsearch`、`postgres`、`all` 四个目标；`down`、`status`、
`metrics`、`clean` 使用同样的目标参数。`env` 输出 `CCE_TEST_QDRANT_URL`、
`CCE_TEST_ES_URL`、`CCE_TEST_PG_URL` 与 cargo PATH，供契约测试和服务基准使用。
`metrics` 从 `/proc` 读取外部进程的 CPU ticks、RSS 与 `VmHWM`，与基准进程自身的
资源指标分开记录。

`clean` 只删除 `/var/lib/cce-services/` 下的数据与 PID，安装目录和压缩包缓存始终保留。
Qdrant 与 Elasticsearch 每次运行通过 `RUN_ID` 生成独立数据目录；未设置时取当前 Unix 秒。
PostgreSQL 使用单一全局 cluster，测试通过唯一项目名和数据库对象隔离。
Elasticsearch 固定 1 GiB JVM heap，不计入基准进程 RSS。

如需重新布置安装目录，先 `down`，再由有 sudo 权限的用户创建 `/opt/cce-services`、
`/var/lib/cce-services`、`/var/log/cce-services` 并交给 `itc`，然后从缓存复制二进制；
正常测试无需 sudo。

## 验证命令

在 devs 的 repo 目录下：

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test -p cce-storage-common --test vector_backend_contract
cargo test -p cce-storage-common --test fulltext_backend_contract
cargo test -p cce-storage-common --test metadata_backend_contract
cargo test -p cce-server --test metadata_backend_smoke
cargo bench -p cce-server --bench service_remote_backend_matrix
cargo bench -p cce-server --bench plugin_overhead
cargo bench -p cce-server --bench highlight_overhead
```

契约为组件级语义验证；`metadata_backend_smoke` 才覆盖服务级远端 metadata。
当前三项契约分别 2/2、2/2、3/3 通过，服务级 smoke 1/1 通过。
服务矩阵会同时写出索引阶段 TSV 与检索阶段 TSV，用于拆分 500 / 5,000 文件档位差异。

## 注意事项

- Elasticsearch 冷查询与本地 Tantivy 不等价：500 文件档 c=1 冷查询约 14 ms 对 1 ms，
  需要在部署侧单独设定首字节 SLA 与 JVM 堆大小，不能把 ES 当作透明替换。
- Qdrant 集合名与 ES 索引名由 `run_id` 和组合名生成，跨后端复用项目会产生混合世代数据。
- PostgreSQL 远端 metadata 已放开配置，但远端 fulltext 同开仍需
  `database.allow_nonstandard_backends = true` 的高级开关。
- devs 缺少桌面端链接库，桌面端不参与本轮验证。
