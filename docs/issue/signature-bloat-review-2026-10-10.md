# 符号签名臃肿问题分析（relation_review / structured 输出复核）

检查时间：2026-10-10。输出为仓库现场已有产物，与当前源码一致。

检查范围：

- `crates/app/cce-e2e-tests/outputs/scenarios/python/relation_review/flask/`（8 种子，`entity/` + `graph/*.json` + `index.md`）
- `crates/app/cce-e2e-tests/outputs/scenarios/rust/relation_review/once_cell/`（9 种子，同上结构）
- 对照基准：`outputs/scenarios/python/structured/flask/`、`outputs/scenarios/rust/structured/once_cell/`、`outputs/scenarios/{go,java,typescript,javascript,php,lua,cpp,c,scala,dart}/structured/` 的按文件报告
- 源码：符号提取（`cce-parser` 的查询方案与捕获解析）、关系图服务（`cce-orchestrator` 的图模型组装）、结构化报告渲染（`cce-e2e-tests`）、签名参与的下游（对齐键、关键词、落库）

结论：**签名处理存在明确缺陷，且是系统性的**。Python 符号的 `signature` 字段实际装的是“定义头 + 文档串 + 完整函数体 / 完整类体”，最长达数千字符；Rust 符号干净（普遍几十字以内）并非因为逻辑正确，而是因为只有 Rust 走了另一条分支。 brace 语言靠运气通过，冒号 / `end` 风格语言必然中招。

---

## 1. 现象：Python 签名里装的是完整代码

### 1.1 relation_review 图快照（机器可读 JSON，`graph/*.json` 的 `nodes[].signature`）

Python 侧全部超标，Rust 侧全部正常：

| 节点 | kind | 签名长度 | 内容物 |
|---|---|---:|---|
| `send_file`（`helpers.py:417`） | function | 5280 字符 / 124 行 | 参数表 + 完整文档串 + 完整函数体 |
| `test_request_context`（`app.py:1517`） | method | 2088 字符 / 48 行 | 参数表 + 完整文档串 + 函数体开头 |
| `send_from_directory`（`helpers.py:543`） | function | 1580 字符 / 42 行 | 参数表 + 完整文档串 + `return` 实现 |
| `test_static_file`（`test_helpers.py:44`） | method | 1420 字符 / 39 行 | 完整测试函数体（含注释与内嵌类定义） |
| `flash`（`helpers.py:326`） | function | 1380 字符 / 32 行 | 参数表 + 文档串 + 函数体 |
| `_prepare_send_file_kwargs`（`helpers.py:402`） | function | 427 字符 / 13 行 | 定义行 + 完整函数体 |
| `test_send_file`（`test_helpers.py:34`） | method | 351 字符 / 9 行 | 定义行 + 完整函数体 |
| `App`（`sansio/app.py`） | class | 7492 字符 | 类头 + 文档串 + 类体开头 |
| `AppContext`（`ctx.py`） | class | 6879 字符 | 类头 + 文档串 + 类体 |
| `Flask`（`app.py`） | class | 4102 字符 | 类头 + 文档串 + 类体 |
| `Blueprint`（`sansio/blueprints.py`） | class | 3189 字符 | 类头 + 文档串 + 类体 |
| `Blueprint`（`blueprints.py`，第二个同名节点） | class | 3065 字符 | 类头 + 紧跟的 `def __init__` 及参数表（类体泄漏） |
| `DefaultJSONProvider` | class | 2927 字符 | 类头 + 文档串 + 类体 |
| `JSONProvider` | class | 3214 字符 / 87 行 | 类头 + 文档串 + 类体 |
| `FlaskClient` | class | 770 字符 | 类头 + 文档串 |
| `FakePath`（`test_helpers.py:11`） | class | 297 字符 / 12 行 | 类头 + 文档串 + 两个方法全文 |
| `StaticFileApp`（测试内嵌类） | class | 114 字符 / 3 行 | 类头 + 内嵌方法全文 |
| `read`（`test_basic.py:507`） | function | 83 字符 / 3 行 | 装饰器 + 定义 + `return` 体 |

其中 `read` 的签名把调用点装饰器也带了进来：

- 期望的签名只是 `def read()` 一行；
- 实际是三行：装饰器行、定义行、函数体行。

`_prepare_send_file_kwargs` 同理：期望到 `-> dict[str, t.Any]:` 结束，实际把 `ctx = ...`、`if ...`、`kwargs.update(...)`、`return kwargs` 全部收录。

Rust 侧对照（`once_cell` 全部图快照）：

| 节点 | kind | 签名长度 | 内容物 |
|---|---|---:|---|
| `impl<T> OnceCell<T>` | inherent_impl | 19 字符 | 干净的 impl 头 |
| `<T>` | struct | 3 字符 | 干净的泛型头 |
| `unsafe impl<T: Sync + Send> Sync for OnceCell<T>` | trait_impl | 48 字符 | 干净的 trait impl 头 |
| `new () OnceCell<T>` | function | 18 字符 | 干净的函数头 |
| `with_value (value: T) OnceCell<T>` | function | 33 字符 | 干净的函数头 |
| `mod once_box` / `pub mod sync` | module | 12–14 字符 | 干净的模块头 |

同一套图服务、同一个 `GraphNode.signature` 字段，两批数据的数量级差了两个数量级，说明问题出在上游的签名生产环节，而不是下游渲染。

### 1.2 structured 按文件报告的旁证

`structured/flask/src/flask/*.txt` 的 `Classs` 表直接打印原始签名（见第 3 章引用），Python 类签名行普遍数百到数千字符，且包含明显的函数体标记：`def `、`"""`、`return `、`self.`、`pass`、`if `、`for `。例如 `_CollectErrors` 一行 944 字符，内含 `__init__`、`__enter__`、`__exit__`、`raise_any` 四个方法的定义与实现；`Flask`、`App`、`Request` 等核心类均为整类体泄漏。

对全部 `structured/**` 按文件报告做同一口径扫描（类表的签名列长度超过 300 即抽看）：超标文件全部来自 Python，逐行均命中上述体标记；抽查到的非 Python 类签名（Go、Java、TypeScript、PHP、Dart、Scala、C、Lua、Cpp）均无体标记，长度超标的也只是字段列表长（如 Java 的常量表、Rust 的参数结构体字段表），签名头本身干净。这与 1.1 的结论互相印证：**当前输出物中实际爆雷的是 Python，但病根覆盖所有非 brace 语言**。

---

## 2. 根因：两级签名提取只有 Rust 走了第一级，第二级只认识 `{`

签名生产链路只有三站：

- 查询方案声明“签名子捕获”（如 `entity.function.signature.params`），
- 捕获解析按优先级组装签名，
- 图服务与报告层原样透传。

### 2.1 第一级：只有 Rust 声明了签名子捕获

`crates/parser/cce-parser/src/tree_sitter_query/scheme/rust.rs` 是全仓唯一包含 `.signature` 子捕获的查询方案（结构体泛型、函数名、参数、返回类型均有独立子捕获）。其余 20 余个方案（`python.rs`、`go.rs`、`java.rs`、`javascript.rs`、`typescript.rs`、`tsx.rs`、`php.rs`、`dart.rs`、`c.rs`、`cpp.rs`、`csharp.rs`、`kotlin.rs`、`scala.rs`、`lua.rs`、`ruby.rs`、`bash.rs` 等）均无任何 `.signature` 子捕获。

对应地，Python 的实体查询把 `body: (block)` 直接绑在主捕获上：类定义、函数定义、装饰函数、类内方法、装饰方法、泛型函数全部以“定义（含体）”为粒度捕获（`crates/parser/cce-parser/src/tree_sitter_query/scheme/python.rs` 的类与函数节）。主捕获的字节区间天然包含整个函数体 / 类体。

### 2.2 第二级回退：`extract_signature_from_text` 只会砍 `{`

`crates/parser/cce-parser/src/parser/extractor/capture/parser.rs` 的提取优先级是：先尝试从签名子捕获重组；子捕获为空则取主捕获全文，再调 `extract_signature_from_text` 修剪。而该修剪函数的全部逻辑是找第一个 `{` 并截断：有 `{` 则取之前部分并做行级清理；无 `{` 则**原样返回全文**。

这正好解释了观测到的分布：

- Rust 命中第一级，签名是子捕获按源码顺序拼接的干净头，所以 `once_cell` 全干净。
- C 系 brace 语言（C、C++、Java、Go、JavaScript、TypeScript、PHP、C#、Dart、Scala、Kotlin 等）命中第二级但恰好有 `{`，函数体 / 类体被 `{` 挡在外面，头部分幸免。这不是“处理正确”，只是分隔符巧合。
- Python（`def ...:` + 缩进块）、Lua（`function ... end`）、Ruby（`def ... end`）等无 `{` 语言命中第二级且无分隔符可砍，全文直通 `entity.signature`。Python 的类（含方法体）、函数（含文档串与实现）、装饰器（主捕获从装饰器起始）全部泄漏，`relation_review` 与 `structured` 的超长签名即来源于此。

### 2.3 透传链：下游没有任何一道关卡

- 装配点 `crates/parser/cce-parser/src/parser/extractor/entity_extractor.rs` 直接把上述结果写入 `entity.signature`，无长度上限、无归一化、无语言分支。
- 图服务 `crates/app/cce-orchestrator/src/query/graph/service.rs` 在组装 `GraphNode` 时仅做“非空即透传”，直接把该字段塞进图节点。
- 结构化报告 `crates/app/cce-e2e-tests/src/structured_output/reports.rs` 的类 / 实现 / 其他实体三张表同样是“空则用名、非空原样打印”。

因此一旦上游装入完整代码，存储快照、图 JSON、Markdown 表格、HTTP 图接口会同时被污染。阅读 `graph/*.json` 时看到的“单行超 2000 字符被阅读工具截断显示”只是显示侧的自我保护，并非落盘时已截断；实测 `send_file` 签名 5280 字符完整落盘。

---

## 3. 现有实现的具体问题清单

按严重度排序，均为本次输出复核中可直接举证的项：

1. **无分隔符语言的回退分支等价于无处理**。`extract_signature_from_text` 对无 `{` 文本返回全文，导致 Python 函数签名包含文档串与实现、类签名包含全部方法。这是本次最严重的问题，也是 `send_file`、`App`、`AppContext` 等超长签名的直接来源。
2. **类签名把类体当签名**。Python 类主捕获含 `body: (block)`，回退又不砍块，`_CollectErrors`、`Blueprint`、`FakePath`、`StaticFileApp` 等签名里出现 `def __init__` 乃至完整方法实现。类签名期望只是 `class Name(Bases):` 头一行。
3. **函数签名把文档串与实现当签名**。`flash`、`send_file`、`send_from_directory`、`test_request_context` 的签名均含完整文档串；`_prepare_send_file_kwargs`、`test_send_file`、`test_static_file` 含完整函数体。文档串已有独立的 `doc_comment` 通道（Python 注释查询已覆盖文档串），当前行为造成同一段文档串在 `signature` 与 `doc_comment` 两处重复存放。
4. **装饰器泄漏进签名**。`read` 签名以 `@app.route("/read")` 开头。主捕获对装饰定义从装饰器起始，回退不剥装饰器，签名与“可调用头”的定义相悖。
5. **嵌套定义泄漏**。`test_static_file` 签名内含内嵌类 `StaticFileApp` 及其方法；`StaticFileApp` 签名内含其方法实现；`Blueprint`（`blueprints.py`）签名内含紧随的 `__init__` 参数表。 span 覆盖嵌套体是正确的，但签名应当只取首层头。
6. **同一字段两种形态**：Rust 图节点签名是无换行的干净头；Python 图节点签名是保留原始换行的多行全文（`JSONProvider` 87 行、`send_file` 124 行）。下游按单行展示（Markdown 表格、Mermaid 标签、前端表格）时必然破版或被迫截断。
7. **无长度上限与无归一化**。签名无字符上限（人名有 `MAX_ENTITY_NAME_LEN` 截断，签名没有），无空白归一（brace 分支做了行拼接，非 brace 分支连这一步都没有），导致对齐键哈希（`cce-types` 的签名寻址键对签名做空白归一后哈希）每次都要处理数千字符，且噪声（实现细节、注释、内嵌定义）会直接污染身份区分。
8. **关键词与检索被正文污染**。`cce-orchestrator` 的展示关键词从 `entity.signature` 提取词条；签名里装着实现体，等于把函数体词频混入“签名关键词”，与参数表 / 返回类型本应提供的干净信号相抵触。
9. **结构化报告的类表不可读**。`helpers.py.txt` 的 `_CollectErrors` 行、`sansio/app.py.txt` 的 `App` 行等均为近千到数千字符的单行，直接破坏表格可读性；这是报告层“原样打印”与上游“装入全文”共同作用的结果。
10. **测试覆盖只守住了 Rust**。`rust.rs` 内有签名子捕获的存在性断言与函数 / 结构体 / impl 提取单测；其他语言既无子捕获，也无“签名不含体”的断言，Python 回归无从发现。`python.rs` 的单测仅校验查询语法有效性。

需要澄清的一个非问题：Go/Java/Rust 结构化报告中偶见的千字符级“长行”，经拆列核实是字段列表长而非签名含体，不属于本缺陷；不要把“字段多”与“签名含体”混为一谈。

---

## 4. 影响面

- **存储与快照**：实体表的签名列、关系快照、图导出 JSON 均被放大一到两个数量级；Python 大类（`App` 7492 字符、`AppContext` 6879 字符）是典型例子。
- **图接口与前端**：`GraphNode.signature` 经 HTTP 图接口直达前端，前端虽有表格截断显示，但传输与内存成本已付出。
- **符号身份**：签名寻址的对齐键把归一化签名作为哈希输入，超长且含实现的签名使哈希成本上升，并让“实现改动”具备改变“签名身份”的能力，语义上是不稳定的。
- **复核成本**：`relation_review` 图 JSON 与 `structured` 类表是人工复核的主入口，当前形态下审查者需要在完整代码中肉眼寻找签名头，与“快速比对”的目标相反。
- **潜在波及**：Lua、Ruby 等同样无 `{` 的语言在现有输出矩阵中尚无关系图快照，但走的是同一回退分支，预期同样含体，只是本次没有可直接量化的产物。

---

## 5. 修复方向

只给方向，不给完整代码：

- 查询层优先：为 Python（以及 Lua、Ruby 等非 brace 语言）补签名子捕获（装饰器、函数名、参数表、返回标注、类名、基类表），使第一级重组分支生效；这是与 Rust 对齐的正道，也是唯一能同时解决装饰器、文档串、嵌套体三类泄漏的做法。
- 回退分支按语言分流：在子捕获缺失时，不应再用统一的“找 `{`”逻辑；至少按 brace / 冒号块 / `end` 块三类分隔符分别取头，并剥离装饰器行与行尾注释。回退只应作为兜底，其输出必须满足“不含体”的不变式，不满足则宁可返回空（下游已有空签名的处理路径）。
- 归一化与上限：在装配点对签名做统一后处理——空白归一为单行、剥离文档串、截断上限（如数百字符级，具体阈值由存储与展示共同决定），超限部分丢弃而非截断后仍保留体片段。
- 职责澄清：文档串归 `doc_comment`，头归 `signature`，体归 `span` 对应的源码读取；三者不应互相重复存放。类签名只取类头，方法体不属于类签名。
- 测试补齐：为每种语言加“签名不含体”断言（定义头之后出现体标记即失败），Python 优先覆盖普通函数、装饰函数、方法、类、文档串、嵌套定义六种形态；现有 Rust 签名单测保持不动。
- 数据重建：本缺陷污染的是落盘数据，修复后需全量重建索引与导出物；项目处于开发期，无需兼容旧签名，不做迁移。

---

## 6. 复核方法与证据位置

- 逐节点签名长度统计：解析 `outputs/scenarios/python/relation_review/flask/graph/*.json` 的 `nodes[].signature` 并按长度排序，`send_file`、`App`、`AppContext`、`Flask`、`Blueprint` 居前；同法解析 `outputs/scenarios/rust/relation_review/once_cell/graph/*.json`，最长仅数十字符。
- 体标记扫描：对 `outputs/scenarios/python/structured/flask/src/flask/*.txt` 的类表签名列检查 `def `、`"""`、`return `、`self.` 等标记，命中即为含体；对非 Python 同目录报告执行同一扫描，应无命中。
- 代码定位：签名回退逻辑见捕获解析的签名节；Python 查询的体绑定见 Python 查询方案的类与函数节；Rust 子捕获见 Rust 查询方案的函数与结构体节；装配写入见实体提取器的捕获级提取段；透传见图服务的节点组装与结构化报告的类表渲染；签名参与身份与关键词见对齐键与展示关键词的相关实现。
