# 插件快速开始

## 5 分钟创建第一个 Lua 插件

### 步骤 1: 创建插件文件

创建 `.cce/plugins/my-plugin.lua`：

```lua
plugin = {
    id = "hello-plugin",
    name = "Hello Plugin",
    version = "0.1.0",
    priority = 10,
    capabilities = {"text_gen"},
    languages = {"rust"},
}

function plugin.generate_bm25(group)
    if group.header then
        return string.format(
            "%s %s: %s",
            group.language,
            group.header.kind,
            group.header.name
        )
    end
    return nil
end
```

### 步骤 2: 注册插件

编辑 `.cce/plugins.json`：

```json
{
  "plugins": [
    {
      "id": "hello-plugin",
      "path": "./plugins/my-plugin.lua",
      "type": "lua",
      "enabled": true
    }
  ]
}
```

### 步骤 3: 测试

运行索引命令，插件会自动加载。

## 常见用例

### 用例 1: 自定义 BM25 文本生成

```lua
plugin = {
    id = "custom-bm25",
    capabilities = {"text_gen"},
}

function plugin.generate_bm25(group)
    local parts = {}
    
    if group.header then
        table.insert(parts, group.header.name)
        table.insert(parts, group.header.kind)
    end
    
    for _, member in ipairs(group.members or {}) do
        table.insert(parts, member.name)
    end
    
    return table.concat(parts, " ")
end
```

### 用例 2: 文件过滤

```lua
plugin = {
    id = "file-filter",
    capabilities = {"file_filter"},
}

function plugin.filter_file(path, is_directory, size)
    -- 排除 node_modules
    if path:match("node_modules") then
        return "exclude"
    end
    
    -- 排除大文件（> 1MB）
    if size > 1048576 then
        return "exclude"
    end
    
    return nil  -- 使用内置逻辑
end
```

### 用例 3: 查询重写

```lua
plugin = {
    id = "query-rewriter",
    capabilities = {"query_rewrite"},
}

function plugin.rewrite_query(query)
    -- 扩展缩写
    local expanded = query
        :gsub("fn", "function")
        :gsub("mod", "module")
        :gsub("struct", "structure")
    
    return {
        rewritten_query = expanded,
        expansion_terms = {"implementation", "definition"}
    }
end
```

### 用例 4: 结果过滤

```lua
plugin = {
    id = "result-filter",
    capabilities = {"result_filter"},
}

function plugin.filter_results(query, results)
    local entries = {}
    
    for _, result in ipairs(results) do
        -- 提升测试文件的分数
        if result.file_path:match("test") then
            table.insert(entries, {
                id = result.id,
                boost = 0.2
            })
        end
        
        -- 移除生成的代码
        if result.file_path:match("generated") then
            table.insert(entries, {
                id = result.id,
                remove = true
            })
        end
    end
    
    return entries
end
```

## 调试技巧

### 1. 启用详细日志

```bash
RUST_LOG=debug cargo run -- index
```

### 2. 插件错误处理

插件错误不会中断主流程，但会记录警告日志：

```
WARN plugin_id=hello-plugin error="..." Plugin generate_bm25 failed
```

### 3. 测试插件

创建测试脚本验证插件逻辑：

```lua
-- test-plugin.lua
package.path = package.path .. ";../plugins/?.lua"

-- 模拟 group 数据
local group = {
    header = { name = "test_function", kind = "function" },
    members = {},
    language = "rust"
}

-- 加载插件
dofile("plugins/my-plugin.lua")

-- 测试
local result = plugin.generate_bm25(group)
print("Result:", result)
```

## 最佳实践

1. **声明能力**: 始终在 `capabilities` 中声明插件支持的能力
2. **返回 nil**: 当不确定时返回 `nil`，让内置逻辑处理
3. **错误处理**: 使用 `pcall` 包装可能失败的操作
4. **性能**: 避免在插件中执行耗时操作
5. **日志**: 使用 `print` 或 tracing 输出调试信息

## 故障排除

### 插件未加载

检查：
- `.cce/plugins.json` 格式是否正确
- 插件文件路径是否正确
- `enabled` 字段是否为 `true`

### 插件未生效

检查：
- `capabilities` 是否正确声明
- `file_patterns` 和 `languages` 是否匹配
- 优先级是否正确设置

### 插件执行失败

检查日志中的错误信息，常见问题：
- Lua 语法错误
- 返回值格式错误
- 超时或内存超限
