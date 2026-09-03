# 插件能力参考

本文档详细说明每个扩展点的使用场景、参数和返回值。

## TextGen - 文本生成

为实体组生成 BM25 和 Embedding 文本。

### 使用场景

- 自定义代码到自然语言的转换逻辑
- 针对特定语言优化文本生成
- 添加领域特定的描述

### 参数

```lua
-- group 结构
{
    group_id = "string",
    header = {
        id = number,
        name = "string",
        kind = "string",  -- function, class, struct, etc.
        modifiers = {"public", "static", ...},
        span = {start_line, end_line}
    },
    members = [{
        id = number,
        name = "string",
        kind = "string",
        span = {start_line, end_line}
    }],
    language = "string",
    group_type = "string"
}
```

### 返回值

```lua
-- BM25 文本（用于全文搜索）
return "函数 calculate_sum 接受两个整数参数并返回它们的和"

-- Embedding 文本（用于语义搜索）
return "A function that calculates the sum of two integers"
```

## FormatParse - 文档解析

解析非代码文件（Markdown、JSON、配置文件等）。

### 使用场景

- 支持自定义文档格式
- 增强现有格式解析
- 添加新的配置文件格式支持

### 参数

- `content`: 文件内容字符串
- `file_path`: 文件路径

### 返回值

```lua
return {
    title = "文档标题",
    language = "markdown",
    entities = [{
        name = "章节名称",
        kind = "heading",
        content = "章节内容"
    }]
}
```

## EntityExtract - 实体提取

从代码文件中提取补充实体。

### 使用场景

- 提取正则表达式匹配的实体
- 为 tree-sitter 不支持的语言添加实体提取
- 提取特定模式的代码结构

### 参数

- `content`: 文件内容
- `file_path`: 文件路径
- `language`: 编程语言

### 返回值

```lua
return {
    {
        name = "实体名称",
        kind = "function",
        content = "实体内容",
        start_line = 1,
        end_line = 10
    }
}
```

## GroupOverride - 分组覆盖

完全替换内置的分组逻辑。

### 使用场景

- 自定义语言的分组策略
- 优化特定代码结构的分组
- 实现复杂的分组算法

### 参数

```lua
-- context 包含
{
    entities = [...],  -- 解析后的实体列表
    relations = [...]  -- 关系信息
}
```

### 返回值

```lua
return {
    {
        group_id = "unique_id",
        header = { ... },
        members = [ ... ],
        language = "rust",
        group_type = "function"
    }
}
```

## Chunk - 分块覆盖

覆盖内置的文本分块逻辑。

### 使用场景

- 自定义分块策略
- 优化特定内容的分块大小
- 实现语义感知的分块

### 参数

```lua
-- conversions 包含
{
    group = { ... },
    bm25_text = "BM25 文本",
    embedding_text = "Embedding 文本"
}
```

### 返回值

```lua
return {
    {
        chunk_id = "unique_chunk_id",
        text = "分块文本",
        source_group_id = "group_id",
        metadata = {
            content_type = "code",
            entity_kind = "function"
        }
    }
}
```

## Rerank - 结果重排序

对搜索结果进行重排序。

### 使用场景

- 基于规则的结果重排
- 集成外部重排序服务
- 自定义相关性评分

### 参数

- `query`: 搜索查询
- `candidates`: 候选结果列表

```lua
-- candidates 结构
{
    {
        id = "result_id",
        content = "结果内容",
        file_path = "文件路径",
        initial_score = 0.8,
        entity_type = "function"
    }
}
```

### 返回值

```lua
return {
    reranked_candidates = {
        {
            id = "result_id",
            rerank_score = 0.95,
            initial_score = 0.8,
            final_score = 0.95,
            rank_change = 2,  -- 排名变化
            reasoning = "更相关"  -- 可选
        }
    }
}
```

## QueryRewrite - 查询重写

在搜索前重写或扩展查询。

### 使用场景

- 查询纠错
- 同义词扩展
- 查询意图识别

### 参数

- `query`: 原始查询字符串

### 返回值

```lua
return {
    rewritten_query = "重写后的查询",
    expansion_terms = {"扩展词1", "扩展词2"}
}
```

## Fusion - 融合权重

覆盖混合搜索的融合权重。

### 使用场景

- 动态调整向量/BM25 权重
- 基于查询类型优化权重
- 实现自定义融合策略

### 参数

- `query`: 搜索查询
- `vector_count`: 向量结果数量
- `bm25_count`: BM25 结果数量

### 返回值

```lua
return {
    vector_weight = 0.7,
    bm25_weight = 0.3,
    min_score = 0.1
}
```

## ResultFilter - 结果过滤

过滤或增强搜索结果。

### 使用场景

- 移除不相关结果
- 基于规则的分数增强
- 结果分类和标注

### 参数

- `query`: 搜索查询
- `results`: 结果列表

### 返回值

```lua
return {
    {
        id = "result_id",
        remove = false,  -- 是否移除
        boost = 0.1      -- 分数增强
    }
}
```

## FileFilter - 文件过滤

决定文件是否包含在扫描中。

### 使用场景

- 排除特定目录
- 匹配自定义文件模式
- 基于内容的文件过滤

### 参数

- `file_path`: 文件路径
- `is_directory`: 是否为目录
- `size`: 文件大小（字节）

### 返回值

```lua
-- "include" - 包含文件
-- "exclude" - 排除文件
return "include"
```

## LangHeuristics - 语言启发式

提供语言相关的启发式分类。

### stdlib 分类

```lua
function plugin.classify_stdlib(module_path)
    -- module_path: "os.path", "std::collections::HashMap", etc.
    -- 返回: "Collection", "Io", "Concurrency", "Utility", etc.
    if module_path:match("^os%.") then
        return "Io"
    end
    return nil  -- 不确定时返回 nil
end
```

### 测试文件检测

```lua
function plugin.is_test_file(file_path, content)
    -- 返回 true/false/nil
    if file_path:match("test_") or file_path:match("_test%.") then
        return true
    end
    return nil  -- 不确定时返回 nil
end
```

### 实体类型映射

```lua
function plugin.entity_kind(capture_name)
    -- capture_name: tree-sitter 查询捕获名
    -- 返回: "function", "class", "module", etc.
    if capture_name == "my_custom_capture" then
        return "function"
    end
    return nil
end
```

## SymbolExtract - 符号提取

提取导入和导出符号。

### 导入提取

```lua
function plugin.extract_imports(content, file_path, language)
    return {
        {
            module_path = "module.name",
            names = {"imported_name"},
            alias = "alias_name",
            is_glob = false
        }
    }
end
```

### 导出提取

```lua
function plugin.extract_exports(content, file_path, language)
    return {
        {
            name = "exported_name",
            kind = "function",
            is_default = false
        }
    }
end
```

## RelationExtract - 关系提取

提取符号间的调用和依赖关系。

### 符号提取

```lua
function plugin.extract_symbols(content, file_path, language)
    return {
        {
            name = "symbol_name",
            kind = "function",
            span = {start_line, end_line}
        }
    }
end
```

### 关系提取

```lua
function plugin.extract_relations(content, file_path, language)
    return {
        {
            source_id = "caller_id",
            target_name = "callee_name",
            kind = "calls"
        }
    }
end
```

## LanguageRemap - 语言重映射

将自定义语言映射到宿主内置语法。

### 配置

```lua
plugin = {
    language_name = "mytemplate",
    language_extensions = {"tpl", "template"},
    remap_grammar_language = "JavaScript",  -- 使用 JS 语法解析
    query_schemes = {
        entity = "(template_block) @entity",
        imports = "(import_statement) @import"
    }
}
```

## AstLanguage - 自定义语言 (Native)

为 Native 插件提供自定义 tree-sitter 语法。

### 要求

- 必须是 Native 插件
- 提供 tree-sitter `TSLanguage` 指针
- 提供查询方案

### 导出函数

```c
// 返回 tree-sitter 语言指针
void* cce_plugin_tree_sitter_language();

// 返回查询方案
char* cce_plugin_query_scheme(uint32_t query_type);
```
