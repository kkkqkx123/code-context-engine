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

### 2.1 总体流程

```
源代码文件
    │
    ▼
[语言识别] → 根据扩展名/shebang 确定语言
    │
    ▼
[语言配置] → 检查是否支持 tree-sitter（Python 直接跳过/返回空）
    │
    ▼
[解析] → 调用对应语言的 tree-sitter 解析器，得到语法树
    │
    ▼
[遍历收集] → 深度优先遍历，收集所有 ERROR 节点 + missing token 节点
    │
    ▼
[最内层过滤] → 如果 ERROR 节点内部包含子 ERROR 节点，只保留最内层
    │
    ▼
[位置提取] → 对每个保留的节点，提取起始行列号
    │
    ▼
[语言特定细化]（可选）→ 针对某些语言（如 C++）调整位置（如从大范围 ERROR 中提取真正出错点）
    │
    ▼
[去重 & 排序] → 按行号、列号排序，移除重复位置
    │
    ▼
[输出] → 打印或返回 JSON 格式的错误点列表
```

尽量复用项目已有功能

### 2.2 关键模块详解

#### (1) 语言识别与配置
- 支持通过文件扩展名映射（`.c`→`c`，`.cpp`→`cpp`，`.js`→`javascript` 等）。
- 支持通过 shebang 行（如 `#!/usr/bin/env node`）识别。
- 配置表记录每个语言是否启用 tree-sitter、解析器名称、是否有细化器模块。

#### (2) tree-sitter 解析调用
- 使用 tree-sitter 的 `parser` 对象，设置语言，解析源代码为字节流。
- 获取根节点（`root_node`）。

#### (3) 遍历收集 ERROR 节点
- 递归遍历每个节点：
  - 若节点类型为 `"ERROR"`，将其加入候选列表。
  - 若节点有 `missing` 标记（通过检查子节点或字段），单独记录。
- 遍历完成后，对候选列表进行**嵌套过滤**：
  - 对于任意两个 `ERROR` 节点 A 和 B，若 B 完全包含在 A 的内部（`start >= A.start && end <= A.end`），且 B 不是 A 本身，则移除 A（保留更内层的 B）。

#### (4) 位置提取
- 使用节点提供的 `start_point`（行、列）和 `end_point`。
- 对于 `missing` token，其位置通常为预期位置（如分号应出现的位置），直接提取。

#### (5) 语言特定细化（可选）
- 针对不同特征给出细化的策略，例如括号平衡检查，各个语言的实现组合需要的策略。

#### (6) 去重与排序
- 将位置转换为元组 `(line, column)`，放入集合去重。
- 按 `line` 升序，`column` 升序排序。

### 2.3 输出格式

每行一个错误位置（或 JSON），示例：

```
/path/to/file.cpp:3:10: syntax error
/path/to/file.cpp:5:0: syntax error
```

或结构化 JSON：
```json
[
  {"file": "test.cpp", "line": 3, "column": 10},
  {"file": "test.cpp", "line": 5, "column": 0}
]
```

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

组合示例：

初步候选（来自 tree-sitter）
    │
    ▼
[ IllegalToken 检测器 ]   → 可能删除重复、添加细粒度位置
    │
    ▼
[ StringUnclosed 检测器 ] → 补充字符串未闭合位置
    │
    ▼
[ BracketBalance 检测器 ] → 修正括号不匹配的大范围错误，替换为精确缺失点
    │
    ▼
[ MissingSemicolon 检测器 ] → 补充缺少分号的位置
    │
    ▼
最终候选（去重、排序后输出）

---

## 三、总结

本设计基于 tree-sitter 的 `ERROR` 节点，能够定位绝大多数语言的常见语法错误。通过“最内层过滤”保证位置最小化，通过可选的细化器改善部分语言（如 C++）的定位精度。
