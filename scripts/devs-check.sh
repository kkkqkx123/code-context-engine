#!/usr/bin/env bash
# devs 远端验收脚本：在 /data/kkkqkx/cce/repo 下执行。
# 用法（本地）：scp scripts/devs-check.sh devs:/tmp/ && ssh devs 'nohup bash /tmp/devs-check.sh > /data/kkkqkx/cce/check-<sha>.log 2>&1 &'
# 覆盖质量验收命令：workspace 全目标检查、CLI/server 编译、lib 测试。依赖由 devs 联网自动下载。
set -u
export PATH="$HOME/.cargo/bin:$PATH"
cd /data/kkkqkx/cce/repo
echo "=== C1 check workspace all-targets ==="
cargo check --workspace --all-targets 2>&1 | tail -n 6
echo "=== B1 build cli+server ==="
cargo build -p cce-cli -p cce-server 2>&1 | tail -n 4
echo "=== T1 lib tests ==="
cargo test --workspace --lib 2>&1 | tail -n 15
echo "ALL DONE"
