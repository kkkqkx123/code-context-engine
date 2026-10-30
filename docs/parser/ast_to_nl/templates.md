# AST to Natural Language 模板系统

## 1. 概述

模板系统负责把 `EntityGroup` 转成自然语言描述，BM25 与 Embedding 两条输出路径各有一套模板，共用基类与噪声过滤：

* **BM25 路径**：保留代码符号的混合文本，服务关键词检索。
* **Embedding 路径**：纯语义描述，服务向量检索。

两条路径的模板 trait、分发器和路由条件完全重复，但输出策略不同，因此按「同名不同实现」并列存在，不共享 trait 对象。

## 2. 目录结构

```
common/templates/
├── group_trait_base.rs   # GroupTemplateBase trait（两条路径共用）
└── helpers.rs            # 名称规范化、关键词提取等辅助
common/noise_filter.rs    # NL 文本噪声过滤（两条路径共用）

bm25/templates/
├── group_trait.rs        # GroupTemplate / PatternGroupTemplate → String
├── dispatcher.rs         # 路由与 pattern 命名
├── getter_setter.rs      # getter/setter 组模板
├── regular.rs            # 常规组模板
└── stdlib.rs             # 标准库组模板

embedding/templates/
├── group_trait.rs        # GroupTemplate / PatternGroupTemplate → Vec<String>
├── dispatcher.rs
├── getter_setter.rs
├── regular.rs
└── stdlib.rs
```

设计模式与样板识别不在模板层：`PatternInfo` 目前只有「无模式」与「getter/setter」两个变体，其余模式交由插件系统处理，模板层不设对应文件。

## 3. 核心 trait

`GroupTemplateBase`（`common/templates/group_trait_base.rs`）提供成员角色判断、重要成员过滤与计数，两条路径的实现类都基于它。

BM25 侧 `GroupTemplate::generate` 返回单个字符串；`PatternGroupTemplate::generate_with_pattern` 接收模式摘要，输出带模式信息的文本。

Embedding 侧 `GroupTemplate::generate` 返回 `Vec<String>`：首个元素是组整体描述，其后是各重要成员的独立描述。

## 4. 输出策略差异

| 维度 | BM25 | Embedding |
| --- | --- | --- |
| 原始名称 | 保留，用于精确匹配 | 排除，避免过拟合代码符号 |
| 规范化名称 | 保留，用于模糊匹配 | 仅用于角色判断，不进入文本 |
| 关键词 | 保留 | 不入文本 |
| 参数与返回类型 | 仅在正文中出现 | 只保留数量与语义，不写类型 |
| 文档注释 | 保留原句（去注释符） | 保留，作为核心意图 |
| 多段分隔 | 空格连接 | 句号加空格连接 |
| stdlib 兜底字面量 | `stdlib` | `standard library` |

## 5. 分发逻辑

`GroupTemplateDispatcher::dispatch` 只有三条规则，顺序即优先级：

| 条件 | 选中模板 |
| --- | --- |
| `PatternInfo::GetterSetter(_)` | getter/setter 模板 |
| `PatternInfo::None` 且 `header.is_stdlib` | stdlib 模板 |
| `PatternInfo::None` 且 header 非 stdlib | regular 模板 |

两条需要记住的约束：

* **模式优先于 stdlib**。既是 getter/setter 组、header 又是 stdlib 的组走 getter/setter 模板，stdlib 模板不参与。这是刻意排序：模式信息比「是不是标准库」更能决定文本形态。
* **只看 header**。组内成员是 stdlib、header 不是的组走 regular 模板；成员级 stdlib 标记不参与组级路由。

`get_pattern_name` 与分发保持一致：模式命中返回模式名，无模式时 stdlib 组返回 `Stdlib`，其余返回 `Regular`。日志与统计据此可以区分 stdlib 模板是否命中。

## 6. 插件覆盖

插件可以整体接管模板输出，入口在 `converter/group_converter.rs` 的 `generate_from_plugins`，按 `above → builtin → below` 三级优先级短路：`above` 命中时内置分发器不执行。这是 `dyn CodePlugin` 在生产路径上唯一的入口，约束记录见 `docs/archive/dynamic.md`。

## 7. 单实体兼容路径

`GroupedEntity` 没有语言字段，语言只挂在 `EntityGroup` 上。单实体转换（成员 chunk 与 header brief 回落）通过 `create_standalone_group(entity, language)` 构造临时组，语言由调用方从其所属组带入，因此语言相关的噪声过滤画像与 stdlib 模板门控在成员级仍然生效。

## 8. 噪声过滤

`common/noise_filter.rs` 的 `filter_nl_noise` 被两条路径共同调用：

* 空 markdown 段标记（`# Example.`、`# Safety.` 等代码块被剥离后残留的标题）无条件过滤；
* 语言相关的样板行（C 头文件保护、JS 严格模式指令、Python main 守卫、shebang 等）只在 `NoiseProfile` 对应开关打开时过滤；
* 带实质内容的 `SAFETY:` 段落保留；过滤后为空则回退原文。
