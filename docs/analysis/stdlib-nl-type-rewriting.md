# 自然语言转换里的标准库类型改写：可行性分析（下一阶段，暂不实现）

> 本文只做分析与方向比选，**不含任何代码改动**。本轮（标准库检测收口）只把 `is_stdlib` / `stdlib_category` 的判定与传播修好，未触碰 NL 生成侧。
> 配套：`docs/plan/stdlib-detector-wiring.md`（本轮改造方案与执行记录）、`docs/analysis/stdlib-and-template-callers.md`（现状盘点）。

---

## 0. 结论摘要

1. **类型文本进入 NL 的位置有四个，全部集中在两个文件族里**：签名（参数 + 返回值）走 `TemplateHelpers::build_signature_with_defaults`，字段类型走 `field_type` 的一次 `format!`，BM25 的 `Types:` 行是对同一批字段的第二次采集。四个落点都是纯字符串拼接，没有任何类型解析——这既是改造的便利，也是难点（没有 AST 可用）。
2. **原料已经就位**：`StdlibDetector::ref_category`（本轮新增）能把 `std::collections::HashMap` 这种路径直接判成 `Collection`；`StdlibCategory::description_label` 提供十个词的粗标签。但**粗标签不足以做改写**——`Vec` 和 `HashMap` 同属 `collection type`，"集合类型的 Vec" 对检索没有增益。
3. **本轮 P1 收窄是这项工作的前置条件**，而且已经落地：`mark_stdlib` 不再给项目自定义类型按名字打 stdlib（`struct Command` 从 `is_stdlib=true` 变 `false`）。若不先做这一步，任何"把 stdlib 类型改写成短语"的逻辑都会顺手改写项目自己的 `Command`、`Error`、`String`。
4. **三条候选方向**：A 逐类型词表（`Vec<T> → list of T`）、B 结构化短语（`Vec<T> → growable array`，不带类型参数）、C 原文 + 补语（`Vec<T> (list of T)`）。推荐 **A 的受限形态**（只替换嵌套 ≥2 层或带参数的泛型，其余保留原文），理由见 §6。
5. **需要拍板的不是技术，是口径**：改写会改变 BM25 的 token 分布与 embedding 的文本长度，属于检索行为变更，必须和召回指标一起放量（§8、§11）。

## 1. 事实基线：类型文本的四个落点

| # | 位置 | 作用 | 现状 |
|---|---|---|---|
| 1 | `common/templates/helpers.rs:32-59` `build_signature_with_defaults` | 拼 `(name: Type, …) -> Ret` | 纯字符串拼接，**没有 language 参数** |
| 2 | `embedding/templates/regular.rs:551-553` | 字段类型 `format!(": {}", field_type)` | 同上 |
| 3 | `embedding/templates/regular.rs:560-564` | 方法/函数签名调用落点 1 | 同上 |
| 4 | `bm25/templates/regular.rs:155-186` | 独立采集 `Types:` 行（参数类型 + 返回类型去重，上限 `MAX_MEMBER_TYPES`） | 与 #1 重复采集同一批字符串 |

两条路径（BM25 / embedding）对同一批类型各写一遍，但**都没解析类型语法**。这是关键约束：改写只能在字符串层面做，或者新引入一个小型类型解析器（§7）。

顺带一个观察：`bm25/templates/regular.rs:136`（成员名）与 `:157`（类型采集）都用 `member.is_stdlib` 跳过成员，`converter/group_converter.rs:945` 同样跳过——这是本轮 P1 收窄后真正生效的那条路径，也是"stdlib 成员进不了 NL"的现有机制。类型改写与它正交：前者省略整个成员，后者保留成员但改写其类型。embedding 侧不直接读 `is_stdlib`，它读 grouper 产出的 `role_map`（`embedding/templates/regular.rs:208-212`），stdlib 标记通过 `MemberRole::BoilerplateMethod` 传导。

## 2. 可复用的原料（本轮已就位）

| 能力 | 位置 | 本轮状态 |
|---|---|---|
| 名字/路径 → 类别 | `cce-stdlib/src/detector.rs` `category` / `ref_category` | `ref_category` 新增：`std::collections::HashMap` → `Collection`，`dart:async` → 命中 |
| 名字/路径 → 是否 stdlib | `is_stdlib_ref` / `is_stdlib_import_path` | 新增，取代四处前缀判断 |
| 类别 → 粗标签 | `cce-types/src/types/stdlib_category.rs:52-65` | 已有，十个词 |
| 实体上的缓存 | `Entity.stdlib_category`（解析期写入） | `mark_stdlib` 收窄为只标引用类实体 |

**结论：判定这一层不用再动**。剩下的是"类型名 → 人类可读短语"的映射表，以及"在哪替换"的工程问题。

## 3. 前置条件：为什么必须先做 P1（已完成）

改造前，`mark_stdlib` 对**所有** `entity.*` 捕获按名字查表（`entity_extractor.rs:619` 对所有实体调用）。实测同一份 Rust 源码：

```
改造前：Struct Command → is_stdlib=true  category=Some(Collection)
改造后：Struct Command → is_stdlib=false category=None
```

也就是说，一旦有了改写逻辑，`Vec<T>` 和用户自己写的 `Command` 会被同一套规则改写——因为两者在数据上无法区分。P1 之后 `is_stdlib` 只对引用类实体为真，改写逻辑可以安全地只看 `stdlib_category`。

同理，`TypeReference` / `Use` 边本轮才开始带 `stdlib_category`（`dependency_extractor.rs`），这意味着**边上的 stdlib 信息也可以作为改写的第二输入源**（例如判断某字段类型在项目里到底有没有被解析到）。

## 4. 三个候选方向

| 方向 | 形态 | 优点 | 缺点 |
|---|---|---|---|
| A 逐类型词表 | `Vec<T>` → `list of T`；`HashMap<K,V>` → `map from K to V` | 语义最准，检索增益最大 | 需要按语言维护一张逐类型表（几十到几百条），且要处理泛型参数重排 |
| B 结构化短语 | `Vec<T>` → `growable array`；`Result<T,E>` → `result` | 表小、实现简单 | 丢掉类型参数，`Vec<Command>` 与 `Vec<String>` 变成同一串，检索反而变糊 |
| C 原文 + 补语 | `Vec<Command>` → `Vec<Command> (list of Command)` | 不损失精确检索，纯增量 | 文本变长（对 embedding 的窗口与 BM25 的 term 频率都有影响），重复表达 |

推荐 **A 的受限形态**：

* 只改写"stdlib 类型 + 至少一个类型参数"，即 `Vec<T>`、`Option<T>`、`Result<T,E>`、`HashMap<K,V>`、`Promise<T>`、`List<T>`；
* 无参数的类型（`String`、`bool`、`i32`、`DateTime`）**不改写**——它们本身已经可读，改写纯属噪声；
* 嵌套 ≥2 层（`Option<Vec<HashMap<K,V>>>`）时只改写最外层，内层保留原文，避免短语爆炸；
* 数组/切片/指针这类**语法形式**（`&[String]`、`*const T`、`[]byte`、`T[]`）单独一条规则：`&[T] → slice of T`。

这个形态把表压到"每语言 5–15 条"，同时保住 `Vec<Command>` 与 `Vec<String>` 的区别。

## 5. 逐语言映射表草案

只列首批要覆盖的，表放在 `cce-stdlib`（与现有表同处，不新开一层）：

| 语言 | 原文 | 改写 |
|---|---|---|
| Rust | `Vec<T>` | `list of T` |
| Rust | `HashMap<K, V>` / `BTreeMap<K, V>` | `map from K to V` |
| Rust | `Option<T>` | `optional T` |
| Rust | `Result<T, E>` | `T or E error` |
| Rust | `Box<T>` / `Arc<T>` / `Rc<T>` | `shared T` |
| Rust | `&[T]` / `&str` | `slice of T` / `string slice` |
| Go | `[]T` / `map[K]V` / `chan T` | `list of T` / `map from K to V` / `channel of T` |
| Go | `*T` / `error` | `pointer to T` / `error` |
| Python | `List[T]` / `Dict[K, V]` / `Optional[T]` | 同 Rust 口径 |
| Java/Kotlin | `List<T>` / `Map<K,V>` / `Optional<T>` | 同上 |
| C# | `IEnumerable<T>` / `Task<T>` / `List<T>` | `sequence of T` / `async T` / `list of T` |
| TS | `Array<T>` / `Promise<T>` / `Record<K,V>` | `list of T` / `async T` / `map from K to V` |
| C++ | `std::vector<T>` / `std::unique_ptr<T>` / `std::optional<T>` | 同 Rust 口径 |
| Dart | `List<T>` / `Future<T>` / `Map<K,V>` | 同上 |
| Ruby/PHP | 无静态类型 | 不适用 |

注意 `Result<T, E>` → `T or E error` 是唯一需要**参数重排 + 词序变化**的条目，其余都是"关键字 + 参数顺序保留"，实现上可以先只做后者。

## 6. 为什么不建议纯 B 或纯 C

* 纯 B（丢参数）会让 `Vec<Command>`、`Vec<String>`、`Vec<T>` 塌成同一个短语。检索场景里"什么容器"远不如"装了什么"重要，丢参数等于丢掉最有判别力的信息。
* 纯 C（原文 + 补语）在 BM25 侧会把 `Vec` 这个 token 的文档频率抬高（每个用 `Vec` 的函数都多一次出现），而 `Types:` 行本身已经是去重后的短列表，加长后更容易撞上 `MAX_MEMBER_TYPES` 上限，反而丢类型。
* A 的受限形态相当于"在原文不可读的地方补一句人话"，改动面最小，且天然可回滚（只影响生成侧，不回写实体）。

## 7. 工程落点与签名变更

| 步骤 | 内容 | 影响面 |
|---|---|---|
| 1 | `cce-stdlib` 新增 `describe_type(name, language) -> Option<&'static str>` 或 `expand_type(raw, language) -> Option<String>` | 1 个文件 + 映射表 |
| 2 | `build_signature_with_defaults` 增加 `language` 参数（与上一轮 `create_standalone_group` 的透传形状相同） | 1 个函数 + 3 个调用点 |
| 3 | `embedding/templates/regular.rs:553` 的字段类型改走同一入口 | 1 行 |
| 4 | `bm25/templates/regular.rs:157-186` 的 `Types:` 采集改走同一入口 | 1 个循环 |
| 5 | 快照测试与 token 断言 | 见 §8 |

第 2 步是唯一有扩散成本的：`language` 目前挂在 `EntityGroup` 上，两条模板路径都能拿到（`group.language`），但 `helpers.rs` 在 `common/` 下、不持有组，必须显式传入。这与上一轮给 `create_standalone_group` 加语言参数是同一个形状，风险已知。

## 8. 检索影响与回归锚点

改写改变的是**索引文本**，不影响任何图数据，因此可以独立放量：

| 观察项 | 现有指标 | 期望 |
|---|---|---|
| BM25 召回 | 检索评测集（`crates/app/cce-e2e-tests`） | 不下降 |
| NL 文本长度 | 无专项指标；建议在生成侧按 `group_type` 记直方图 | 中位数上升 < 15%，长尾（>2KB）不增加 |
| `Types:` 行截断 | `MAX_MEMBER_TYPES` 命中次数（当前未计数） | 若显著上升说明改写让类型更长，需要调整上限 |
| embedding 维度溢出 | 现有截断逻辑 | 无变化（改写只在短语级别） |

回归锚点建议直接复用探针（§附录），把 `Vec<T>` / `Option<Vec<T>>` / `Result<T, E>` 三类形态的输出钉住。

## 9. 插件语言与未知语言

插件语言的检测表由插件提供（`metadata["is_stdlib"]` + `stdlib_category`），**没有类型语法知识**。策略：

* 默认关闭改写，只走现有 `StdlibTemplate`；
* 若插件在 metadata 里提供 `type_aliases`（形如 `Vec=T list of T`），再开启；
* 未注册语言的 `Language::Custom(_)` 一律不改写。

这样改写能力不会成为插件契约的一部分，避免又一次"同一份知识写两遍"。

## 10. 风险清单

| 编号 | 风险 | 缓解 |
|---|---|---|
| R1 | 改写让精确检索退化（用户搜 `Vec<Command>` 搜不到，因为索引里变成了 `list of Command`） | 采用 A 的受限形态 + 原文保留内层；必要时对 `Types:` 行保留原文、只对 `Methods:` 签名改写 |
| R2 | 类型字符串不是合法类型（解析期可能带 `<closure>`、错误恢复残留） | 改写前做形态校验，不匹配就原样输出 |
| R3 | 逐类型表与检测表脱节（表里写了 `Vec`，检测表改了名） | 映射表的键必须是 `cce-stdlib` 现有表的子集，加一条同步测试 |
| R4 | 两条 NL 路径改写不一致 | 只允许一个入口（`helpers.rs` + `Types:` 各调一次同一函数） |
| R5 | 与 `is_stdlib` 成员跳过逻辑互相干扰（成员被跳过，类型改写没机会执行） | 改写只作用于**保留下来**的成员，顺序无关 |

## 11. 待拍板

| 编号 | 问题 | 建议 |
|---|---|---|
| N1 | 走 A（逐类型短语）、B（粗短语）还是 C（原文 + 补语）？ | A 的受限形态（§4） |
| N2 | 改写范围是"全部类型"还是"仅泛型/带参数类型"？ | 仅泛型/带参数类型，无参数类型保持原样 |
| N3 | BM25 的 `Types:` 行是否也改写？（它服务关键词匹配，改写可能伤召回） | 建议**不改写**，只改 `Methods:` 签名与 embedding 文本，先看一轮指标 |
| N4 | 原文是否保留到文本里？ | 保留内层原文，外层替换（`Vec<Command>` → `list of Command`） |
| N5 | 映射表放 `cce-stdlib` 还是 `cce-types`？ | `cce-stdlib`：它已经是类型知识的唯一归属，`cce-types` 只放类别枚举 |

## 附录：复现

```shell
# 探针（/tmp/probe，path 依赖 cce-stdlib + cce-types + cce-parser）
cd /tmp/probe && CARGO_TARGET_DIR=/workspace/code-context-engine/target cargo run -q
```

当前输出（本轮改造后，可作为改写前的基线）：

```
--- entities ---
Import         std::collections::HashMap        is_stdlib=true  category=Some(Collection)
Struct         Command                          is_stdlib=false category=None
--- relations ---
Use                  std::collections::HashMap          stdlib_category=Some(Collection)
TypeReference        std::io::Error                    stdlib_category=Some(Error)
TypeReference        HashMap::new                       stdlib_category=Some(Collection)
--- nl ---
pattern=Regular len=136
Command
A CLI command wrapper defined by the project itself.
Members: name tags parse.
Types: &[String] Result<Command, std::io::Error>.
```

`Types:` 行里的 `&[String]`、`Result<Command, std::io::Error>` 正是 §5 要处理的两种形态。
