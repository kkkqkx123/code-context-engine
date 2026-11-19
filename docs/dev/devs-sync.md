# devs 服务器同步与远程编译说明

本地全量编译耗时高；devs（`ssh devs`）持有暖构建缓存，
且是基准测试的执行机。代码同步到 devs 编译，本说明约定同步方式。
devs 上 cargo 可联网自动下载依赖，cargo 缓存无需同步；
`Cargo.lock` 必须进包，保证两端解析一致、构建可复现。

不使用校验和：按项目政策，同步包不用哈希校验，改用包内文件数与解压前后体积抽查。

## 目标位置

- 同步包存放：`/data/kkkqkx/cce/`
- 代码解压位置：`/data/kkkqkx/cce/repo/`（直接覆盖，非 git 仓库）
- 构建日志：`/data/kkkqkx/cce/` 下按 `check-<主题>.log`、`build-<主题>.log` 命名

## 传输方式选择

| 变更规模 | 内容 | 方式 |
|---|---|---|
| 小规模（数个文件） | 改动的源文件 | `scp` 直接覆盖远端对应路径 |
| 大规模（多文件/重构/首次） | 整个工作树 | `tar.gz` 打包 + `scp` + 远端覆盖解压 |
| e2e 运行数据 | `cce-e2e-tests` 的 `data/`、`outputs/` | 不进日常代码包，跑基准时在 devs 现场生成 |
| 参考资料与备份 | `ref/`、`frontend/node_modules/` | 不进包 |
| 任何情况 | `target/`（各级）、构建产物 | 永不传输 |

构建产物永不传输：devs 靠自身暖缓存增量编译；`target/` 体积大（本地约 59G）
且与本地工具链绑定，传过去也无法复用。

## 打包排除清单

以仓库根为基准，打包时排除：`target/`（各级，含 `crates/app/cce-desktop/target` 等
嵌套 workspace 的构建目录）、各级 `.git`（含 submodule 的 gitlink）、`ref/`、
各级 `node_modules/`、`.bench_logs/`、`logs/`、
`crates/app/cce-e2e-tests/data/`、`crates/app/cce-e2e-tests/outputs/`、
`__pycache__/`、`*.bak`、本地密钥文件 `.env`（`.env.example` 正常进包）。
按此排除后整包约 26MB（约 1.2 万个文件），远小于断点续传阈值，无需 `rsync`。

## 同步步骤

本地打包（包名 `cce-src-<sha>.tar.gz`，`sha` 取本地 HEAD 短哈希；
未提交的工作树改动会一并进包，并在 BUILD_INFO 中登记）：

```bash
cd ~/code
tar -zcf ~/sync-pkgs/cce-src-<sha>.tar.gz \
  --exclude='*/target' --exclude='*/.git' \
  --exclude='code-context-engine/ref' \
  --exclude='*/node_modules' \
  --exclude='code-context-engine/.bench_logs' --exclude='code-context-engine/logs' \
  --exclude='code-context-engine/crates/app/cce-e2e-tests/data' \
  --exclude='code-context-engine/crates/app/cce-e2e-tests/outputs' \
  --exclude='*/__pycache__' --exclude='*.bak' \
  --exclude='code-context-engine/.env' \
  code-context-engine
```

用 gzip 而不用 zstd：devs 上没有 zstd，gzip 处处可用。

记录构建信息（HEAD 哈希、分支、日期、本地工具链、未提交变更），随包传输：

```bash
{ echo "sha=<sha>"; echo "branch=$(git branch --show-current)"; echo "date=$(date -u +%FT%TZ)";
  rustc --version | sed 's/^/local_rustc=/'; echo '--- uncommitted:'; git status --short; } \
  > ~/sync-pkgs/cce-src-<sha>.BUILD_INFO.txt
scp ~/sync-pkgs/cce-src-<sha>.tar.gz ~/sync-pkgs/cce-src-<sha>.BUILD_INFO.txt devs:/data/kkkqkx/cce/
```

远端覆盖解压并抽查（文件数对得上即视为完整）：

```bash
ssh devs "cd /data/kkkqkx/cce && tar -xzf cce-src-<sha>.tar.gz -C repo --strip-components=1 \
  && cp cce-src-<sha>.BUILD_INFO.txt repo/BUILD_INFO.txt \
  && tar -tzf cce-src-<sha>.tar.gz | wc -l && du -sh repo"
```

小规模变更直接传文件，路径与仓库根一一对应：

```bash
scp crates/app/cce-server/src/api/response.rs devs:/data/kkkqkx/cce/repo/crates/app/cce-server/src/api/response.rs
```

## 远端编译

devs 上 cargo 不在默认 PATH，验证脚本 `scripts/devs-check.sh` 已处理 PATH。
传脚本、后台执行、日志落盘：

```bash
scp scripts/devs-check.sh devs:/tmp/
ssh devs 'nohup bash /tmp/devs-check.sh > /data/kkkqkx/cce/check-<sha>.log 2>&1 &'
```

脚本覆盖验收命令：workspace 全目标 `cargo check --workspace --all-targets`、
`cargo build -p cce-cli -p cce-server`、`cargo test --workspace --lib`。
单次快速验证可直接在远端执行其中一条，无需走脚本：

```bash
ssh devs 'export PATH=$HOME/.cargo/bin:$PATH; cd /data/kkkqkx/cce/repo && cargo check -p cce-orchestrator'
```

## 反向取回

日志和结果体积小，直接取回本地（`.bench_logs/` 已在打包排除清单，不会二次进包）：

```bash
scp devs:/data/kkkqkx/cce/check-<sha>.log .bench_logs/
```

## 注意事项

- 覆盖解压不删除远端多余文件：改名或删除源文件后，需手动删除远端对应文件，
  否则旧文件仍会被编译。基准性结论以前建议用空目录全新解压一次复核。
- submodule（tantivy、simvec、llm-suite、gpui-kit、encoding-guess、
  各 tree-sitter 语法包、cce-e2e-tests）以包内 checkout 为准：
  升级 submodule 后必须走压缩包全量同步。
- `cce-desktop`、`llm-suite`、`gpui-kit` 是独立 workspace（根 `Cargo.toml` 已 exclude），
  根级 check/build 不覆盖它们；需要验证桌面端时进入对应目录单独构建。devs 当前缺少
  `libxkbcommon-x11` 与 `libxcb-xkb`，桌面端链接前需将这两个库放到
  `/data/kkkqkx/cce/syslibs/`，并显式设置 `LIBRARY_PATH`、`LD_LIBRARY_PATH` 指向该目录。
- 非高峰时段传大包；若未来包体超过约 2GB（一般不至于），改用
  `rsync --partial --append-verify` 断点续传。
