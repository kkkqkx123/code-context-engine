# 部署目录说明

> 本 deploy 目录面向 Linux 服务部署；构建脚本可在其他平台运行，但服务安装、systemd 单元与开机自启仅在 Linux 上执行。

本目录集中存放 Code Context Engine 的构建、安装与服务部署资产。

## 文件清单

| 文件 | 作用 |
|---|---|
| `build.sh` | 构建 `cce`、`cce-cli`、`cce-mcp` 到 `build/` |
| `install.sh` | 构建（可选）、初始化配置、安装二进制、可选安装 systemd 服务 |
| `cce-server.service` | systemd 单元模板，安装时渲染占位符 |
| `configs/` | 配置模板与环境变量模板 |
| `configs/config.example.toml` | 完整配置示例 |
| `configs/config.minimal.toml` | 最小配置示例 |
| `configs/config.prod.toml` | 生产覆盖示例（loopback） |
| `configs/config.remote.example.toml` | 远程/admission 示例 |
| `configs/.env.example` | 环境变量模板 |

## 示例工程配置

`deploy/.cce/` 提供项目级配置与插件的演示模板（`.cce.example.toml`、`.cce.minimal.toml`、`plugins.json`、`plugins/*.lua`），用于初始化新项目的 `.cce` 目录；运行时数据不由仓库提交。

## 构建

```bash
# 构建全部（release）
./deploy/build.sh

# 仅构建服务端
./deploy/build.sh server

# 包含 admission feature
./deploy/build.sh all --admission

# 调试构建
./deploy/build.sh all --debug
```

产物输出到 `build/`。

## 安装

用户级（默认，无需 root）：

```bash
./deploy/install.sh --service --enable-autostart
```

系统级（需 root）：

```bash
sudo ./deploy/install.sh --system --service --enable-autostart
```

仅构建与初始化配置，不安装服务：

```bash
./deploy/install.sh --no-service
```

## 路径约定

- 二进制安装目录：用户级 `~/.local/bin`，系统级 `/usr/local/bin`
- 配置目录：用户级 `~/.config/cce`，系统级 `/etc/cce`
- 数据目录：用户级 `~/.local/share/cce`，系统级 `/var/lib/cce`
- 服务单元：用户级 `~/.config/systemd/user/cce-server.service`，系统级 `/etc/systemd/system/cce-server.service`

## 服务管理

用户级：

```bash
systemctl --user start cce-server.service
systemctl --user stop cce-server.service
systemctl --user restart cce-server.service
systemctl --user status cce-server.service
journalctl --user -u cce-server.service -f
```

系统级：

```bash
sudo systemctl start cce-server.service
sudo systemctl status cce-server.service
sudo journalctl -u cce-server.service -f
```

## 配置

安装后编辑 `$CONFIG_DIR/config.toml` 与 `$CONFIG_DIR/.env`。服务通过 `CCE_CONFIG` 环境变量加载配置，数据文件写入数据目录。

## MCP 服务

`cce-mcp` 会随二进制一起安装，可按需通过 stdio 或独立端口运行：

```bash
cce-mcp
```

## 前端

前端位于 `frontend/`，需单独构建并由反向代理或静态服务器提供；开发时通过 Vite 代理 `/api` 到后端。
