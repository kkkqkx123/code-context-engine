这份数据是 **Tree-sitter** 解析器（针对 Ruby 语言）的节点类型定义。根据你的要求，我已将纯符号（`named: false`）的节点剔除，仅整理具有实际语法意义的 **Named Node Types**（命名节点类型）。

为了方便查阅，我将它们按照**功能类别**进行了分类整理：

### 1. 程序入口与基础结构

- `program`: 根节点，包含整个源代码。
- `empty_statement`: 空语句。
- `uninterpreted`: 未解释的内容（通常用于注释或特殊标记）。

### 2. 表达式与计算 (Expressions & Arithmetic)

这些节点构成了代码中的逻辑运算和值计算：

- `_expression`: 表达式的抽象基类。
- `_arg`: 参数的抽象基类。
- `_primary`: 基本单元（左值、右值、字面量等）的抽象基类。
- `binary`: 二元运算 (`a + b`, `a && b`)。
- `unary`: 一元运算 (`!a`, `-b`, `~c`)。
- `conditional`: 三元条件运算 (`cond ? a : b`)。
- `range`: 范围 (`1..10`, `1...10`)。
- `assignment`: 赋值 (`a = 1`)。
- `operator_assignment`: 运算符赋值 (`a += 1`)。
- `break`: 循环中断。
- `next`: 循环跳过。
- `redo`: 重新执行当前迭代。
- `retry`: 重试当前块。
- `return`: 返回。
- `yield`: 生成器调用。
- `match_pattern`: 模式匹配 (`x in pattern`)。
- `test_pattern`: 测试模式 (`x.is_a?(pattern)`)。
- `call`: 方法调用 (`obj.method(args)`).
- `element_reference`: 元素引用/数组访问 (`arr[0]`).
- `scope_resolution`: 作用域解析 (`Module::Class`).
- `parenthesized_statements`: 括号内的语句组。

### 3. 控制流语句 (Control Flow)

- `if`: 条件判断 (`if ... end`)。
- `unless`: 非条件判断 (`unless ... end`)。
- `elsif`: 否则如果 (`elsif ... then`).
- `else`: 否则分支 (`else ... end`)。
- `then`: 然后分支 (`then ... end`)。
- `case`: 多路分支 (`case ... when ... end`)。
- `case_match`: 新的 case-match 语法 (`case x in ... end`)。
- `while`: 当循环 (`while ... do ... end`)。
- `until`: 直到循环 (`until ... do ... end`)。
- `for`: 遍历循环 (`for ... in ... do ... end`)。
- `begin`: 异常处理块 (`begin ... rescue ... end`)。
- `rescue`: 捕获异常 (`rescue ... end`)。
- `ensure`: 确保执行 (`ensure ... end`)。
- `modifier` 系列 (带后缀的控制流):
  - `if_modifier`: `puts "hi" if cond`.
  - `unless_modifier`: `puts "hi" unless cond`.
  - `while_modifier`: `loop while cond`.
  - `until_modifier`: `loop until cond`.
  - `rescue_modifier`: `expr rescue handler`.

### 4. 声明与定义 (Declarations & Definitions)

- `class`: 类定义 (`class Foo ... end`)。
- `module`: 模块定义 (`module Bar ... end`)。
- `singleton_class`: 单例类 (`class << obj ... end`)。
- `method`: 普通方法定义 (`def foo ... end`)。
- `singleton_method`: 单例方法定义 (`class << self; def foo ... end; end`)。
- `lambda`: Lambda 表达式 (`-> { ... }`)。
- `block`: 代码块 (`do ... end` 或 `{ ... }`)。
- `do_block`: 显式的 do-end 块。
- `alias`: 别名定义 (`alias new_name old_name`)。
- `undef`: 取消定义 (`undef method_name`).

### 5. 变量与作用域 (Variables & Scope)

- `_variable`: 变量的抽象基类。
- `_nonlocal_variable`: 非局部变量（全局、实例、类变量）。
- `identifier`: 标识符。
- `constant`: 常量。
- `self`: 自我引用。
- `super`: Super 引用。
- `instance_variable`: 实例变量 (`@var`).
- `class_variable`: 类变量 (`@@var`).
- `global_variable`: 全局变量 (`$var`).
- `exception_variable`: 异常捕获变量 (`rescue => e`).

### 6. 数据结构 (Data Structures)

- `array`: 数组 (`[...]`).
- `hash`: 哈希 (`{...}`).
- `pair`: 键值对 (`key => value` 或 `key: value`).
- `string`: 字符串 (`"..."`).
- `symbol`: (通过 `simple_symbol` 和 `delimited_symbol` 表示)。
  - `simple_symbol`: 简单符号 (`:foo`).
  - `delimited_symbol`: 分隔符符号 (`:"foo bar"`).
- `string_array`: 字符串数组 (`%w(...)`).
- `symbol_array`: 符号数组 (`%i(...)`).
- `regex`: 正则表达式 (`/.../`).
- `heredoc_beginning`: Here-doc 开始标记。
- `heredoc_body`: Here-doc 内容体。
- `subshell`: 子 shell (`\`...\``).
- `character`: 字符 (`?\n`).
- `file`: 文件对象 (`__FILE__`).
- `line`: 行号 (`__LINE__`).
- `encoding`: 编码 (`Encoding.default_external`).

### 7. 数值类型 (Numeric Types)

- `_simple_numeric`: 简单数字的抽象基类。
- `integer`: 整数。
- `float`: 浮点数。
- `rational`: 有理数。
- `complex`: 复数。

### 8. 参数列表 (Parameters & Arguments)

- `argument_list`: 实参列表。
- `method_parameters`: 方法形参列表。
- `block_parameters`: 代码块形参列表。
- `lambda_parameters`: Lambda 形参列表。
- `splat_argument`: 展开参数 (`*args`).
- `hash_splat_argument`: 展开哈希参数 (`**kwargs`).
- `forward_argument`: 转发参数 (`...`).
- `block_argument`: 代码块参数 (`&block`).
- `optional_parameter`: 可选参数 (`def f(a=1)`).
- `keyword_parameter`: 关键字参数 (`def f(a:)`).
- `splat_parameter`: 展开形参 (`def f(*args)`).
- `hash_splat_parameter`: 展开哈希形参 (`def f(**kwargs)`).
- `destructured_parameter`: 解构形参 (`def f([a, b])`).
- `forward_parameter`: 转发形参 (`def f(...)`)。
- `rest_assignment`: 剩余赋值 (`a, *rest = arr`).
- `left_assignment_list`: 左侧赋值列表。
- `right_assignment_list`: 右侧赋值列表。

### 9. 模式匹配 (Pattern Matching)

- `_pattern_expr`: 模式表达式的抽象基类。
- `_pattern_expr_basic`: 基本模式表达式。
- `_pattern_primitive`: 原始模式（字面量等）。
- `_pattern_constant`: 常量模式。
- `_pattern_top_expr_body`: 顶层模式表达式体。
- `alternative_pattern`: 替代模式 (`a | b`).
- `as_pattern`: 别名模式 (`x as y`).
- `parenthesized_pattern`: 括号包裹的模式。
- `array_pattern`: 数组模式 (`[a, b]`).
- `hash_pattern`: 哈希模式 (`{a: _, b: _}`).
- `find_pattern`: 查找模式 (`Array.new { ... }`).
- `expression_reference_pattern`: 表达式引用模式。
- `variable_reference_pattern`: 变量引用模式。
- `keyword_pattern`: 关键字模式 (`key: value`).
- `in_clause`: Case-when 中的子句 (`when pattern`).
- `if_guard`: 守卫条件 (`if condition`).
- `unless_guard`: 非守卫条件 (`unless condition`).

### 10. 辅助与元数据 (Helpers & Metadata)

- `interpolation`: 插值 (`#{...}`).
- `escape_sequence`: 转义序列 (`\n`, `\t`).
- `string_content`: 字符串内容。
- `heredoc_content`: Here-doc 内容。
- `heredoc_end`: Here-doc 结束标记。
- `comment`: 注释 (虽然通常在 Token 中，但在某些配置下作为节点存在)。
- `body_statement`: 语句体（用于 class/module/method 内部）。
- `block_body`: 代码块体。
- `uninterpreted`: 未解释内容。

---

**注：**

- 所有以 `_` 开头的类型（如 `_expression`, `_primary`）通常是 Tree-sitter 中的**抽象基类**（Abstract Base Nodes），在生成的 AST 树中通常不会直接出现在叶节点，而是作为内部层级存在。
- 纯符号节点（如 `"type": "!"`, `"type": "("` 等 `named: false` 的类型）已按你的要求忽略。
