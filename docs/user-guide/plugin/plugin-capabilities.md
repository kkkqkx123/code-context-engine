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

## Vocabulary - 领域术语表

提供领域缩写 / 术语的释义，宿主把命中的术语追加为一行 `Glossary: ...`，
同时进入 BM25 与 Embedding 两条检索路径。

### 使用场景

- 媒体服务等领域存在大量缩写（`stts`、`ctts`、`sdp`），释义有助于召回
- 通用引擎不内置任何领域术语，术语知识由插件按项目提供
- 多包结构（monorepo）中同一个词在不同包含义不同，或某些目录根本不需要这批术语

### 返回值

```lua
function plugin.vocabulary()
    return {
        { token = "stts", expansion = "sync time-to-sample" },
        { token = "sdp",  expansion = "session description protocol" }
    }
end
```

条目可以带作用域，把术语限制到一部分文件：

```lua
{ token = "track", expansion = "media track",
  scope = { include = { "packages/player/**" } } }
```

### 宿主行为

- 加法档：所有匹配插件的术语表按 token 与作用域合并去重（首次出现优先）
- 匹配发生在标识符 token 及其分段上：`stts` 命中 `parse_stts`、`stts_box`、`sttsCount`；
  分段边界是真实的标识符边界，因此短术语不会误命中长词内部（`ts` 不命中 `stts`）
- 匹配忽略大小写：索引分词器会把每个 token 小写化，术语匹配与之保持一致，
  因此代码里的 `STTS` 同样命中术语 `stts`，插件不必枚举大小写变体
- 扫描范围：实体名称、签名、文档注释、索引富化阶段的函数体文本
- 每个实体最多一行 `Glossary: stts = sync time-to-sample; sdp = ...`
- 宿主限制条目数量与单条长度，避免索引膨胀：单张插件表最多 1024 条，
  合并所有插件后的术语表最多 4096 条，单条释义最多 200 字符，
  超长释义被截断，缺少 token 或释义的条目被丢弃
- 未安装插件时行为与普通文本生成完全一致

### 作用域

作用域是**条目级**的路径约束，用来在多包结构里区分同名词汇、避免给无关目录转写术语。

| 字段 | 含义 |
|---|---|
| `include` | 命中其一即适用；为空表示不限制 |
| `exclude` | 命中其一即不适用 |
| `priority` | 同名条目之间的显式优先级，越大越优先，默认 0 |

- 作用域按**项目相对路径**匹配，模式语言与插件级 `file_patterns` 完全一致
  （globset 默认语义：无通配符的模式按路径后缀匹配）
- 文件名模式与后缀都是路径 glob 的特例，不需要单独字段：
  `*.c` 匹配任意层级下的 C 文件，`*_box.c` 匹配以 `_box.c` 结尾的文件名，
  `packages/player/**` 匹配该包内的全部文件
- 没有作用域的条目对插件路由到的所有文件生效
- 多个条目共享同一个 token 时，宿主按固定顺序决出胜者：
  **作用域 `priority` 大者 → 命中的作用域更窄者（字面路径段数、再字面字符数）→
  插件优先级高者（同级则先注册者）**
- 作用域读不出来或永远无法满足的条目会被丢弃，而不是退化成"到处适用"：
  配置错误的约束不应让错误的含义进入索引

### 落点

术语行写在**分块**上，而不是实体文本上。分块才是真正被检索的单元：BM25 侧一个分块
就是一个文档，向量侧一个分块就是一个向量。

- 宿主按分块自身文本匹配术语，只有真正用到该术语的分块才会带上释义
- 分组（类、结构体、模块）的成员分块同样带术语行，不会被分组头压缩裁掉
- 一个实体被切成多个分块时，每个分块各自带上它用到的术语
- 文档管线（markdown / json / yaml / xml / 纯文本等）同样按分块追加术语行

导出路径不经过分块，因此导出的实体说明里带的是**实体级**术语行。

### 路由

与其他能力一致，按 `file_patterns` / `languages` / `enabled` 过滤，按优先级排序。
语言名支持别名：`cpp` 与 `c++`、`js` 与 `javascript`、`py` 与 `python` 指向同一语言。

文档没有语言概念，因此文档只按 `file_patterns` 过滤：想覆盖文档的插件不要声明
`languages`，或者声明一个覆盖文档的 glob。

插件级 `file_patterns` 决定"这张表参与哪些文件"，条目级 `scope` 决定"表里的哪些词
参与哪些文件"，两层叠加：前者是粗筛，后者是细选。

术语表按"贡献术语的插件集合 + 注册表修订号"缓存：词汇接口不接收文件参数，
同一插件集合对所有文件产出同一张术语表，因此缓存条目数与文件数无关。
缓存由插件注册表持有，转换器、切分器与文档管线共用一份，一个插件集合只查询一次。
表里一旦出现带作用域的条目，宿主按文件解析出"该文件适用的术语"，该结果按文件缓存。

示例插件见 [`examples/media_vocabulary.lua`](examples/media_vocabulary.lua)，
包含 ISO 基础媒体文件格式的常用缩写。

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
