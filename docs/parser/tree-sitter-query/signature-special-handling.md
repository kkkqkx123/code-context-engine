# 特殊签名处理参考

本文档记录实体签名提取中的特殊处理逻辑，供修改查询方案或解析器时参考。完整规范见同目录 `spec.md`。

## 一、总体决策顺序

签名提取入口 `extract_signature`（`crates/parser/cce-parser/src/parser/extractor/capture/parser.rs`）按以下顺序产出签名，前一步产出非空即终止：

1. 结构化组合：存在 `@entity.*.signature.*` 子捕获时，按源码顺序拼接各部分文本。
2. 头切片（过渡形态）：无组合子捕获但存在 `.body` 捕获时，取 body 起点之前的主捕获文本作为声明头，不清洗不截断。
3. 无体全文回退：主捕获所属实体概念无 body 语义时，全文即签名，按 500 字符封顶。
4. 计数缺失：有 body 语义却既无组合又无 body 捕获时，返回空并累加缺失计数，不做任何兜底。

头切片属于旧查询向签名别名迁移的过渡分支，新查询不应依赖它。

## 二、组合分支中的特殊角色

只有两种角色不保留原文：

- `.signature.source`（来源角色）：来源绑定的是数据而非声明。`utils::summarize_provenance_source` 保留集合头部，丢弃数据行：短来源（不超过 200 字符）原文透传；复合字面量 `类型{形状}{数据行}` 在类型/值接缝处截断保留类型头；其余长来源保留 200 字符预算内的前导完整行。所有摘要形态以 `...` 显式收尾，消费方不会误读为完整表达式。
- `.signature.arguments`（实参表角色）：块体代码不属于声明。`collapse_long_brace_blocks` 用字符串与注释感知的配平扫描，把超过 100 字符的花括号块折叠为 `{...}`，保留参数结构、标量实参和形参表；无法配平的尾部文本原文透传。

其余角色（name、params、return_type、type_params、base、extends、implements、description）一律原文保留，绝不截断——截断声明角色会把标识符从中间切断，制造虚假词项。

## 三、body 语义判定

`main_expects_body` 决定第 3、4 步的分支归属：主捕获名含 function、method、constructor、class、struct、enum、interface、trait、union、impl、getter、setter 等词时视为“有体概念”。

内联形态被排除在外，全文即其声明头：arrow、callback、lambda、closure、literal、comprehension、iife、top_call、top_if；变体类（variant、`enum_`、`enum.`）也在排除之列，避免枚举变体被误判为空签名。

新增语言模式时，内联形态与变体形态的捕获名必须落入排除词，否则会退化为空签名。

## 四、长度常量一览

| 常量 | 值 | 位置 | 作用范围 |
| --- | --- | --- | --- |
| `MAX_SIGNATURE_LEN` | 500 | capture/parser.rs | 仅无体全文回退分支 |
| `MAX_SOURCE_HEAD_LEN` | 200 | extractor/utils.rs | 仅来源角色摘要 |
| `MAX_INLINE_BLOCK_LEN` | 100 | capture/parser.rs | 仅实参表内块体折叠 |
| `MAX_ENTITY_NAME_LEN` | 200 | extractor/utils.rs | 实体名（非签名） |

这些界限只压数据角色；组合签名与头切片由构造保证有界，保持原文。

## 五、堆叠别名约定

签名子捕获以堆叠方式与裸捕获并写，例如 `@entity.function.name @entity.function.signature.name`。裸捕获继续服务名称提取等既有消费方，签名别名只参与组合。新增签名覆盖时保持堆叠写法，不删除裸捕获。

## 六、按语言的特殊处理

- JavaScript/TypeScript 回调：签名由被调者名加可选首个字符串实参（description）组成，函数/箭头实参捕获 body，整段调用文本不会进入签名。
- JavaScript/TypeScript 链式赋值：仅当链终值是函数表达式或箭头函数时才产出方法实体并穿透签名与体；非函数链回落为变量，不产出空签名方法。
- C# 命名空间与属性：块命名空间取签名名并捕获 declaration_list 为体，文件作用域命名空间只取名；属性取类型加名，可选访问器列表为体。
- TypeScript 命名空间/模块：签名名加可选 statement_block 体。
- Java 枚举常量：签名名为变体名，实参表为 `.signature.arguments`（受块体折叠约束），匿名类体单独作为 body。
- Python except：异常源与别名共同组成签名，冒号后的块作为 body。
- Go 等语言的 range/for 循环变量：迭代集合走来源角色，受头部摘要约束。
- 模式绑定 fan-out（loop/case 兄弟名，`entity_extractor.rs`）：下划线空白标识符不建实体；首个具名 sibling 承接主实体；全为空白则整组丢弃。

## 七、签名之外的同一数据角色

`source_type` 元数据与签名来源角色携带相同表达式，但分工不同：元数据保持原文供类型推断（`cce-codegraph` 的 variable_patterns）解析，自然语言发射时（embedding 模板的循环关系行）才做同样的头部摘要。修改来源处理时两处必须同步，只封签名会泄漏进检索文本。

## 八、有意不做的事

- 不做全签名总长度截断：成因治理优先（去空白实体、摘要数据行、折叠块体）；尾切破坏标识符且掩盖问题。
- 不新增长度/详略配置项：签名参与符号哈希（`StableSymbolKey`），配置分叉导致索引行为不确定。检索文本详略调节使用既有的 `ast_to_nl` BM25 关键词数、摘要词数与分块配置。
- 来源摘要与块体折叠只作用于数据角色；若未来语言出现新的“声明角色嵌块”模式，复用折叠逻辑，不另起截断。

## 九、验证入口

审计脚本 `cargo run --example sig_audit -p cce-parser -- --threshold 500` 扫描 `crates/app/cce-e2e-tests/fixtures` 下的真实夹具，输出空签名与超长签名计数及按语言/实体类别的分布。修改签名相关逻辑后以此脚本核对残留形态，配合 `cargo test -p cce-parser` 中的组合摘要、实参折叠、空白跳过与关系行回归测试。
