# Eclipse JDT 类型推断模型研究

## 概述

Eclipse JDT Core 是 Eclipse IDE 的 Java 开发工具核心，包含增量/批量 Java 编译器（ECJ）。其类型推断系统实现了 JLS Chapter 18 的完整类型推断规范，处理泛型方法调用、lambda 表达式、以及 `var` 局部变量推断。

**仓库**：`https://github.com/eclipse-jdt/eclipse.jdt.core`
**关键包**：`org.eclipse.jdt.internal.compiler.lookup`

## 核心架构

### 推断变量（Inference Variable）

JDT 使用**推断变量**（`InferenceVariable`）作为未知类型的占位符：

```java
// 对于 <T> T foo(List<T> list)
// 调用 foo(List.of(1, 2, 3)) 时：
// 创建推断变量 α，约束 α = int
// 解析后 α → int
```

**InferenceVariable 结构**：
- `id`：唯一标识符
- `bounds`：上界和下界约束
- `relatedVariables`：关联的推断变量（用于联合类型推断）

### 约束求解器（Constraint Solver）

JLS Chapter 18 定义了约束求解的两个阶段：

**阶段 1（18.5.1）**：
- 产生约束集（bound set）
- 测试解是否存在
- 不提交最终解

**阶段 2（18.5.2）**：
- 使用目标类型（target type）细化约束
- 提交最终解

### BoundSet

`BoundSet` 是约束求解的核心数据结构，存储：
- 等式约束（`α = T`）
- 子类型约束（`α <: T`）
- 超类型约束（`T <: α`）
- 相等性约束（`α = β`）

### Lambda 表达式处理

Lambda 是 JDT 类型推断中最复杂的部分。

**核心挑战**：Lambda 需要**目标类型**（target type）来确定其实现的函数式接口类型。但目标类型往往来自同一个推断过程，形成循环依赖。

**解决方案**：`LambdaExpression.cachedResolvedCopy()` 方法——
1. 给定候选目标类型，计算匹配的 SAM 方法
2. 创建 Lambda 的 AST 拷贝
3. 用候选目标类型解析拷贝的 body
4. 记录解析结果
5. 如果有多个候选目标类型，重复上述过程
6. 最终选择正确的解析结果

**实现方式选择**：
- 方案 1：实现"unresolve"方法 → 未采用（信息可能后续需要）
- 方案 2：复制 AST → 未采用（样板代码太多）
- 方案 3：重新解析输入文本 → **采用**（通过 `Parser` 重新解析 Lambda 区域）

### Functional Interface 分析

编译器识别函数式接口（只有一个抽象方法的接口），将 Lambda 视为该方法的实现：
- 支持表达式 Lambda：`(params) -> expression`
- 支持块 Lambda：`(params) -> { statements; }`
- 支持参数类型推断：`(String s) -> s.length()` 或 `s -> s.length()`

### Method Reference

支持多种方法引用形式：
- 静态方法引用：`ClassName::staticMethod`
- 实例方法引用：`instance::instanceMethod`
- 构造函数引用：`ClassName::new`
- 数组构造函数引用：`TypeName[]::new`

### 泛型与 Lambda 的集成

**泛型函数式接口**：
```java
interface Processor<T, R> {
    R process(T input);
}

Processor<String, Integer> p = s -> s.length();  // T=String, R=Integer
```

**目标类型推断**：编译器实现 JLS 18.5.4 的"更具体方法推断"规则来解决 Lambda 重载歧义。

### 类型擦除

JDT 编译器执行类型擦除以生成兼容旧 JVM 的字节码：
1. 用上界或 Object 替换类型参数
2. 在字节码中插入必要的类型转换
3. 生成桥接方法处理泛型类型的方法重写

## 性能问题

JDT 类型推断的性能瓶颈在于**约束传播的指数复杂度**：

- 问题根源在 JLS 规范本身，甚至是 ML 类型系统的固有问题
- IntelliJ 有检查+快速修复，建议用户在复杂场景添加显式类型参数
- JDT 通过 `BoundSet.deriveTypeArgumentConstraints` 处理约束对
- 2024-09 版本的性能回归由约束推导的变更引起
- 某些情况下推断变量被分开处理而非同时处理，导致性能问题

## Java 类型推断规范（JLS Chapter 18）

### 推断变量

每个泛型方法调用或构造函数调用创建一组推断变量。

### 约束公式

| 约束类型 | 含义 |
|----------|------|
| `‹S → T›` | 赋值兼容性约束 |
| `‹S ← T›` | 赋值兼容性约束（反向） |
| `‹S = T›` | 相等性约束 |
| `‹S <: T›` | 子类型约束 |

### Lambda 约束（JLS 18.2.5）

Lambda 的约束推导关键：
- 如果 Lambda 参数有显式类型 F₁...Fₙ，函数类型参数为 G₁...Gₙ，则约束为 `‹Fi = Gi›`
- 如果函数类型返回类型 R 不是 proper 类型，对 Lambda body 中的每个结果表达式 eᵢ，约束为 `‹eᵢ → R›`
- 异常类型单独约束（不影响重载决议）

### 解析过程

1. **收集约束**：从方法签名、参数类型、目标类型收集
2. **减少约束**：将复合约束分解为基本约束
3. **合一**：将等式约束应用到推断变量
4. **实例化**：将最终类型代入泛型签名

## 对 CCE 的借鉴价值

### 可采用的设计

1. **推断变量占位符**模型：
   - CCE 的 `placeholder TypeEntry`（跨文件 impl 块）可借鉴此设计
   - 创建推断变量 → 收集约束 → 求解 → 实例化

2. **两阶段推断**：
   - 阶段 1：收集约束（不提交）
   - 阶段 2：使用上下文细化（提交）
   - CCE 的 resolver 可借鉴：先尝试所有候选，再用上下文消歧

3. **约束传播**：
   - `α = T` 可传播到所有引用 α 的约束
   - CCE 可借鉴：当一个变量类型确定后，传播到相关调用

4. **Lambda 重解析策略**：
   - 对于不确定的类型推断，CCE 可采用类似的"多次尝试"策略
   - 每次用不同的候选类型解析，选择最合适的

### 需要避免的

1. **指数复杂度**：JDT 的约束传播在复杂泛型场景下性能很差
   - CCE 应设置约束传播深度限制

2. **完整 JLS 规范**：CCE 不需要实现完整的 Java 类型推断
   - 只需要覆盖常见模式

3. **AST 重新解析**：JDT 为 Lambda 重新解析输入文本的方式较重
   - CCE 可用更轻量的"尝试不同候选"方式

### 实现建议

CCE 的 Java 推断器重点：
1. 从方法签名提取泛型参数约束
2. 从 `new Constructor<T>()` 推断 `T`
3. 从 `var x = expr` 推断 `x` 的类型
4. 从 lambda 表达式推断函数式接口类型
5. 设置约束传播深度限制（如 3 层）
