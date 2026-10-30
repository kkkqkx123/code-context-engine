# 标准库检测的收口：单一判定入口与三处传播缺口

> 本文记录"标准库判定应该收口到哪、判定结果应该传到哪"的完整判断与落地结果。所有结论给出 `file:line`；行号以本轮改造后的代码为准，改造前的行号只在与历史结论对照时出现。
> 配套：`docs/analysis/stdlib-and-template-callers.md`（改造前的现状盘点）、`docs/plan/stdlib-and-template-refactor.md`（上一轮七阶段改造）、`docs/analysis/stdlib-nl-type-rewriting.md`（下一阶段：NL 类型改写）。

---

## 0. 结论摘要

1. **判定收口到两个入口，都在 `cce-stdlib`**：`is_stdlib_import_path`（导入路径）与 `is_stdlib_ref` / `ref_category`（引用类目标的并集 + 类别）。原来散在六个位置的本地前缀判断全部删除，调用方从七个降到零个前缀表。
2. **`mark_stdlib` 收窄到引用类实体**（`Import` / `Require` / `Include`）。定义类实体不再按名字查表——`struct Command` 与 `std::process::Command` 同名，但前者是项目代码。实测该项目定义的 `is_stdlib` 由 `true` 变 `false`，NL 文本由 72 字恢复到 136 字，字段/方法/参数/返回类型全部回来。
3. **非调用关系补上了 `stdlib_category`**：`dependency_extractor` 的两条构造路径都标注目标类别，`Use` / `TypeReference` 边从此受 `filter_stdlib_calls` 管辖（此前永久逃逸为 unresolved 边）。
4. **`external.rs` 的命名约定兜底整段删除**，不做替代。stdlib 身份只剩一个来源：提取期的 `stdlib_category`。
5. **`StdlibEntityDetector` / `StdlibEntityInfo` / 三个同名 `is_stdlib_name` 全部删除**——检测部分 100% 转发，净增量只有一句错拼的 `std::` 前缀和一个没人用的描述串。
6. **`pattern_detection` 不再读 `is_stdlib`**。是否为访问器由名字、后备字段与同类方法对决定，与名字来自哪个库无关。
7. **NL 类型改写不在本轮**，只出分析文档（§11），因为它是检索行为变更，需要单独放量。

## 1. 事实基线：同一件事曾散在七个判定面

| # | 位置 | 判定对象 | 本轮处置 |
|---|---|---|---|
| 1 | `cce-stdlib/src/*.rs` | 符号名 / 类型名 / 模块路径 | 唯一权威表，保留 |
| 2 | `symbol_extractor/common/classifier.rs:97` | 导入路径 | 近 50 行前缀 `match` → 一行委托 |
| 3 | `post_processing/stdlib_classifier.rs:52` `mark_stdlib` | 实体 | 收窄到引用类实体（§6） |
| 4 | `relation_extractor/call_extractor.rs:393` `classify_stdlib_category` | 调用关系 | 上一轮已收口到 `category`，本轮不动 |
| 5 | `index/resolver/external.rs` `try_classify_from_naming_convention` | 未解析目标兜底 | **整段删除**（§5.2） |
| 6 | 七个语言提取器各自的前缀/子集判断 | 导入路径 | 全部改为委托入口 2（§8） |
| 7 | `ast_to_nl/converter/index_enrichment/calls_prune.rs:200,208` | NL 文本里的调用冗余 | 保留（调用域语义正确） |

两个开关不变：索引期 `filter_stdlib_calls`（`cce-config/src/modules/relation.rs:52,61`，默认 `true`）、查询期 `filter_stdlib`（消费点 `annotator.rs:351`，默认 `true`）。

改造前的形态是"同一份知识写七遍"：`#1` 是表，`#2`/`#5` 是纯前缀，`#3`/`#4` 各自选表，`#6` 绕开 `#2`，`#7` 只看名字。

## 2. 标记的流向：谁在消费 `is_stdlib` / `stdlib_category`

| 消费方 | 位置 | 作用 |
|---|---|---|
| 分组插件 | `grouper/plugin_grouping.rs:276-285` | stdlib 实体不进分组 |
| 调用合并 | `grouper/processors/call_merger.rs:234-274,444` | stdlib 调用压成一行摘要 |
| NL 分发 | `bm25/templates/dispatcher.rs:45,62`、`embedding/templates/dispatcher.rs:62` | 命中即走 `StdlibTemplate` |
| NL 成员 | `converter/group_converter.rs:945`、`bm25/templates/regular.rs:136,157` | stdlib 成员直接跳过 |
| 索引解析 | `index/resolver.rs:299` | `stdlib_category.is_some()` 决定丢弃还是留成外链 |
| 查询扩展 | `query/annotation/annotator.rs:351` | `filter_stdlib` 时剔除扩展单元 |

关键点：**除 `#4` 之外的全部消费方读的都是实体上的 `is_stdlib`**，而实体标记曾来自一个会误伤用户定义的判定。收窄之后，这条链上的"误伤"入口关掉了。

`pattern_detection.rs:53` 曾在这张表里，本轮移除（§9）。

## 3. 删掉的三样东西

| 对象 | 原位置 | 为什么删 |
|---|---|---|
| `StdlibEntityDetector` / `StdlibEntityInfo` | `grouper/recognizers/stdlib.rs`（整文件） | `detect_stdlib_entity` 100% 转发 `category`；`full_name` 错拼成 `std::HashMap`（真名 `std::collections::HashMap`）；`semantic_description` 无人调用 |
| `PythonStdlibDetector::is_stdlib_name` | `cce-stdlib/src/python.rs` | 零生产调用，且与 `grouper` 的同名函数语义不同 |
| `RustStdlibDetector::is_stdlib_name` | `cce-stdlib/src/rust.rs` | 同上 |

三个同名函数、三种语义、全部死代码。若将来需要"名字并集判定"，用 `StdlibDetector::is_stdlib_ref`，别再另起一个同名函数。

## 4. 关系侧：排除链路（改造后的完整形态）

```
dependency_extractor.rs:24   process_dependency_match(..., language)
        ↓                     Use / TypeReference / Inheritance / ImportStandard
dependency_extractor.rs:46,62  .with_stdlib_category(ref_category(dst, language))
        ↓
ParsedFile.raw_relations[].stdlib_category
        ↓
resolver.rs:299        let is_stdlib = raw_data.stdlib_category.is_some();
resolver.rs:443        let is_external = callee_id.is_none();
resolver.rs:466-488    if is_external { if is_stdlib { if filter_stdlib_calls { return None } } }
```

三个限定条件缺一不可：**未解析** / **标了 stdlib** / **开关打开**。第三条是配置（默认开），前两条是硬编码。

"解析到项目内实体的同名目标保留"这条设计不变（`resolver.rs:464-465` 注释），否则自己写的 `fn println` 会被吞掉——实测 `ImplAssociation Command` 的目标是项目内定义，`stdlib_category=None`，边保留。

## 5. 关系侧：三个缺口与本轮修法

### 5.1 非调用关系没有标注（已修）

改造前，`dependency_extractor` 构造 `Use` / `TypeReference` / `Inheritance` 时从不写 `stdlib_category`，全仓只有 `call_extractor` 调 `with_stdlib_category`。实测（Rust 片段）：

```
改造前                                    改造后
Use         std::collections::HashMap  None  →  Some(Collection)
TypeReference std::collections         None  →  Some(Other)
TypeReference String                   None  →  Some(String)
TypeReference std::io::Error           None  →  Some(Error)
```

修法：`process_dependency_match` 增加 `language` 参数（`relation_extractor.rs:209` 传入），两条构造路径统一走 `StdlibDetector::ref_category`。**不动 resolver，不加新开关**。

### 5.2 命名约定兜底绕过开关（已删）

`external.rs` 原来的 `try_classify_from_naming_convention` 在 `java.` / `javax.` / `std::` / `core::` / `alloc::` 前缀命中后直接返回 `standard_library`，**不看 `filter_stdlib_calls`**。后果是同一个 stdlib 出现三种命运：`Vec::new()` 的调用边被丢（提取期标了）、`std::collections::HashMap` 的类型引用边被留（兜底期标了）、`String` 的边变 `Unknown`（没人标）。

处置：整段删除，**不保留兜底入口**。理由是两个真相并行本身就是缺陷：兜底判出来的 stdlib 不受配置管辖，用户关掉 `filter_stdlib_calls` 也拿它没办法。删掉之后，未被提取期识别的目标一律 `Unknown`——要修就修提取期的表，而不是在 resolver 里加第二套规则。

### 5.3 缺少"与关系类型无关"的名字级判定（已补）

调用域有 `is_stdlib_by_type`，非调用域不该套关系类型。新增 `is_stdlib_ref` 填这个形状，并补上**路径形态**（`os`、`fmt`、`json`、`dart:async`、`System` 都不是"符号名"，而是模块/包路径）。

## 6. 实体侧：`mark_stdlib` 收窄到引用

`entity_extractor.rs:619` 对每个 `entity.*` 捕获产出的实体调用 `mark_stdlib`，而这些捕获产出的就是**本文件的定义**。改造前按名字查表，结果：

```
改造前：Import  std::collections::HashMap  is_stdlib=true   category=Some(Other)
        Struct  Command                    is_stdlib=true   category=Some(Collection)   ← 误判
改造后：Import  std::collections::HashMap  is_stdlib=true   category=Some(Collection)
        Struct  Command                    is_stdlib=false  category=None
```

撞表密度不低：Rust 表里有 `Command`、`Error`、`Path`、`Result`、`String`、`File`、`Metadata`；JS 表里有 `Error`、`String`、`Buffer`。

传播链（`GroupedEntity::from_entity` 原样拷贝）导致同一份源码两种 NL 输出：

```
改造前 -> "collections Command A CLI command wrapper..."                    (72 字符)
改造后 -> "Command\nA CLI command wrapper...\nMembers: name tags parse.\n
          Types: &[String] Result<Command, std::io::Error>."               (136 字符)
```

丢的不是措辞，是**字段名、方法名、参数与返回类型**——恰是检索最需要的判别信息。

新规则（`stdlib_classifier.rs:33-52`）：

1. 插件元数据 `metadata["is_stdlib"]` 最高优先级，任何 kind 都生效；
2. 其余只对 `Import` / `Require` / `Include` 查表；
3. 类别由 `ref_category` 一次给出（整路径 → 末段 → `Other`）。

`Export` 故意不在引用类里：`export function foo()` 是定义，只有 re-export 才是引用，两者在 `EntityKind::Export` 上无法区分。

### 6.1 副作用：组级 `is_stdlib` 只剩插件路径

收窄的直接后果要写清楚，否则会误判 nl 模板的命中率。实测（同一探针，`use std::collections::HashMap; use std::io::Read;` 单独成文件）：

```
2 entities -> 0 groups
Import std::collections::HashMap is_stdlib=true category=Some(Collection)
Import std::io::Read            is_stdlib=true category=Some(Io)
```

实体层标注是对的，但纯 import 组会在分组末尾被 `drop_import_only_groups` 删掉（`grouper/builtin_stages.rs:152-170`：header 与全部成员都 `is_import_like()` 即丢弃），所以**内置路径下不存在 `header.is_stdlib == true` 的组**。NL 的 stdlib 模板因此只在插件用 `metadata["is_stdlib"] = "true"` 标注定义类实体时才触发。也就是说：收窄修好了「项目定义被误压缩」，同时让 stdlib 模板从"主要服务内置 import"退回"只服务插件声明"。前者是目标，后者是可接受的连带结果，但意味着不能再拿 stdlib 模板命中率当内置检测的健康度指标。

### 6.2 后续复核：这一刀留下的是存储层问题（已在后续轮次修掉）

> 本节记录的是复核时的问题；修法见 `docs/plan/import-classification-and-stdlib-semantics.md`：`is_stdlib` 收归插件专用、`mark_stdlib` 更名 `mark_plugin_declared_external`、归一化进检测器、`ImportClassifier` 正式集成。下文保留原分析。

收窄改的是判定入口（按 import 形状查表），没改存储位置（仍写 `Entity.is_stdlib`）。`is_stdlib` 这个名字承诺"实体本身是 stdlib"，而 import 实体是**本项目语句**，"外部"的是它指向的目标。四点后果已实测：

1. 同一个布尔在 `Import` 上表示"指向 stdlib"、在插件定义上表示"本身不是项目代码"，消费方只能靠 `|| is_import_like(kind)` 绕开（`bm25/templates/regular.rs:136`）。
2. 内置路径上完全空转：import 不进组、不做成员，字段只落进 DB 与插件 API。
3. 覆盖远窄于函数文档：`Require`（Ruby/Lua 不产实体）、`Include`（C/C++ 恒 false）基本是死分支；JS/TS/Dart 因实体名带引号恒为 false；C#/Go 不产 import 实体。
4. 与关系层分叉：实体层用 `is_stdlib_import_path`、关系层用 `ref_category`，同一条 `from collections import OrderedDict` 得到 false 与 `Some(Other)` 两个答案。

完整分析与正确形态（三个事实、三个归属地、Q1–Q6 待拍板）见 `docs/analysis/mark-stdlib-semantics.md`。

## 7. 前缀判断与检测表：统一（原 Q3）

**应该统一，已统一**。原来"谁先碰到谁自己判"，同一门语言在导入期和调用期给出不同答案。三处实测证伪：

```
csharp System                is_dotnet_namespace=true            prefix("System.")=false
dart   dart:async            is_dart_path=true                   prefix("dart.")=false
ruby   json                  is_stdlib_require=true              is_stdlib_call=false
```

逐语言处置：

| 语言 | 改造前（导入期） | 改造后（表） |
|---|---|---|
| Python | `is_stdlib_module(首段)` | 同（保留首段切分） |
| Rust | `STDLIB_CRATES` 前缀 | `is_stdlib_path`（同一张表） |
| Go | `is_stdlib_package(首段)` | 同 |
| JS/TS | `is_node_stdlib(整串)` | 同，另剥 `node:` 协议前缀 |
| Java | `java.`/`javax.`/`jdk.` 前缀 | `is_jdk_path` |
| Kotlin | `java.`/`javax.`/`kotlin.`/`scala.` 前缀 | `is_kotlin_path ∨ is_jdk_path` |
| Scala | 同上 | `scala` 根 ∨ `is_jdk_path`（见下） |
| C# | `System.`/`Microsoft.` 前缀 | `is_dotnet_path` |
| Dart | `dart.` 前缀 | `is_dart_path`（`dart:` + `package:`） |
| Ruby | `is_stdlib_call`（**调用表，调错函数**） | `is_stdlib_require ∨ is_stdlib_path` |
| PHP | `is_stdlib_call` | `is_builtin_class ∨ is_core_extension` |
| Lua | 无（缺口） | `is_stdlib_module`（含末段切分） |
| C/C++ | 交给 `is_system_header` | 保持 `false`：系统头是解析器标志，导入分类仍是 `ExternalPackage` |
| Bash | 无 | 保持 `false`：没有模块表，`source` 的目标是文件 |

两个刻意的例外，都写进了代码注释：

* **Scala 不用 `is_scala_package`**：`SCALA_PACKAGES` 里混着 `cats`、`zio`、`akka` 这些第三方框架（`scala/packages.rs`），用它判"是否 stdlib"会把三方库吞进来。导入期只认 `scala` 根 + JDK。
* **C/C++ 不并入导入入口**：若把 `is_system_header` 折成 `true`，`stdio.h` 会从 `ExternalPackage` 变 `StandardLibrary`，改动面远超本次范围（`classifier.rs` 的 `test_c_system_header` 就是这条边界的锚点）。

## 8. 统一入口（已实现）

```rust
impl StdlibDetector {
    // 符号 → 类别（上一轮新增）
    pub fn category(name: &str, language: &Language) -> Option<StdlibCategory>;

    // 新增：符号名 ∪ 调用表达式 ∪ 模块路径
    pub fn is_stdlib_ref(name: &str, language: &Language) -> bool;

    // 新增：引用类目标的"是否 + 类别"一次问完（整路径 → 末段 → Other）
    pub fn ref_category(name: &str, language: &Language) -> Option<StdlibCategory>;

    // 新增：导入路径专用，逐语言查表，不含任何调用语义
    pub fn is_stdlib_import_path(path: &str, language: &Language) -> bool;
}
```

调用方收敛结果：

| 调用方 | 现在 |
|---|---|
| `stdlib_classifier.rs:52` | `ref_category`（仅引用类实体 + 插件元数据） |
| `dependency_extractor.rs:46,62` | `ref_category` |
| `common/classifier.rs:97` | `is_stdlib_import_path` |
| `symbol_extractor/python.rs:25`、`go.rs:50`、`php.rs:31`、`lua.rs:31`、`ruby.rs:31`、`csharp.rs:74`、`rust.rs:282` | 七个语言各自的包装，全部改为一行委托 |
| `external.rs` | 兜底删除，不再有 stdlib 判定 |
| `calls_prune.rs:200,208` | 保留 `is_stdlib_call`（调用域语义正确） |
| `call_extractor.rs:393` | 保留 `is_stdlib_by_type` + 插件启发式 |

## 9. `pattern_detection` 为什么不再读 stdlib

原代码在识别 getter/setter 之前拦一道：`if method.is_stdlib { mark_boilerplate; continue; }`。这道判断的语义依赖"stdlib 方法不该被当成访问器"，只有正在解析标准库源码时才成立；对项目代码，它唯一的实际效果是把同名方法整条踢出模式识别。

收窄 `mark_stdlib` 之后，`Method` 类实体的 `is_stdlib` 恒为 `false`（除插件显式标注），这道判断已是死代码，直接删除。`class_method/tests.rs` 里对应的断言改为"stdlib 标记不影响模式处理"。

构造函数/析构函数的早退分支保留，但理由改写为结构性成员必须保留（原来写的"即使被名字匹配成 stdlib"已不成立）。

## 10. 落地记录

| 阶段 | 内容 | 主要文件 | 状态 |
|---|---|---|---|
| P0 | `cce-stdlib` 新增 `is_stdlib_ref` / `ref_category` / `is_stdlib_import_path`；`classifier.rs` 与七个语言提取器改为委托 | `cce-stdlib/src/detector.rs`、`symbol_extractor/*.rs` | 完成 |
| P1 | `mark_stdlib` 收窄到 `Import`/`Require`/`Include` | `post_processing/stdlib_classifier.rs` | 完成 |
| P2 | `dependency_extractor` 补 `stdlib_category`；非调用域的丢弃单独计数 | `dependency_extractor.rs`、`relation_extractor.rs`、`resolver.rs`、`cce-metrics` | 完成 |
| Q1 | 删除 `external.rs` 命名约定兜底，不做替代 | `index/resolver/external.rs` | 完成 |
| Q3 | `pattern_detection` 停止读 `is_stdlib` | `processors/class_method/pattern_detection.rs` | 完成 |
| P4 | 删 `StdlibEntityDetector` / `StdlibEntityInfo` / 三个 `is_stdlib_name` | `grouper/recognizers/`（删两个文件）、`cce-stdlib/src/{python,rust}.rs` | 完成 |
| P3 | NL 类型改写 | —— | **不做**，只出文档（§11） |

新增指标 `relation_stdlib_filtered_non_call_total`：非调用域（导入 / 类型引用）被 `filter_stdlib_calls` 丢弃的边数。它与 `relation_stdlib_filtered_total` 分开计的理由是召回后果不同——丢掉一次 `Vec::new()` 没人会查，丢掉一条 `Vec<T>` 类型引用等于图上少一条类型边。

验证：

```
cargo test -p cce-stdlib --lib        → 133 passed
cargo test -p cce-parser --lib        → 1345 passed
cargo test -p cce-codegraph --lib     → 1043 passed
cargo test -p cce-metrics --lib       → 186 passed
cargo test -p cce-orchestrator --lib  → 643 passed
cargo check --workspace --all-targets → 通过
```

探针输出（`/tmp/probe`，见 §12）确认三件事同时成立：`struct Command` 不再被标 stdlib、非调用边带上了类别、NL 走 `Regular` 模板且保留成员与类型。

## 11. 未做与下一步

1. **NL 类型改写（P3）**：单独文档 `docs/analysis/stdlib-nl-type-rewriting.md`，含三条候选方向、逐语言映射表草案、五个待拍板项。前置条件（P1）已完成，可独立启动。
2. **非调用域的召回观测**：P2 之后 `filter_stdlib_calls=true` 会开始丢 `Use` / `TypeReference` 边。`relation_stdlib_filtered_non_call_total` 是唯一的观测口，建议在真实仓库上跑一轮，确认量级符合预期再决定是否给非调用域单独开关（当前刻意不拆）。
3. **stdlib 模板是否还该存在**：见 6.1，内置路径已不产生 stdlib 组，`StdlibTemplate` 现在只服务插件声明。保留是因为 `metadata["is_stdlib"]` 是插件契约的一部分，删模板等于删能力；若要收，得先决定插件还能不能声明 stdlib。
4. **`Inheritance` 目标不判 stdlib**：`is_stdlib_ref` 覆盖"类别 ∪ 调用 ∪ 路径"三种形态，不含 trait 表。`impl Display for Foo` 的 `Display` 因此不会被识别，边保留。这是保守选择——trait 名与项目内 trait 无法靠名字区分，误删的代价高于保留噪声。
5. ~~**`mark_stdlib` 的语义收口**~~ 已落地：`Entity.is_stdlib` 定死为"插件声明的外部定义"，内置不再写；归一化放进 `cce-stdlib` 的 `normalize_ref`；`ImportClassifier` 接上符号表、import 统计与外部调用归属。见 `docs/plan/import-classification-and-stdlib-semantics.md`。

## 12. 复现

```shell
# 探针 crate（/tmp/probe，path 依赖 cce-stdlib + cce-types + cce-parser）
cd /tmp/probe && CARGO_TARGET_DIR=/workspace/code-context-engine/target cargo run -q
```

预期输出：

```
--- entities ---
Import         std::collections::HashMap        is_stdlib=true  category=Some(Collection)
Struct         Command                          is_stdlib=false category=None
InherentImpl   Command                          is_stdlib=false category=None
--- relations ---
Use                  std::collections::HashMap          stdlib_category=Some(Collection)
TypeReference        std::collections                   stdlib_category=Some(Other)
TypeReference        String                              stdlib_category=Some(String)
TypeReference        std::io::Error                      stdlib_category=Some(Error)
ImplAssociation      Command                             stdlib_category=None
--- nl ---
pattern=Regular len=136
```

前缀 vs 表的三个反例可直接在 `cce-stdlib` 里复现：

```rust
CSharpStdlibDetector::is_dotnet_namespace("System") // true
DartStdlibDetector::is_dart_path("dart:async")      // true
RubyStdlibDetector::is_stdlib_require("json")       // true
```

## 13. 变更清单

代码 24 个文件（含 2 个删除、1 个新增指标），文档 4 个（1 个新增分析文档 + 3 个同步更新）。总计 28 文件，`+815 / -955`。刻意排除 `crates/app/cce-e2e-tests`、`crates/infra/llm-suite` 两个子模块指针——它们与本次改造无关，混进 patch 会让本地应用时误改子模块版本。
