# 语言服务器类型推断研究总结

## 1. 概述

本文档总结了主流语言服务器（rust-analyzer、Pyright、TypeScript、Go types、Eclipse JDT）的类型推断实现，为 CCE 的类型推断系统改进提供参考。

## 2. rust-analyzer 类型推断

### 2.1 架构特点

- **三层 hir crate**：hir-expand（宏展开）→ hir-def（名称解析）→ hir-ty（类型推断）
- **ECS 风格**：所有实体使用原始 ID，直接查询数据库
- **salsa 增量计算**：函数体内的修改不会使全局派生数据失效

### 2.2 类型推断流程

1. **AST → HIR 降低**：函数体 AST 降低为位置无关的中间表示
2. **类型推断**：`infer_query` 分析 Body，产生 `InferenceResult`
3. **trait 求解**：通过 chalk 查询 trait 实现
4. **结果缓存**：salsa 自动缓存推断结果

### 2.3 对 CCE 的借鉴

- **分两遍收集-解析**：先收集声明，再解析引用
- **保守降级策略**：推断失败时回退到 `unknown` 类型
- **per-function 粒度推断**：CCE 可按文件粒度

### 2.4 不适合 CCE 的

- **salsa 框架**：重量级增量计算框架
- **chalk trait 求解**：CCE 不需要 trait bound 求解
- **完整 HIR**：CCE 的 Entity 模型已足够

## 3. Pyright 类型推断

### 3.1 架构特点

- **控制流图（CFG）**：由 `Binder` 在解析阶段构建
- **FlowNode 节点模型**：FlowAssignment + FlowCondition + FlowBranchLabel
- **TypeNarrowingCallback 模式**：返回闭包而非直接修改上下文

### 3.2 支持的收窄模式

- **身份/相等性**：`x is None`、`x == 0`
- **类检查**：`isinstance(x, str)`、`issubclass(x, MyBase)`
- **真值性**：`if x:` → 排除 None/False/0/空序列
- **判别联合**：检查字面量字段缩小 TypedDict 或 Class 的联合类型
- **用户定义类型守卫**：`TypeGuard`（PEP 647）、`TypeIs`（PEP 742）
- **模式匹配（PEP 634）**：match 语句中的模式收窄

### 3.3 复杂度控制

- **收敛限制**：循环中最多尝试 256 次收敛
- **递归守卫**：`maxTypeRecursionCount` 防止深窄化分析的栈溢出
- **不完全类型**：`IncompleteType` 跟踪循环中的部分结果直到稳定

### 3.4 对 CCE 的借鉴

- **FlowNode 节点模型**：足够覆盖 CCE 的窄化需求
- **TypeNarrowingCallback 模式**：保持窄化逻辑纯净
- **收敛限制**：循环中的类型推断需要收敛保护
- **TypeScope 作用域管理**：独立作用域管理类型绑定

### 3.5 需要简化的

- **完整控制流图**：CCE 不需要完整类型检查，只需要 receiver 类型推断
- **IncompleteType 机制**：CCE 的推断是保守的，循环场景直接降级
- **TypeScope 完整嵌套**：CCE 的 `ScopedTypeContext` 已足够

## 4. TypeScript 类型推断

### 4.1 架构特点

- **控制流分析（CFA）**：过程内（intra-procedural）分析
- **checker.ts**：约 10 万行，核心逻辑所在
- **TypeFacts 位标志系统**：表示类型在特定程序点的已知事实

### 4.2 收窄规则清单

| 模式 | 窄化效果 |
|------|----------|
| `typeof x === "string"` | x: string |
| `x instanceof Class` | x: Class |
| `"prop" in x` | x: 有 prop 的类型 |
| `x === value` | x: typeof value |
| `x == null` | x: null \| undefined |
| `x` (truthiness) | 排除 falsy 类型 |
| `!x` | 取反 |

### 4.3 可辨识联合

TypeScript 通过字面量类型字段自动窄化：

```typescript
type Circle = { kind: "circle"; radius: number };
type Square = { kind: "square"; sideLength: number };
type Shape = Circle | Square;

if (shape.kind === "circle") {
    shape.radius; // OK, shape: Circle
}
```

### 4.4 泛型类型窄化

```typescript
function f<T extends string | undefined>(x: T) {
    if (x) {
        x; // T & {}（非空窄化）
        x.length; // OK
    }
}
```

### 4.5 const 变量内联（TypeScript 4.4+）

```typescript
const isString = typeof x === "string";
if (isString) {
    x; // string（通过内联 isString 的定义）
}
```

**限制**：
- 仅对 `const` 变量有效
- 最多支持 5 级间接引用
- 变量不能在函数体内被重新赋值

### 4.6 对 CCE 的借鉴

- **typeof 窄化**：可直接翻译为 Python 的 `isinstance(x, str)`
- **instanceof 窄化**：与 Python 的 `isinstance(x, Class)` 语义一致
- **可辨识联合**：CCE 已有 `EntityKind` 枚举，可用于类似的窄化
- **const 变量内联**：CCE 可支持 `const` 变量的条件推断

### 4.7 需要简化的

- **泛型窄化**：CCE 不需要处理泛型
- **完整 CFA**：CCE 只需要条件分支内的窄化，不需要完整的控制流合并
- **satisfies 表达式**：Python 无此语法
- **5 级内联**：CCE 可限制为 1-2 级

## 5. TypeScript Go 控制流分析

### 5.1 架构特点

- **FlowNode 图**：由 Binder 在绑定阶段构建的有向图
- **FlowFlags 类型**：10 种节点类型
- **FlowType 和 FlowState**：跟踪分析状态

### 5.2 FlowFlags 类型

| 标志 | 用途 |
|------|------|
| `FlowFlagsAssignment` | 变量赋值 |
| `FlowFlagsCall` | 函数调用（潜在类型守卫） |
| `FlowFlagsCondition` | 条件分支 |
| `FlowFlagsSwitchClause` | switch case |
| `FlowFlagsBranchLabel` | 控制流合并点 |
| `FlowFlagsLoopLabel` | 循环入口/合并 |
| `FlowFlagsArrayMutation` | 数组变更 |
| `FlowFlagsShared` | 多前驱节点 |

### 5.3 性能优化

- **Shared Flow 缓存**：防止分支结构的指数复杂度
- **FlowState 池化**：减少 GC 压力
- **深度限制**：递归深度上限 2000
- **不动点迭代**：循环标签使用不动点迭代

### 5.4 对 CCE 的借鉴

- **FlowNode 三节点模型**：足够覆盖 CCE 需求
- **TypeFacts 位标志**：简洁高效的事实表示系统
- **Shared Flow 缓存**：防止分支结构的指数复杂度
- **深度限制**：防止栈溢出

### 5.5 需要简化的

- **完整 CFA**：CCE 不需要完整的控制流分析
- **ArrayMutation**：CCE 不需要数组变更追踪
- **ReduceLabel**：CCE 不需要联合归约优化
- **常量变量内联**：CCE 不需要多级内联

## 6. Go types 类型推断

### 6.1 架构特点

- **多阶段检查**：声明和函数体分离检查，允许前向引用
- **三色算法**：white（未处理）→ grey（处理中）→ black（完成）
- **表达式类型检查模式**：通过 operand 返回结果

### 6.2 类型推断

- **泛型类型推断**：统一算法（Unification）
- **短变量声明推断**：从右侧表达式类型推断左侧变量类型
- **方法 Receiver 类型**：从 receiver 声明推断方法所属类型

### 6.3 对 CCE 的借鉴

- **多阶段检查**：声明和函数体分离检查，允许前向引用
- **统一算法**：类型参数匹配的统一算法
- **三色循环检测**：简洁的循环依赖检测
- **receiver 类型绑定**：从函数签名提取 receiver 类型

### 6.4 不需要的

- **包级初始化顺序**：CCE 不需要
- **常量折叠**：CCE 不需要
- **完整泛型推断**：CCE 只需要简单类型匹配

## 7. Eclipse JDT 类型推断

### 7.1 架构特点

- **推断变量（Inference Variable）**：未知类型的占位符
- **约束求解器（Constraint Solver）**：JLS Chapter 18 的完整类型推断规范
- **BoundSet**：存储等式约束、子类型约束、超类型约束

### 7.2 Lambda 表达式处理

- **目标类型推断**：Lambda 需要目标类型来确定其实现的函数式接口类型
- **重解析策略**：通过 `Parser` 重新解析 Lambda 区域

### 7.3 泛型与 Lambda 的集成

```java
interface Processor<T, R> {
    R process(T input);
}

Processor<String, Integer> p = s -> s.length();  // T=String, R=Integer
```

### 7.4 对 CCE 的借鉴

- **推断变量占位符模型**：CCE 的 `placeholder TypeEntry` 可借鉴此设计
- **两阶段推断**：阶段 1 收集约束，阶段 2 使用上下文细化
- **约束传播**：当一个变量类型确定后，传播到相关调用
- **Lambda 重解析策略**：对于不确定的类型推断，采用"多次尝试"策略

### 7.5 需要避免的

- **指数复杂度**：JDT 的约束传播在复杂泛型场景下性能很差
- **完整 JLS 规范**：CCE 不需要实现完整的 Java 类型推断
- **AST 重新解析**：JDT 为 Lambda 重新解析输入文本的方式较重

## 8. 关键设计模式总结

### 8.1 可直接采用的

1. **FlowNode 三节点模型**：FlowAssignment + FlowCondition + FlowBranchLabel
2. **TypeNarrowingCallback 模式**：返回闭包而非直接修改上下文
3. **收敛限制**：循环中的类型推断需要收敛保护
4. **TypeScope 作用域管理**：独立作用域管理类型绑定
5. **TypeFacts 位标志**：简洁高效的事实表示系统
6. **Shared Flow 缓存**：防止分支结构的指数复杂度
7. **深度限制**：防止栈溢出
8. **多阶段检查**：声明和函数体分离检查，允许前向引用
9. **三色循环检测**：简洁的循环依赖检测
10. **推断变量占位符模型**：用于跨文件 impl 块

### 8.2 需要简化的

1. **完整控制流图**：CCE 不需要完整类型检查，只需要 receiver 类型推断
2. **IncompleteType 机制**：CCE 的推断是保守的，循环场景直接降级
3. **TypeScope 完整嵌套**：CCE 的 `ScopedTypeContext` 已足够
4. **泛型窄化**：CCE 不需要处理泛型
5. **完整 CFA**：CCE 只需要条件分支内的窄化，不需要完整的控制流合并
6. **satisfies 表达式**：Python 无此语法
7. **5 级内联**：CCE 可限制为 1-2 级
8. **ArrayMutation**：CCE 不需要数组变更追踪
9. **ReduceLabel**：CCE 不需要联合归约优化
10. **完整泛型推断**：CCE 只需要简单类型匹配

### 8.3 需要避免的

1. **salsa 框架**：重量级增量计算框架
2. **chalk trait 求解**：CCE 不需要 trait bound 求解
3. **完整 HIR**：CCE 的 Entity 模型已足够
4. **完整控制流图**：CCE 不需要完整类型检查
5. **Lambda/闭包类型推断**：需要目标类型推断，循环依赖复杂
6. **完整 JLS 规范**：CCE 不需要实现完整的 Java 类型推断
7. **AST 重新解析**：JDT 为 Lambda 重新解析输入文本的方式较重
8. **指数复杂度**：JDT 的约束传播在复杂泛型场景下性能很差

## 9. Context7 查询补充信息

### 9.1 rust-analyzer 类型推断

**查询来源**：`/rust-lang/rust-analyzer`

**关键发现**：
- 类型推断按函数粒度进行（per-function granularity）
- 使用 `check_infer` 测试框架验证表达式类型
- 支持批量分析模式（`rust-analyzer analysis-stats`）

**实现模式**：
```rust
// 测试框架示例
check_infer(
    r#"#
fn main() {
    let x = 1;
}
    "#,
    expect![[r#"#
        10..28 '{     ...= 1; }': ()
        20..21 'x': i32
        24..25 '1': i32
    "#]],
);
```

### 9.2 Pyright 类型收窄

**查询来源**：`/microsoft/pyright`

**关键发现**：
- 使用联合类型（Union）保持类型信息
- 支持条件类型（Conditional Types）
- 使用 `reveal_type()` 调试类型推断

**实现模式**：
```python
# Pyright 使用联合操作保持类型信息
def func1(val: object):
    if isinstance(val, str):
        pass
    elif isinstance(val, int):
        pass
    else:
        return
    reveal_type(val) # mypy: object, pyright: str | int
```

### 9.3 TypeScript 类型收窄

**查询来源**：`/microsoft/typescript`

**关键发现**：
- 支持泛型类型参数收窄
- 支持可辨识联合（Discriminated Unions）
- 支持 `in` 操作符收窄

**实现模式**：
```typescript
// 泛型类型收窄
function f<T extends string | undefined>(x: T) {
    if (x) {
        x; // T & {}（非空窄化）
        x.length; // OK
    }
}

// 可辨识联合收窄
type Fish = { type: 'fish', hasFins: true }
type Dog = { type: 'dog', saysWoof: true }
type Pet = Fish | Dog;

function handleDog(pet: Pet) {
    if (pet.type === 'dog') {
        pet.saysWoof; // OK, pet: Dog
    }
}
```

### 9.4 TypeScript Go 控制流分析

**查询来源**：`/microsoft/typescript-go`

**关键发现**：
- 使用 FlowNode 图进行控制流分析
- 支持常量变量内联（最多 5 级）
- 使用 Shared Flow 缓存防止指数复杂度

**实现模式**：
```go
// narrowType 分发器
func (c *Checker) narrowType(f *FlowState, t *Type, expr *ast.Node, assumeTrue bool) *Type {
    switch expr.Kind {
    case ast.KindIdentifier:
        // 常量变量内联（最多 5 级）
        if !c.isMatchingReference(f.reference, expr) && c.inlineLevel < 5 {
            symbol := c.getResolvedSymbol(expr)
            if c.isConstantVariable(symbol) {
                declaration := symbol.ValueDeclaration
                if declaration != nil && ast.IsVariableDeclaration(declaration) && declaration.Type() == nil && declaration.Initializer() != nil && c.isConstantReference(f.reference) {
                    c.inlineLevel++
                    result := c.narrowType(f, t, declaration.Initializer(), assumeTrue)
                    c.inlineLevel--
                    return result
                }
            }
        }
        fallthrough
    case ast.KindBinaryExpression:
        return c.narrowTypeByBinaryExpression(f, t, expr.AsBinaryExpression(), assumeTrue)
    }
    return t
}
```

### 9.5 泛型类型推断

**查询来源**：`/microsoft/typescript`

**关键发现**：
- 泛型类型参数必须在成员中使用才能推断
- 支持约束类型推断
- 支持条件类型

**实现模式**：
```typescript
// 泛型类型推断需要类型参数在成员中使用
interface Named<T> {
    name: string;
    value: T; // <-- 必须有这个成员才能推断 T
}

function findByName<T>(x: Named<T>): T {
    return undefined;
}

var x: MyNamed<string>;
var y = findByName(x); // got y: string（如果 Named 没有 value 成员，会得到 y: {}）
```

## 10. 对 CCE 的具体建议

基于 Context7 查询结果，以下改进是可行的：

### 10.1 可直接采用的

1. **TypeScript 的 `in` 操作符收窄**
   - 支持 `"prop" in x` 模式
   - 实现复杂度：低

2. **TypeScript 的可辨识联合收窄**
   - 支持 `x.kind === "circle"` 模式
   - 实现复杂度：低

3. **Pyright 的联合类型保持**
   - 在收窄时保持联合类型信息
   - 实现复杂度：中等

4. **TypeScript Go 的 Shared Flow 缓存**
   - 防止分支结构的指数复杂度
   - 实现复杂度：中等

### 10.2 需要简化的

1. **常量变量内联**
   - TypeScript Go 支持 5 级内联
   - CCE 可限制为 1-2 级

2. **泛型类型收窄**
   - TypeScript 支持泛型约束收窄
   - CCE 可简化为简单类型匹配

3. **完整控制流图**
   - TypeScript Go 构建完整 FlowNode 图
   - CCE 只需要条件分支内的收窄

### 10.3 需要避免的

1. **Lambda/闭包类型推断**
   - 需要目标类型推断，循环依赖复杂

2. **完整泛型约束求解**
   - 需要约束传播，复杂度高

3. **AST 重新解析**
   - JDT 为 Lambda 重新解析输入文本的方式较重
