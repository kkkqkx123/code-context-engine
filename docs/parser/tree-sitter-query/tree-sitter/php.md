基于提供的 `PHP_NODE_TYPES` 常量字符串，我为您整理并去除了纯符号（如 `+`, `-`, `*`, `"`, `$` 等）和保留字（如 `if`, `function`, `class` 等，除非它们作为特定节点类型出现），仅保留了具有实际语法结构意义的 **Named Node Types**。

以下是分类整理后的 PHP 节点类型列表：

### 1. 程序入口与基础结构

- `program`: 程序的根节点。
- `php_tag`: PHP 开始标签 (`<?php`)。
- `php_end_tag`: PHP 结束标签 (`?>`)。
- `text`: 非 PHP 代码的文本内容。
- `text_interpolation`: 混合在文本中的插值片段。
- `comment`: 注释。

### 2. 表达式 (Expressions)

**核心表达式类型：**

- `expression`: 通用表达式基类。
- `literal`: 字面量基类。
  - `boolean`: 布尔值。
  - `integer`: 整数。
  - `float`: 浮点数。
  - `string`: 普通字符串。
  - `encapsed_string`: 双引号字符串或变量插值字符串。
  - `heredoc`: Heredoc 字符串。
  - `nowdoc`: Nowdoc 字符串。
  - `null`: null 值。
- `primary_expression`: 基础表达式基类。
  - `name`: 标识符名称。
  - `qualified_name`: 限定名称 (如 `Namespace\Class`).
  - `relative_name`: 相对名称 (如 `self::`, `parent::`).
  - `variable_name`: 变量名 (`$var`).
  - `dynamic_variable_name`: 动态变量名 (`${$var}`).
  - `parenthesized_expression`: 括号表达式。
  - `array_creation_expression`: 数组创建 (`[]` 或 `array()`).
  - `object_creation_expression`: 对象创建 (`new`).
  - `anonymous_function`: 匿名函数。
  - `arrow_function`: 箭头函数 (`fn`).
  - `anonymous_class`: 匿名类。
  - `cast_expression`: 类型转换 (`(int)`).
  - `clone_expression`: 克隆 (`clone`).
  - `throw_expression`: 抛出异常 (`throw`).
  - `yield_expression`: Yield 表达式。
  - `error_suppression_expression`: 错误抑制 (`@`).
  - `include_expression`: 包含文件 (`include`).
  - `include_once_expression`: 包含一次 (`include_once`).
  - `require_expression`: 要求文件 (`require`).
  - `require_once_expression`: 要求一次 (`require_once`).
  - `shell_command_expression`: Shell 命令执行。
  - `print_intrinsic`: print 语言构造。
  - `unary_op_expression`: 一元运算符表达式。
  - `update_expression`: 自增/自减 (`++`, `--`).
  - `subscript_expression`: 下标访问 (`[]`).
  - `member_access_expression`: 成员访问 (`->`).
  - `nullsafe_member_access_expression`: 空安全成员访问 (`?->`).
  - `scoped_property_access_expression`: 作用域属性访问 (`::`).
  - `class_constant_access_expression`: 类常量访问。
  - `function_call_expression`: 函数调用。
  - `member_call_expression`: 方法调用 (`->()`).
  - `nullsafe_member_call_expression`: 空安全方法调用 (`?->()`).
  - `scoped_call_expression`: 作用域调用 (`::()`).
  - `conditional_expression`: 三元条件表达式。
  - `match_expression`: Match 表达式。
  - `binary_expression`: 二元运算符表达式。
  - `assignment_expression`: 赋值表达式。
  - `augmented_assignment_expression`: 复合赋值 (`+=`, `-=`, etc.).
  - `reference_assignment_expression`: 引用赋值。

**特殊表达式子项：**

- `match_block`: Match 的主体块。
- `match_conditional_expression`: Match 的条件分支。
- `match_default_expression`: Match 的默认分支。
- `sequence_expression`: 序列表达式。
- `variadic_unpacking`: 可变参数解包 (`...`).
- `variadic_placeholder`: 可变参数占位符。

### 3. 语句 (Statements)

- `statement`: 通用语句基类。
- `compound_statement`: 复合语句 (代码块 `{ ... }`)。
- `expression_statement`: 表达式语句。
- `empty_statement`: 空语句。
- `break_statement`: Break 语句。
- `continue_statement`: Continue 语句。
- `return_statement`: Return 语句。
- `echo_statement`: Echo 语句。
- `unset_statement`: Unset 语句。
- `goto_statement`: Goto 语句。
- `named_label_statement`: 命名标签语句。
- `declare_statement`: Declare 语句。
- `if_statement`: If 语句。
  - `else_if_clause`: Else if 子句。
  - `else_clause`: Else 子句。
- `switch_statement`: Switch 语句。
  - `switch_block`: Switch 块。
  - `case_statement`: Case 语句。
  - `default_statement`: Default 语句。
- `for_statement`: For 循环。
- `foreach_statement`: Foreach 循环。
- `while_statement`: While 循环。
- `do_statement`: Do-While 循环。
- `try_statement`: Try-Catch-Finally 块。
  - `catch_clause`: Catch 子句。
  - `finally_clause`: Finally 子句。
- `exit_statement`: Exit 语句。

### 4. 声明 (Declarations)

**类、接口、Trait 与 Enum：**

- `class_declaration`: 类声明。
- `interface_declaration`: 接口声明。
- `trait_declaration`: Trait 声明。
- `enum_declaration`: 枚举声明。
  - `enum_case`: 枚举案例。
  - `enum_declaration_list`: 枚举声明列表。
- `method_declaration`: 方法声明。
- `property_declaration`: 属性声明。
  - `property_element`: 属性元素。
  - `property_hook`: 属性钩子 (Property Hooks)。
  - `property_hook_list`: 属性钩子列表。
  - `property_promotion_parameter`: 构造函数属性提升参数。
- `const_declaration`: 常量声明。
  - `const_element`: 常量元素。
- `namespace_definition`: 命名空间定义。
- `namespace_use_declaration`: 命名空间导入声明。
  - `namespace_use_clause`: 命名空间导入条款。
  - `namespace_use_group`: 命名空间导入组。
  - `namespace_name`: 命名空间名称。
- `use_declaration`: Use 声明 (Traits 或 Classes)。
  - `use_as_clause`: Use as 条款。
  - `use_instead_of_clause`: Instead of 条款。
  - `use_list`: Use 列表。
- `function_definition`: 函数定义。
- `function_static_declaration`: 静态变量声明。
  - `static_variable_declaration`: 静态变量元素。
- `global_declaration`: Global 声明。

### 5. 类型 (Types)

- `type`: 通用类型基类。
- `named_type`: 命名类型 (类名等)。
- `primitive_type`: 原始类型 (int, string, bool 等)。
- `union_type`: 联合类型 (`A|B`)。
- `intersection_type`: 交集类型 (`A&B`)。
- `optional_type`: 可选类型 (`?T`)。
- `disjunctive_normal_form_type`: 析取范式类型 (较新的复杂类型结构)。
- `bottom_type`: 底部类型 (never 等)。
- `type_list`: 类型列表。

### 6. 参数与调用 (Parameters & Arguments)

- `formal_parameters`: 形式参数列表。
- `simple_parameter`: 简单参数。
- `variadic_parameter`: 可变参数 (`...$args`)。
- `arguments`: 实参列表。
- `argument`: 单个实参。
  - `variadic_placeholder`: 可变参数占位符。
- `anonymous_function_use_clause`: 匿名函数 use 子句。
- `by_ref`: 引用修饰符 (在参数或数组中)。

### 7. 修饰符与特性 (Modifiers & Attributes)

- `visibility_modifier`: 可见性修饰符 (public, private, protected)。
- `abstract_modifier`: Abstract 修饰符。
- `final_modifier`: Final 修饰符。
- `static_modifier`: Static 修饰符。
- `readonly_modifier`: Readonly 修饰符。
- `reference_modifier`: 引用修饰符 (在函数返回等位置)。
- `var_modifier`: Var 修饰符。
- `attribute`: 属性注解。
- `attribute_group`: 属性组。
- `attribute_list`: 属性列表。

### 8. 结构与辅助节点

- `declaration_list`: 声明列表 (类体内部)。
- `base_clause`: 继承子句 (extends/implements)。
- `class_interface_clause`: 类/接口实现子句。
- `colon_block`: 冒号块 (用于 if/foreach/while 等的大括号替代语法)。
- `list_literal`: List 结构 (用于 destructuring assignment)。
- `pair`: 键值对 (用于 foreach 或数组)。
- `cast_type`: 强制转换类型。
- `operation`: 操作符 (在 visibility modifier 内部)。
- `escape_sequence`: 转义序列。
- `string_content`: 字符串内容。
- `heredoc_start`: Heredoc 开始标记。
- `heredoc_end`: Heredoc 结束标记。
- `heredoc_body`: Heredoc 内容。
- `nowdoc_string`: Nowdoc 内容。
- `relative_scope`: 相对作用域。
