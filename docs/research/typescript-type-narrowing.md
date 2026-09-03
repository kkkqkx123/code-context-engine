# TypeScript 编译器类型窄化研究

## 概述

TypeScript 编译器（tsc）的类型窄化系统实现在 `checker.ts` 中，是控制流分析（CFA）的经典实现。窄化发生在类型检查阶段，基于 AST 遍历。

**仓库**：`https://github.com/microsoft/TypeScript`
**关键文件**：`src/compiler/checker.ts`（约 10 万行，核心逻辑所在）

## 窄化规则完整清单

### 内置类型守卫

| 模式 | 窄化效果 | 实现位置 |
|------|----------|----------|
| `typeof x === "string"` | x: string | `narrowTypeBytypeof` |
| `typeof x !== "string"` | 排除 string | `narrowTypeBytypeof` |
| `x instanceof Class` | x: Class | `narrowTypeByInstanceof` |
| `"prop" in x` | x: 有 prop 的类型 | `narrowTypeByIn` |
| `x === value` | x: typeof value | `narrowTypeByEquality` |
| `x !== value` | 排除 typeof value | `narrowTypeByEquality` |
| `x == null` | x: null \| undefined | `narrowTypeByTruthiness` |
| `x` (truthiness) | 排除 falsy 类型 | `narrowTypeByTruthiness` |
| `!x` | 取反 | `narrowTypeByTruthiness` |
| `x instanceof RegExp` | x: RegExp | 同 instanceof |

### 用户定义类型守卫

```typescript
// 类型谓词
function isString(x: unknown): x is string {
    return typeof x === "string";
}

// 可辨识联合
type Shape = Circle | Square;
function getArea(shape: Shape) {
    if (shape.kind === "circle") {
        // shape: Circle（因为 Circle.kind === "circle"）
    }
}
```

### 可辨识联合（Discriminated Unions）

TypeScript 通过字面量类型字段自动窄化：
```typescript
type Circle = { kind: "circle"; radius: number };
type Square = { kind: "square"; sideLength: number };
type Shape = Circle | Square;

if (shape.kind === "circle") {
    shape.radius; // OK, shape: Circle
}
```

### 控制流分析（CFA）

TypeScript 的 CFA 是**过程内**（intra-procedural）分析：
- 跟踪每个变量在程序每个位置的类型
- 分支处分裂，合并处合并
- 赋值时窄化

```typescript
function example() {
    let x: string | number | boolean;
    x = Math.random() < 0.5 ? 10 : "hello";
    x; // string | number
    if (typeof x === "string") {
        x; // string
    } else {
        x; // number
    }
    x; // string | number（合并后）
}
```

### checker.ts 中的关键方法

| 方法 | 职责 |
|------|------|
| `narrowType` | 主入口，根据表达式类型分发到具体窄化方法 |
| `isMatchingReference` | 判断两个引用是否指向同一变量 |
| `narrowTypeBytypeof` | typeof 窄化 |
| `narrowTypeByInstanceof` | instanceof 窄化 |
| `narrowTypeByIn` | in 操作符窄化 |
| `narrowTypeByEquality` | 相等性窄化 |
| `narrowTypeByTruthiness` | 真值性窄化 |
| `narrowTypeByDiscriminant` | 可辨识联合窄化 |
| `getContextualType` | 从上下文推断表达式类型 |

### 泛型类型窄化

TypeScript 支持泛型类型的窄化：
```typescript
function f<T extends string | undefined>(x: T) {
    if (x) {
        x; // T & {}（非空窄化）
        x.length; // OK
    }
}
```

实现方式：窄化时将泛型约束与窄化结果取交集。

### const 变量内联（TypeScript 4.4+）

TypeScript 4.4 引入了 const 变量的控制流分析内联：
```typescript
const isString = typeof x === "string";
if (isString) {
    x; // string（通过内联 isString 的定义）
}
```

**限制**：
- 仅对 `const` 变量有效，`let` 变量不支持
- 最多支持 5 级间接引用
- 变量不能在函数体内被重新赋值

## 对 CCE 的借鉴价值

### 可直接采用的规则

1. **typeof 窄化**：TypeScript 的 `typeof x === "string"` 规则可直接翻译为 Python 的 `isinstance(x, str)`
2. **instanceof 窄化**：TypeScript 的 `x instanceof Class` 与 Python 的 `isinstance(x, Class)` 语义一致
3. **可辨识联合**：CCE 已有 `EntityKind` 枚举，可用于类似的窄化
4. **const 变量内联**：CCE 可支持 `const` 变量的条件推断

### 需要简化的

1. **泛型窄化**：CCE 不需要处理泛型
2. **完整 CFA**：CCE 只需要条件分支内的窄化，不需要完整的控制流合并
3. **satisfies 表达式**：Python 无此语法
4. **5 级内联**：CCE 可限制为 1-2 级

### 实现建议

在 `ControlFlowNarrower` 中实现的规则优先级：
1. `isinstance(x, Type)` — Python 最常用
2. `typeof x === "string"` — TypeScript 常用
3. `x instanceof Class` — TypeScript 常用
4. `x is None` / `x is not None` — Python 常用
5. 可辨识联合 — 已在 TypeMemberIndex 中隐式支持
