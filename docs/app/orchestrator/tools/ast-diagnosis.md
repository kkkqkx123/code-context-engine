## 一、可发现的常见问题列表

基于 tree-sitter 解析（排除 Python 缩进问题），以下语法错误能够通过 `ERROR` 节点或 `missing` token 准确定位：

| 序号 | 错误类型 | 典型示例（C/C++） | tree-sitter 表现 | 定位精度 |
|------|----------|-------------------|------------------|----------|
| 1 | 缺少分号 | `int a = b` | `ERROR` 节点覆盖表达式末尾，或 `missing ';'` | 高（指向缺失位置） |
| 2 | 未闭合字符串 | `char s[] = "abc` | `ERROR` 节点包含不完整的 `string_literal` | 高（指向字符串起始） |
| 3 | 未闭合括号/花括号 | `if (x) {` | 大范围 `ERROR` 节点从开括号延伸到文件末尾 | 中（指向开括号，需推断闭合点） |
| 4 | 未闭合方括号 | `arr[0] = 1` 中缺少 `]` | `ERROR` 节点覆盖 `[` 及之后内容 | 高（指向 `[` 后） |
| 5 | 缺少右括号/右花括号 | `int fun() {` 缺少 `}` | 大范围 `ERROR` 节点，可能伴随 `missing '}'` | 中（需多候选） |
| 6 | 数组声明语法错误 | `char[] a = "123"` | 细粒度 `ERROR` 节点覆盖 `[` 和 `]` 之间 | 高 |
| 7 | 不完整的声明 | `int` （无标识符） | `ERROR` 节点覆盖 `int` 之后 | 高 |
| 8 | 表达式不完整 | `a +` （缺少右操作数） | `ERROR` 节点覆盖 `+` 及之后 | 高 |
| 9 | 非法 token | `int @foo = 1` | `ERROR` 节点覆盖非法字符 `@` | 高 |
| 10 | 函数参数列表不完整 | `int fun(int a, )` | `ERROR` 节点覆盖多余的逗号及之后 | 高 |
| 11 | 缺少函数体 | `int fun();` 后无定义但期望有 `{}` | `ERROR` 节点或解析为声明，需上下文 | 低（可能不产生错误） |
| 12 | 多余的标点符号 | `int a = b;;` | 第二个分号产生 `ERROR` 节点 | 高 |
| 13 | 结构体/类定义不完整 | `struct Foo { int x;` 缺少 `};` | `ERROR` 节点覆盖定义结束位置 | 中 |
| 14 | 预处理指令错误 | `#include <stdio.h` 缺少 `>` | `ERROR` 节点覆盖指令 | 高 |
| 15 | 枚举定义错误 | `enum Color { RED, GREEN` 缺少 `}` | 大范围 `ERROR` 节点 | 中 |

**示例**
```cpp
int a=b
int a
```

解析得到：
```
translation_unit [0, 0] - [1, 5]
ERROR [0, 0] - [1, 5]
type: primitive_type [0, 0] - [0, 3]
init_declarator [0, 4] - [0, 7]
declarator: identifier [0, 4] - [0, 5]
value: identifier [0, 6] - [0, 7]
identifier [1, 0] - [1, 3]
identifier [1, 4] - [1, 5]
```

```cpp
int fun(){
//这里缺少结束标记。但也可以理解为fun2后未正确闭合。可以诊断出多种情况
int fun2(){}
```

解析得到：
```
translation_unit [0, 0] - [2, 12]
ERROR [0, 0] - [2, 12]
type: primitive_type [0, 0] - [0, 3]
function_declarator [0, 4] - [0, 9]
declarator: identifier [0, 4] - [0, 7]
parameters: parameter_list [0, 7] - [0, 9]
comment [1, 0] - [1, 39]
type: primitive_type [2, 0] - [2, 3]
init_declarator [2, 4] - [2, 12]
declarator: function_declarator [2, 4] - [2, 10]
declarator: identifier [2, 4] - [2, 8]
parameters: parameter_list [2, 8] - [2, 10]
value: initializer_list [2, 10] - [2, 12]
```

```cpp
char[] a="123123"
char str[] ="123123"
char str2[] ="123123
```

解析得到：
```
translation_unit [0, 0] - [2, 20]
declaration [0, 0] - [0, 17]
type: primitive_type [0, 0] - [0, 4]
declarator: init_declarator [0, 4] - [0, 17]
declarator: structured_binding_declarator [0, 4] - [0, 8]
ERROR [0, 5] - [0, 6]
identifier [0, 7] - [0, 8]
value: string_literal [0, 9] - [0, 17]
string_content [0, 10] - [0, 16]
declaration [1, 0] - [1, 20]
type: primitive_type [1, 0] - [1, 4]
declarator: init_declarator [1, 5] - [1, 20]
declarator: array_declarator [1, 5] - [1, 10]
declarator: identifier [1, 5] - [1, 8]
value: string_literal [1, 12] - [1, 20]
string_content [1, 13] - [1, 19]
ERROR [2, 0] - [2, 20]
type: primitive_type [2, 0] - [2, 4]
array_declarator [2, 5] - [2, 11]
declarator: identifier [2, 5] - [2, 9]
string_content [2, 14] - [2, 20]
```


**说明**：
- “定位精度高”表示 tree-sitter 通常能给出一个很小的 `ERROR` 节点（覆盖 1~2 个 token），可以直接取起始点作为错误位置。
- “定位精度中”表示 `ERROR` 节点范围较大（如跨多行），需要结合括号平衡扫描或语言特定细化才能给出精确点。
- Python 缩进错误（如混合空格/制表符、不正确的 dedent）不在此列表内，因为 tree-sitter-python 不产生 `ERROR` 节点。

---

## 二、核心处理逻辑设计

### 2.0 模块结构

实现位于 `crates/app/cce-orchestrator/src/tools/ast_diagnosis/`：

```
ast_diagnosis/
├── mod.rs                 # 请求/响应类型（DiagnosisRequest/Response、Diagnostic、DiagnosticKind 等）
├── diagnosis.rs           # 主流程 AstDiagnosis::diagnose
├── error_collector.rs     # ErrorCandidate / ErrorCandidateKind / ErrorCollector
└── strategies/
    ├── strategies.rs（父级）  # refine_diagnostics 静态分发入口
    ├── c_family.rs        # C / C++ / Java / C#
    ├── rust.rs            # Rust
    ├── python.rs          # Python（专门策略）
    ├── javascript.rs      # JavaScript / TypeScript
    └── common.rs          # 其他语言的兜底策略
```

### 2.1 总体流程

```
源代码文本（DiagnosisRequest { code, language?, file_name?, include_ast? }）
    │
    ▼
[语言识别] → 显式 language 优先，其次按 file_name 扩展名检测；两者皆无则报错
    │
    ▼
[语言支持检查] → AstParser::is_supported，不支持则返回 UnsupportedLanguage
    │
    ▼
[解析] → AstParser::parse_with_tree 得到 tree-sitter 语法树与内部 AST
    │
    ▼
[遍历收集] → ErrorCollector::collect：深度优先遍历，
             收集 ERROR 节点（ErrorCandidateKind::Error）
             与 missing token（ErrorCandidateKind::Missing）
    │
    ▼
[统一细化] → strategies::refine_diagnostics 按语言静态分发到
             c_family::refine / rust::refine / python::refine /
             javascript::refine / common::refine
             （各策略内部完成最内层过滤、去重、排序与位置细化）
    │
    ▼
[输出] → DiagnosisResponse { language, is_valid, diagnostics, ast? }
         is_valid = diagnostics.is_empty()
```

此外提供轻量入口 `AstDiagnosis::is_valid(code, language)`：仅检查是否存在 ERROR/missing 节点，不产出诊断明细。

尽量复用项目已有功能

### 2.2 关键模块详解

#### (1) 语言识别与支持检查
- `DiagnosisRequest::with_language` 显式指定优先；否则按 `file_name` 扩展名经 `LanguageInfo::detect_from_path` 检测；两者皆无则返回 `LanguageDetection` 错误。
- `AstParser::is_supported` 决定该语言是否可诊断；`AstDiagnosis::supported_languages()` 列出全部支持语言（C、C++、C#、JavaScript、TypeScript、Rust、Go、Java、Python）。

#### (2) tree-sitter 解析调用
- 使用 tree-sitter 的 `parser` 对象，设置语言，解析源代码为字节流。
- 获取根节点（`root_node`）。

#### (3) 遍历收集 ERROR 节点
- 递归遍历每个节点：
  - 若节点类型为 `"ERROR"`，将其加入候选列表。
  - 若节点有 `missing` 标记（通过检查子节点或字段），单独记录。
- 遍历与后处理集中在 `ErrorCollector`：`collect` 收集候选后由 `collect_and_process` 依次执行 `filter_innermost`（最内层过滤）、`deduplicate`、`sort_by_position`。

#### (4) 位置提取
- 使用节点提供的 `start_point`（行、列）和 `end_point`。
- 对于 `missing` token，其位置通常为预期位置（如分号应出现的位置），直接提取。

#### (5) 语言特定细化
- 由 `strategies::refine_diagnostics` 统一入口按语言静态分发到各语言的 `refine` 实现；各语言策略内部组合所需的细化手段（如括号平衡检查、token 级位置修正）。

#### (6) 去重与排序
- 将位置转换为元组 `(line, column)`，放入集合去重。
- 按 `line` 升序，`column` 升序排序。

### 2.3 输出格式

统一输出结构化的 `DiagnosisResponse`（不再有 `file:line:column` 文本与 JSON 二选一的格式）：

- `language`：诊断使用的语言
- `is_valid`：布尔值，等价于 `diagnostics` 为空
- `diagnostics`：`Vec<Diagnostic>`，每项含 `message`、`kind`（`DiagnosticKind`）、`precision`（`DiagnosticPrecision`）及可选 `span`
- `ast`：请求 `include_ast = true` 时返回完整 AST 树，否则为 `None`

**说明**：Python 不再"跳过"——`strategies/python.rs` 提供了专门的 `python::refine` 策略。只是 tree-sitter-python 对缩进错误通常不产生 `ERROR` 节点（见第一节的说明），Python 策略在此基础上处理其候选特征。

### 2.4 边界情况处理

- **解析失败**：tree-sitter 解析器本身可能抛出异常（如内存不足），此时应捕获并返回错误信息。
- **空文件**：返回空列表。
- **多语言混合**：单个文件只使用一种语言的解析器，不考虑嵌入式代码（如 HTML 中的 JS）。

### 2.5 性能考虑

- 单次解析 + 遍历整个语法树（复杂度 O(N)）。
- 嵌套过滤需要 O(M²) 比较 M 个 ERROR 节点，但 M 通常很小（< 100），可接受。
- 避免对整个文件进行多次扫描，除非细化器需要额外分析（如括号平衡扫描，也是 O(N)）。

### 2.6 扩展性

- 新增语言：添加专门的实现，组合各个策略。
- 策略实现：每个语法特征可独立实现(例如py的缩进问题只会产生混乱的ast节点，不会生成ERROR节点。可以基于缩进计数来判断)

组合方式：错误候选由 `ErrorCandidateKind::{Error, Missing}` 统一建模，语言特定的细化逻辑组合在各语言策略（`strategies/{c_family,rust,python,javascript}.rs`）内部完成，由 `refine_diagnostics` 按语言静态分发（零动态分发开销）：

```
初步候选（ErrorCollector::collect，来自 tree-sitter ERROR / missing）
    │
    ▼
[refine_diagnostics 按语言分发]
    │
    ├─ C/C++/Java/C# → c_family::refine（含 IllegalToken、字符串未闭合、
    │                    括号平衡、缺少分号等策略组合）
    ├─ Rust           → rust::refine
    ├─ Python         → python::refine（专门策略，不再跳过）
    ├─ JS/TS          → javascript::refine
    └─ 其他           → common::refine（兜底）
    │
    ▼
最终候选（去重、排序后输出为 Vec<Diagnostic>）

---

## 三、总结

本设计基于 tree-sitter 的 `ERROR` 节点，能够定位绝大多数语言的常见语法错误。通过“最内层过滤”保证位置最小化，通过可选的细化器改善部分语言（如 C++）的定位精度。
