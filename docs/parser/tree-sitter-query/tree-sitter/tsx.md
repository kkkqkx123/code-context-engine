根据提供的 `TSX_NODE_TYPES` 数据，以下是整理后的 TypeScript/JSX 节点类型列表。已按功能分类，并**跳过**了纯符号（如 `+`, `-`, `{}`, `;` 等）和关键字（如 `const`, `function`, `if` 等）。

### 1. 程序与声明 (Program & Declarations)

这些是代码的顶层结构和变量/函数定义。

- **program**: 根节点，包含整个文件内容。
- **declaration**: 抽象节点，包含以下子类型：
  - `abstract_class_declaration`: 抽象类声明。
  - `ambient_declaration`: 环境声明 (declare)。
  - `class_declaration`: 类声明。
  - `enum_declaration`: 枚举声明。
  - `function_declaration`: 函数声明。
  - `function_signature`: 函数签名。
  - `generator_function_declaration`: 生成器函数声明。
  - `import_alias`: 导入别名。
  - `interface_declaration`: 接口声明。
  - `internal_module`: 内部模块 (namespace/module)。
  - `lexical_declaration`: 词法声明 (`let` / `const`)。
  - `module`: 模块声明。
  - `type_alias_declaration`: 类型别名声明。
  - `variable_declaration`: 变量声明 (`var` / `let` / `const`)。
- **export_statement**: 导出语句。
- **import_statement**: 导入语句。

### 2. 表达式 (Expressions)

用于计算值的节点。

- **expression**: 抽象节点，包含以下子类型：
  - `as_expression`: `as` 类型断言。
  - `assignment_expression`: 赋值表达式。
  - `augmented_assignment_expression`: 复合赋值 (如 `+=`, `||=`)。
  - `await_expression`: `await` 表达式。
  - `binary_expression`: 二元运算 (如 `a + b`, `===`)。
  - `instantiation_expression`: 实例化表达式。
  - `jsx_element`: JSX 元素标签。
  - `jsx_self_closing_element`: JSX 自闭合标签。
  - `new_expression`: `new` 对象创建。
  - `primary_expression`: 基础表达式。
  - `satisfies_expression`: `satisfies` 操作符。
  - `ternary_expression`: 三元运算符。
  - `unary_expression`: 一元运算 (如 `!a`, `typeof a`)。
  - `update_expression`: 自增/自减 (`++`, `--`)。
  - `yield_expression`: `yield` 表达式。
  - `call_expression`: 函数调用。
  - `member_expression`: 成员访问 (`obj.prop`, `arr[0]`)。
  - `subscript_expression`: 下标访问。
  - `non_null_expression`: 非空断言 (`!`).
  - `meta_property`: 元属性 (`new.target`, `import.meta`).
  - `arrow_function`: 箭头函数。
  - `function_expression`: 函数表达式。
  - `generator_function`: 生成器函数表达式。
  - `class`: 类表达式。
  - `array`: 数组字面量。
  - `object`: 对象字面量。
  - `template_string`: 模板字符串。
  - `regex`: 正则表达式字面量。
  - `string`, `number`, `boolean` (true/false), `null`, `undefined`, `this`, `super`: 字面量和标识符。

### 3. 语句 (Statements)

控制流和逻辑执行的节点。

- **statement**: 抽象节点，包含以下子类型：
  - `break_statement`: break 语句。
  - `continue_statement`: continue 语句。
  - `debugger_statement`: debugger 语句。
  - `do_statement`: do-while 循环。
  - `empty_statement`: 空语句。
  - `expression_statement`: 表达式语句。
  - `for_in_statement`: for-in 循环。
  - `for_statement`: for 循环。
  - `if_statement`: if 语句。
  - `labeled_statement`: 带标签的语句。
  - `return_statement`: return 语句。
  - `statement_block`: 代码块 (`{ ... }`)。
  - `switch_statement`: switch 语句。
  - `throw_statement`: throw 语句。
  - `try_statement`: try-catch-finally 语句。
  - `while_statement`: while 循环。
  - `with_statement`: with 语句。

### 4. 模式 (Patterns)

用于解构赋值的结构。

- **pattern**: 抽象节点，包含：
  - `array_pattern`: 数组解构。
  - `identifier`: 标识符。
  - `member_expression`: 成员访问模式。
  - `object_pattern`: 对象解构。
  - `rest_pattern`: 剩余参数/属性。
  - `subscript_expression`: 下标模式。
  - `undefined`: undefined 占位。
- **assignment_pattern**: 带有默认值的解构 (`{a = 1}`).

### 5. 类型系统 (Types)

TypeScript 特有的类型定义和检查。

- **type**: 抽象节点，包含：
  - `call_expression`: 调用签名。
  - `constructor_type`: 构造函数类型。
  - `function_type`: 函数类型。
  - `infer_type`: infer 推断。
  - `member_expression`: 成员类型。
  - `primary_type`: 基础类型。
  - `readonly_type`: readonly 修饰。
- **primary_type**: 基础类型，包含：
  - `array_type`: 数组类型 (`T[]`)。
  - `conditional_type`: 条件类型 (`A extends B ? C : D`)。
  - `existential_type`: 存在性类型 (`?`).
  - `flow_maybe_type`: Flow 可选类型 (`?`).
  - `generic_type`: 泛型类型 (`Map<string, number>`).
  - `index_type_query`: 索引查询 (`keyof T`).
  - `intersection_type`: 交叉类型 (`A & B`).
  - `literal_type`: 字面量类型 (`"hello"`, `123`).
  - `lookup_type`: 查找类型。
  - `nested_type_identifier`: 嵌套类型标识符。
  - `object_type`: 对象类型 (`{ a: string }`).
  - `parenthesized_type`: 括号包裹的类型。
  - `predefined_type`: 预定义类型 (`any`, `void`, `never` 等)。
  - `template_literal_type`: 模板字面量类型。
  - `this_type`: this 类型。
  - `tuple_type`: 元组类型。
  - `type_identifier`: 类型标识符。
  - `type_query`: 类型查询 (`typeof x`).
  - `union_type`: 联合类型 (`A | B`).
- **type_annotation**: 类型注解 (`: Type`)。
- **asserts_annotation**: asserts 类型谓词 (`x is Type`)。
- **type_predicate_annotation**: 类型谓词注解。
- **type_arguments**: 泛型参数列表。
- **type_parameters**: 泛型参数定义。
- **type_parameter**: 单个泛型参数。
- **constraint**: 约束条件。
- **default_type**: 默认类型。
- **optional_type**: 可选类型 (`Type?`)。
- **rest_type**: 剩余类型 (`...Type[]`)。
- **mapped_type_clause**: 映射类型。
- **extends_type_clause**: 继承类型。

### 6. 类与接口细节 (Class & Interface Details)

- **class_body**: 类的主体。
- **class_heritage**: 类的继承链 (extends/implements)。
- **class_static_block**: 类静态块 (`static { ... }`)。
- **abstract_method_signature**: 抽象方法签名。
- **method_definition**: 类中的方法定义。
- **method_signature**: 接口或对象类型中的方法签名。
- **property_signature**: 接口或对象类型中的属性签名。
- **public_field_definition**: 公共字段定义。
- **accessibility_modifier**: 访问修饰符 (`public`, `private`, `protected`)。
- **override_modifier**: override 修饰符。
- **decorator**: 装饰器。
- **implements_clause**: implements 子句。
- **extends_clause**: extends 子句。
- **call_signature**: 调用签名。
- **construct_signature**: 构造签名。
- **index_signature**: 索引签名 (`[key: string]: number`)。
- **interface_body**: 接口的主体。
- **enum_body**: 枚举的主体。
- **enum_assignment**: 枚举成员赋值。

### 7. JSX 特有节点 (JSX Specific)

- **jsx_attribute**: JSX 属性。
- **jsx_closing_element**: JSX 闭合标签。
- **jsx_element**: JSX 元素。
- **jsx_expression**: JSX 表达式 (`{ ... }`)。
- **jsx_namespace_name**: JSX 命名空间名称。
- **jsx_opening_element**: JSX 开始标签。
- **jsx_self_closing_element**: JSX 自闭合标签。
- **jsx_text**: JSX 文本内容。
- **html_character_reference**: HTML 字符引用 (`&nbsp;`).
- **html_comment**: HTML 注释。
- **jsx_namespace_name**: 命名空间名称。

### 8. 其他辅助节点

- **arguments**: 函数调用参数列表。
- **formal_parameters**: 形式参数列表。
- **required_parameter**: 必需参数。
- **optional_parameter**: 可选参数。
- **catch_clause**: catch 块。
- **finally_clause**: finally 块。
- **else_clause**: else 分支。
- **switch_body**: switch 主体。
- **switch_case**: switch case。
- **switch_default**: switch default。
- **named_imports**: 具名导入集合。
- **namespace_import**: 命名空间导入。
- **import_specifier**: 导入说明符。
- **export_specifier**: 导出说明符。
- **import_require_clause**: require 导入。
- **import_attribute**: import 属性。
- **hash_bang_line**: shebang 行 (`#!/bin/bash`)。
- **escape_sequence**: 转义序列。
- **string_fragment**: 字符串片段。
- **template_substitution**: 模板替换。
- **template_type**: 模板类型。
- **computed_property_name**: 计算属性名。
- **pair**: 键值对 (在对象中)。
- **pair_pattern**: 键值对模式 (在解构中)。
- **shorthand_property_identifier**: 简写属性标识符。
- **shorthand_property_identifier_pattern**: 简写属性模式。
- **sequence_expression**: 序列表达式。
- **spread_element**: 扩展元素 (`...args`)。
- **non_null_expression**: 非空断言。
- **optional_chain**: 可选链。
- **asserts**: asserts 关键字相关。
- **type_predicate**: 类型谓词。
- **regex_flags**: 正则标志。
- **regex_pattern**: 正则模式。
