# 标准库检测的收口：`StdlibEntityDetector` / `is_stdlib_name` 的定位与统一入口设计

> 本文只做判断与方案，不含代码改动。所有结论均给出 `file:line` 与可复现的实测输出。
> 配套：`docs/analysis/stdlib-and-template-callers.md`（现状盘点）、`docs/plan/stdlib-and-template-refactor.md`（上一轮七阶段改造）。

---

## 0. 结论摘要

1. **`StdlibEntityDetector` / `is_stdlib_name` 目前是死代码**。它只被 `grouper.rs:70`、`recognizers.rs:16` 再导出，以及 `recognizers/tests.rs` 使用；检测部分 100% 转发给 `cce_stdlib::StdlibDetector::category`（`recognizers/stdlib.rs:103`），自身净增量只有 `full_name` 的字符串拼接和一句 `"Standard library {label}: {name}"`。
2. **"关系解析时排除 stdlib" 这件事已经做了一半**：链路是 `stdlib_category`（提取期）→ `filter_stdlib_calls`（默认 `true`）→ 丢弃边。但它只覆盖**调用**关系，且只对**未解析**（`is_external`）的目标生效（`resolver.rs:443-482`）。
3. **缺口在非调用关系**：`Use` / `TypeReference` / `Inheritance` / `ImportStandard` 等由 `dependency_extractor.rs:15-48` 构造，**从不写 `stdlib_category`**。实测 `use std::collections::HashMap`、`Vec<T>` 字段类型产出的边 stdlib 一律为空。
4. **`mark_stdlib` 打在了错误的对象上**：它对**用户自己定义的**实体也按名字查表（`entity_extractor.rs:619` 对所有 `entity.*` 捕获调用）。实测 `pub struct Command` 被判为 `is_stdlib=true / Collection`，NL 输出从 139 字符缩成 72 字符，字段与类型全丢。定义实体在语义上不可能是 stdlib（除非正在解析标准库源码本身）。
5. **前缀判断与检测表不该并存**，而且现有前缀规则已被实测证伪三处：Dart `dart:async`、C# `using System;`、Ruby `require 'json'` 全判 `false`；另外 `ImportClassifier` 对 PHP/Ruby 复用 `is_stdlib_call`（调用表）去判模块路径，而两门语言各自的 `is_stdlib_path` / `is_stdlib_require` 正躺在 `cce-stdlib` 里无人调用。

---

## 1. 事实基线：同一件事有七个判定面

| # | 位置 | 判定对象 | 依据 |
|---|---|---|---|
| 1 | `cce-stdlib/src/*.rs` | 符号名 / 类型名 / 模块路径 | 唯一权威表 |
| 2 | `symbol_extractor/common/classifier.rs:93-141` | **导入路径** | 4 类前缀 + 3 张表 + 2 处调错函数 |
| 3 | `post_processing/stdlib_classifier.rs:33-95` | **实体**（含用户定义） | 按 `EntityKind` 选不同表 |
| 4 | `relation_extractor/call_extractor.rs:69-91` | **调用关系** | `is_stdlib_by_type` + 插件启发式 |
| 5 | `codegraph/index/resolver/external.rs:255-291` | **未解析目标的兜底分类** | `java.` / `javax.` / `std::` / `core::` / `alloc::` 前缀 |
| 6 | `symbol_extractor/ruby.rs:31-33`、`csharp.rs:73-76`、`rust.rs:67` | 各语言提取器自带判断 | 直接调 `is_core_module` / `is_dotnet_namespace` / `is_stdlib_path` |
| 7 | `ast_to_nl/converter/index_enrichment/calls_prune.rs:200,208` | NL 文本里的调用冗余 | 直接调 `is_stdlib_call` |

再加两个开关：索引期 `filter_stdlib_calls`（`cce-config/src/modules/relation.rs:52,61`，默认 `true`）、查询期 `filter_stdlib`（`search.rs:736,776`，默认 `true`，消费点 `annotator.rs:351`）。

七个面里，只有 #1 是权威表；#2/#5 是纯前缀；#3/#4 各自选表；#6 绕开 #2；#7 只看名字。这就是"同一份知识写七遍"的实际形态。

## 2. 标记的流向：谁在真正消费 `is_stdlib` / `stdlib_category`

| 消费方 | 位置 | 作用 |
|---|---|---|
| 分组插件 | `grouper/plugin_grouping.rs:276-285` | stdlib 实体不进分组 |
| 调用合并 | `grouper/processors/call_merger.rs:234-274,444` | stdlib 调用压成一行摘要 |
| 模式识别 | `grouper/processors/class_method/pattern_detection.rs:53` | stdlib 方法不参与 getter/setter 识别 |
| NL 分发 | `bm25/templates/dispatcher.rs:45`、`embedding/templates/dispatcher.rs:62` | 命中即走 `StdlibTemplate` |
| NL 成员 | `converter/group_converter.rs:945-947`、`bm25/templates/regular.rs:136,157` | stdlib 成员直接跳过 |
| 索引解析 | `codegraph/index/resolver.rs:299` | `stdlib_category.is_some()` 决定后续是丢弃还是留成 `StandardLibrary` 外链 |
| 查询扩展 | `orchestrator/query/annotation/annotator.rs:351` | `filter_stdlib` 时剔除扩展单元 |

关键点：**除 #4 之外的全部消费方读的都是实体上的 `is_stdlib`**，而实体标记来自 #3 的名字匹配。也就是说，"是否 stdlib"的最终裁判权落在了一个会误伤用户定义的判定上。

## 3. `StdlibEntityDetector` / `is_stdlib_name` 现在做什么

```rust
// recognizers/stdlib.rs:71-95
pub fn detect_stdlib_entity(&self, entity: &Entity, language: &Language) -> Option<StdlibEntityInfo> {
    let category = self.get_stdlib_category(&entity.name, language)?;   // 纯转发
    let full_name = match effective_language(language) {
        Language::Rust => format!("std::{}", entity.name),              // 拼字符串
        _ => entity.name.clone(),
    };
    Some(StdlibEntityInfo::new(entity.id, full_name, entity.name.clone(), category))
}

pub fn is_stdlib_name(&self, name: &str, language: &Language) -> bool {
    if self.get_stdlib_category(name, language).is_some() { return true; }
    cce_stdlib::StdlibDetector::is_stdlib_call(name, language)           // 名字并集
}
```

- 净增量：`StdlibEntityInfo { entity_id, full_name, short_name, category, semantic_description }`（`recognizers/stdlib.rs:14-25`）。
- `full_name` 的 `format!("std::{}")` 是错的：`Vec` → `std::Vec`（真名 `std::vec::Vec`），`HashMap` → `std::HashMap`。这大概也是它没人用的原因之一。
- 与之同名的还有两个语言级 `is_stdlib_name`（`cce-stdlib/python.rs:236`、`rust.rs:611`），同样是零调用。三个同名函数、三种语义、全部死代码。
- 它唯一有价值但没被使用的东西是 `semantic_description`：`"Standard library collection type: Vec"`（`stdlib_category.rs:52-65` 提供 `description_label`）。这正好是"NL 里把不清晰的类型改写成人话"需要的那块砖。

## 4. 关系侧：已经生效的排除链路

```
call_extractor.rs:69   classify_stdlib_category(callee, relation_type, language, heuristics)
        ↓              （is_stdlib_by_type + 插件 heuristics）
call_extractor.rs:79/90  Relation::with_stdlib_category(...)
        ↓
ParsedFile.raw_relations[].stdlib_category
        ↓
resolver.rs:299        let is_stdlib = raw_data.stdlib_category.is_some();
resolver.rs:443        let is_external = callee_id.is_none();
resolver.rs:466-482    if is_external { if is_stdlib { if filter_stdlib_calls { return None } } }
```

三个限定条件缺一不可：**调用关系** / **未解析** / **开关打开**。第三条是配置，默认开；前两条是硬编码。

设计上"解析到项目内实体的同名调用保留"是对的（`resolver.rs:464-465` 注释也这么说），否则自己写的 `fn println` 会被吞掉。

## 5. 关系侧：三个实测缺口

### 5.1 非调用关系完全没有标注

`dependency_extractor.rs:26-47` 构造 `Use` / `TypeReference` / `Inheritance` / `ImportStandard` 时没有任何 `with_stdlib_category`。全仓只有两处调用 `with_stdlib_category`，都在 `call_extractor.rs`。

实测（Rust 片段，`ParseCoordinator::parse` 直出）：

```
DirectCall           HashMap::new                 stdlib_category=Some(Collection)
DirectCall           String::new                  stdlib_category=Some(String)
Use                  std::collections::HashMap    stdlib_category=None      ← 应排除
TypeReference        std::collections             stdlib_category=None      ← 应排除
TypeReference        String                      stdlib_category=None      ← 应排除
TypeReference        HashMap::new                 stdlib_category=None      ← 应排除
ImplAssociation      Command                     stdlib_category=None
```

后果：`Vec<T>`、`HashMap<K,V>`、`Result<T, E>` 这类**类型引用**变成永久 unresolved 边，`filter_stdlib_calls` 对它们完全无效。

### 5.2 命名约定兜底绕过了开关

`external.rs:263-275`：`java.` / `javax.` / `std::` / `core::` / `alloc::` 前缀命中后直接返回 `ExternalCallType::standard_library`，**不看 `filter_stdlib_calls`**。所以同样是 stdlib：`Vec::new()` 的调用边被丢（提取期标了），`std::collections::HashMap` 的类型引用边被留下（兜底期标的），`String` 的边变成 `Unknown`（没人标）。三条边、三种命运。

这也意味着 `stdlib_category` 与 `external_type=StandardLibrary` 是两个平行的"stdlib 真相"，只有前者受开关管辖。

### 5.3 缺少"与关系类型无关"的名字级判定

调用域有 `is_stdlib_by_type`，非调用域什么都不该套关系类型——现有的 `is_stdlib_name` 恰是这个形状，却被放在了 grouper 里且没人调。它缺的是**路径形态**：`os`、`fmt`、`json`、`dart:async`、`System` 都不是"符号名"，而是模块/包路径，`is_stdlib_name` 一条都判不出来。

## 6. 实体侧：`mark_stdlib` 打击面过大

`entity_extractor.rs:619` 对**每个**从 `entity.*` 捕获产出的实体调用 `mark_stdlib`，`stdlib_classifier.rs:56-78` 按 `EntityKind` 选表。而 `entity.*` 捕获产出的就是**本文件的定义**。

实测（`pub struct Command` + `enum Error` + `use std::collections::HashMap`）：

```
Import   std::collections::HashMap  is_stdlib=true   category=Some(Other)       ← 正确
Struct   Command                    is_stdlib=true   category=Some(Collection)  ← 误判
Enum     Error                      is_stdlib=false  category=None              ← 侥幸（Enum 走 call 表）
```

名字撞表的密度并不低：`STDLIB_TYPES`（`rust.rs:11-207`）里有 `Command`、`Error`、`Path`、`Result`、`String`、`File`、`Metadata`；JS 表里有 `Error`、`String`、`Buffer`。实测：

```
   Command: rust type=true   Path=true   Result=true   String=true
     Error: rust type=true   js type=true
    Buffer: js type=true
```

传播链（`GroupedEntity::from_entity` 原样拷贝，`grouped.rs:102`）：

| 层 | 后果 |
|---|---|
| 分组 | `plugin_grouping.rs:276` 当 stdlib 处理 |
| NL 分发 | `dispatcher.rs:45` 命中 `StdlibTemplate` |
| NL 成员 | `group_converter.rs:945`、`regular.rs:136` 跳过成员 |
| 模式 | `pattern_detection.rs:53` 不识别 getter/setter |

实测同一份源码两种输出（Struct `Command` + 2 字段 + 1 方法）：

```
as parsed  -> "collections Command A CLI command wrapper defined by the project itself."   (72 字符)
if fixed   -> "Command ... Members: name verbose parse. Types: &[String] Result<Command, std::io::Error>."  (139 字符)
```

丢的不是措辞，是**字段名、方法名、参数与返回类型**——恰是检索最需要的判别信息。

结论：名字判定对"引用"成立，对"定义"不成立。`mark_stdlib` 现在把两件事混在一个开关里。

## 7. 应该在哪里使用（三个落点）

### 落点 A：关系提取的非调用分支（补标注，不改判定）

在 `dependency_extractor.rs:26-47` 的两条构造路径上，用"目标是否 stdlib + 类别"补 `stdlib_category`：

```rust
let stdlib_category = StdlibDetector::category_or_ref(&dst_name, language); // 新增：见 §9
Some(Relation::file_relation(caller_id, RelationTarget::unresolved(dst_name),
                             relation_type, span)
        .with_stdlib_category(stdlib_category))
```

补上之后 `resolver.rs:466` 的既有逻辑原样生效：`filter_stdlib_calls=true` 丢边，`false` 留成 `StandardLibrary` 外链。**不需要动 resolver**，也不需要新开关。

注意 `ImportStandard` 的目标是**路径**（`std::collections::HashMap`、`os.path`、`fmt`），判定要用路径形态（§9 的 `is_stdlib_import_path`），不能只查符号表。

### 落点 B：实体标记按"是否为定义"分流（收窄，不扩面）

`mark_stdlib` 应只在**实体本身不是项目定义**时按名字判：

- `EntityKind::Import | Require | Include | Export`（`kind.rs:233,381`）→ 这些是"引用别人"，名字判定成立，继续判。
- 插件元数据 `metadata["is_stdlib"]`（`stdlib_classifier.rs:38-48`）→ 保持最高优先级。
- 其余（`Struct` / `Class` / `Function` / `Method` / `Trait` / `Constant` / …）→ 它们是**本文件的定义**，默认 `false`；只有"当前文件属于标准库源码/`vendor` 且被显式配置"时才允许按名字判。

这一步会立刻修掉 `struct Command`、`enum Error`、`class String` 这类误判，且不影响正确路径（导入实体照旧标 stdlib，NL 照旧压缩）。

风险：`grouper/processors/class_method/pattern_detection.rs:53` 依赖"stdlib 方法不参与 getter/setter 识别"，该逻辑只在解析 stdlib 源码时才有意义，收窄后行为不变；但如果有人靠这个开关避免 `impl` 里的 `get_x` 被误判成 getter，需要回归验证（见 §11 Q3）。

### 落点 C：NL 的类型改写（新增能力，`StdlibEntityDetector` 的正当归属）

单一收敛点已经存在：`TemplateHelpers::build_signature_with_defaults`（`common/templates/helpers.rs:32-59`），所有参数类型与返回类型都从这里拼进 NL 文本（`embedding/templates/regular.rs:214,560`）。BM25 侧的类型列表在 `bm25/templates/regular.rs:157-181`。

在这两个位置把不可读的类型替换成短语，键用 `(Language, stdlib 类型名)`：

```
Rust   Vec<T>        → "list of T"          HashMap<K,V> → "map from K to V"
       Box<T>/Arc<T> → "shared T"           Option<T>    → "optional T"
       Result<T,E>   → "T or E error"
Go     []T / map[K]V / chan T
C++    std::vector<T> / std::unique_ptr<T>
C#     IEnumerable<T> / Task<T>
JS/TS  Array<T> / Promise<T>
```

`StdlibCategory::description_label`（`stdlib_category.rs:52-65`）可作为兜底词表，但**类别粒度不够**：`Vec` 和 `HashMap` 都是 `Collection`，"集合类型的 Vec" 对检索没有增益。真正需要的是逐类型的短句表——这张表应该和现有表放在一起（`cce-stdlib`），而不是新开一层。

`StdlibEntityInfo.semantic_description` 可以直接复用为输出格式，但内容要从"Standard library collection: Vec"换成"list of T"，并且只在**生成侧**用，不要回写到实体。

## 8. 前缀判断与检测表是否应该统一

**应该，而且是 P0。** 现在的分工是"谁先碰到谁自己判"，导致同一门语言在导入期和调用期给出不同答案。逐语言对照：

| 语言 | 导入期（classifier.rs） | 库内既有能力 | 是否一致 | 实测反例 |
|---|---|---|---|---|
| Python | `is_stdlib_module` | 表 | ✅ | — |
| Rust | `STDLIB_CRATES` 前缀 | 表 | ✅ | — |
| Go | `is_stdlib_package` | 表 | ✅ | — |
| JS/TS | `is_node_stdlib` | 表 | ✅ | — |
| Java | `java.` / `javax.` / `jdk.` 前缀 | 无表 | ⚠️ 只靠前缀 | — |
| Kotlin/Scala | `java.`/`javax.`/`kotlin.`/`scala.` 前缀 | 有 `is_kotlin_package`/scala 表 | ❌ 双份 | — |
| C# | `System.` / `Microsoft.` 前缀 | `is_dotnet_namespace` / `is_dotnet_path` | ❌ | `using System;` → 前缀 `false`，表 `true` |
| Dart | `dart.` 前缀 | `is_dart_path` | ❌ | `dart:async` → 前缀 `false`，表 `true` |
| Ruby | `is_stdlib_call`（调用表） | `is_stdlib_require` / `is_stdlib_path` | ❌ 调错函数 | `require 'json'` → `false`/`true` |
| PHP | `is_stdlib_call`（调用表） | 无路径表 | ⚠️ 语义错配 | `require 'vendor/autoload.php'` → 外链 |
| C/C++ | 交给 `is_system_header` | 由解析器给 | ✅（但需保留特例） | — |
| Bash/Lua | 无 | 有表 | ❌ 缺口 | `source`/`require` 恒为外链 |

实测输出（复刻 classifier 分支，`cce-stdlib` 公开函数对照）：

```
csharp System                           is_dotnet_namespace=true   prefix=false
csharp Microsoft                        is_dotnet_namespace=true   prefix=false
dart   dart:async                       is_dart_path=true          prefix=false
dart   package:flutter/material.dart    is_dart_path=true          prefix=false
ruby   json                             is_stdlib_require=true     is_stdlib_call=false
```

`is_stdlib_path`（`ruby.rs:962`）、`is_stdlib_require`（`ruby.rs:956` 宏生成）、`is_dart_path`（`dart.rs:967`）目前只有 `cce-stdlib` 自己的测试在调——现成的正确实现，配上错误的调用点。

统一的分界线应该是：**表负责"这个名字/路径属于标准库"，前缀只作为表内的数据而非独立逻辑**。Java 这类纯前缀语义（没有"模块清单"这回事）反过来应该把前缀收进 `cce-stdlib`，暴露成 `is_stdlib_import_path`，而不是留在 `classifier.rs` 里。

## 9. 建议的统一入口（都在 `cce-stdlib`）

```rust
impl StdlibDetector {
    // 已有（上一轮新增）：符号 → 类别
    pub fn category(name: &str, language: &Language) -> Option<StdlibCategory>;

    // 新增 1：符号名或模块路径的并集判定，替代 grouper 里那个 is_stdlib_name
    //   覆盖 category(name) ∪ is_stdlib_call(name) ∪ is_stdlib_module/package/crate/require(dart path)
    pub fn is_stdlib_ref(name_or_path: &str, language: &Language) -> bool;

    // 新增 2：导入路径专用（不带关系类型，C/C++ 的系统头由调用方显式传入）
    pub fn is_stdlib_import_path(path: &str, language: &Language, is_system_header: bool) -> bool;

    // 新增 3：NL 改写用（§7 落点 C），返回 None 表示"不是 stdlib 或没有可读短语"
    pub fn describe_type(name: &str, language: &Language) -> Option<&'static str>;
}
```

调用方随之收敛：

| 调用方 | 改成 |
|---|---|
| `stdlib_classifier.rs:33` | `is_stdlib_ref`（仅 Import 类实体 + 插件元数据） |
| `dependency_extractor.rs:26-47` | `category_or_ref` / `is_stdlib_import_path` |
| `classifier.rs:93-141` | 整段 `match` 换成 `is_stdlib_import_path` |
| `external.rs:255-291` | 删掉前缀表，换 `is_stdlib_ref`（或整体并入提取期标注，见 §11 Q1） |
| `calls_prune.rs:200,208` | 已经是 `is_stdlib_call`，保留（调用域语义正确） |
| `StdlibEntityDetector` | 只保留 `describe` / `semantic_description` 的封装，`detect_stdlib_entity` 删除 |

`StdlibEntityDetector` 的最终形态建议：一个**纯展示层适配器**（`(name, category) -> 短语`），或者干脆删掉、由 `StdlibDetector::describe_type` + `StdlibCategory::description_label` 取代。它现在既不是检测器（检测全转发），也不是描述器（描述没人用），留着只会让下一个人再写第八个判定面。

## 10. 落地顺序

| 阶段 | 内容 | 影响面 | 风险 |
|---|---|---|---|
| P0 | `cce-stdlib` 加 `is_stdlib_ref` / `is_stdlib_import_path` / `describe_type`；`classifier.rs` 换实现 | 3 个文件，行为修正集中在 C#/Dart/Ruby | 低：只影响导入分类，有单测锚点 |
| P1 | `mark_stdlib` 收窄到 Import 类实体（落点 B） | 1 个文件 + 若干 NL 快照 | 中：会改变已有索引里 `is_stdlib` 的分布，需回归 NL 质量 |
| P2 | `dependency_extractor` 补 `stdlib_category`（落点 A） | 1 个文件，索引体积下降 | 中：`filter_stdlib_calls=true` 时类型引用边会消失，查询侧可能少召回 |
| P3 | NL 类型改写（落点 C） | `helpers.rs` + `bm25/regular.rs` | 低：纯生成侧，可回滚 |
| P4 | 删 `StdlibEntityDetector` / 两个语言级 `is_stdlib_name` | 4 个文件 | 低：全死代码 |

P2 与 P1 的顺序不能反：先收窄实体标记（P1）再扩大关系标注（P2），否则"用户定义的 `Command`"会同时被 NL 压缩和关系丢弃，问题更难定位。

## 11. 待拍板

| 编号 | 问题 | 建议 |
|---|---|---|
| Q1 | `external.rs:263-275` 的前缀兜底是删掉（改为提取期标注 + 无兜底），还是保留但补 `filter_stdlib_calls` 判断？ | 删掉前缀，保留兜底入口但只判 `stdlib_category`；理由是 §5.2 的双真相 |
| Q2 | 非调用关系的 stdlib 边，默认丢还是留？ | 跟随 `filter_stdlib_calls` 现有语义（默认丢），但把 `Use`/`TypeReference` 的丢失计数单列一个 metric，便于确认召回变化 |
| Q3 | `pattern_detection.rs:53` 依赖 stdlib 实体标记来跳过 getter/setter 识别，收窄后是否需要单独的"stdlib 源码目录"配置？ | 先收窄并回归；若确有业务依赖，再加 `index.stdlib_source_globs` |
| Q4 | NL 类型改写要不要保真原文？ | 保留原文作为主句、短语作为补充（`Vec<T>` 同时出现），避免损失精确检索；只对嵌套 ≥2 层的泛型做替换 |

## 12. 复现

```shell
# 1) 探针 crate（/tmp/probe，path 依赖 cce-stdlib + cce-types + cce-parser）
#    复用仓库 target 目录，避免重复编译
CARGO_TARGET_DIR=/workspace/code-context-engine/target cargo run -q

# 2) 实体误判 + 关系漏标：解析 Rust 片段并打印
#    ParseCoordinator::new().parse("src/demo.rs", src)
#    → Struct Command is_stdlib=true / Collection
#    → Use/TypeReference of std::collections::HashMap 的 stdlib_category=None

# 3) NL 差异：GroupedEntity::from_entity + GroupTemplateDispatcher::dispatch
#    → "collections Command ..." (72)  vs  "Command ... Members: ..." (139)

# 4) 前缀 vs 表：
#    CSharpStdlibDetector::is_dotnet_namespace("System") == true, starts_with("System.") == false
#    DartStdlibDetector::is_dart_path("dart:async")     == true, starts_with("dart.")   == false
#    RubyStdlibDetector::is_stdlib_require("json")      == true, is_stdlib_call("json") == false
```

---

## 附：本轮代码改动的 patch

见 `/workspace/stdlib-template-refactor.patch`（41 文件，含 1 处重命名与 3 个新文档；`git apply` 校验通过，应用后与工作区逐字节一致）。
