# 标准库检测与模板调用者：现状分析与设计评估

分析对象：`crates/parser/cce-stdlib`（标准库检测）、`crates/parser/cce-stdlib/src/macros.rs` 的宏模板及其 15 个语言调用者、`crates/parser/cce-parser/src/ast_to_nl/**/templates`（NL 模板）的调用链。

所有结论均以 `file:line` 为据；标注「已验证」的条目附有实际执行的命令或编译输出。

## 1. 结论摘要

| 维度 | 结论 | 等级 |
| --- | --- | --- |
| 分层（表 → 宏 → 统一入口 → 消费方） | 结构清晰，`cce-stdlib` 独立成 crate、无环依赖，方向正确 | 合理 |
| 分类查询（`get_category`）入口 | 缺统一入口，调用方各自复制 15 路 `match`，共 3 份 | 必须收敛 |
| 宏模板 `impl_stdlib_by_type!` | **无法展开**（编译期报错），0 个调用者，属死模板 | 立即处理 |
| 关系类型覆盖 | 3 个语言用 `impl_stdlib_by_type_simple!` 只列 7 种关系，`GoroutineCall`/`DeferredCall`/`AsyncCall` 等被静默判否 | 正确性问题 |
| 单一事实来源 | 导入分类层（`ImportClassifier`）另起一套字符串前缀判断，与表并存 | 需收敛 |
| 死代码 | `StdlibEntityDetector`、`is_stdlib_name`、`impl_stdlib_by_type!` 无生产调用 | 应清理 |
| 文档 | `docs/parser/ast_to_nl/templates.md` 描述的分发规则（19 种 pattern）与代码（2 种）严重不符 | 需更新 |

一句话：**骨架合理，腐蚀点在「重复的 15 路分发」和「模板宏本身不可用」两处**，其余多为漂移与死代码。

> 改造进展：本报告的 D1、D2、D3、D4、D6、D7、D8、D9、D10 已按 `docs/plan/stdlib-and-template-refactor.md` 落地，该文档第 12 章记录了每阶段的落点与验证结果。
>
> **D5 与「`ImportClassifier` 职责边界」也已在后续轮次落地**：`StdlibEntityDetector` / `StdlibEntityInfo` / 三个同名 `is_stdlib_name` 全部删除，导入分类的前缀判断整体下沉到 `cce-stdlib` 的 `is_stdlib_import_path`。因此本文第 3 章（`StdlibEntityDetector` 现状）、第 4 章（七个判定面）以及所有引用 `grouper/recognizers/stdlib.rs` / `symbol_extractor/common/classifier.rs` 行号的内容**均已过期**，只作历史盘查保留。当前状态见 `docs/plan/stdlib-detector-wiring.md`；下一阶段（NL 类型改写）见 `docs/analysis/stdlib-nl-type-rewriting.md`。

## 2. 标准库检测的代码地图

### 2.1 crate 结构

`crates/parser/cce-stdlib`，15,700 行，只依赖 `cce-types`（`crates/parser/cce-stdlib/Cargo.toml`）。每个语言一个文件，`scala` 额外拆 `detector/packages/types` 三个子模块（`src/scala.rs:1-10`）。

15 个检测器：Bash、C、Cpp、CSharp、Dart、Go、Java、JavaScript（覆盖 TS/JSX/TSX）、Kotlin、Lua、Php、Python、Ruby、Rust、Scala。全部在 `src/lib.rs:27-43` 重导出。

### 2.2 对外接口（`src/detector.rs`）

| 接口 | 行号 | 语义 | 语言覆盖 |
| --- | --- | --- | --- |
| `is_stdlib_by_type(name, &RelationType, &Language)` | `detector.rs:24-51` | 按「关系类型」判定，关系层专用 | 15 种 |
| `is_stdlib_call(name, &Language)` | `detector.rs:57-80` | 只按名字判定（注释自称 legacy） | 15 种 |
| `is_stdlib_type(name, &Language)` | `detector.rs:86-106` | 类型名判定 | 13 种（无 Bash/Lua） |
| `is_stdlib_trait(name, &Language)` | `detector.rs:112-118` | 仅 Rust 有实现 | 1 种 |
| `is_stdlib_constant(name, &Language)` | `detector.rs:124-141` | 常量判定 | 10 种 |
| `get_category(name)` | **不在 detector.rs** | 名称 → `StdlibCategory` | 各语言独立实现 |

两个细节：

- 所有入口都先过 `effective_language()`（`detector.rs:30` 等），插件语言（`Language::Custom(i)`）按宿主语法降级查表；而 `effective_language` 只映射 `Custom(i)`（`crates/core/cce-types/src/types/language.rs:833-838`），`Vue/Svelte/Ejs` 不会被映射成 JS —— 之所以没出问题，是因为 SFC 内实体的 `language` 在解析期就被替换成块语言（`crates/parser/cce-parser/src/parser/embedded_types.rs:286-297`，测试 `test_resolve_entity_language_prefers_block`）。这条链路是自洽的。
- `get_category` **没有** crate 级统一入口，只在 `macros.rs:61` 由宏为每个检测器各生成一份。

### 2.3 六个消费方（"使用方式"的事实清单）

| # | 消费方 | 入口 | 判定维度 | 结果去向 |
| --- | --- | --- | --- | --- |
| 1 | 解析期实体打标 | `mark_stdlib`（`post_processing/stdlib_classifier.rs:33-95`） | 按 `EntityKind` 选 type/call/trait/constant | 写入 `Entity.is_stdlib` + `stdlib_category`，成为后续唯一事实 |
| 2 | 导入分类 | `ImportClassifier::is_stdlib_import`（`symbol_extractor/common/classifier.rs:93-141`） | **自带一套前缀判断**（`java.`/`System.`/`dart.` 等硬编码） | `ImportClass::StandardLibrary` |
| 3 | 调用关系分类 | `classify_stdlib_category`（`relation_extractor/call_extractor.rs:393-489`） | `is_stdlib_by_type` + 15 路 `match` 取 category | 关系边上的 `StdlibCategory` |
| 4 | Grouper 语义描述 | `StdlibEntityDetector`（`grouper/recognizers/stdlib.rs:62-129`） | `get_stdlib_category` + `is_stdlib_call` | `StdlibEntityInfo`（**无生产调用者，见 D5**） |
| 5 | 文本裁剪 | `callee_is_redundant`（`ast_to_nl/converter/index_enrichment/calls_prune.rs:187-215`） | `is_stdlib_call` | 删除 `calls:` 行里的 stdlib 冗余目标 |
| 6 | NL 模板选择 | `dispatcher` 读 `group.header.is_stdlib`（`bm25/templates/dispatcher.rs:44-50`） | 读第 1 步的缓存字段 | 走 `StdlibTemplate` 而非 `RegularGroupTemplate` |

消费方 1 是**唯一写者**，其余 5 个都是读者或旁路判断。这条「解析期打标 + 下游读缓存」的设计是对的：避免了在 Grouper/关系/文本三处重复检测。

## 3. 检测逻辑的实现方式

### 3.1 数据形态

每个语言文件都是「常量表 + 少量手写判定 + 宏生成」的混合体。表全部是 `&[&str]` 常量切片，例如 `RustStdlibDetector::STDLIB_TYPES`（`rust.rs:11` 起）、`PythonStdlibDetector::BUILTIN_FUNCTIONS`（`python.rs:92-162`）。

### 3.2 五个宏模板与调用者矩阵

模板定义：`crates/parser/cce-stdlib/src/macros.rs`。

| 模板 | 定义位置 | 生成内容 | 调用者数 | 调用者 |
| --- | --- | --- | --- | --- |
| `impl_list_checker!` | `macros.rs:17-34` | `fn is_xxx(name) -> bool`（`slice::contains`） | 7 | csharp / dart / javascript / php / python / ruby / rust |
| `impl_stdlib_categorizer!` | `macros.rs:49-105` | `fn get_category` + `OnceLock<HashMap>` | 15 | 全部语言（`scala.rs:11` 亦用） |
| `impl_stdlib_by_type!` | `macros.rs:120-156` | 按 `RelationType` 定制的 `is_stdlib_by_type` | **0** | 无（且不可用，见 D1） |
| `impl_stdlib_call!` | `macros.rs:170-195` | `is_stdlib_call`（先查内建函数，再查模块前缀） | 1 | python（`python.rs:199-202`） |
| `impl_stdlib_by_type_simple!` | `macros.rs:215-240` | 简单版 `is_stdlib_by_type`，列表内关系类型全部委托 `is_stdlib_call` | 3 | go（`go.rs:722`）、python（`python.rs:205`）、ruby（`ruby.rs:1032`） |

`is_stdlib_by_type` 的实现分布：**11 个语言手写**（合计约 460 行：rust 152、cpp 90、javascript 43、kotlin 30、c/csharp/dart/java/php 各 23、lua 18、bash 12），3 个用简单宏，1 个（scala）在 `scala/detector.rs:117`。

### 3.3 分类查询机制（`impl_stdlib_categorizer!`）

生成的函数（`macros.rs:61-102`）：

1. 首次调用时把「category → 名称数组」摊平成 `HashMap<&'static str, StdlibCategory>`，存 `OnceLock`（每检测器一份，`macros.rs:68-81`）；
2. 查表前先剥掉宏名的 `!`（`macros.rs:84-88`）；
3. 查不到时取「根段」再查一次：`lookup_name.split(['.', ':']).next()`（`macros.rs:95-100`），因此 `console.log` → `console`、`Vec::new` → `Vec` 能命中。

这一层的性能是 O(1)（HashMap），但同名的 `is_stdlib_*` 判定仍是 `slice::contains` 线性扫描 —— 见 D9 关于「O(1)」注释的说明。

### 3.4 实体层与关系层的判定路径差异

同一次调用在两条链路上会得到不同答案：

- 实体层：`EntityKind::Function` → `is_stdlib_call(name)`（`stdlib_classifier.rs:61-64`），只看名字。
- 关系层：`is_stdlib_by_type(name, relation_type)`（`call_extractor.rs:399`），按关系类型分支。

理论上关系层更准（能用 `MacroCall` 区分 `println!` 与同名函数），但前提是每个语言把全部关系类型都覆盖到 —— 现状没做到（D3）。

## 4. 模板调用者的逻辑

「模板」在本项目有两层，二者的调用者是不同的东西，分开说。

### 4.1 第一层：宏模板的调用者（15 个语言文件）

调用形态统一为「常量表 → 宏展开」。以 Python 为例（`python.rs:186-216`），一个文件里连续调用 4 个宏：

```
impl_list_checker!(...)        // 生成 is_stdlib_module / is_builtin_function / is_builtin_type
impl_stdlib_call!(...)         // 生成 is_stdlib_call
impl_stdlib_by_type_simple!(…) // 生成 is_stdlib_by_type
impl_stdlib_categorizer!(...)  // 生成 get_category
```

调用者与模板之间存在**隐性耦合**：宏参数里直接写 `StdlibCategory::Collection` 这类短路径（如 `scala.rs:14-16`），依赖 `impl_stdlib_categorizer!` 展开后函数体内的 `use cce_types::stdlib_category::StdlibCategory;`（`macros.rs:66`）才成立。参数表达式的可见性受生成代码影响，这一点在宏文档里没有说明。

### 4.2 第二层：NL 模板的调用者

调用链（三跳，全部静态分发，无 `dyn`）：

```
convert_group_with_generators        group_converter.rs:695
  → Bm25Generator::generate_for_group      bm25/generator.rs:46-50
      → GroupTemplateDispatcher::dispatch   bm25/templates/dispatcher.rs:38-52
  → EmbeddingGenerator::generate_for_group embedding/generator.rs:53-113
      → GroupTemplateDispatcher::dispatch   embedding/templates/dispatcher.rs:55-69
```

分发条件只有两条（`bm25/templates/dispatcher.rs:39-51`）：

| 条件 | 选中模板 |
| --- | --- |
| `PatternInfo::GetterSetter(_)` | `GetterSetterTemplate` |
| `PatternInfo::None` 且 `header.is_stdlib == true` | `StdlibTemplate` |
| `PatternInfo::None` 且 header 非 stdlib | `RegularGroupTemplate` |

要点：

- **stdlib 分支排在 pattern 之后**：一个既是 getter/setter 类、header 又是 stdlib 的组，会走 `GetterSetterTemplate`，stdlib 模板被跳过。这是有意的优先级，但代码与文档都没写。
- 判定只看 `header`。组成员是 stdlib、header 不是的组，会走 regular 模板，`is_stdlib` 不参与组级聚合。
- 插件可整体覆盖模板输出：`generate_from_plugins`（`group_converter.rs:1031-1085`）按 `above → builtin → below` 三级优先级短路，`above` 命中时内置 dispatcher 完全不执行（`group_converter.rs:663-673`）。这是 `dyn CodePlugin` 唯一的入口（`group_converter.rs:1036-1037`），与 AGENTS.md「尽量少用 dyn」的要求需在 `docs/archive/dynamic.md` 有对应记录。
- 两个模板树的 `StdlibTemplate` 输出策略不同但结构几乎相同（`bm25/templates/stdlib.rs:24-50` vs `embedding/templates/stdlib.rs:23-51`）：BM25 用空格连接、兜底字面量 `"stdlib"`；Embedding 用 `". "` 连接、兜底 `"standard library"`，且返回 `vec![desc]`。

## 5. 已核实的缺陷

### D1 `impl_stdlib_by_type!` 无法展开（已验证）

`macros.rs:139-141` 在 `match` **之外**定义了一个辅助函数，函数体直接引用重复变量 `$logic`：

```rust
fn check_macro(name: &str, detector: &str) -> bool {
    $logic
}
```

实际调用即报错（独立探针 crate 复现）：

```
error: variable `logic` is still repeating at this depth
   --> crates/parser/cce-stdlib/src/macros.rs:140:21
```

即便修掉重复变量，该辅助函数在 `impl` 内的嵌套 `fn` 中也无法访问 `Self`，而所有预期传参都写成 `Self::STDLIB_TYPES.contains(...)`（见 `macros.rs:114-118` 的示例）。结论：这个模板从未被使用过，也不可能被使用。这解释了为什么 11 个语言宁可口径不一地手写 460 行。

### D2 `get_category` 的 15 路分发复制了 3 份

| 位置 | 用途 |
| --- | --- |
| `parser/extractor/post_processing/stdlib_classifier.rs:101-122` | 实体细分类 |
| `grouper/recognizers/stdlib.rs:101-123` | Grouper 语义描述 |
| `parser/extractor/relation_extractor/call_extractor.rs:422-489` | 关系边分类（额外套 `.or(Some(Other))`） |

三份内容等价（语言 → 检测器），只是缺失分支的兜底值不同（`None` vs `Some(Other)`）。新增一门语言要改 3 处，漏改不会编译报错（都有 `_ =>` 兜底），是典型的静默漂移点。

### D3 关系类型覆盖不全导致漏判

`impl_stdlib_by_type_simple!` 的语义是「列表内委托 `is_stdlib_call`，**其余一律 false**」（`macros.rs:234-235`）。三个使用者都只列了 7 种：

| 语言 | 已列 | 未列（`_ => false`） |
| --- | --- | --- |
| Go（`go.rs:722-731`） | Direct/Instance/Static/Chained/Constructor/Callback/Generic | **`GoroutineCall`、`DeferredCall`**、Pointer、Macro、Async、HigherOrder |
| Python（`python.rs:205-216`） | 同上 7 种 | **`AsyncCall`**、Pointer、Macro、Goroutine、Deferred、HigherOrder |
| Ruby（`ruby.rs:1032-1043`） | 同上 7 种 | 同上 |

而这些关系类型在生产里是真实产生的：`(Some(CallCategory::Goroutine), _) => RelationType::GoroutineCall`（`relation_extractor/relation_handlers.rs:445`），`(Some(CallCategory::Deferred), _) => RelationType::DeferredCall`（同文件 `:448`），`Async` 在 `:451`。

后果：`go fmt.Println()`、`defer file.Close()`、`await asyncio.sleep(1)` 这类调用在关系层不被判为 stdlib，于是 stdlib 调用不会按 stdlib 处理（不参与调用图裁剪/统计口径），与 `is_stdlib_call` 的结论直接矛盾。更隐蔽的是：`RelationType` 以后再加变体，这三处不会编译报错，只会继续静默判否。

实测（探针 crate 直接调用 `cce-stdlib`，非推测）：

```
Go     GoroutineCall    fmt.Println:     by_type=false  by_call=true
Go     DeferredCall     fmt.Println:     by_type=false  by_call=true
Go     DirectCall       fmt.Println:     by_type=true   by_call=true
Python AsyncCall        asyncio.sleep:   by_type=false  by_call=true
Python DirectCall       asyncio.sleep:   by_type=true   by_call=true
Rust   HigherOrderCall  Vec::new:        by_type=false  by_call=true
Rust   GoroutineCall    Vec::new:        by_type=true   by_call=true   ← Rust 不存在 goroutine，却能命中
```

### D4 Rust 检测器里存在 Go 语义分支（复制粘贴痕迹）

`rust.rs:728-752`:

```rust
// Goroutine call: check the underlying call
RelationType::GoroutineCall => { ... }
// Deferred call: check the underlying call
RelationType::DeferredCall => { ... }
```

Rust 没有 goroutine / defer。同时 Rust 漏了 `HigherOrderCall`（落到 `_ => false`），而 `HigherOrderCall` 在 Rust 侧是有产出的（`relation_handlers.rs:459`）。这说明 11 份手写实现是互相拷贝迭代出来的，没有共同的形状约束。

### D5 死代码：Grouper 检测器与 `is_stdlib_name` 无生产调用者

- `StdlibEntityDetector` / `StdlibEntityInfo`：只有 `grouper.rs:70`、`grouper/recognizers.rs:16` 的重导出，其余引用全在 `grouper/recognizers/tests.rs`。Grouper 实际压缩判断用的是解析期缓存的 `entity.stdlib_category`（`grouper/processors/call_merger.rs:234`）。
- `is_stdlib_name`（`grouper/recognizers/stdlib.rs:90`、`rust.rs:611`、`python.rs:245`）：唯一引用是 `recognizers/tests.rs:47-50`。

也就是说，D2 表里第 2 份 15 路 `match` 服务于一个没人调用的 API。

### D6 单实体兼容路径丢失语言，噪声过滤降级

`create_standalone_group` 把 `language` 硬编码为 `Language::Unknown`（`ast_to_nl/common/utils.rs:472`），因为 `GroupedEntity` 根本没有 language 字段（`crates/core/cce-types/src/types/entity/full.rs:135-155`），语言只挂在 `EntityGroup` 上。

这条路径在生产里被用到了两处：
- 组成员转换：`for member in &group.members { ... self.convert_grouped(member, ...) }`（`group_converter.rs:826-827`，注释直言 "Member conversion runs without group context"）；
- 头部 brief 生成：`group_converter.rs:729`。

后果：成员 chunk 与 brief 走模板时，`NoiseProfile::for_language(group.language)`（`bm25/templates/stdlib.rs:40`、`embedding/templates/stdlib.rs:39`）拿到 `Unknown`，语言专属样板噪声过滤失效。stdlib 模板的语言门控也一并失效。

### D7 BM25 模板反向依赖 Embedding 模块

`bm25/templates/stdlib.rs:3` 与 `bm25/templates/regular.rs:7` 都 `use crate::ast_to_nl::embedding::filter_embedding_noise;`（定义在 `embedding/noise_filter.rs:26`）。该函数本身与 embedding 无关（纯文本 + `NoiseProfile` 过滤），放在 `common` 才合理。当前形态让 BM25 路径编译期依赖 embedding 路径，`common` 目录的存在意义被架空。

### D8 文档与代码漂移（`docs/parser/ast_to_nl/templates.md`）

| 文档描述 | 代码事实 |
| --- | --- |
| `bm25/templates/design_patterns.rs`、`boilerplate_patterns.rs`（`templates.md:20-21`、`:28-29`） | 两个文件都不存在；实际只有 dispatcher / getter_setter / group_trait / regular / stdlib |
| 19 条分发规则，含 Builder/Factory/Singleton/…（`templates.md:122-143`） | `PatternInfo` 只有 `None` 与 `GetterSetter` 两个变体（`crates/core/cce-types/src/types/grouper/pattern.rs:40-45`） |
| 「Framework/design patterns are delegated to the plugin system」 | 与代码一致（`getter_setter.rs:3-6` 注释），文档未反映 |

同目录 `group_templates.md`、`architecture.md` 是否同步未逐条核对，建议一并校。

### D9 `get_pattern_name` 对 stdlib 组报 "Regular"（已解决，但结论与初判不同）

`bm25/templates/dispatcher.rs:55-61` 与 `embedding/templates/dispatcher.rs:75-81`：只有 `pattern_info` 参与命名，stdlib 分支返回 `"Regular"`。若该值进入日志或统计（`PatternInfo::pattern_name()` 同名方法在 `pattern.rs:60-65`），stdlib 模板命中率会被统计成 Regular，无法从指标上验证 stdlib 模板是否生效。

> **最终落地**：先按本文结论加 stdlib 分支返回 `"Stdlib"`，后续复核发现真正的问题是方法名——它承诺「模式名」，实际返回「模板名」。`PatternInfo::pattern_name` 已经是模式名的唯一答案，一个只做转发的 `get_pattern_name` 没有存在理由；而模板名只有 dispatcher 能答。因此改名为 `template_name`（保留 `Stdlib`，`Regular` 会与实跑模板不符），三路判定下沉到 `common/templates/selection.rs:GroupTemplateKind::select`，`dispatch` 与 `template_name` 共用它，两者因此不可能漂移。当前状态见 `docs/parser/ast_to_nl/templates.md` 第 5 节。

> **附带实测**：收窄 `mark_stdlib` 之后，`import` 类实体仍被正确标注 stdlib，但纯 import 组会被 `drop_import_only_groups` 丢弃（`grouper/builtin_stages.rs:152-170`），所以内置路径下已经没有 `header.is_stdlib == true` 的组——stdlib 模板退出内置路径，只剩插件 `metadata["is_stdlib"]` 一条触发路径。

### D10 category 字符串无法回读

`StdlibCategory::Other` 的 Display 是 `"other_stdlib"`（`crates/core/cce-types/src/types/stdlib_category.rs:45`），但 `parse_category` 对 `"other_stdlib"` 返回 `None`（`stdlib_classifier.rs:167` 的测试明确断言 `None`）。插件用 `metadata["stdlib_category"] = "other_stdlib"` 时会被丢弃并回落 `Other`。当前结果相同，属于潜在陷阱而非线上缺陷。

### 关于「O(1)」的性能注释

`detector.rs:21-23`、`macros.rs:131` 声称按 `RelationType` 静态分发带来 O(1)。分发本身确实是 O(1)，但每个分支内部是 `Self::STDLIB_TYPES.contains(&name)`，即对 `&[&str]` 的线性扫描（`macros.rs:29`）；Rust 的 `STDLIB_TYPES` 有 170 条（`rust.rs:11-207`）。调用点在每个 call site 上（`call_extractor.rs:399`），属热路径。真正的 O(1) 只出现在 `get_category` 的 HashMap。注释里的承诺与实现不匹配，容易让人误判优化空间。

## 6. 设计合理性评估

| 维度 | 评价 | 依据 |
| --- | --- | --- |
| 模块边界 | **合理**。`cce-stdlib` 只依赖 `cce-types`，无反向依赖；解析层打标、下游读缓存，避免了重复检测 | `stdlib_classifier.rs:19-23` 的注释与实现一致 |
| 状态约定 | **合理**。`is_stdlib` 是解析期唯一写者，六个消费方里五个只读 | 2.3 表 |
| 扩展性（新增语言） | **较差**。需同时改 `lib.rs` 重导出、`detector.rs` 5 个分发、3 份 `get_category` 分发、`grouper` 哨兵测试，漏改不报错 | D2、`grouper/recognizers/stdlib.rs:314-366` 的哨兵测试是唯一兜底 |
| 扩展性（新增 RelationType） | **差**。所有实现都有 `_ => false`，新增变体静默走 false | D3、D4 |
| 模板机制 | **半成品**。5 个宏里 1 个不可用、1 个仅 1 处使用、1 个覆盖不全；11 份手写实现形状不一 | D1、D3、D4 |
| 数据源 | **可接受但脆弱**。纯手工表，无生成/校验流程；127 个单测主要断言「表里有这个名字」，无法发现漏项 | `grep -c '#\[test\]'` 得 127 |
| 误判控制 | **偏弱**。纯名字匹配，无作用域/导入解析；用户自定义 `format`/`type`（Python 内建表含两者，`python.rs:116,158`）会被误判为 stdlib，进而被 `calls_prune` 从文本里删掉 | `calls_prune.rs:208` |
| 性能 | **可接受**。HashMap 分类 + 线性表扫描；单次成本小，但注释与实际不符 | 3.3、D9 |
| 双路径模板 | **合理但需约束**。BM25/Embedding 输出策略确实不同（`getter_setter.rs` 对比可见），不是纯复制；但 dispatcher 的路由逻辑完全重复，可参数化 | 4.2 |

总体判断：**架构方向没问题，工程实现处于「快速迭代后未收口」的状态**。最值得先动的不是重写检测表，而是把「15 路分发」收敛成一处、把关系类型覆盖补齐、把不可用的模板删掉。

## 7. 改进建议（按性价比排序）

| 优先级 | 动作 | 落点 | 收益 |
| --- | --- | --- | --- |
| P0 | 在 `cce-stdlib` 增加 `StdlibDetector::category(name, &Language) -> Option<StdlibCategory>`，三处复制改为调用它 | 新增 `detector.rs` 方法；替换 `stdlib_classifier.rs:101-122`、`grouper/recognizers/stdlib.rs:101-123`、`call_extractor.rs:422-489` | 消除 3 份 15 路 match；新增语言只改 1 处 |
| P0 | 修 `impl_stdlib_by_type!` 或直接删除 | `macros.rs:120-156` | 去掉一个「看着能用其实不能用」的模板；若修，需去掉嵌套 `fn` |
| P1 | 三个 `impl_stdlib_by_type_simple!` 补齐 `GoroutineCall`/`DeferredCall`/`AsyncCall`/`HigherOrderCall`，并去掉 `_ => false` 改为显式列举 | `macros.rs:216-240` + 3 个调用点 | 修复 Go/Python/Ruby 漏判；新增变体时编译期报错 |
| P1 | 清理 Rust 检测器里的 `GoroutineCall`/`DeferredCall` 分支，补 `HigherOrderCall` | `rust.rs:728-757` | 去掉误导性语义 |
| P1 | `create_standalone_group` 传入真实语言（`GroupedEntity` 增 `language` 字段，或调用点显式传参） | `common/utils.rs:458-482`、`group_converter.rs:826` | 修复成员 chunk 噪声过滤降级 |
| P2 | `filter_embedding_noise` 迁到 `ast_to_nl/common` | `embedding/noise_filter.rs:26` → `common/` | 解除 BM25 → Embedding 反向依赖 |
| P2 | 删除 `StdlibEntityDetector`/`StdlibEntityInfo`/`is_stdlib_name` | `grouper/recognizers/stdlib.rs`、`rust.rs:611`、`python.rs:245` | 减少死代码与维护面；若保留需说明用途 |
| P2 | `ImportClassifier::is_stdlib_import` 改为调用 `cce-stdlib` 的表（Java/Kotlin/Scala/CSharp/Dart 的硬编码前缀下沉到各检测器的 `is_stdlib_package`） | `symbol_extractor/common/classifier.rs:93-141` | 恢复「单一事实来源」 |
| P2 | 两个 dispatcher 的路由抽成一处（泛型或返回枚举交由模板渲染） | `bm25/templates/dispatcher.rs:38-52`、`embedding/templates/dispatcher.rs:55-69` | 消除路由重复；顺带修 D9 的命名 |
| P3 | 更新 `docs/parser/ast_to_nl/templates.md` | 全文 | 文档回到可用的程度 |
| P3 | 给 `parse_category` 补 `"other_stdlib"` | `stdlib_classifier.rs:128-145` | 消除回读陷阱 |

## 8. 待确认（需要作者拍板）

1. **`StdlibEntityDetector` 是遗留还是待接入**？如果是为「stdlib 实体的语义描述」预留，应写明接入点；否则建议删除。
2. **`GoroutineCall`/`DeferredCall` 判否是否是刻意**？如果认为 `go`/`defer` 包装后不应影响 stdlib 判定，需要改成「解包后判断」而不是「直接 false」。
3. **stdlib 组是否应该由 header 单独决定**？成员 stdlib、header 不是的组目前走 regular 模板，是否会稀释 stdlib 语义？
4. **`ImportClassifier` 与 `cce-stdlib` 的职责边界**：导入路径判定是否统一交给 `cce-stdlib`（新增 `is_stdlib_package` 一类的接口）？

## 9. 复现与验证记录

```shell
# 1) 代理拉取（主仓 + 9 个子模块）。
#    注意：只配 url.<proxy>.insteadOf 不够——实测 submodule clone 仍直连 github.com，
#    卡在 index-pack 超过 4 分钟无进展；把 submodule.<name>.url 直接写成代理地址后秒级恢复。
git clone https://gh-proxy.com/https://github.com/kkkqkx123/code-context-engine.git
cd code-context-engine
for p in $(git config -f .gitmodules --get-regexp 'submodule\..*\.path' | awk '{print $2}'); do
  url=$(git config -f .gitmodules "submodule.$p.url")
  git config "submodule.$p.url" "https://gh-proxy.com/$url"
  git submodule update --init --depth 1 "$p"
done
git submodule status --recursive      # 9 项均无 +/- 前缀

# 2) 全局镜像（~/.cargo/config.toml）
#    [source.crates-io] replace-with = "rsproxy-sparse"
#    [source.rsproxy-sparse] registry = "sparse+https://rsproxy.cn/index/"
#    [net] git-fetch-with-cli = true

# 3) 编译与单测（仓库内）
cargo check -p cce-stdlib            # Finished in 27.18s
cargo test  -p cce-stdlib --lib      # 127 passed; 0 failed

# 4) 独立探针 crate（/tmp/probe，path 依赖 cce-stdlib + cce-types，绕开整仓 manifest 依赖）
#    4.1 impl_stdlib_by_type! 不可展开：
#        error: variable `logic` is still repeating at this depth
#        --> crates/parser/cce-stdlib/src/macros.rs:140:21
#    4.2 关系类型漏判：输出见 D3 的实测表
```

补充统计：`cce-stdlib` 共 15,700 行、127 个 `#[test]`；`is_stdlib_by_type` 手写实现 11 份约 460 行；`cargo check`/`cargo test` 均通过，说明 D1/D3 属于「静默正确性缺口」而非编译期问题。
