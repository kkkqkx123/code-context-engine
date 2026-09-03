# 插件系统指南

CCE 插件系统允许你扩展核心功能，支持 Lua 脚本和原生动态库两种插件类型。

## 文档导航

| 文档 | 说明 |
|------|------|
| [插件系统概述](plugin-system.md) | 插件类型、注册、扩展点、执行模式 |
| [插件能力参考](plugin-capabilities.md) | 每个扩展点的详细参数和返回值 |
| [快速开始](plugin-quickstart.md) | 5 分钟创建第一个插件 |

## 快速链接

### 插件类型

- **Lua 插件**: 适合快速开发，使用 `mlua` 加载
- **Native 插件**: 适合高性能场景，使用 `libloading` 加载

### 核心扩展点

| 管道 | 扩展点 | 用途 |
|------|--------|------|
| 索引 | TextGen | 自定义文本生成 |
| 索引 | FormatParse | 文档格式解析 |
| 索引 | GroupOverride | 分组逻辑覆盖 |
| 索引 | Chunk | 分块逻辑覆盖 |
| 索引 | FileFilter | 文件过滤 |
| 查询 | QueryRewrite | 查询重写 |
| 查询 | Rerank | 结果重排序 |
| 查询 | Fusion | 融合权重调整 |
| 查询 | ResultFilter | 结果过滤 |

### 执行模式

- **Override**: 第一个非空结果生效
- **Chain**: 按顺序执行，前输出作为后输入
- **Additive**: 所有结果合并

## 示例

### 最小 Lua 插件

```lua
plugin = {
    id = "my-plugin",
    capabilities = {"text_gen"},
}

function plugin.generate_bm25(group)
    return group.header and group.header.name or nil
end
```

### 注册插件

`.cce/plugins.json`:

```json
{
  "plugins": [{
    "id": "my-plugin",
    "path": "./plugins/my-plugin.lua",
    "type": "lua",
    "enabled": true
  }]
}
```

## 相关资源

- [配置参考](../config/)
- [架构设计](../architecture/)
- [API 文档](../api/)
