# 标准库检测与模板调用者改造方案

## 0. 依据

事实来源为 `docs/analysis/stdlib-and-template-callers.md`（下称「分析报告」），其中的行号与实测结论均已核对。本方案只处理能被代码事实支撑的缺陷，涉及产品语义取舍的条目移入待拍板清单，不在本轮改代码。

## 1. 问题清单

按「能否在不改变产品语义的前提下修掉」分成三组。

### 1.1 结构性缺陷（本轮必修）

| 编号 | 问题 | 事实依据 | 影响 |
| --- | --- | --- | --- |
| D2 | `get_category` 的十五路语言分发复制三份 | `stdlib_classifier.rs:101-122`、`grouper/recognizers/stdlib.rs:101-123`、`call_extractor.rs:419-484` | 新增语言要改三处，三处都有通配回落，漏改不报错 |
| D1 | `impl_stdlib_by_type!` 无法展开且零调用者 | `macros.rs:120-156`，探针调用报 `variable logic is still repeating at this depth` | 一个「看着能用」的模板误导后来者；十一个语言为此手写约四百六十行 |

### 1.2 静默正确性缺口（本轮必修）

| 编号 | 问题 | 事实依据 | 影响 |
| --- | --- | --- | --- |
| D3 | 十个检测器的 `is_stdlib_by_type` 只列七种调用型关系，其余走 `_ => false` | `macros.rs:215-240` 与 `go.rs:722`、`python.rs:205`、`ruby.rs:1032`；另 c/csharp/dart/java/lua/php/kotlin/javascript/bash 的七个臂加通配臂 | `go f()`、`defer f()`、`await f()`、`arr.map(cb)` 在关系层被判非标准库，与 `is_stdlib_call` 直接矛盾；新增关系变体静默判否 |
| D4 | Rust 检测器含 goroutine/defer 分支，缺 `HigherOrderCall` | `rust.rs:727-757` | Rust 无此语义，`Vec::new` 加 `GoroutineCall` 反而命中；高阶调用漏判 |
| D6 | 单实体兼容路径把语言硬编码为 `Unknown` | `common/utils.rs:472`，调用点 `group_converter.rs:729`、`:826` | 成员 chunk 与 header brief 的语言画像失效，去噪与 stdlib 模板的语言门控一并降级 |

### 1.3 依赖方向与文档漂移（本轮顺手修）

| 编号 | 问题 | 事实依据 | 影响 |
| --- | --- | --- | --- |
| D7 | BM25 模板反向依赖 embedding 模块的噪声过滤器 | `bm25/templates/stdlib.rs:3`、`bm25/templates/regular.rs:7` 引用 `embedding/noise_filter.rs:26` | 两条并列路径产生编译期依赖，`common` 目录失去意义 |
| D9 | stdlib 组在 `get_pattern_name` 里报 `Regular` | 两个 `dispatcher.rs` 的同名方法只读 `pattern_info` | 命中率无法从指标上验证 |
| D10 | `StdlibCategory::Other` 的显示名无法回读 | `stdlib_category.rs:45` 与 `stdlib_classifier.rs:167` 的断言 | 插件写 `other_stdlib` 会被丢弃，结果相同但语义丢失 |
| D8 | `docs/parser/ast_to_nl/templates.md` 描述十九种 pattern 与两个不存在的文件 | 实际 `PatternInfo` 只有两个变体 | 文档不可用 |

### 1.4 本轮不动的条目

`StdlibEntityDetector` 与 `is_stdlib_name` 是死代码（D5），但删留取决于是否有接入计划；`ImportClassifier` 自带的前缀判断与检测表并存，属于职责边界问题。两者都需要作者判断，列入第 8 章待拍板。

## 2. 方案总览

改造遵循一条主线：**把「按语言分发」和「按关系类型判定」各自收敛成唯一形状，把语言信息补齐到单实体路径**。不重写检测表，不动数据内容。

| 阶段 | 目标 | 主要落点 | 风险 |
| --- | --- | --- | --- |
| 一 | 分类查询单一入口 | `cce-stdlib/src/detector.rs` 新增 `category`，三个消费方改为调用 | 低，语义等价 |
| 二 | 宏模板收口 | `macros.rs` 删一个宏、改造一个宏，十个语言文件改调用形态 | 中，会改变部分判定结果（预期方向） |
| 三 | Rust 关系语义修正 | `rust.rs` 删两臂、加一臂 | 低 |
| 四 | 单实体路径语言透传 | `common/utils.rs`、两个 generator、`entity_converter.rs`、`group_converter.rs` | 中，签名变更面较宽 |
| 五 | 噪声过滤下沉 | `embedding/noise_filter.rs` 移到 `common/`，七处引用改路径 | 低 |
| 六 | 小修 | 两个 dispatcher、`parse_category` | 低 |
| 七 | 文档同步 | `docs/parser/ast_to_nl/templates.md` | 无 |

阶段间无硬依赖，但一、二、三都属于关系层判定，合并验证更省时间。

## 3. 阶段一：分类查询单一入口

在 `cce-stdlib` 的 `StdlibDetector` 上新增 `category(name, language)`，内部先解析有效语言（复用现有 `effective_language`），再按语言取对应检测器的 `get_category`，模板语言与未知语言返回空。

三个消费方改为调用：

* 解析期实体打标：删除本地 `fine_stdlib_category`，直接取 `category` 的返回值；缺失时仍由既有的粗分类兜底，行为不变。
* Grouper 语义描述：`get_stdlib_category` 退化为一行委托，同时删掉该方法上方的语言分发注释。
* 关系边分类：`classify_stdlib_category` 里十五臂 `match` 换成 `category(...).or(Some(Other))`，插件启发式的入口位置与判定顺序保持不变。

同时给 `category` 补一条哨兵测试：对规范列表中每个有分支的语言取一个已知名称，断言分类非空且不等于 `Other`。这条测试与分析报告里已有的两条哨兵测试互补，覆盖的是新入口而非检测器本身。

## 4. 阶段二：宏模板收口

* 删除 `impl_stdlib_by_type!`。它零调用者且不可展开，修的成本高于价值；宏文档里给不出可用示例这件事本身就是证据。
* `impl_stdlib_by_type_simple!` 改名为 `impl_stdlib_by_type_name_only!`，参数表取消，语义固定为「仅调用域关系参与判定，判定结果等于 `is_stdlib_call`」。判断调用域用关系类型自带的 `is_call()`，非调用域（导入、结构、引用、模板）返回否。这样新增调用型关系变体自动继承正确行为，新增非调用型变体自动被挡掉，不再有静默判否。
* 调用者：go、python、ruby 三个原本用简单宏的语言直接换宏；c、csharp、dart、java、lua、php、bash 七个手写实现与大括号七个臂完全等价，一并换成同一宏，删掉重复的二十余行；kotlin、javascript 保留各自的语言逻辑，但改用「先判调用域再执行语言逻辑」的守卫形态。
* cpp 与 rust 的专用分支保持不变，只把兜底臂从无条件 `is_stdlib_call` 收紧为「调用域内才走 legacy 判定」。

判定结果的变化是单向的：原先被判否的 goroutine、defer、await、高阶调用，在其名字本身命中标准库表时会变成是。这与 `is_stdlib_call` 已有的结论一致，属于修缺口而非改口径。

## 5. 阶段三：Rust 关系语义修正

删除 `GoroutineCall`、`DeferredCall` 两臂（Rust 无此语义，且两臂的判据与其他臂重复），新增 `HigherOrderCall` 臂，委托 `is_stdlib_call`。同时把注释里「O(1)」的承诺改成如实描述：按关系类型分支是常量时间，分支内部仍是表扫描。

## 6. 阶段四：单实体路径语言透传

`GroupedEntity` 没有语言字段，语言只挂在 `EntityGroup` 上，因此透传只能从调用点带下来。

* `create_standalone_group` 增加语言参数，写入组的 `language` 字段。
* 两个 generator 的 `generate(entity)` 增加语言参数，向下传给 `create_standalone_group`；`generate_for_group` 不动。
* `entity_converter.rs` 的 `convert_grouped` 及三个私有分支增加语言参数，向下传给 generator。
* 调用点两处：成员转换与 header brief 回落，都传 `group.language`。

测试里构造单实体调用时统一传 `Language::Unknown` 或构造语言，保持断言不变。

## 7. 阶段五：噪声过滤下沉

`embedding/noise_filter.rs` 整体移到 `ast_to_nl/common/`，函数更名为 `filter_nl_noise`（它服务两条路径，名字里不该出现 embedding），模块文档同步改写为「NL 文本噪声过滤」。七处引用改指新路径，`embedding.rs` 不再导出该函数。

## 8. 阶段六：小修

* 两个 dispatcher 的 `get_pattern_name` 在读 `pattern_info` 之前先看 `header.is_stdlib`，stdlib 组返回 `Stdlib`。
* `parse_category` 增加 `other_stdlib` 与 `other` 两个别名，与 `StdlibCategory::Other` 的显示名对齐；同步更新那条断言 `None` 的测试。

## 9. 阶段七：文档同步

`docs/parser/ast_to_nl/templates.md` 按代码重写：删除两个不存在的文件与十九种 pattern 的分发表，改为「两种 pattern + stdlib 分支」的真实分发规则，并写明 stdlib 分支排在 pattern 之后的优先级、只看 header 的判定范围、插件三级覆盖的短路位置。

## 10. 验证

每个阶段结束后跑 `cargo check -p cce-stdlib` 与 `cargo check -p cce-parser`，阶段二、三结束跑 `cargo test -p cce-stdlib --lib`，阶段四、五结束跑 `cargo test -p cce-parser --lib`。全部完成后跑 `cargo clippy --all-targets` 与 `cargo fmt`。

补三条测试锁定结论：

* `category` 的哨兵测试（阶段一）。
* 关系域一致性测试：对全部调用型关系变体，断言每个 name-only 语言的 `is_stdlib_by_type` 等于 `is_stdlib_call`；对非调用型关系断言为否（阶段二）。
* 单实体组携带语言：断言 `create_standalone_group` 产出的 `language` 与入参一致（阶段四）。

## 11. 待拍板

1. `StdlibEntityDetector` / `StdlibEntityInfo` 是遗留还是待接入？本轮保留原样，只在其中一处改为委托新入口。
2. `ImportClassifier` 的硬编码前缀是否下沉到各检测器的包名表？涉及 Java、Kotlin、Scala、C#、Dart 五个语言，需要先确认导入判定与调用判定是否应共用同一张表。
3. 成员 chunk 是否应改用 `group.language` 之外的信息（例如实体自身在解析期记录的语言）？本轮采用组语言，属于近似。
4. stdlib 组的判定是否要下沉到组级聚合（成员 stdlib、header 非 stdlib 的组）？本轮维持只看 header。

## 12. 执行记录

七个阶段已全部落地，验证命令与结果如下。

| 阶段 | 落地内容 | 主要文件 |
| --- | --- | --- |
| 一 | 新增 `StdlibDetector::category`；实体打标、Grouper、关系边三处 15 路 `match` 删除，改为调用新入口 | `cce-stdlib/src/detector.rs`、`post_processing/stdlib_classifier.rs`、`grouper/recognizers/stdlib.rs`、`relation_extractor/call_extractor.rs` |
| 二 | 删除 `impl_stdlib_by_type!`；`impl_stdlib_by_type_simple!` 换成 `impl_stdlib_by_type_name_only!`（无参数、`is_call()` 守卫）；新增 `impl_stdlib_by_type_calls!` 供需要按关系细分的语言使用 | `cce-stdlib/src/macros.rs` |
| 二 | go/python/ruby 换宏；c/csharp/dart/java/lua/php/bash 七个重复实现删除并换宏；scala 一并归入（其 `by_type` 是 `is_stdlib_call` 的子集，收敛后等价）；kotlin/javascript 改为调用域守卫 + 原语言逻辑 | 各语言文件 |
| 三 | Rust 删除 goroutine/defer 两臂，新增 `HigherOrderCall`，其余形状委托 `is_stdlib_call`；去掉「O(1)」注释 | `cce-stdlib/src/rust.rs` |
| 四 | `create_standalone_group` 增语言参数；两个 generator 的 `generate`、`convert_grouped` 及三个私有分支全部透传 `group.language` | `common/utils.rs`、`bm25/generator.rs`、`embedding/generator.rs`、`converter/entity_converter.rs`、`converter/group_converter.rs` |
| 五 | 噪声过滤器移动到 `common/`，更名 `filter_nl_noise`，六处引用改路径，`embedding` 不再导出 | `common/noise_filter.rs`、`common.rs`、`embedding.rs` 等 |
| 六 | 两个 dispatcher 的 `get_pattern_name` 增加 stdlib 分支返回 `Stdlib`；`parse_category` 接受 `other_stdlib`/`other` 并回读为 `Other` | 两个 `templates/dispatcher.rs`、`stdlib_classifier.rs` |
| 七 | 模板文档按代码重写：删除不存在的两个文件与 19 种 pattern，写明三条分发规则、优先级与只看 header 的边界 | `docs/parser/ast_to_nl/templates.md` |

验证结果：

* `cargo test -p cce-stdlib --lib` → 131 通过（新增 4 条：`category` 哨兵、关系域一致性、非调用域拒绝、包装/高阶调用回归）。
* `cargo test -p cce-parser --lib` → 1354 通过。
* `cargo check --workspace --all-targets` → 通过。
* `cargo clippy -p cce-stdlib -p cce-parser --all-targets` → 无告警。
* `cargo fmt` 后回退了两处与本改造无关的既有格式差异（`grouper/pipeline.rs`、`grouper/processors/small_fragment_merger.rs`），保持改动集中。

与方案的偏差，两处：

1. 阶段二的范围比方案多了一个 scala。它的 `is_stdlib_by_type` 手写体是 `is_stdlib_call` 的真子集（缺属性化接收者的兜底），归入同一宏后判定结果只会从「否」变成「是」，方向与 Go/Python 的修复一致，因此没有单列为待拍板项。
2. 测试里两份重复的 `create_standalone_group` helper 改为委托公共实现，避免测试自己再拼一遍 `EntityGroup` 的 19 个字段。
