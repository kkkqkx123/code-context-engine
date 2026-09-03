# 配置重载 API

本文档描述配置重载相关的 API 端点，用于手动触发系统配置的热更新。

## POST /api/config/reload

手动触发全局配置重载，重新加载所有处理器的业务配置。

### 功能说明

此端点使用**失效-重建模式**（invalidate-rebuild pattern）来简化配置管理：
1. 使现有配置缓存失效
2. 从配置文件重新读取最新配置
3. 通知所有注册的处理器重新加载配置
4. 使用两阶段提交确保原子性

### 请求

**方法**: `POST`

**路径**: `/api/config/reload`

**Content-Type**: `application/json`

**请求体**: 空对象或省略

```json
{}
```

### 响应

**成功响应** (HTTP 200):

```json
{
  "success": true,
  "message": "Configuration reloaded successfully",
  "project_root": "/path/to/project",
  "processors_count": 5
}
```

**响应字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `success` | boolean | 操作是否成功 |
| `message` | string | 操作说明 |
| `project_root` | string | 项目根目录路径 |
| `processors_count` | number | 参与重载的处理器数量 |

**失败响应** (HTTP 503):

```json
{
  "success": false,
  "error": "Hot update coordinator not configured"
}
```

**失败响应** (HTTP 500):

```json
{
  "success": false,
  "error": "Failed to reload configuration: [错误详情]"
}
```

### 示例

**使用 curl**:

```bash
curl -X POST "http://localhost:3000/api/config/reload" \
  -H "Content-Type: application/json" \
  -d '{}'
```

**使用 cce-cli**:

```bash
cce config reload --verbose
```

### 使用场景

1. **配置文件修改后**: 手动修改了 `config.toml` 或其他配置文件后，无需重启服务即可生效
2. **动态调整参数**: 运行时调整扫描规则、索引策略、防抖参数等
3. **故障恢复**: 当某些处理器配置异常时，重新加载正确的配置
4. **多环境切换**: 在开发、测试、生产环境之间快速切换配置

### 工作原理

1. **获取协调器**: 从应用状态中获取 `HotUpdateCoordinator`
2. **创建处理器**: 使用 `ProcessorFactory` 创建所有启用的处理器实例
3. **执行重载**: 通过 `ConfigReloadManager` 处理待处理的配置变更
4. **版本控制**: 使用 `ConfigVersionRegistry` 防止旧配置覆盖新配置
5. **错误隔离**: 单个处理器重载失败不影响其他处理器

### 支持的处理器

以下处理器支持配置重载：

- **EmbeddingUpdateProcessor**: 向量嵌入配置（模型、批大小等）
- **RelationUpdateProcessor**: 关系提取配置（调用链深度、依赖分析等）
- **SummaryUpdateProcessor**: 摘要生成配置（策略、模板等）
- **BM25UpdateProcessor**: BM25 索引配置（分词器、权重等）
- **NlDocumentUpdateProcessor**: 自然语言文档配置

### 注意事项

⚠️ **重要提示**:

1. **原子性保证**: 使用两阶段提交确保要么全部成功，要么全部回滚
2. **性能影响**: 配置重载期间可能会短暂影响索引和搜索性能
3. **并发安全**: 重载过程中会锁定协调器，阻止其他更新操作
4. **配置验证**: 建议先验证配置文件语法正确性再执行重载
5. **日志记录**: 所有重载操作都会记录到日志中，便于问题排查

### 与其他重载方式的区别

| 重载方式 | 端点 | 作用范围 | 适用场景 |
|---------|------|---------|----------|
| 全局配置重载 | `POST /api/config/reload` | 所有处理器 | 修改全局配置文件 |
| **项目配置重载** | `POST /api/project/:id/reload` | **单个项目** | **修改项目特定配置（推荐）** |
| 项目配置更新 | `PUT /api/project/:id/config` | 单个项目 + 热重载 | 通过 API 动态更新配置 |
| 文件监控 | `POST /api/project/{project_id}/watch/start` | 自动检测文件变化 | 开发环境实时监控 |

**推荐使用项目配置重载**：
- 系统采用项目级缓存架构，每个项目有独立的 IndexOrchestrator 和 Searcher 实例
- 项目配置包括：batch size、grouper 配置、ast_to_nl 配置、summary 配置等
- 调用 `POST /api/project/:id/reload` 会清除该项目的组件缓存，下次访问时重新加载

### 相关 API

- [POST /api/project/:id/reload](./project-config.md#post-apiprojectidreload) - 重新加载项目配置
- [PUT /api/project/:id/config](./project-config.md#put-apiprojectidconfig) - 更新项目配置并触发热重载
- [POST /api/project/{project_id}/watch/start](./watch.md#post-apiprojectproject_idwatchstart) - 启动文件监控实现自动重载

### 故障排查

**问题**: 返回 "Hot update coordinator not configured"

**原因**: 服务启动时未启用热更新功能

**解决**: 
1. 检查 `config.toml` 中 `hot_update.enabled` 是否为 `true`
2. 确认服务启动时正确初始化了 `HotUpdateCoordinator`

**问题**: 部分处理器重载失败

**原因**: 配置文件格式错误或处理器内部错误

**解决**:
1. 检查日志中的详细错误信息
2. 验证配置文件语法（可使用 `toml` 验证工具）
3. 逐个检查处理器的配置要求

**问题**: 重载后配置未生效

**原因**: 配置文件未被正确读取或缓存未清除

**解决**:
1. 确认配置文件路径正确
2. 检查文件权限是否允许读取
3. 尝试再次执行重载命令
4. 查看日志确认配置版本号是否更新
