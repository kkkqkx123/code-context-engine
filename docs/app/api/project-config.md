# 项目配置管理 API

## 架构说明

系统采用**项目级懒加载缓存**架构：
- 每个项目的 IndexOrchestrator 和 Searcher 实例都是独立创建并缓存的
- 首次访问项目时，根据项目配置创建组件实例
- 后续请求复用缓存的实例，提高性能
- 配置变更需要调用 reload API 清除缓存

---

## POST /api/project/:id/reload

重新加载项目配置，使缓存失效并从文件重新读取。

### 请求

**方法**: `POST`

**路径参数**:

| 参数 | 类型 | 描述 |
|-----|------|------|
| `id` | string | 项目ID |

### 响应

**Content-Type**: `application/json`

```json
{
  "success": true,
  "message": "Configuration cache invalidated. Will reload from file on next access.",
  "project_id": "1",
  "config_version": 2
}
```

**响应字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `success` | boolean | 操作是否成功 |
| `message` | string | 操作说明 |
| `project_id` | string | 项目ID |
| `config_version` | number | 配置版本号 |

### 示例

```bash
curl -X POST "http://localhost:3000/api/project/1/reload"
```

### 使用场景

1. **配置更新后刷新**: 手动修改配置文件后重新加载
2. **调试配置问题**: 验证配置更改是否生效
3. **强制刷新**: 清除缓存获取最新配置

**注意**: 调用此 API 会清除以下缓存：
- IndexOrchestrator 缓存（包含 batch、grouper、ast_to_nl、summary 等配置）
- Searcher 缓存
- HotUpdateCoordinator 缓存
- ProjectRegistry 配置缓存

下次访问该项目时，系统会从磁盘重新加载配置并创建新的组件实例。

---

## PUT /api/project/:id/config

更新项目配置并触发热重载。

### 请求

**方法**: `PUT`

**路径参数**:

| 参数 | 类型 | 描述 |
|-----|------|------|
| `id` | number | 项目ID |

**Content-Type**: `application/json`

```json
{
  "config": {
    "name": "my-project",
    "scanner": {
      "follow_symlinks": false,
      "respect_gitignore": true,
      "exclude_patterns": ["node_modules", "dist"]
    },
    "orchestrator": {
      "hot_update": {
        "enabled": true,
        "scan_interval_seconds": 5
      }
    }
  }
}
```

**请求字段**:

| 字段 | 类型 | 必填 | 描述 |
|-----|------|------|------|
| `config` | object | 是 | 项目配置对象（部分配置） |

### 响应

**Content-Type**: `application/json`

```json
{
  "success": true,
  "hot_reload_applied": true,
  "message": "Configuration updated and hot reload triggered successfully"
}
```

**响应字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `success` | boolean | 操作是否成功 |
| `hot_reload_applied` | boolean | 热重载是否成功应用 |
| `message` | string | 操作说明 |

### 示例

```bash
curl -X PUT "http://localhost:3000/api/project/1/config" \
  -H "Content-Type: application/json" \
  -d '{
    "config": {
      "scanner": {
        "exclude_patterns": ["build", "target"]
      }
    }
  }'
```

### 使用场景

1. **动态调整配置**: 运行时修改扫描规则、索引策略等
2. **自动化配置管理**: CI/CD流程中自动更新配置
3. **多环境配置切换**: 开发/测试/生产环境配置切换
