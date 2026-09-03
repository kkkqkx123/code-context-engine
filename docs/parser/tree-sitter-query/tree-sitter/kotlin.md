0.3.8

根据提供的文件内容，我为您整理了 Kotlin 的节点类型（Node Types）。已按照您的要求，**排除了所有 `named: false` 的纯符号节点**（如操作符、关键字等），仅保留了具有实际语法结构的命名节点。

为了方便阅读，我将节点按功能类别进行了分类：

### 1. 表达式 (Expressions)

这些节点代表代码中的计算逻辑和值。

- **基础运算与结构**
  - `additive_expression` (加减法表达式)
  - `multiplicative_expression` (乘除法表达式)
  - `comparison_expression` (比较表达式)
  - `equality_expression` (相等性表达式)
  - `conjunction_expression` (逻辑与表达式)
  - `disjunction_expression` (逻辑或表达式)
  - `elvis_expression` (Elvis 运算符 ?: 表达式)
  - `infix_expression` (中缀表达式)
  - `as_expression` (类型转换 as / as? 表达式)
  - `check_expression` (检查表达式 is / !is)
  - `range_expression` (范围表达式 ..)
  - `prefix_expression` (前缀表达式，如 `!`, `-`)
  - `postfix_expression` (后缀表达式，如 `++`, `--`)
  - `parenthesized_expression` (括号表达式)
  - `spread_expression` (展开表达式 ...)

- **流程控制与条件**
  - `if_expression` (if 表达式)
  - `when_expression` (when 表达式)
  - `try_expression` (try-catch-finally 表达式)
  - `jump_expression` (跳转语句 return, break, continue)

- **调用与访问**
  - `call_expression` (函数调用)
  - `indexing_expression` (索引访问 [])
  - `navigation_expression` (导航访问 . 或 ?. )
  - `callable_reference` (可调用引用 ::)

- **字面量与构造**
  - `integer_literal` (整数字面量)
  - `long_literal` (长整型字面量)
  - `real_literal` (浮点数字面量)
  - `bin_literal` (二进制字面量)
  - `hex_literal` (十六进制字面量)
  - `unsigned_literal` (无符号字面量)
  - `boolean_literal` (布尔字面量 true/false)
  - `character_literal` (字符字面量)
  - `string_literal` (字符串字面量，包含插值)
  - `collection_literal` (集合字面量 [1, 2])
  - `object_literal` (对象字面量 object {...})
  - `lambda_literal` (Lambda 表达式)
  - `anonymous_function` (匿名函数)
  - `this_expression` (this 引用)
  - `super_expression` (super 引用)

### 2. 声明与定义 (Declarations & Definitions)

这些节点代表程序的结构单元，如类、函数、变量等。

- **顶层与模块**
  - `source_file` (源文件根节点)
  - `package_header` (包声明)
  - `import_list` (导入列表)
  - `import_header` (单个导入头)
  - `file_annotation` (文件级注解)

- **类与接口**
  - `class_declaration` (类声明)
  - `interface` (虽然关键字是 interface，但这里对应的是 `class_declaration` 或特定的接口结构，通常归类在 class*body 下) -> *注：文件中未直接列出独立的 interface 节点名，通常由 class*declaration 处理或通过 modifiers 区分，但在 AST 结构中常作为一类。此处主要关注显式列出的节点*。
  - `object_declaration` (伴生对象/单例对象声明)
  - `enum_class_body` (枚举类体)
  - `enum_entry` (枚举条目)
  - `type_alias` (类型别名)

- **函数与方法**
  - `function_declaration` (函数声明)
  - `getter` (属性 getter)
  - `setter` (属性 setter)
  - `primary_constructor` (主构造函数)
  - `secondary_constructor` (次构造函数)
  - `anonymous_initializer` (匿名初始化块)

- **属性与变量**
  - `property_declaration` (属性声明)
  - `variable_declaration` (变量声明 val/var)
  - `multi_variable_declaration` (多变量声明)
  - `parameter` (参数声明)
  - `function_value_parameters` (函数值参数列表)
  - `lambda_parameters` (Lambda 参数列表)

### 3. 类型系统 (Type System)

这些节点用于描述类型信息。

- `user_type` (用户自定义类型)
- `not_nullable_type` (非空类型)
- `nullable_type` (可空类型)
- `parenthesized_type` (括号包裹的类型)
- `function_type` (函数类型)
- `type_parameters` (类型参数列表)
- `type_parameter` (单个类型参数)
- `type_arguments` (泛型类型实参)
- `type_constraint` (类型约束 where 子句)
- `type_constraints` (类型约束列表)
- `type_projection` (类型投影)
- `variance_modifier` (方差修饰符 out/in)
- `reification_modifier` (内联重化修饰符 reified)
- `type_modifiers` (类型修饰符列表)
- `type_identifier` (类型标识符)

### 4. 语句与控制流 (Statements & Control Flow)

这些节点代表执行流。

- `statements` (语句列表)
- `control_structure_body` (控制结构体，如 if/while 的块)
- `for_statement` (for 循环)
- `while_statement` (while 循环)
- `do_while_statement` (do-while 循环)
- `assignment` (赋值语句)
- `directly_assignable_expression` (可直接赋值的表达式)

### 5. 注解与修饰符 (Annotations & Modifiers)

虽然很多修饰符本身是原子节点，但以下节点用于组织它们。

- `annotation` (注解)
- `annotated_lambda` (带注解的 Lambda)
- `modifiers` (修饰符列表)
- `parameter_modifiers` (参数修饰符列表)
- `use_site_target` (使用位置目标 @get/@set 等)
- `constructor_invocation` (构造器调用)
- `delegation_specifier` (委托说明)
- `explicit_delegation` (显式委托)
- `property_delegate` (属性委托)

### 6. 特殊结构 (Special Structures)

- `catch_block` (catch 块)
- `finally_block` (finally 块)
- `class_body` (类体)
- `companion_object` (伴生对象声明)
- `value_arguments` (值参数列表)
- `value_argument` (单个值参数)
- `constructor_delegation_call` (构造器委托调用)
- `shebang_line` (Shebang 行 #!)
- `label` (标签)
- `identifier` (标识符)
- `simple_identifier` (简单标识符)
- `binding_pattern_kind` (绑定模式类型)

### 7. 字符串与注释相关

- `interpolated_expression` (字符串插值表达式)
- `interpolated_identifier` (字符串插值标识符)
- `string_content` (字符串内容 - 注意：此节点在原始数据中 `named: true`，属于内容节点)
- `line_comment` (单行注释)
- `multiline_comment` (多行注释)
- `character_escape_seq` (字符转义序列)

---

**整理说明：**

- **排除项**：所有的操作符（如 `+`, `==`, `.`）、关键字（如 `fun`, `class`, `val`）以及标点符号（如 `{`, `}`, `(`）均未列入，因为它们在提供的 JSON 数据中 `named` 字段均为 `false`。
- **层级关系**：每个节点都定义了其 `children`（子节点）的可能类型，这反映了 Kotlin 语法的嵌套结构。例如，`function_declaration` 必须包含 `function_body` 和 `function_value_parameters`。
