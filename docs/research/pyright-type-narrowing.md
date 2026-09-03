# Pyright 类型窄化与控制流分析研究

## 概述

Pyright 是 Microsoft 开发的高性能 Python 静态类型检查器，TypeScript 编写，运行于 Node.js。其类型窄化系统基于控制流图（Control Flow Graph），是目前 Python 类型检查器中窄化能力最强的实现。

**仓库**：`https://github.com/microsoft/pyright`
**关键文件**：
- `packages/pyright-internal/src/analyzer/typeGuards.ts` — 窄化核心逻辑
- `packages/pyright-internal/src/analyzer/codeFlowEngine.ts` — 控制流引擎
- `packages/pyright-internal/src/analyzer/codeFlowTypes.ts` — 流节点类型定义
- `packages/pyright-internal/src/analyzer/binder.ts` — 控制流图构建
- `packages/pyright-internal/src/analyzer/patternMatching.ts` — match 语句窄化

## 核心架构

### 控制流图（Code Flow Graph）

由 `Binder` 在解析阶段构建，由 `FlowNode` 对象组成。节点类型：

| 节点类型 | 代码符号 | 说明 |
|----------|----------|------|
| 赋值 | `FlowAssignment` | 变量赋值时创建 |
| 条件 | `FlowCondition` | if/while 测试条件时创建 |
| 分支合并 | `FlowBranchLabel` | 合并多条流路径（如 if/else 结束处） |
| 调用 | `FlowCall` | 表示可能不返回的函数调用 |
| 模式匹配 | `FlowNarrowForPattern` | PEP 634 match 语句 |

### 类型窄化入口

核心入口函数 `getTypeNarrowingCallback`：
1. 接收测试表达式（test expression）
2. 返回 `TypeNarrowingCallback`（闭包）
3. 回接收基础类型，返回 `TypeNarrowingResult`（narrowed type + 判断是否总是 true）

### 支持的窄化模式

**身份/相等性**：
- `x is None` / `x is not None`
- `x == 0` / `x != 0`
- `x is MySentinel`

**类检查**：
- `isinstance(x, str)` / `isinstance(x, (int, float))`
- `issubclass(x, MyBase)`

**真值性**：
- `if x:` → 排除 None/False/0/空序列
- `if not x:` → 取反

**判别联合（Discriminated Unions）**：
- 检查字面量字段（如 `x.type == "a"`）缩小 TypedDict 或 Class 的联合类型

**用户定义类型守卫**：
- `TypeGuard`（PEP 647）：`def is_str(val: object) -> TypeGuard[str]`
- `TypeIs`（PEP 742）：`def is_str(val: object) -> TypeIs[str]`（更精确，支持 else 分支）

**模式匹配（PEP 634）**：
- `match` 语句中的 `PatternSequence`、`PatternClass`、`PatternLiteral` 等
- 由 `patternMatching.ts` 的 `narrowTypeBasedOnPattern` 递归处理

### 复杂度控制

- **收敛限制**：循环中最多尝试 256 次收敛，超过则"钉住"类型
- **递归守卫**：`maxTypeRecursionCount` 防止深窄化分析的栈溢出
- **不完全类型**：`IncompleteType` 跟踪循环中的部分结果直到稳定

## TypeScope 模型

Pyright 使用 `TypeScope` 管理类型推断的上下文：

- 每个函数/模块/类创建独立的 TypeScope
- TypeScope 存储当前作用域内的类型绑定
- 支持嵌套作用域查找
- 窄化操作在 TypeScope 上执行

## 对 CCE 的借鉴价值

### 可直接采用的设计

1. **FlowNode 节点模型**：`FlowAssignment` + `FlowCondition` + `FlowBranchLabel` 的三节点模型足够覆盖 CCE 的窄化需求
2. **TypeNarrowingCallback 模式**：返回闭包而非直接修改上下文，保持窄化逻辑纯净
3. **收敛限制**：循环中的类型推断需要收敛保护
4. **TypeScope 作用域管理**：独立作用域管理类型绑定

### 需要简化的设计

1. Pyright 的完整控制流图对 CCE 过重——CCE 不需要完整类型检查，只需要 receiver 类型推断
2. 可以简化为：仅在 `if isinstance(x, Type)` 和 `if let Some(val) = x` 两种模式上做窄化
3. 不需要 `IncompleteType` 机制——CCE 的推断是保守的，循环场景直接降级
4. 不需要 TypeScope 的完整嵌套——CCE 的 `ScopedTypeContext` 已足够

### 实现建议

在 `ControlFlowNarrower` 中实现：
```
narrow_type(test_expr, base_type) -> Option<TypeBinding>
```
仅处理：
- `isinstance(x, Type)` → True 分支: x: Type; False 分支: x: base \ Type
- `if let Some(val) = opt` → True 分支: val: T
- `typeof x === "string"` → True 分支: x: string
- `x instanceof Class` → True 分支: x: Class
