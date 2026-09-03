# Go types 包类型推断研究

## 概述

Go 标准库 `go/types` 是 Go 语言的类型检查器，由 Robert Griesemer 设计，Go 1.5 进入标准库。其类型推断系统处理泛型函数的类型参数推断、短变量声明推断、以及方法 receiver 类型关联。

**仓库**：`https://github.com/golang/go/tree/master/src/go/types`
**编译器内部版本**：`cmd/compile/internal/types2`（使用 `syntax` 包而非 `go/ast`）

## 类型检查流程

Go 类型检查是**多阶段**过程，非单次遍历：

| 阶段 | 方法 | 职责 |
|------|------|------|
| 1 | `initFiles` | 验证包结构，提取版本信息 |
| 2 | `collectObjects` | 扫描所有声明，创建占位符 |
| 3 | `packageObjects` | 类型检查声明（跳过函数体） |
| 4 | `processDelayed` | 检查函数体 |
| 5 | `initOrder` | 计算包级变量初始化顺序 |

**为什么多阶段**：Go 允许前向引用和递归类型——可以在声明之前引用。

### 三色算法

类型检查使用三色标记法检测依赖循环：

```
white（未处理）→ grey（处理中）→ black（完成）
```

如果处理 grey 对象时遇到已标记为 grey 的对象，说明存在循环依赖。

### 表达式类型检查模式

标准方法签名：
```go
func (check *Checker) f(x *operand, e syntax.Expr, /* additional args */) {
    // 类型检查逻辑
    // 结果通过 operand x 返回
    // 如果出错，x.mode == invalid
}
```

## 类型推断

### 泛型类型推断

Go 1.18+ 支持泛型，类型推断处理泛型函数调用：

```go
func Min[T constraints.Ordered](a, b T) T {
    if a < b { return a }
    return b
}

result := Min(3, 5)  // T = int（从参数推断）
```

推断过程：
1. 解析类型参数声明
2. 从调用参数推断类型参数
3. **实例化**：用具体类型替换类型参数
4. 检查类型参数满足约束

### 推断算法（infer.go）

**统一算法（Unification）**：
- 递归比较 LHS 和 RHS 类型
- 维护 `typeParam → typeArg` 映射
- 已知类型参数替换为具体类型后继续比较
- 无失败步骤且所有类型参数都有映射时，推断成功

**约束类型推断**：
- 类型参数与其约束的核心类型统一
- 处理 tilde（`~int`）与非 tilde（`int`）的区别
- O(n²) 算法，n 为类型参数数量（实际很小，通常 < 5）

**统一规则**：
- 精确匹配模式（exact）：递归比较复合类型的元素
- 宽松匹配模式（loose）：顶层赋值兼容性宽松，元素类型精确
- 元素匹配模式通常与父级相同，但赋值兼容性顶层宽松

### 短变量声明推断

```go
x := 42        // x: int
y := "hello"   // y: string
z := 3.14      // z: float64
```

从右侧表达式类型推断左侧变量类型。

### 方法 Receiver 类型

Go 的方法通过 receiver 参数关联到类型：

```go
type MyStruct struct { Value int }

func (s MyStruct) GetValue() int {
    return s.Value
}
```

类型检查器从 receiver 声明推断方法所属类型。

## types2 与 go/types 的关系

两个几乎相同的类型检查器：
- `cmd/compile/internal/types2`：编译器内部使用，利用 `syntax` 包的 AST
- `go/types`：标准库，必须保持严格向后兼容

任何修改必须同时应用到两个代码库以保持同步。

## 对 CCE 的借鉴价值

### 可采用的设计

1. **多阶段检查**：声明和函数体分离检查，允许前向引用
   - CCE 可借鉴：先收集所有函数签名，再分析函数体

2. **统一算法**：类型参数匹配的统一算法
   - CCE 可借鉴：用于处理 `T extends Base` 泛型约束

3. **三色循环检测**：简洁的循环依赖检测
   - CCE 可借鉴：用于 `FileDependencyGraph` 的循环检测

4. **receiver 类型绑定**：从函数签名提取 receiver 类型
   - CCE 已在 `policy/type_member/go.rs` 中实现

### 不需要的

1. **包级初始化顺序**：CCE 不需要
2. **常量折叠**：CCE 不需要
3. **完整泛型推断**：CCE 只需要简单类型匹配

### 实现建议

CCE 的 Go 推断器重点：
1. 从 `func (r Receiver) Method()` 提取 receiver 类型
2. 从短变量声明 `x := expr` 推断 `x` 的类型
3. 从 `var x Type = expr` 推断 `x` 的类型
4. 从函数签名的类型注解提取参数和返回类型
5. 统一算法可参考 `infer.go` 的实现
