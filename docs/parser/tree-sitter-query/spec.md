# 查询规范支持清单

本文档记录了 `src/query/scheme` 目录中所有语言支持的 Tree-sitter 查询捕获名称，以及它们是否符合命名规范。

## 命名规范概览

所有捕获名称必须遵循以下格式：

```
@[domain].[category].[attribute]                    (3段，最简单)
@[domain].[category].[subtype].[attribute]          (4段，标准)
  ↑         ↑         ↑        ↑
 必填      必填      必填     必填
```

支持的四类域：

- `@entity` - 实体定义
- `@call` - 调用关系
- `@dependency` - 依赖关系
- `@comment` - 注释

### 简化原则

- **去除冗余的默认值**：entity 默认就是定义，method 默认就是实例方法，function 默认就是直接调用
- **合并相似概念**：class, struct, interface, enum 等都是类型，直接提升为 category
- **简化路径结构**：如 `variable.declaration.name` 简化为 `variable.name`
- **保留必要的区分**：method vs function, static vs instance, constructor 需要区分

### 签名子捕获规范

签名信息通过嵌入实体捕获内部的子捕获获取，而非独立的签名匹配。签名子捕获使用 `signature` 作为 subtype，命名格式为：

```
@entity.<category>.signature.<attribute>
```

支持的签名子捕获属性：

| 子捕获名称                          | 描述               | 适用实体                          |
| ----------------------------------- | ------------------ | --------------------------------- |
| `@entity.*.signature.name`          | 实体名称           | 所有实体                          |
| `@entity.*.signature.type_params`   | 类型参数           | struct, class, enum               |
| `@entity.*.signature.params`        | 参数列表           | function, method                  |
| `@entity.*.signature.return_type`   | 返回类型           | function, method                  |
| `@entity.*.signature.base`          | 基类/父接口        | class, interface                  |
| `@entity.*.signature.extends`       | 继承               | class (PHP, Ruby)                 |
| `@entity.*.signature.implements`    | 实现接口           | class (PHP)                       |

**设计原则**：

1. 签名子捕获嵌入实体捕获内部，无需独立匹配
2. 签名提取通过 `reconstruct_signature_from_subcaptures` 按源码顺序拼接子捕获文本
3. 无签名子捕获时，从实体捕获全文提取签名（fallback）

---

## 通用捕获名称

### 注释域 (@comment)

| 捕获名称        | 描述                     | 支持语言               |
| --------------- | ------------------------ | ---------------------- |
| `@comment.line` | 单行注释 (// ...)        | Java, Rust             |
| `@comment.doc`  | 文档注释 (/\*_ ... _/)   | Java, Rust             |
| `@comment`      | 所有注释（单行和块注释） | C, C++, C#, Go, JS, TS |

---

## Entity Domain 完整列表

### 类型定义 (@entity.\*)

| 捕获名称                               | 描述             | 支持语言                          |
| -------------------------------------- | ---------------- | --------------------------------- |
| `@entity.class.name`                   | 类名             | C#, Go, Java, JS, TS, C++, Python |
| `@entity.class.body`                   | 类体             | C#, C++, Python                   |
| `@entity.class.signature.type_params`  | 类型参数         | Rust                              |
| `@entity.class.signature.base`         | 基类             | Java, Python, Ruby                |
| `@entity.struct.name`                  | 结构体名         | C, C++, Go, Rust                  |
| `@entity.struct.body`                  | 结构体体         | C, C++, Go                        |
| `@entity.struct.signature.type_params` | 类型参数         | Rust                              |
| `@entity.enum.name`                    | 枚举名           | C, C++, Go, Java, Rust, TS        |
| `@entity.enum.body`                    | 枚举体           | C, C++                            |
| `@entity.enum_constant.name`           | 枚举常量名       | Java                              |
| `@entity.enum_variant.name`            | 枚举变体名       | Rust                              |
| `@entity.enum_member.name`             | 枚举成员名       | C#, TS                            |
| `@entity.enum_value.name`              | 枚举值名         | C                                 |
| `@entity.interface.name`               | 接口名           | Go, Java, TS                      |
| `@entity.interface.body`               | 接口体           | Go                                |
| `@entity.interface_method.name`        | 接口方法名       | Go                                |
| `@entity.interface_method.params`      | 接口方法参数     | Go                                |
| `@entity.interface_method.return_type` | 接口方法返回类型 | Go                                |
| `@entity.union.name`                   | 联合体名         | C, Rust                           |
| `@entity.union.body`                   | 联合体体         | C                                 |
| `@entity.trait.name`                   | Trait 名         | Rust                              |
| `@entity.type.name`                    | 类型别名名       | Go, Rust, TS                      |
| `@entity.type_alias.name`              | 类型别名名（Go） | Go                                |
| `@entity.type_alias.underlying_type`   | 类型别名底层类型 | Go                                |
| `@entity.record.name`                  | Record 名        | C#, Java                          |
| `@entity.annotation.name`              | 注解类型名       | Java                              |
| `@entity.abstract_class.name`          | 抽象类名         | TS                                |
| `@entity.class_expression.name`        | 类表达式名       | JS, TS                            |

### 函数定义 (@entity.function)

| 捕获名称                               | 描述             | 支持语言                          |
| -------------------------------------- | ---------------- | --------------------------------- |
| `@entity.function.name`                | 函数名           | 所有语言                          |
| `@entity.function.params`              | 参数列表         | 所有语言                          |
| `@entity.function.return_type`         | 返回类型         | C, C++, Go, Java, Rust, PHP       |
| `@entity.function.body`                | 函数体           | 所有语言                          |
| `@entity.function.signature.name`      | 函数名           | C, Go, Java, Rust, PHP, Lua       |
| `@entity.function.signature.params`    | 参数列表         | C, Go, Java, Rust, PHP, Lua       |
| `@entity.function.signature.return_type`| 返回类型         | C, Go, Java, Rust, PHP            |

### 方法定义 (@entity.method)

| 捕获名称                               | 描述             | 支持语言                          |
| -------------------------------------- | ---------------- | --------------------------------- |
| `@entity.method.name`                  | 方法名           | 所有语言                          |
| `@entity.method.params`                | 参数列表         | 所有语言                          |
| `@entity.method.return_type`           | 返回类型         | C++, Java, Rust, TS               |
| `@entity.method.body`                  | 方法体           | 所有语言                          |
| `@entity.method.signature.name`        | 方法名           | C++, Dart, Java, Rust, PHP        |
| `@entity.method.signature.params`      | 参数列表         | C++, Dart, Java, Rust, PHP        |
| `@entity.method.signature.return_type` | 返回类型         | C++, Java, Rust, TS               |

| 捕获名称                                 | 描述                      | 支持语言                           |
| ---------------------------------------- | ------------------------- | ---------------------------------- |
| `@entity.function.name`                  | 函数定义名                | C, C++, Go, Java, JS, Rust, Python |
| `@entity.function.params`                | 函数定义参数              | C, C++, Go, Python                 |
| `@entity.function.return_type`           | 函数定义返回类型          | C, C++, Go                         |
| `@entity.function.body`                  | 函数定义体                | C, C++, Go, Python                 |
| `@entity.function.prototype.name`        | 函数原型名                | C, C++                             |
| `@entity.function.prototype.params`      | 函数原型参数              | C, C++                             |
| `@entity.function.prototype.return_type` | 函数原型返回类型          | C++                                |
| `@entity.function.signature.name`        | 函数签名名                | Rust, TS                           |
| `@entity.function.generator.name`        | 生成器函数名              | JS                                 |
| `@entity.function.arrow.name`            | 箭头函数名（let/const）   | JS, TS                             |
| `@entity.function.arrow_var.name`        | 箭头函数名（var）         | JS                                 |
| `@entity.function.expression.name`       | 函数表达式名（let/const） | JS, TS                             |
| `@entity.function.expression_var.name`   | 函数表达式名（var）       | JS                                 |

### 方法定义 (@entity.method)

| 捕获名称                          | 描述             | 支持语言                          |
| --------------------------------- | ---------------- | --------------------------------- |
| `@entity.method.name`             | 方法定义名       | C#, Go, Java, JS, TS, C++, Python |
| `@entity.method.params`           | 方法定义参数     | C++, Go, Python                   |
| `@entity.method.receiver`         | 方法接收者       | Go                                |
| `@entity.method.return_type`      | 方法定义返回类型 | Go                                |
| `@entity.method.body`             | 方法定义体       | C++, Go, Python                   |
| `@entity.method.prototype.name`   | 方法原型名       | C++                               |
| `@entity.method.prototype.params` | 方法原型参数     | C++                               |

### 构造函数/析构函数 (@entity.constructor/@entity.destructor)

| 捕获名称                     | 描述         | 支持语言          |
| ---------------------------- | ------------ | ----------------- |
| `@entity.constructor.name`   | 构造函数名   | C#, Java, C++, JS |
| `@entity.constructor.params` | 构造函数参数 | C++               |
| `@entity.constructor.body`   | 构造函数体   | C++               |
| `@entity.destructor.name`    | 析构函数名   | C#, C++           |
| `@entity.destructor.params`  | 析构函数参数 | C++               |
| `@entity.destructor.body`    | 析构函数体   | C++               |

### 操作符重载 (@entity.method.operator)

| 捕获名称                         | 描述           | 支持语言 |
| -------------------------------- | -------------- | -------- |
| `@entity.method.operator.name`   | 操作符重载名   | C++      |
| `@entity.method.operator.params` | 操作符重载参数 | C++      |
| `@entity.method.operator.body`   | 操作符重载体   | C++      |

### Getter/Setter (@entity.method.getter/@entity.method.setter)

| 捕获名称                     | 描述          | 支持语言 |
| ---------------------------- | ------------- | -------- |
| `@entity.method.getter.name` | Getter 方法名 | JS, TS   |
| `@entity.method.setter.name` | Setter 方法名 | JS, TS   |

### 变量声明 (@entity.variable)

| 捕获名称                        | 描述         | 支持语言       |
| ------------------------------- | ------------ | -------------- |
| `@entity.variable.name`         | 变量声明名   | Java, Go, Rust |
| `@entity.variable.type`         | 变量声明类型 | Go, Rust       |
| `@entity.variable.value`        | 变量声明值   | Go, Rust       |
| `@entity.variable.normal.type`  | 普通变量类型 | C              |
| `@entity.variable.normal.name`  | 普通变量名   | C              |
| `@entity.variable.normal.value` | 普通变量值   | C              |
| `@entity.variable.const.name`   | 常量名       | JS             |
| `@entity.variable.const.value`  | 常量值       | JS             |
| `@entity.variable.let.name`     | Let 声明名   | JS             |
| `@entity.variable.var.name`     | Var 声明名   | JS             |

### 字段声明 (@entity.field)

| 捕获名称                    | 描述           | 支持语言                   |
| --------------------------- | -------------- | -------------------------- |
| `@entity.field.name`        | 字段名         | C, C++, C#, Go, Rust, Java |
| `@entity.field.type`        | 字段类型       | C, Go                      |
| `@entity.field_tagged.name` | 带标签字段名   | Go                         |
| `@entity.field_tagged.type` | 带标签字段类型 | Go                         |
| `@entity.field_tagged.tag`  | 字段标签       | Go                         |
| `@entity.embedded.name`     | 嵌入字段名     | Go                         |
| `@entity.bitfield.name`     | 位字段名       | C                          |
| `@entity.bitfield.type`     | 位字段类型     | C                          |

### 参数声明 (@entity.parameter)

| 捕获名称                 | 描述         | 支持语言          |
| ------------------------ | ------------ | ----------------- |
| `@entity.parameter.name` | 参数名       | C, Go, Java, Rust |
| `@entity.parameter.type` | 参数类型     | C, Go             |
| `@entity.variadic.name`  | 可变参数名   | Go                |
| `@entity.variadic.type`  | 可变参数类型 | Go                |

### 数组声明 (@entity.variable.array)

| 捕获名称                       | 描述     | 支持语言 |
| ------------------------------ | -------- | -------- |
| `@entity.variable.array.type`  | 数组类型 | C        |
| `@entity.variable.array.name`  | 数组名   | C        |
| `@entity.variable.array.size`  | 数组大小 | C        |
| `@entity.variable.array.value` | 数组值   | C        |

### 指针声明 (@entity.variable.pointer)

| 捕获名称                         | 描述     | 支持语言 |
| -------------------------------- | -------- | -------- |
| `@entity.variable.pointer.type`  | 指针类型 | C        |
| `@entity.variable.pointer.name`  | 指针名   | C        |
| `@entity.variable.pointer.value` | 指针值   | C        |

### 其他实体 (@entity.\*)

| 捕获名称                                       | 描述                     | 支持语言           |
| ---------------------------------------------- | ------------------------ | ------------------ |
| `@entity.package.name`                         | 包名                     | Go, Java           |
| `@entity.import.path`                          | 导入路径                 | Go, Rust           |
| `@entity.import.alias`                         | 导入别名                 | Go, Rust           |
| `@entity.import_alias`                         | 导入别名声明             | Go                 |
| `@entity.import_dot.dot`                       | 导入点标记               | Go                 |
| `@entity.import_dot.path`                      | 导入点路径               | Go                 |
| `@entity.selector.package.name`                | 选择器包名               | Go                 |
| `@entity.selector.member.name`                 | 选择器成员名             | Go                 |
| `@entity.selector.object.name`                 | 选择器对象名             | Go                 |
| `@entity.label.name`                           | 标签名                   | Go                 |
| `@entity.namespace.name`                       | 命名空间名               | C#, TS             |
| `@entity.namespace.definition.name`            | 命名空间定义名           | C++                |
| `@entity.namespace.definition.body`            | 命名空间定义体           | C++                |
| `@entity.namespace.nested.name`                | 嵌套命名空间名           | C++                |
| `@entity.namespace.qualified_name`             | 限定命名空间名           | C#                 |
| `@entity.namespace.file_scoped.name`           | 文件作用域命名空间名     | C#                 |
| `@entity.namespace.file_scoped.qualified_name` | 限定文件作用域命名空间名 | C#                 |
| `@entity.module.name`                          | 模块名                   | Java, Rust, TS     |
| `@entity.template.class.params`                | 模板类参数               | C++                |
| `@entity.template.class.name`                  | 模板类名                 | C++                |
| `@entity.template.class.body`                  | 模板类体                 | C++                |
| `@entity.template.struct.params`               | 模板结构体参数           | C++                |
| `@entity.template.struct.name`                 | 模板结构体名             | C++                |
| `@entity.template.struct.body`                 | 模板结构体体             | C++                |
| `@entity.template.function.params`             | 模板函数参数             | C++                |
| `@entity.template.function.name`               | 模板函数名               | C++                |
| `@entity.template.function.return_type`        | 模板函数返回类型         | C++                |
| `@entity.template.method.params`               | 模板方法参数             | C++                |
| `@entity.template.method.name`                 | 模板方法名               | C++                |
| `@entity.constant.name`                        | 常量名                   | Go, Rust           |
| `@entity.constant.type`                        | 常量类型                 | Go                 |
| `@entity.constant.value`                       | 常量值                   | Go                 |
| `@entity.static.name`                          | 静态项名                 | Rust               |
| `@entity.macro.name`                           | 宏定义名                 | Rust               |
| `@entity.macro.attribute.content`              | 属性宏内容               | Rust               |
| `@entity.macro.attribute.inner.content`        | 内部属性宏内容           | Rust               |
| `@entity.impl.type.name`                       | 实现类型名               | Rust               |
| `@entity.impl.trait.name`                      | 实现 Trait 名            | Rust               |
| `@entity.impl.for.type.name`                   | 实现的目标类型名         | Rust               |
| `@entity.parameter.self`                       | Self 参数                | Rust               |
| `@entity.lifetime.name`                        | 生命周期名               | Rust               |
| `@entity.type_parameter.name`                  | 类型参数名               | C#, Java, Rust, TS |
| `@entity.where_clause`                         | Where 子句               | Rust               |
| `@entity.match.value`                          | Match 值                 | Rust               |
| `@entity.match_arm.pattern`                    | Match 分支模式           | Rust               |
| `@entity.unsafe_block`                         | Unsafe 块                | Rust               |
| `@entity.foreign_mod`                          | 外部模块                 | Rust               |
| `@entity.delegate.name`                        | 委托名                   | C#                 |
| `@entity.attribute.name`                       | 属性名                   | C, C++             |
| `@entity.property.name`                        | 属性名                   | C#, JS, TS         |
| `@entity.property.value`                       | 属性值                   | JS, TS             |
| `@entity.event.name`                           | 事件名                   | C#                 |
| `@entity.object.definition`                    | 对象定义                 | JS, TS             |
| `@entity.array.definition`                     | 数组定义                 | JS, TS             |
| `@entity.decorator.name`                       | 装饰器名                 | JS, TS             |
| `@entity.decorator.call.name`                  | 装饰器调用名             | JS, TS             |
| `@entity.export.declaration`                   | 导出声明                 | JS, TS             |
| `@entity.export.default.name`                  | 默认导出名               | JS, TS             |
| `@entity.lambda`                               | Lambda 表达式            | Java               |
| `@entity.linq_expression`                      | LINQ 表达式              | C#                 |
| `@entity.public_field_definition`              | 公共字段定义             | TS                 |
| `@entity.preprocessor.macro.name`              | 预处理器宏名             | C                  |
| `@entity.preprocessor.macro_function.name`     | 预处理器宏函数名         | C                  |
| `@entity.preprocessor.macro_function.params`   | 预处理器宏函数参数       | C                  |

### 前端语言特有实体 (@entity.\*)

#### JavaScript/TypeScript 特有

| 捕获名称                             | 描述                 | 支持语言            |
| ------------------------------------ | -------------------- | ------------------- |
| `@entity.class.base`                 | 类基类               | JS                  |
| `@entity.method.signature.name`      | 方法签名名           | TS                  |
| `@entity.method.abstract.name`       | 抽象方法名           | TS                  |
| `@entity.variable.let.value`         | Let 声明值           | JS                  |
| `@entity.variable.var.value`         | Var 声明值           | JS                  |
| `@entity.variable.parameter.prop.name`   | Prop 参数名        | Vue, Svelte         |
| `@entity.variable.parameter.prop.value`  | Prop 参数值        | Vue                 |
| `@entity.variable.parameter.bind.name`   | Bind 参数名        | Svelte              |
| `@entity.variable.parameter.bind.value`  | Bind 参数值        | Svelte              |
| `@entity.variable.parameter.slot.name`   | Slot 参数名        | Vue                 |
| `@entity.template.name`              | 模板名               | JSX, Svelte, Vue    |
| `@entity.template.value`             | 模板值               | Svelte, Vue         |
| `@entity.template.bind.name`         | 模板 bind 名         | Svelte, Vue         |
| `@entity.template.bind.value`        | 模板 bind 值         | Svelte, Vue         |
| `@entity.template.ref.attr`          | 模板 ref 属性        | Svelte, Vue         |
| `@entity.template.ref.value`         | 模板 ref 值          | Svelte, Vue         |
| `@entity.template.id.attr_name`      | 模板 id 属性名       | HTML                |
| `@entity.template.id.value`          | 模板 id 值           | HTML                |
| `@entity.attribute.class.name`       | 类属性名             | JSX                 |
| `@entity.attribute.style.name`       | 样式属性名           | JSX                 |
| `@entity.attribute.style.attr.name`  | 样式属性 attr 名     | JSX, Vue            |
| `@entity.attribute.style.directive.name`  | 样式指令名      | Svelte              |
| `@entity.attribute.style.directive.value` | 样式指令值     | Svelte              |
| `@entity.attribute.class.directive.name`  | 类指令名       | Svelte              |
| `@entity.attribute.class.directive.value` | 类指令值       | Svelte              |
| `@entity.attribute.transition.name`   | 过渡属性名           | Svelte              |
| `@entity.attribute.transition.value`  | 过渡属性值           | Svelte              |
| `@entity.attribute.animation.name`    | 动画属性名           | Svelte              |
| `@entity.attribute.animation.value`   | 动画属性值           | Svelte              |
| `@entity.attribute.slot.name`         | Slot 属性名          | Vue                 |
| `@entity.attribute.slot.value`        | Slot 属性值          | Vue                 |
| `@entity.attribute.slot.value_full`   | Slot 属性完整值      | Vue                 |
| `@entity.attribute.class.static.name` | 静态类属性名         | Vue                 |
| `@entity.attribute.class.static.value`| 静态类属性值         | Vue                 |
| `@entity.attribute.class.dynamic.shorthand` | 动态类简写   | Vue                 |
| `@entity.attribute.class.dynamic.arg`      | 动态类参数   | Vue                 |
| `@entity.attribute.class.dynamic.value`    | 动态类值     | Vue                 |
| `@entity.attribute.style.static.name`  | 静态样式属性名       | Vue                 |
| `@entity.attribute.style.static.value` | 静态样式属性值       | Vue                 |
| `@entity.attribute.style.dynamic.shorthand` | 动态样式简写 | Vue                 |
| `@entity.attribute.style.dynamic.arg`      | 动态样式参数  | Vue                 |
| `@entity.attribute.style.dynamic.value`    | 动态样式值    | Vue                 |
| `@entity.attribute.style.scope.name`  | 样式作用域名         | Vue                 |
| `@entity.attribute.ref.name`           | Ref 属性名           | Vue                 |
| `@entity.attribute.ref.value`          | Ref 属性值           | Vue                 |
| `@entity.attribute.ref.value_full`     | Ref 属性完整值       | Vue                 |
| `@entity.attribute.key.name`           | Key 属性名           | Vue, JSX            |
| `@entity.attribute.key.value`          | Key 属性值           | Vue, JSX            |
| `@entity.attribute.key.value_full`     | Key 属性完整值       | Vue                 |

#### JSX/TSX 特有

| 捕获名称                             | 描述                 | 支持语言            |
| ------------------------------------ | -------------------- | ------------------- |
| `@entity.jsx.element.name`           | JSX 元素名           | JSX, TSX            |
| `@entity.jsx.element.attributes`     | JSX 元素属性         | JSX, TSX            |
| `@entity.jsx.element.opening`        | JSX 元素开始标签     | JSX, TSX            |
| `@entity.jsx.element.close_name`     | JSX 元素闭合名       | JSX, TSX            |
| `@entity.jsx.element.closing`        | JSX 元素闭合标签     | JSX, TSX            |
| `@entity.jsx.element.self_closing.name` | JSX 自闭合元素名 | JSX, TSX            |
| `@entity.jsx.element.self_closing.attributes` | JSX 自闭合属性 | JSX, TSX        |
| `@entity.jsx.element.full.opening`   | JSX 完整开始标签     | JSX, TSX            |
| `@entity.jsx.element.full.closing`   | JSX 完整闭合标签     | JSX, TSX            |
| `@entity.jsx.component.opening.name` | JSX 组件开始名       | JSX, TSX            |
| `@entity.jsx.component.self_closing.name` | JSX 自闭合组件名 | JSX, TSX        |
| `@entity.jsx.component.closing.name` | JSX 组件闭合名       | JSX, TSX            |
| `@entity.jsx.attribute.name`         | JSX 属性名           | JSX, TSX            |
| `@entity.jsx.attribute.expr.value`   | JSX 属性表达式值     | JSX, TSX            |
| `@entity.jsx.expression.content`     | JSX 表达式内容       | JSX, TSX            |
| `@entity.jsx.expression.conditional` | JSX 条件表达式       | JSX, TSX            |
| `@entity.jsx.expression.logical`     | JSX 逻辑表达式       | JSX, TSX            |
| `@entity.jsx.text`                   | JSX 文本             | JSX, TSX            |
| `@entity.jsx.html_entity`            | JSX HTML 实体        | JSX, TSX            |
| `@entity.jsx.comment`                | JSX 注释             | JSX, TSX            |
| `@entity.jsx.key.name`               | JSX key 属性名       | JSX, TSX            |
| `@entity.jsx.ref.attr.name`          | JSX ref 属性名       | JSX, TSX            |
| `@entity.jsx.ref.callback`           | JSX ref 回调         | JSX, TSX            |
| `@entity.jsx.ref.string.name`        | JSX ref 字符串名     | JSX, TSX            |
| `@entity.jsx.className.name`         | JSX className 属性名 | JSX, TSX            |
| `@entity.jsx.style.attr.name`        | JSX style 属性名     | JSX, TSX            |
| `@entity.jsx.event.name`             | JSX 事件名           | JSX, TSX            |
| `@entity.jsx.event.handler`          | JSX 事件处理器       | JSX, TSX            |
| `@entity.jsx.dangerous.name`         | JSX dangerouslySetInnerHTML | JSX, TSX  |
| `@entity.jsx.parent.name`            | JSX 父元素名         | JSX, TSX            |
| `@entity.jsx.child.name`             | JSX 子元素名         | JSX, TSX            |
| `@entity.jsx.children`               | JSX 子元素           | JSX, TSX            |
| `@entity.jsx.expression.child`       | JSX 表达式子元素     | JSX, TSX            |
| `@entity.jsx.namespace.dependency.ns`| JSX 命名空间依赖 ns  | JSX, TSX            |
| `@entity.jsx.namespace.dependency.name`| JSX 命名空间依赖名  | JSX, TSX            |
| `@entity.jsx.namespace.dependency.self_closing.name` | JSX 命名空间自闭合依赖名 | JSX, TSX |

#### Svelte 特有

| 捕获名称                             | 描述                 | 支持语言            |
| ------------------------------------ | -------------------- | ------------------- |
| `@entity.document`                   | 文档根               | Svelte              |
| `@entity.script.context.attr`        | Script 上下文属性    | Svelte              |
| `@entity.script.context.value`       | Script 上下文值      | Svelte              |
| `@entity.script.context`             | Script 上下文        | Svelte              |
| `@entity.script.start_tag`           | Script 开始标签      | Svelte              |
| `@entity.script.content`             | Script 内容          | Svelte, Vue, HTML   |
| `@entity.script.end_tag`             | Script 结束标签      | Svelte              |
| `@entity.style.start_tag`            | Style 开始标签       | Svelte, Vue, HTML   |
| `@entity.style.content`              | Style 内容           | Svelte, Vue, HTML   |
| `@entity.style.end_tag`              | Style 结束标签       | Svelte, Vue, HTML   |
| `@entity.element.name`               | 元素名               | Svelte, Vue, HTML   |
| `@entity.element.start_tag`          | 元素开始标签         | Svelte, Vue, HTML   |
| `@entity.element.end_tag`            | 元素结束标签         | Svelte, Vue, HTML   |
| `@entity.element.void.name`          | 空元素名             | Svelte, Vue, HTML   |
| `@entity.tag.start.name`             | 标签开始名           | Svelte              |
| `@entity.tag.start`                  | 标签开始             | Svelte              |
| `@entity.tag.end.name`               | 标签结束名           | Svelte              |
| `@entity.tag.end`                    | 标签结束             | Svelte              |
| `@entity.component.name`             | 组件名               | Svelte, Vue         |
| `@entity.component.start_tag`        | 组件开始标签         | Svelte              |
| `@entity.component.end_tag`          | 组件结束标签         | Svelte              |
| `@entity.component.self_closing.name`| 组件自闭合名         | Svelte, Vue         |
| `@entity.if.start`                   | If 开始              | Svelte              |
| `@entity.if.else_if`                 | If else_if           | Svelte              |
| `@entity.if.end`                     | If 结束              | Svelte              |
| `@entity.else.start`                 | Else 开始            | Svelte              |
| `@entity.else_if.start`              | Else_if 开始         | Svelte              |
| `@entity.each.start`                 | Each 开始            | Svelte              |
| `@entity.each.end`                   | Each 结束            | Svelte              |
| `@entity.each.else.start`            | Each else 开始       | Svelte              |
| `@entity.each.else.end`              | Each else 结束       | Svelte              |
| `@entity.await.start`                | Await 开始           | Svelte              |
| `@entity.await.then`                 | Await then           | Svelte              |
| `@entity.await.end`                  | Await 结束           | Svelte              |
| `@entity.catch.start`                | Catch 开始           | Svelte              |
| `@entity.then.start`                 | Then 开始            | Svelte              |
| `@entity.key.start`                  | Key 开始             | Svelte              |
| `@entity.key.end`                    | Key 结束             | Svelte              |
| `@entity.expression`                 | 表达式               | Svelte              |
| `@entity.html.expression`            | HTML 表达式          | Svelte              |
| `@entity.const.expression`           | Const 表达式         | Svelte              |
| `@entity.attribute.name`             | 属性名               | Svelte, Vue, HTML   |
| `@entity.attribute.value`            | 属性值               | Svelte, Vue, HTML   |
| `@entity.attribute.quoted_value`     | 属性引用值           | Svelte              |
| `@entity.attribute.expr_value`       | 属性表达式值         | Svelte              |
| `@entity.event.handler.name`         | 事件处理器名         | Svelte              |
| `@entity.event.handler.value`        | 事件处理器值         | Svelte              |
| `@entity.event.modifier.name`        | 事件修饰符名         | Svelte              |
| `@entity.binding.name`               | 绑定名               | Svelte              |
| `@entity.binding.value`              | 绑定值               | Svelte              |
| `@entity.transition.name`            | 过渡名               | Svelte              |
| `@entity.transition.value`           | 过渡值               | Svelte              |
| `@entity.transition.in.name`         | 进入过渡名           | Svelte              |
| `@entity.transition.in.value`        | 进入过渡值           | Svelte              |
| `@entity.transition.out.name`        | 离开过渡名           | Svelte              |
| `@entity.transition.out.value`       | 离开过渡值           | Svelte              |
| `@entity.animation.name`             | 动画名               | Svelte              |
| `@entity.animation.value`            | 动画值               | Svelte              |
| `@entity.class_directive.name`       | 类指令名             | Svelte              |
| `@entity.class_directive.value`      | 类指令值             | Svelte              |
| `@entity.style_directive.name`       | 样式指令名           | Svelte              |
| `@entity.style_directive.value`      | 样式指令值           | Svelte              |
| `@entity.use_directive.name`         | Use 指令名           | Svelte              |
| `@entity.use_directive.value`        | Use 指令值           | Svelte              |
| `@entity.text`                       | 文本                 | Svelte, Vue, HTML   |
| `@entity.raw_text`                   | 原始文本             | Svelte, Vue, HTML   |
| `@entity.raw_text_expr`              | 原始文本表达式       | Svelte              |
| `@entity.raw_text_await`             | 原始文本 await       | Svelte              |
| `@entity.raw_text_each`              | 原始文本 each        | Svelte              |
| `@entity.comment`                    | 注释                 | Svelte, Vue, HTML   |
| `@entity.contains.element.parent.name`| 包含元素父名       | Svelte, Vue, HTML   |
| `@entity.contains.element.child.name` | 包含元素子名       | Svelte, Vue, HTML   |
| `@entity.contains.element.children`  | 包含元素子元素       | Svelte, Vue, HTML   |
| `@entity.contains.element.child.component` | 包含元素子组件 | Svelte, Vue    |
| `@entity.control.flow.if.start`      | 控制流 if 开始       | Svelte              |
| `@entity.control.flow.if.condition`  | 控制流 if 条件       | Svelte              |
| `@entity.control.flow.if.content`    | 控制流 if 内容       | Svelte              |
| `@entity.control.flow.if.else_if`    | 控制流 if else_if    | Svelte              |
| `@entity.control.flow.if.else`       | 控制流 if else       | Svelte              |
| `@entity.control.flow.each.start`    | 控制流 each 开始     | Svelte              |
| `@entity.control.flow.each.collection`| 控制流 each 集合   | Svelte              |
| `@entity.control.flow.each.content`  | 控制流 each 内容     | Svelte              |
| `@entity.control.flow.each.empty`    | 控制流 each 空       | Svelte              |
| `@entity.control.flow.await.start`   | 控制流 await 开始    | Svelte              |
| `@entity.control.flow.await.promise` | 控制流 await promise | Svelte              |
| `@entity.control.flow.await.pending` | 控制流 await pending | Svelte              |
| `@entity.control.flow.await.resolved`| 控制流 await resolved| Svelte              |
| `@entity.control.flow.await.rejected`| 控制流 await rejected| Svelte              |
| `@entity.style.scope.content`        | 样式作用域内容       | Svelte, Vue         |

#### Vue 特有

| 捕获名称                             | 描述                 | 支持语言            |
| ------------------------------------ | -------------------- | ------------------- |
| `@entity.component.root`             | 组件根               | Vue                 |
| `@entity.template.start`             | 模板开始             | Vue                 |
| `@entity.template.end`               | 模板结束             | Vue                 |
| `@entity.directive.bind.shorthand`   | Bind 简写            | Vue                 |
| `@entity.directive.bind.arg`         | Bind 参数            | Vue                 |
| `@entity.directive.bind.value`       | Bind 值              | Vue                 |
| `@entity.directive.on.shorthand`     | On 简写              | Vue                 |
| `@entity.directive.on.arg`           | On 参数              | Vue                 |
| `@entity.directive.on.value`         | On 值                | Vue                 |
| `@entity.directive.model.name`       | Model 名             | Vue                 |
| `@entity.directive.model.arg`        | Model 参数           | Vue                 |
| `@entity.directive.model.value`      | Model 值             | Vue                 |
| `@entity.directive.if.name`          | If 名                | Vue                 |
| `@entity.directive.if.value`         | If 值                | Vue                 |
| `@entity.directive.if.value_full`    | If 完整值            | Vue                 |
| `@entity.directive.else_if.name`     | Else_if 名           | Vue                 |
| `@entity.directive.else_if.value`    | Else_if 值           | Vue                 |
| `@entity.directive.else.name`        | Else 名              | Vue                 |
| `@entity.directive.for.name`         | For 名               | Vue                 |
| `@entity.directive.for.value`        | For 值               | Vue                 |
| `@entity.directive.show.name`        | Show 名              | Vue                 |
| `@entity.directive.show.value`       | Show 值               | Vue                 |
| `@entity.directive.slot.name`        | Slot 名              | Vue                 |
| `@entity.directive.slot.arg`         | Slot 参数            | Vue                 |
| `@entity.directive.slot.value`       | Slot 值              | Vue                 |
| `@entity.directive.text.name`        | Text 名              | Vue                 |
| `@entity.directive.text.value`       | Text 值              | Vue                 |
| `@entity.directive.html.name`        | Html 名              | Vue                 |
| `@entity.directive.html.value`       | Html 值              | Vue                 |
| `@entity.directive.pre.name`         | Pre 名               | Vue                 |
| `@entity.directive.cloak.name`        | Cloak 名             | Vue                 |
| `@entity.directive.once.name`        | Once 名              | Vue                 |
| `@entity.directive.memo.name`         | Memo 名              | Vue                 |
| `@entity.directive.memo.value`       | Memo 值              | Vue                 |
| `@entity.directive.generic.name`     | Generic 名           | Vue                 |
| `@entity.directive.generic.arg`      | Generic 参数         | Vue                 |
| `@entity.directive.generic.value`    | Generic 值           | Vue                 |
| `@entity.interpolation.content`      | 插值内容             | Vue                 |
| `@entity.script.lang.attr`           | Script lang 属性     | Vue                 |
| `@entity.script.lang.value`          | Script lang 值       | Vue                 |
| `@entity.script.lang.value_full`     | Script lang 完整值   | Vue                 |
| `@entity.script.setup.attr`          | Script setup 属性    | Vue                 |
| `@entity.script.setup`               | Script setup         | Vue                 |
| `@entity.style.scoped.attr`          | Style scoped 属性    | Vue                 |
| `@entity.style.scoped`               | Style scoped         | Vue                 |
| `@entity.style.module.attr`          | Style module 属性    | Vue                 |
| `@entity.style.module`               | Style module         | Vue                 |
| `@entity.attribute.value_full`       | 属性完整值           | Vue                 |
| `@entity.slot_content.attr`          | Slot 内容属性        | Vue                 |
| `@entity.slot_content.value`         | Slot 内容值          | Vue                 |
| `@entity.slot_content.value_full`    | Slot 内容完整值      | Vue                 |
| `@entity.doctype`                    | 文档类型             | HTML                |

#### CSS 特有

| 捕获名称                             | 描述                 | 支持语言            |
| ------------------------------------ | -------------------- | ------------------- |
| `@entity.style_rule.selectors`       | 样式规则选择器       | CSS                 |
| `@entity.style_rule.block`           | 样式规则块           | CSS                 |
| `@entity.style_selector.class.name`  | 样式选择器类名       | CSS                 |
| `@entity.style_selector.id.name`     | 样式选择器 ID 名     | CSS                 |
| `@entity.style_selector.tag.name`    | 样式选择器标签名     | CSS                 |
| `@entity.style_selector.universal`   | 样式选择器通配符     | CSS                 |
| `@entity.style_selector.attribute.name` | 样式选择器属性名 | CSS                 |
| `@entity.style_selector.attribute.value` | 样式选择器属性值 | CSS                 |
| `@entity.style_selector.pseudo_class.name` | 伪类选择器名   | CSS                 |
| `@entity.style_selector.pseudo_class.args` | 伪类选择器参数 | CSS                 |
| `@entity.style_selector.pseudo_element.name` | 伪元素选择器名 | CSS             |
| `@entity.style_selector.nesting`     | 嵌套选择器           | CSS                 |
| `@entity.style_selector.descendant.left` | 后代选择器左侧   | CSS                 |
| `@entity.style_selector.descendant.right` | 后代选择器右侧  | CSS                 |
| `@entity.style_selector.child.left`  | 子选择器左侧         | CSS                 |
| `@entity.style_selector.child.right` | 子选择器右侧         | CSS                 |
| `@entity.style_selector.sibling.left`| 兄弟选择器左侧       | CSS                 |
| `@entity.style_selector.sibling.right`| 兄弟选择器右侧      | CSS                 |
| `@entity.style_selector.adjacent.left`| 相邻兄弟选择器左侧   | CSS                 |
| `@entity.style_selector.adjacent.right`| 相邻兄弟选择器右侧 | CSS                 |
| `@entity.style_property.name`        | 样式属性名           | CSS                 |
| `@entity.style_property.value`       | 样式属性值           | CSS                 |
| `@entity.style_property.important`   | 样式属性 important   | CSS                 |
| `@entity.style_value.function.name`  | 样式值函数名         | CSS                 |
| `@entity.style_value.function.args`  | 样式值函数参数       | CSS                 |
| `@entity.style_value.string`         | 样式值字符串         | CSS                 |
| `@entity.style_value.color`          | 样式值颜色           | CSS                 |
| `@entity.style_value.integer`        | 样式值整数           | CSS                 |
| `@entity.style_value.unit`           | 样式值单位           | CSS                 |
| `@entity.style_value.float`          | 样式值浮点数         | CSS                 |
| `@entity.style_value.plain`          | 样式值纯文本         | CSS                 |
| `@entity.at.charset.encoding`        | @charset 编码        | CSS                 |
| `@entity.at.namespace.name`          | @namespace 名        | CSS                 |
| `@entity.at.namespace.url`           | @namespace URL       | CSS                 |
| `@entity.at.media.block`             | @media 块            | CSS                 |
| `@entity.at.supports.block`          | @supports 块         | CSS                 |
| `@entity.at.keyframes.name`          | @keyframes 名        | CSS                 |
| `@entity.at.keyframes.blocks`        | @keyframes 块        | CSS                 |
| `@entity.keyframe.selector`          | 关键帧选择器         | CSS                 |
| `@entity.keyframe.block`             | 关键帧块             | CSS                 |
| `@entity.at.scope.block`             | @scope 块            | CSS                 |
| `@entity.at.generic.keyword`         | 通用 @ 规则关键字    | CSS                 |
| `@entity.at.generic.block`           | 通用 @ 规则块        | CSS                 |
| `@entity.contains.media.rule`        | 包含 media 规则      | CSS                 |
| `@entity.contains.media.rules`       | 包含 media 规则集合  | CSS                 |
| `@entity.contains.keyframes.name`    | 包含 keyframes 名    | CSS                 |
| `@entity.contains.keyframes.block`   | 包含 keyframes 块   | CSS                 |
| `@entity.contains.keyframes.blocks`  | 包含 keyframes 块集合| CSS                 |
| `@entity.contains.supports.rule`     | 包含 supports 规则   | CSS                 |
| `@entity.contains.supports.rules`    | 包含 supports 规则集合| CSS                |
| `@entity.contains.style.parent.selector`| 包含样式父选择器 | CSS                 |
| `@entity.contains.style.nested.rule` | 包含样式嵌套规则     | CSS                 |
| `@entity.contains.style.parent.block` | 包含样式父块       | CSS                 |

#### HTML 特有

| 捕获名称                             | 描述                 | 支持语言            |
| ------------------------------------ | -------------------- | ------------------- |
| `@entity.script.tag_name`            | Script 标签名         | HTML, Svelte, Vue   |
| `@entity.script.attributes`          | Script 属性集合      | HTML, Svelte, Vue   |
| `@entity.script.attribute.name`      | Script 属性名        | HTML                |
| `@entity.script.attribute.value`     | Script 属性值        | HTML                |
| `@entity.style.tag_name`             | Style 标签名          | HTML, Svelte, Vue   |
| `@entity.style.attributes`           | Style 属性集合       | HTML, Svelte, Vue   |
| `@entity.style.attribute.name`       | Style 属性名         | HTML                |
| `@entity.style.attribute.value`      | Style 属性值         | HTML                |
| `@entity.attribute.quoted.name`      | 引用属性名           | HTML                |
| `@entity.attribute.quoted.value`     | 引用属性值           | HTML                |
| `@entity.contains.element.text_container.name`| 包含文本容器元素名 | HTML         |
| `@entity.contains.element.text_content`| 包含元素文本内容    | HTML                |

#### 嵌入块实体 (@entity.embedded.*)

| 捕获名称                             | 描述                 | 支持语言            |
| ------------------------------------ | -------------------- | ------------------- |
| `@embedded.script.tag_name`          | 嵌入 Script 标签名   | Svelte, Vue, HTML   |
| `@embedded.script.attributes`        | 嵌入 Script 属性集合 | Svelte, Vue, HTML   |
| `@embedded.script.attr.name`         | 嵌入 Script 属性名   | Svelte, Vue, HTML   |
| `@embedded.script.attr.value`        | 嵌入 Script 属性值   | Svelte, Vue, HTML   |
| `@embedded.script.attr.value_full`   | 嵌入 Script 属性完整值 | Svelte, Vue, HTML |
| `@embedded.script.start_tag`         | 嵌入 Script 开始标签 | Svelte, Vue, HTML   |
| `@embedded.script.content`           | 嵌入 Script 内容     | Svelte, Vue, HTML   |
| `@embedded.script.end_tag`           | 嵌入 Script 结束标签 | Svelte, Vue, HTML   |
| `@embedded.style.tag_name`           | 嵌入 Style 标签名    | Svelte, Vue, HTML   |
| `@embedded.style.attributes`         | 嵌入 Style 属性集合  | Svelte, Vue, HTML   |
| `@embedded.style.attr.name`          | 嵌入 Style 属性名    | Svelte, Vue, HTML   |
| `@embedded.style.attr.value`         | 嵌入 Style 属性值    | Svelte, Vue, HTML   |
| `@embedded.style.attr.value_full`    | 嵌入 Style 属性完整值 | Svelte, Vue, HTML |
| `@embedded.style.start_tag`          | 嵌入 Style 开始标签  | Svelte, Vue, HTML   |
| `@embedded.style.content`            | 嵌入 Style 内容      | Svelte, Vue, HTML   |
| `@embedded.style.end_tag`            | 嵌入 Style 结束标签  | Svelte, Vue, HTML   |

#### CSS-in-JS 相关 (@entity.css_in_js.*)

| 捕获名称                             | 描述                 | 支持语言            |
| ------------------------------------ | -------------------- | ------------------- |
| `@css_in_js.styled.object`           | styled 对象          | JS, TS              |
| `@css_in_js.styled.tag`              | styled 标签          | JS, TS              |
| `@css_in_js.styled.content`          | styled 内容          | JS, TS              |
| `@css_in_js.styled_func.name`        | styled 函数名        | JS, TS              |
| `@css_in_js.styled_func.target`      | styled 函数目标      | JS, TS              |
| `@css_in_js.styled_func.content`     | styled 函数内容      | JS, TS              |
| `@css_in_js.styled_attrs.styled`     | styled attrs styled  | JS, TS              |
| `@css_in_js.styled_attrs.method`     | styled attrs 方法    | JS, TS              |
| `@css_in_js.styled_attrs.content`    | styled attrs 内容    | JS, TS              |
| `@css_in_js.emotion.css_func`        | emotion css 函数     | JS, TS              |
| `@css_in_js.emotion.content`         | emotion 内容         | JS, TS              |
| `@css_in_js.emotion.cx_func`         | emotion cx 函数      | JS, TS              |
| `@css_in_js.emotion.cx_content`      | emotion cx 内容      | JS, TS              |
| `@css_in_js.global_style.func`       | global_style 函数    | JS, TS              |
| `@css_in_js.global_style.content`    | global_style 内容    | JS, TS              |
| `@css_in_js.keyframes.func`          | keyframes 函数       | JS, TS              |
| `@css_in_js.keyframes.content`       | keyframes 内容       | JS, TS              |
| `@css_in_js.inject_global.func`      | inject_global 函数   | JS, TS              |
| `@css_in_js.inject_global.content`   | inject_global 内容   | JS, TS              |

### Python 特有实体 (@entity.\*)

| 捕获名称                             | 描述                   | 支持语言 |
| ------------------------------------ | ---------------------- | -------- |
| `@entity.function.async.name`        | 异步函数名             | Python   |
| `@entity.function.async.params`      | 异步函数参数           | Python   |
| `@entity.function.async.body`        | 异步函数体             | Python   |
| `@entity.function.generator.name`    | 生成器函数名           | Python   |
| `@entity.method.class.name`          | 类方法名               | Python   |
| `@entity.method.class.cls_param`     | 类方法 cls 参数        | Python   |
| `@entity.method.instance.name`       | 实例方法名             | Python   |
| `@entity.method.instance.self_param` | 实例方法 self 参数     | Python   |
| `@entity.method.static.name`         | 静态方法名             | Python   |
| `@entity.method.static.decorator`    | 静态方法装饰器         | Python   |
| `@entity.method.getter.name`         | Property getter 名     | Python   |
| `@entity.method.getter.decorator`    | Property getter 装饰器 | Python   |
| `@entity.lambda.name`                | Lambda 表达式名        | Python   |
| `@entity.lambda.params`              | Lambda 表达式参数      | Python   |
| `@entity.comprehension.list.name`    | 列表推导式名           | Python   |
| `@entity.comprehension.dict.name`    | 字典推导式名           | Python   |
| `@entity.comprehension.set.name`     | 集合推导式名           | Python   |
| `@entity.variable.typed.name`        | 类型注解变量名         | Python   |
| `@entity.variable.typed.type`        | 类型注解类型           | Python   |
| `@entity.variable.multiple.name`     | 多重赋值变量名         | Python   |
| `@entity.variable.global.name`       | 全局变量名             | Python   |
| `@entity.variable.nonlocal.name`     | Nonlocal 变量名        | Python   |
| `@entity.parameter.typed.name`       | 类型注解参数名         | Python   |
| `@entity.parameter.typed.type`       | 类型注解参数类型       | Python   |
| `@entity.statement.with`             | With 语句              | Python   |
| `@entity.statement.try`              | Try 语句               | Python   |
| `@entity.statement.match`            | Match 语句             | Python   |
| `@entity.decorator.call.arguments`   | 装饰器调用参数         | Python   |

### Typedef 相关 (@entity.typedef\*)

| 捕获名称                                  | 描述                 | 支持语言 |
| ----------------------------------------- | -------------------- | -------- |
| `@entity.typedef.original_type`           | Typedef 原始类型     | C        |
| `@entity.typedef.alias`                   | Typedef 别名         | C        |
| `@entity.typedef_struct.original_name`    | Typedef 结构体原始名 | C        |
| `@entity.typedef_struct.body`             | Typedef 结构体体     | C        |
| `@entity.typedef_struct.alias`            | Typedef 结构体别名   | C        |
| `@entity.typedef_union.original_name`     | Typedef 联合体原始名 | C        |
| `@entity.typedef_union.body`              | Typedef 联合体体     | C        |
| `@entity.typedef_union.alias`             | Typedef 联合体别名   | C        |
| `@entity.typedef_enum.original_name`      | Typedef 枚举原始名   | C        |
| `@entity.typedef_enum.body`               | Typedef 枚举体       | C        |
| `@entity.typedef_enum.alias`              | Typedef 枚举别名     | C        |
| `@entity.typedef_function_pointer.params` | Typedef 函数指针参数 | C        |
| `@entity.typedef_function_pointer.alias`  | Typedef 函数指针别名 | C        |

---

## Call Domain 完整列表

### 函数调用 (@call.function)

| 捕获名称                    | 描述             | 支持语言                           |
| --------------------------- | ---------------- | ---------------------------------- |
| `@call.function.name`       | 直接函数调用名   | C, C++, Go, Java, JS, Rust, Python |
| `@call.function.arguments`  | 直接函数调用参数 | C, Go, JS, Python                  |
| `@call.function.async.name` | 异步函数调用名   | Python                             |

### 实例方法调用 (@call.method)

| 捕获名称                      | 描述             | 支持语言                       |
| ----------------------------- | ---------------- | ------------------------------ |
| `@call.method.object`         | 实例方法对象     | C#, Go, Java, JS, Rust, Python |
| `@call.method.function`       | 实例方法函数名   | C#, Go, Java, JS, Rust, Python |
| `@call.method.arguments`      | 方法调用参数     | Go, Python                     |
| `@call.method.class.object`   | 类方法调用对象   | Python                         |
| `@call.method.class.function` | 类方法调用函数名 | Python                         |

### 静态方法调用 (@call.method.static)

| 捕获名称                                   | 描述                   | 支持语言     |
| ------------------------------------------ | ---------------------- | ------------ |
| `@call.method.static.object`               | 静态方法类名           | C#, Go, Java |
| `@call.method.static.function`             | 静态方法函数名         | C#, Go, Java |
| `@call.method.static.qualified.expression` | 限定静态方法调用表达式 | C#           |
| `@call.method.static.qualified.function`   | 限定静态方法调用函数名 | C#           |

### 链式调用 (@call.method.chained)

| 捕获名称                    | 描述         | 支持语言                      |
| --------------------------- | ------------ | ----------------------------- |
| `@call.method.chained.from` | 链式调用起点 | C, Go, Java, JS, Rust, Python |
| `@call.method.chained.to`   | 链式调用终点 | C, Go, Java, JS, Rust, Python |

### 构造函数调用 (@call.constructor)

| 捕获名称                            | 描述               | 支持语言       |
| ----------------------------------- | ------------------ | -------------- |
| `@call.constructor.name`            | 构造函数类型名     | C#, JS, Python |
| `@call.constructor.arguments`       | 构造函数参数       | JS, Python     |
| `@call.constructor.member.object`   | 构造函数成员对象   | JS             |
| `@call.constructor.member.property` | 构造函数成员属性   | JS             |
| `@call.constructor.super`           | Super 构造函数调用 | Java           |
| `@call.constructor.qualified.name`  | 限定构造函数类型名 | C#             |

### 指针调用 (@call.pointer)

| 捕获名称                      | 描述       | 支持语言 |
| ----------------------------- | ---------- | -------- |
| `@call.pointer.variable.name` | 指针变量名 | C        |

### 回调调用 (@call.callback)

| 捕获名称                       | 描述       | 支持语言 |
| ------------------------------ | ---------- | -------- |
| `@call.callback.function.name` | 回调函数名 | C, Go    |
| `@call.callback.argument`      | 回调参数   | C, Go    |
| `@call.callback.method.name`   | 回调方法名 | Go       |

### 模板/泛型调用 (@call.template/@call.generic)

| 捕获名称                            | 描述             | 支持语言 |
| ----------------------------------- | ---------------- | -------- |
| `@call.template.function.name`      | 模板函数调用名   | C++      |
| `@call.template.function.arguments` | 模板函数调用参数 | C++      |
| `@call.template.method.object`      | 模板方法调用对象 | C++      |
| `@call.template.method.name`        | 模板方法调用名   | C++      |
| `@call.template.method.arguments`   | 模板方法调用参数 | C++      |
| `@call.generic.function`            | 泛型函数调用     | C#, TS   |
| `@call.generic.function.name`       | 泛型函数调用名   | Rust, TS |
| `@call.generic.type_args`           | 泛型类型参数     | TS       |
| `@call.generic.method.name`         | 泛型方法调用名   | C#, TS   |
| `@call.generic.object.name`         | 泛型对象名       | C#       |

### 其他调用 (@call.\*)

| 捕获名称                                | 描述                 | 支持语言 |
| --------------------------------------- | -------------------- | -------- |
| `@call.macro.name`                      | 宏调用名             | Rust     |
| `@call.macro.scoped.name`               | 作用域宏调用名       | Rust     |
| `@call.closure`                         | 闭包调用             | Rust     |
| `@call.async`                           | Async 调用           | JS       |
| `@call.promise.then.object`             | Promise.then 对象    | JS       |
| `@call.promise.then.method`             | Promise.then 方法    | JS       |
| `@call.promise.catch.object`            | Promise.catch 对象   | JS       |
| `@call.promise.catch.method`            | Promise.catch 方法   | JS       |
| `@call.special.call.object`             | call() 对象          | JS       |
| `@call.special.call.method`             | call() 方法          | JS       |
| `@call.special.apply.object`            | apply() 对象         | JS       |
| `@call.special.apply.method`            | apply() 方法         | JS       |
| `@call.special.bind.object`             | bind() 对象          | JS       |
| `@call.special.bind.method`             | bind() 方法          | JS       |
| `@call.delegate.name`                   | 委托调用名           | C#       |
| `@call.goroutine.function.name`         | Goroutine 函数名     | Go       |
| `@call.goroutine.arguments`             | Goroutine 参数       | Go       |
| `@call.goroutine.method.object.name`    | Goroutine 方法对象名 | Go       |
| `@call.goroutine.method.function.name`  | Goroutine 方法函数名 | Go       |
| `@call.goroutine.method.arguments`      | Goroutine 方法参数   | Go       |
| `@call.deferred.function.name`          | Deferred 函数名      | Go       |
| `@call.deferred.arguments`              | Deferred 参数        | Go       |
| `@call.deferred.method.object.name`     | Deferred 方法对象名  | Go       |
| `@call.deferred.method.function.name`   | Deferred 方法函数名  | Go       |
| `@call.deferred.method.arguments`       | Deferred 方法参数    | Go       |
| `@call.associated.type.name`            | 关联函数类型名       | Rust     |
| `@call.associated.function.name`        | 关联函数名           | Rust     |
| `@call.associated.nested.path`          | 嵌套关联函数路径     | Rust     |
| `@call.associated.nested.function.name` | 嵌套关联函数名       | Rust     |
| `@call.reference`                       | 方法引用             | Java     |
| `@call.super.name`                      | Super 调用名         | Python   |
| `@call.super.method.class`              | Super 方法调用类     | Python   |
| `@call.super.method.name`               | Super 方法调用名     | Python   |
| `@call.yield`                           | Yield 调用           | JS       |
| `@call.yield.arguments`                 | Yield 参数           | JS       |
| `@call.return`                          | Return 表达式        | JS       |

### 前端语言特有调用 (@call.\*)

#### JSX/TSX 组件调用

| 捕获名称                                | 描述                 | 支持语言            |
| --------------------------------------- | -------------------- | ------------------- |
| `@call.constructor.component.name`      | 构造函数组件名       | JSX, TSX, Svelte, Vue |
| `@call.constructor.component.self_closing.name` | 自闭合组件名 | JSX, TSX, Svelte, Vue |
| `@call.constructor.component.self_closing` | 自闭合组件调用   | JSX, TSX, Svelte, Vue |
| `@call.method.chained.to.name`          | 链式调用终点名       | JS, TS             |

#### 事件回调调用

| 捕获名称                                | 描述                 | 支持语言            |
| --------------------------------------- | -------------------- | ------------------- |
| `@call.callback.event.name`             | 回调事件名           | JSX, TSX, Svelte, Vue |
| `@call.callback.event.handler`          | 回调事件处理器       | JSX, TSX, Svelte, Vue |
| `@call.callback.event.modifier.full_name` | 回调事件修饰符完整名 | Svelte           |
| `@call.callback.event.modifier.with`    | 回调事件修饰符带     | Svelte              |

---

## Dependency Domain 完整列表

### 导入依赖 (@dependency.import/@dependency.include/@dependency.use)

| 捕获名称                              | 描述             | 支持语言 |
| ------------------------------------- | ---------------- | -------- |
| `@dependency.include.path`            | 头文件路径       | C, C++   |
| `@dependency.import.source`           | 导入源           | JS, TS   |
| `@dependency.import.default.name`     | 默认导入名       | JS, TS   |
| `@dependency.import.default.path`     | 默认导入路径     | JS, TS   |
| `@dependency.import.namespace.alias`  | 命名空间导入别名 | JS, TS   |
| `@dependency.import.namespace.path`   | 命名空间导入路径 | JS, TS   |
| `@dependency.import.named.name`       | 命名导入名       | JS, TS   |
| `@dependency.import.named.alias`      | 命名导入别名     | JS, TS   |
| `@dependency.import.named.path`       | 命名导入路径     | JS, TS   |
| `@dependency.import.dynamic.function` | 动态导入函数     | JS, TS   |
| `@dependency.import.dynamic.path`     | 动态导入路径     | JS, TS   |
| `@dependency.import.standard.path`    | 标准导入路径     | Go       |
| `@dependency.import.alias.alias`      | 导入别名别名     | Go       |
| `@dependency.import.alias.path`       | 导入别名路径     | Go       |
| `@dependency.import.dot.dot`          | 导入点标记       | Go       |
| `@dependency.import.dot.path`         | 导入点路径       | Go       |
| `@dependency.import.blank.blank`      | 空标识符         | Go       |
| `@dependency.import.blank.path`       | 空标识符路径     | Go       |
| `@dependency.import.name`             | 导入名           | Java     |
| `@dependency.use.path`                | Use 路径         | Rust     |
| `@dependency.use.wildcard`            | Use 通配符       | Rust     |
| `@dependency.use.list`                | Use 列表         | Rust     |
| `@dependency.use.alias.path`          | Use 别名路径     | Rust     |
| `@dependency.use.alias.name`          | Use 别名名       | Rust     |
| `@dependency.use.scoped_list`         | 作用域 Use 列表  | Rust     |

### Macro 依赖 (@dependency.macro)

| 捕获名称                        | 描述         | 支持语言 |
| ------------------------------- | ------------ | -------- |
| `@dependency.macro.ifdef.name`  | #ifdef 宏名  | C, C++   |
| `@dependency.macro.ifndef.name` | #ifndef 宏名 | C        |
| `@dependency.macro.if.name`     | #if 宏名     | C        |

### Namespace 依赖 (@dependency.namespace/@dependency.using)

| 捕获名称                               | 描述             | 支持语言 |
| -------------------------------------- | ---------------- | -------- |
| `@dependency.namespace.qualified.name` | 限定命名空间名   | C#, C++  |
| `@dependency.using.namespace.name`     | Using 命名空间名 | C#       |
| `@dependency.using.namespace`          | Using 命名空间   | C++      |
| `@dependency.using.type`               | Using 类型       | C++      |
| `@dependency.using.qualified.name`     | 限定 Using 名    | C#       |

### Package 依赖 (@dependency.package)

| 捕获名称                             | 描述     | 支持语言 |
| ------------------------------------ | -------- | -------- |
| `@dependency.package.reference.name` | 包引用名 | Go       |
| `@dependency.module.name`            | 模块名   | Rust     |

### Type 依赖 (@dependency.type/@dependency.trait_bound)

| 捕获名称                           | 描述         | 支持语言 |
| ---------------------------------- | ------------ | -------- |
| `@dependency.type.base`            | 基类引用     | C#       |
| `@dependency.type.interface`       | 接口引用     | C#       |
| `@dependency.type.reference`       | 类型引用     | TS       |
| `@dependency.type.extends`         | 类型扩展     | TS       |
| `@dependency.type.extends.method`  | 类型扩展方法 | TS       |
| `@dependency.trait_bound`          | Trait 约束   | Rust     |
| `@dependency.type_parameter.name`  | 类型参数名   | Rust     |
| `@dependency.type_parameter.bound` | 类型参数约束 | Rust     |

### 继承/实现依赖 (@dependency.extend/@dependency.implement)

| 捕获名称                     | 描述       | 支持语言 |
| ---------------------------- | ---------- | -------- |
| `@dependency.extend.name`    | 扩展父类名 | Java     |
| `@dependency.implement.name` | 实现接口名 | Java     |
| `@dependency.interface`      | 接口依赖   | TS       |

### Module 依赖 (@dependency.module)

| 捕获名称                           | 描述             | 支持语言 |
| ---------------------------------- | ---------------- | -------- |
| `@dependency.module.requires.name` | 模块 requires 名 | Java     |
| `@dependency.module.exports.name`  | 模块 exports 名  | Java     |
| `@dependency.module.opens.name`    | 模块 opens 名    | Java     |
| `@dependency.module.uses.name`     | 模块 uses 名     | Java     |
| `@dependency.module.provides.name` | 模块 provides 名 | Java     |

### Require 依赖 (@dependency.require)

| 捕获名称                                | 描述                 | 支持语言 |
| --------------------------------------- | -------------------- | -------- |
| `@dependency.require.function`          | Require 函数         | JS, TS   |
| `@dependency.require.path`              | Require 源           | JS, TS   |
| `@dependency.require.name`              | Require 变量名       | JS, TS   |
| `@dependency.require.variable.function` | Require 变量函数     | JS, TS   |
| `@dependency.require.lexical.name`      | Require lexical 名   | JS, TS   |
| `@dependency.require.lexical.function`  | Require lexical 函数 | JS, TS   |
| `@dependency.require.lexical.path`      | Require lexical 源   | JS, TS   |

### 其他依赖 (@dependency.\*)

| 捕获名称                            | 描述           | 支持语言 |
| ----------------------------------- | -------------- | -------- |
| `@dependency.export.from.source`    | 导出源         | JS, TS   |
| `@dependency.where_predicate`       | Where 谓词     | Rust     |
| `@dependency.extern_crate.name`     | 外部 crate 名  | Rust     |
| `@dependency.reference.path`        | 引用路径       | Rust     |
| `@dependency.reference.scoped`      | 作用域引用     | Rust     |
| `@dependency.reference.type_path`   | 类型引用路径   | Rust     |
| `@dependency.reference.scoped_type` | 作用域类型引用 | Rust     |

### 前端语言特有依赖 (@dependency.\*)

#### JavaScript/TypeScript 依赖

| 捕获名称                            | 描述           | 支持语言            |
| ----------------------------------- | -------------- | ------------------- |
| `@dependency.export.from.source`    | 导出源         | JS, TS              |
| `@dependency.import.source`         | 导入源         | JS, TS              |
| `@dependency.import.component.name` | 导入组件名     | JSX, TSX, Svelte, Vue |
| `@dependency.import.component.self_closing.name` | 导入自闭合组件名 | JSX, TSX, Svelte, Vue |
| `@dependency.import.action.name`    | 导入 action 名  | Svelte              |
| `@dependency.import.transition.name`| 导入 transition 名 | Svelte           |
| `@dependency.import.animation.name` | 导入 animation 名 | Svelte          |
| `@dependency.import.css.path`       | 导入 CSS 路径   | CSS                 |
| `@dependency.import.css.url.func`   | 导入 CSS url 函数 | CSS              |
| `@dependency.import.css.url.path`   | 导入 CSS url 路径 | CSS              |

#### HTML 依赖

| 捕获名称                            | 描述           | 支持语言            |
| ----------------------------------- | -------------- | ------------------- |
| `@dependency.script.src.attr_name`  | Script src 属性名 | HTML              |
| `@dependency.script.src.value`      | Script src 值   | HTML                |
| `@dependency.script.external`       | 外部脚本       | HTML                |
| `@dependency.script.module.attr_name`| Script module 属性名 | HTML           |
| `@dependency.script.module.value`   | Script module 值 | HTML                |
| `@dependency.script.module`         | 模块脚本       | HTML                |
| `@dependency.link.tag`              | Link 标签       | HTML                |
| `@dependency.link.rel.name`         | Link rel 名     | HTML                |
| `@dependency.link.rel.value`        | Link rel 值     | HTML                |
| `@dependency.link.href.name`        | Link href 名    | HTML                |
| `@dependency.link.href.value`       | Link href 值    | HTML                |
| `@dependency.link.stylesheet`       | 样式表链接     | HTML                |
| `@dependency.link.resource.tag`     | 资源链接标签   | HTML                |
| `@dependency.link.resource.href.name` | 资源 href 名   | HTML                |
| `@dependency.link.resource.href.value` | 资源 href 值  | HTML                |

#### CSS 依赖

| 捕获名称                            | 描述           | 支持语言            |
| ----------------------------------- | -------------- | ------------------- |
| `@dependency.url.func`              | url 函数        | CSS                 |
| `@dependency.url.path`              | url 路径        | CSS                 |

### Python 特有依赖 (@dependency.import.\*)

| 捕获名称                                 | 描述              | 支持语言 |
| ---------------------------------------- | ----------------- | -------- |
| `@dependency.import.module.name`         | 导入模块名        | Python   |
| `@dependency.import.alias.module`        | 导入别名模块      | Python   |
| `@dependency.import.alias.name`          | 导入别名名        | Python   |
| `@dependency.import.from.module`         | From 导入模块     | Python   |
| `@dependency.import.from.name`           | From 导入名       | Python   |
| `@dependency.import.from.alias.module`   | From 导入别名模块 | Python   |
| `@dependency.import.from.alias.original` | From 导入别名原名 | Python   |
| `@dependency.import.from.alias.name`     | From 导入别名名   | Python   |
| `@dependency.import.relative.module`     | 相对导入模块      | Python   |
| `@dependency.import.wildcard`            | 通配符导入        | Python   |
| `@dependency.import.future.name`         | Future 导入名     | Python   |

---

## 验证工具

项目提供了验证工具来检查查询是否符合命名规范：

```rust
use code_context_engine::query::scheme::{is_entity_capture, is_call_capture, is_dependency_capture};

// 检查捕获类型
assert!(is_entity_capture("@entity.class.name"));
assert!(is_call_capture("@call.function.name"));
assert!(is_dependency_capture("@dependency.include.path"));
```

### 格式验证规则

```rust
use code_context_engine::query::scheme::validate_capture_name;

// 验证捕获名称格式
assert!(validate_capture_name("@entity.class.name").is_ok());           // 3段，合法
assert!(validate_capture_name("@call.method.static.function").is_ok()); // 4段，合法
assert!(validate_capture_name("@entity.type.class.name").is_err());    // 5段，非法
assert!(validate_capture_name("@call").is_err());                       // 1段，非法
assert!(validate_capture_name("@call.method").is_err());                // 2段，非法
```

### 自动转换工具

```rust
use code_context_engine::query::scheme::convert_legacy_name;

// 自动转换旧名称到新名称
assert_eq!(
    convert_legacy_name("@entity.type.class.name"),
    "@entity.class.name"
);
assert_eq!(
    convert_legacy_name("@entity.method.definition.name"),
    "@entity.method.name"
);
assert_eq!(
    convert_legacy_name("@call.method.instance.function"),
    "@call.method.function"
);
```

---

## 参考文档

- [简化命名方案](./简化命名方案.md)
- [命名规范设计](./命名规范设计.md)
- [命名规范实施指南](./命名规范实施指南.md)
- [命名迁移指南](./命名迁移指南.md)
