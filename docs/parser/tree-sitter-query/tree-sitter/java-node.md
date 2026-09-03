这段文本定义了 Java (特别是引入 `record` 特性后的) 语法的抽象语法树 (AST) 节点类型。

以下是从文本中提取的主要节点类型分类整理：

### 1. 字面量与简单类型 (`_literal` & `_simple_type`)

这些是编译期可直接解析的值和基础数据类型。

- **字面量 (\_literal):**
  - `binary_integer_literal` (二进制整型)
  - `character_literal` (字符)
  - `decimal_floating_point_literal` (十进制浮点)
  - `decimal_integer_literal` (十进制整数)
  - `false`, `true` (布尔值)
  - `hex_floating_point_literal` (十六进制浮点)
  - `hex_integer_literal` (十六进制整型)
  - `null_literal` (空指针)
  - `octal_integer_literal` (八进制整型)
  - `string_literal` (字符串)
- **简单类型 (\_simple_type):**
  - `boolean_type`
  - `floating_point_type`
  - `generic_type`
  - `integral_type`
  - `scoped_type_identifier`
  - `type_identifier`
  - `void_type`

### 2. 声明与定义 (Declarations)

用于定义类、接口、枚举等结构体。

- **通用声明 (`declaration`):** 包含各类特定声明的父集。
- **类与结构:**
  - `class_declaration` / `class_body`
  - `record_declaration` / `record_pattern`
  - `interface_declaration` / `interface_body`
  - `enum_declaration` / `enum_body` / `enum_constant`
  - `annotation_type_declaration` (注解类型)
- **模块:**
  - `module_declaration` / `module_body`
  - `package_declaration`

### 3. 表达式 (Expressions)

表示代码中的计算片段或值。

- **主要表达式 (`expression`):**
  - `assignment_expression` (赋值)
  - `binary_expression` (二元运算)
  - `cast_expression` (类型转换)
  - `instanceof_expression` (实例检查)
  - `lambda_expression` (Lambda)
  - `parenthesized_expression` (括号包裹)
  - `switch_expression` (Switch 表达)
  - `ternary_expression` (三元运算符)
  - `unary_expression` (一元操作符)
  - `update_expression` (自增/自减)
- **主要表达式子项 (`primary_expression`):**
  - `_literal`
  - `array_access` (数组访问)
  - `array_creation_expression` (数组创建)
  - `class_literal`
  - `field_access` (字段访问)
  - `identifier` (标识符)
  - `method_invocation` (方法调用)
  - `method_reference` (方法引用)
  - `object_creation_expression` (对象实例化)
  - `template_expression` (模板扩展，Java 17+)
  - `this`
  - `super` (虽然在此列表中未作为 primary 直接列出，但在后续字段中常见)

### 4. 语句 (Statements)

控制流和逻辑单元。

- \*\*块:`
  - `block`
- **流程控制:**
  - `if_statement`
  - `for_statement`
  - `do_statement`
  - `while_statement`
  - `labeled_statement`
  - `enhanced_for_statement` (增强 for)
  - `assert_statement`
  - `break_statement`, `continue_statement`
  - `return_statement`
  - `switch_expression`
  - `synchronized_statement`
  - `try_statement`, `catch_clause`, `finally_clause`
  - `throw_statement`
  - `yield_statement` (Generator 相关)
- **其他:**
  - `expression_statement` (表达式语句)
  - `declaration` (作为语句时的变量声明)

### 5. 注释与其他字面量 (`_literal` 相关及单独定义)

- `line_comment` (单行注释)
- `block_comment` (多行注释)
- `escape_sequence` (转义序列)

### 6. 特殊关键字 (Keywords)

在 AST 中通常作为独立的叶子节点存在（非表达式也不是声明），标记为 `"named": false`。

- 包括: `abstract`, `assert`, `case`, `catch`, `char`, `class`, `default`, `do`, `else`, `extends`, `final`, `finally`, `import`, `implements`, `instanceof`, `new`, `private`, `public`, `protected`, `static`, `strictfp`, `this`, `throws`, `transient`, `try`, `varargs` (implicit), `volatile`, `while`, `yield` 等。

### 7. 修饰符与参数 (Modifiers & Parameters)

- **修饰符:**
  - `modifiers`
  - `requires_modifier` (Java Modules)
- **参数:**
  - `formal_parameter`
  - `receiver_parameter` (Java 10+ 接收者参数)
  - `spread_parameter` (Java 10+ 参数展开)
  - `inferred_parameters` (推断参数，Java 10+)
- **维度:**
  - `dimensions`, `dimensions_expr` (泛型和数组维度限定符)

### 8. 注解系统 (Annotations)

- `annotation`
- `marker_annotation`
- `annotation_argument_list`
- `element_value_array_initializer`, `element_value_pair`
- `annotation_type_body`, `annotation_type_element_declaration`

### 9. 程序结构与元数据

- `program` (整个源码文件的根节点)
- `argument_list` (参数列表)
- `wildcard` (泛型通配符)
- `type_arguments`, `type_bound`, `type_parameter`, `type_parameters`
- `type_pattern`, `record_pattern`, `record_pattern_component`, `record_pattern_body` (模式匹配)
- `guard`, `pattern` (守卫条件和模式)
- `resource_specification`, `resource` (资源自动关闭)
- `opens_module_directive`, `uses_module_directive`, `provides_module_directive`, `requires_module_directive`, `exports_module_directive` (模块操作指令)
- `super_interfaces`, `superclass`, `permits`

### 10. 语法符号 (Symbols/Tokens)

这些不是 AST 的“节点”逻辑含义，而是具体的标点符号，在文本中定义为 `"named": false` 的节点。

- `=, +=, -=, <<=, >>=, >>>=, %=, &=, |=, ^=, ++, --`, `!=, ==, <, <=, >, >=`, `.`, `->`, `...`, `[`, `]`, `{`, `}`, `(`, `)`, `,`, `;`, `"`, `\{`, `\}` 等。

这个列表涵盖了该 JSON 定义中的全部节点类型。
