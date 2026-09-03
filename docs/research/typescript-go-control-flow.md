# TypeScript Go 控制流分析架构研究

## 概述

TypeScript Go（`typescript-go`）是 Microsoft 将 TypeScript 编译器从 JavaScript 重写为 Go 的项目（代号 Corsa），随 TypeScript 7.0 于 2026 年 7 月发布。控制流分析（CFA）系统是类型检查的核心组件，负责跟踪类型在条件分支、赋值、循环等控制流结构中的演化和窄化。

**仓库**：`https://github.com/microsoft/typescript-go`
**关键文件**：
- `internal/checker/flow.go` — CFA 核心实现（`getFlowTypeOfReferenceEx`、`narrowType`、`getTypeAtFlowNode`）
- `internal/binder/binder.go` — FlowNode 图构建
- `internal/ast/ast.go` — FlowNode 和 FlowFlags 定义

## 核心架构

### FlowNode 图

由 Binder 在绑定阶段构建的有向图。每个节点代表程序控制流中的一个点，边（antecedents）指向前面的 FlowNode。

**FlowNode 结构**：
- `Flags`（FlowFlags）：标识节点类型
- `Antecedent`（*FlowNode）：单前驱节点
- `Antecedents`（*FlowList）：多前驱节点（分支/循环标签）
- `Node`（*Node）：关联的 AST 节点

### FlowFlags 类型

| 标志 | 值 | 用途 | 创建时机 |
|------|-----|------|----------|
| `FlowFlagsUnreachable` | 1<<0 | 不可达代码 | return/throw/break/continue 后 |
| `FlowFlagsStart` | 1<<1 | 函数/模块入口 | 函数体开始 |
| `FlowFlagsAssignment` | 1<<3 | 变量赋值 | `x = value`、参数初始化 |
| `FlowFlagsCall` | 1<<4 | 函数调用（潜在类型守卫） | 带类型谓词的函数调用 |
| `FlowFlagsCondition` | 1<<5 | 条件分支 | if、三元 ? :、&&、|| |
| `FlowFlagsSwitchClause` | 1<<6 | switch case | switch 语句的 case 子句 |
| `FlowFlagsBranchLabel` | 1<<7 | 控制流合并点 | if-else 结束处、循环退出 |
| `FlowFlagsLoopLabel` | 1<<8 | 循环入口/合并 | for、while、do-while 开始 |
| `FlowFlagsArrayMutation` | 1<<9 | 数组变更 | push()、unshift()、索引赋值 |
| `FlowFlagsReduceLabel` | 1<<11 | 联合归约优化 | 编译器对大联合的优化 |
| `FlowFlagsShared` | 1<<12 | 多前驱节点 | Binder 标记被多次引用的节点 |

### FlowType 和 FlowState

**FlowType**：表示特定 FlowNode 处的类型，可能不完整（如循环不动点迭代期间）。

**FlowState**：跟踪分析过程中反向遍历的状态：
- `reference`：正在分析的引用
- `declaredType`：声明类型
- `initialType`：初始类型
- `flowContainer`：流容器
- `depth`：递归深度（上限 2000）

## 流分析算法

### 主入口：getFlowTypeOfReferenceEx

```go
func (c *Checker) getFlowTypeOfReferenceEx(
    reference *ast.Node, declaredType *Type, 
    initialType *Type, flowContainer *ast.Node, 
    flowNode *ast.FlowNode,
) *Type
```

**算法步骤**：
1. **深度检查**：递归深度上限 2000，超过则禁用流分析并报错
2. **缓存查找**：检查 `sharedFlows` 中共享节点的已计算结果
3. **反向遍历**：根据 FlowFlags 分发处理
4. **最终化**：将 evolving array 类型转换为最终形式

### getTypeAtFlowNode — 核心递归遍历器

处理所有 10 种 FlowNode 类型：

| 节点类型 | 处理方法 | 说明 |
|----------|----------|------|
| Assignment | `getTypeAtFlowAssignment` | 如果引用是赋值目标，返回赋值类型 |
| Call | `getTypeAtFlowCall` | 处理类型守卫函数调用 |
| Condition | `getTypeAtFlowCondition` | 基于条件真值性/类型守卫窄化 |
| SwitchClause | `getTypeAtSwitchClause` | switch case 窄化 |
| BranchLabel | `getTypeAtFlowBranchLabel` | 合并多条路径的类型（如 if/else 后） |
| LoopLabel | `getTypeAtFlowLoopLabel` | 迭代解析直到达到不动点 |
| ArrayMutation | `getTypeAtFlowArrayMutation` | 数组变更后的类型更新 |
| ReduceLabel | 递归处理 | 联合归约优化 |
| Start | 返回 initialType | 函数/模块入口 |

### narrowType — 按表达式类型分发

```go
func (c *Checker) narrowType(f *FlowState, t *Type, expr *ast.Node, assumeTrue bool) *Type
```

**处理的表达式类型**：
- **Identifier**：常量变量内联（最多 5 级）
- **This/Super/PropertyAccess/ElementAccess**：真值性窄化
- **CallExpression**：类型谓词、断言函数
- **Parenthesized/NonNull/Satisfies**：递归处理内部表达式
- **BinaryExpression**：typeof、instanceof、equality、in、&&、||
- **PrefixUnaryExpression**：`!` 取反

**常量变量内联**：TypeScript Go 支持最多 5 级的 const 变量内联：
```typescript
const isString = typeof x === "string";
if (isString) { x; } // x: string（通过内联 isString 的定义）
```

### TypeFacts 系统

位标志系统，表示类型在特定程序点的已知事实：

| 类别 | 标志示例 | 表达式示例 |
|------|----------|------------|
| typeof 相等 | `TypeFactsTypeofEQString` | `typeof x === "string"` |
| null/undefined | `TypeFactsEQNull`, `TypeFactsNEUndefined` | `x === null`, `x !== undefined` |
| 真值性 | `TypeFactsTruthy`, `TypeFactsFalsy` | `if (x)`, `if (!x)` |

`getTypeWithFacts` 函数根据事实过滤类型（如联合类型）。

## 性能优化

### Shared Flow 缓存

共享节点（`FlowFlagsShared`）的计算结果缓存在 `sharedFlows` 切片中，防止大型分支结构的指数复杂度。

### FlowState 池化

`FlowState` 对象通过空闲链表（`freeFlowState`）管理，减少 GC 压力。

### 深度限制

递归深度上限 2000，超过则禁用流分析并报告流控制错误。

### 不动点迭代

循环标签（LoopLabel）使用不动点迭代，类型不再变化时停止。

## 与 TypeScript 原版的差异

TypeScript Go 的 CFA 与原版 TypeScript 保持语义一致，但有以下实现差异：

1. **UTF-8 偏移量**：节点位置使用 UTF-8 偏移量而非 UTF-16
2. **并行类型检查**：通过 `CheckerPool` 支持多文件并行检查
3. **原生代码**：Go 编译为原生代码，无 V8 开销
4. **内存管理**：Go GC 处理短生命周期对象，无需手动管理

## 对 CCE 的借鉴价值

### 可直接采用的设计

1. **FlowNode 三节点模型**：`FlowAssignment` + `FlowCondition` + `FlowBranchLabel` 足够覆盖 CCE 需求
2. **TypeFacts 位标志**：简洁高效的事实表示系统
3. **Shared Flow 缓存**：防止分支结构的指数复杂度
4. **深度限制**：防止栈溢出

### 需要简化的

1. **完整 CFA**：CCE 不需要完整的控制流分析，只需条件分支内的窄化
2. **ArrayMutation**：CCE 不需要数组变更追踪
3. **ReduceLabel**：CCE 不需要联合归约优化
4. **常量变量内联**：CCE 不需要多级内联

### 实现建议

在 CCE 的 `ControlFlowNarrower` 中实现的核心算法：
```
narrow_type(test_expr, base_type, assume_true) -> Type
```

处理优先级：
1. `isinstance(x, Type)` — Python/TypeScript 常用
2. `typeof x === "string"` — TypeScript 常用
3. `x instanceof Class` — TypeScript 常用
4. `x is None` / `x is not None` — Python 常用
5. `x == null` — TypeScript 常用
