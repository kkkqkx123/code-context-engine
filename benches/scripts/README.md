# CCE 基准测试与功能验证脚本指南

本文档描述了位于 `benches/scripts/` 目录下的 Python 测试套件的设计思路、架构及使用方法。该套件旨在通过自动化手段完成 Code Context Engine (CCE) 的功能验证与性能基准测试。

## 1. 架构设计

测试脚本采用 **Setup-Execute-Cleanup** 模式，确保测试环境的纯净与隔离：

1.  **环境探测 (Discovery)**: 使用 `where` (Windows) 或 `which` (Linux/Mac) 命令定位 Qdrant 可执行文件。
2.  **服务启动 (Provisioning)**: 在后台异步启动 Qdrant 向量数据库和 CCE 服务端。
3.  **健康检查 (Health Check)**: 轮询 HTTP 端口，确保所有依赖服务已就绪。
4.  **用例执行 (Execution)**: 依次运行索引、查询、关系分析等功能测试用例。
5.  **资源回收 (Teardown)**: 测试结束后自动终止所有后台进程，清理临时数据。

## 2. 核心模块说明

### 2.1 `service_manager.py` (服务管理器)
负责底层进程的调度：
- **Qdrant 管理**: 
  - 自动查找系统路径下的 `qdrant.exe`。
  - 使用 `subprocess.Popen` 配合 `creationflags=CREATE_NO_WINDOW` (Windows) 实现静默后台启动。
  - 监听标准输出以捕获启动日志。
- **CCE 服务端管理**: 
  - 支持通过 `--config` 参数加载 `benches/config.toml` 全局配置。
  - 注入环境变量（如 `CCE_LLM_API_KEY_SILICONFLOW`）。

### 2.2 `test_runner.py` (测试执行器)
封装了针对 CCE API 的测试逻辑：
- **HTTP 客户端**: 基于 `requests` 库，提供统一的 JSON 响应解析。
- **断言工具**: 包含对索引结果（文件数、实体数）和查询结果（相关性分数）的校验函数。
- **Fixture 加载**: 自动识别 `benches/fixtures/` 下的项目（如 `once_cell`）并触发索引。

### 2.3 `main.py` (入口点)
协调整个测试流程：
```python
def main():
    manager = ServiceManager()
    try:
        manager.start_qdrant()
        manager.start_cce_server(config_path="benches/config.toml")
        runner = TestRunner(base_url="http://localhost:9001")
        runner.run_all_tests()
    finally:
        manager.cleanup()
```

## 3. 前置要求

在运行脚本前，请确保满足以下条件：

1.  **Python 环境**: Python 3.8+。
2.  **依赖安装**: 
    ```bash
    pip install requests psutil
    ```
3.  **Qdrant 二进制文件**: 
    - 建议将 `qdrant.exe` 所在目录加入系统 `PATH` 环境变量。
    - 或者在脚本中显式指定 `QDRANT_PATH` 环境变量。
4.  **SiliconFlow API Key**: 
    - 必须在环境中设置 `CCE_LLM_API_KEY_SILICONFLOW`。

## 4. 运行测试

### 4.1 运行完整功能测试
```bash
cd benches/scripts
python main.py --scenario functional
```

### 4.2 运行性能基准测试
```bash
python main.py --scenario benchmark --fixture once_cell --iterations 5
```

### 4.3 仅启动服务（用于手动调试）
```bash
python main.py --action start-only
```

## 5. 测试用例覆盖范围

| 模块 | 测试内容 | 预期指标 |
|------|----------|----------|
| **Index** | Rust 项目全量索引 | 实体提取准确率 > 95% |
| **Search** | 向量语义搜索 (BGE-M3) | P95 延迟 < 200ms |
| **Relation** | 调用链追踪 (Call Chain) | 深度为 3 的链路完整 |
| **Hot Update** | 文件修改后的增量更新 | 更新耗时 < 2s |

## 6. 故障排查

- **Qdrant 启动失败**: 检查端口 `6333` 是否被占用。
- **连接拒绝**: 确认 CCE 服务端是否在 `9001` 端口成功监听。
- **API Key 错误**: 检查环境变量是否正确传递给了子进程。

## 7. 扩展性设计

脚本采用模块化设计，未来可以轻松扩展：
- **新增 Fixture**: 只需在 `benches/fixtures/` 下添加新项目并配置 `.cce.toml`。
- **自定义断言**: 在 `test_runner.py` 中添加新的 `assert_` 方法即可。
- **多语言支持**: 通过修改 `service_manager.py` 中的进程启动参数，支持跨平台运行。
