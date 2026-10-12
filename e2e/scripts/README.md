# CCE 端到端脚本指南

本文描述位于 `e2e/scripts/` 目录下的 Python 测试套件。入口为 `main.py`，通过超文本接口驱动真实服务；向量进程由服务端内部管理，本目录不再管理外部向量进程。

## 1. 架构设计

测试脚本采用启动、执行、回收模式，保证环境隔离：

1. **服务启动**：后台启动 CCE 服务端，配置来自 `e2e/config.toml`，可经 `CCE_CONFIG` 环境变量覆盖。
2. **健康检查**：轮询超文本端口，确认服务就绪后再执行用例。
3. **用例执行**：依次运行索引、查询、关系分析等功能场景与性能轮次。
4. **资源回收**：测试结束后终止后台进程，清理临时数据。

## 2. 核心模块说明

### 2.1 `service_manager.py`（服务管理器）

负责服务端进程的生命周期：后台启动、端口存活探测、优雅退出。缺省二进制为 `e2e/bin/cce`，缺省配置为 `e2e/config.toml`，调用方可显式覆盖。

### 2.2 `test_runner.py`（测试执行器）

封装针对 CCE 接口的测试逻辑：基于 `requests` 的超文本客户端、索引与查询结果校验、规模分档加载。规模映射集中在 `FIXTURES`，小规模为 `once_cell`，中规模为 `flask`，新增规模只改该映射。

### 2.3 `main.py`（入口点）

协调全流程：解析 `--fixture` 与 `--iterations`，管理服务启停，分别运行功能与性能模式并落盘报告。缺省分档即最小可用分档。

## 3. 前置要求

1. **Python 环境**：Python 3.8+。
2. **依赖安装**：
   ```bash
   pip install requests
   ```
3. **服务端二进制**：先构建发布版并复制到 `e2e/bin/cce`。
4. **模型密钥**：环境中设置 `CCE_LLM_API_KEY_SILICONFLOW`。

## 4. 运行测试

```bash
python e2e/scripts/main.py functional --fixture once_cell
python e2e/scripts/main.py benchmark --fixture flask --iterations 3
python e2e/scripts/main.py all --fixture once_cell --iterations 5
```

仅连接已运行服务时可加 `--no-service-mgmt` 并配合 `--server-url` 使用。

## 5. 测试用例覆盖范围

| 模块 | 测试内容 | 预期指标 |
|------|----------|----------|
| **Index** | 真实项目全量索引 | 实体提取完整且阶段无错误 |
| **Search** | 向量与全文与混合检索 | 有索引工程上命中且延迟分位记录 |
| **Relation** | 调用链追踪 | 种子工程至少一条调用关系 |
| **Hot Update** | 文件修改后的增量更新 | 更新后视图一致 |

## 6. 故障排查

- **连接拒绝**：确认 CCE 服务端是否在 `9001` 端口成功监听。
- **API Key 错误**：检查环境变量是否正确传递给了子进程。
- **项目已存在**：入口启动前会清理 `e2e/data` 下的持久数据，手动运行时不要指向生产数据目录。

## 7. 扩展性设计

- **新增场景**：只改 `test_runner.py`。
- **新增规模**：只改 `FIXTURES` 映射并同步 `e2e/README.md` 的分档表格。
- **自定义断言**：在 `test_runner.py` 中添加新的校验方法即可。
