# rust-analyzer 类型推断架构研究

## 概述

rust-analyzer 是 Rust 语言的模块化编译器前端，提供 IDE 支持。其类型推断系统基于 salsa 增量计算框架和 chalk trait 求解器，采用 ECS（Entity-Component-System）风格设计。

**仓库**：`https://github.com/rust-lang/rust-analyzer`
**关键 crate**：
- `crates/hir-ty` — 类型推断核心
- `crates/hir-def` — 名称解析和定义
- `crates/hir-expand` — 宏展开
- `crates/base-db` — salsa 基础设施

## 核心架构

### 三层 hir crate

| crate | 职责 | 关键数据结构 |
|-------|------|-------------|
| `hir-expand` | 宏展开 | `MacroCallLoc`, `ExpandedHygiene` |
| `hir-def` | 名称解析、模块树 | `DefMap`, `ItemTree`, `Body` |
| `hir-ty` | 类型推断、trait 求解 | `InferenceResult`, `InferTy` |

### ECS 风格

- 所有实体使用原始 ID（`StructId`, `FunctionId`, `VariantId`）
- 直接查询数据库，无抽象层
- 与 salsa 深度集成

### salsa 增量计算

**核心不变式**：函数体内的修改不会使全局派生数据失效。

```
修改 foo() 的函数体
  → 仅 foo() 的 InferenceResult 重新计算
  → bar() 的所有事实保持不变
```

**salsa 工作原理**：
1. 定义输入查询（`SourceDatabase`）
2. 定义派生查询（`fn infer(db, function_id) -> InferenceResult`）
3. salsa 自动追踪查询依赖
4. 输入变更时，仅重新计算受影响的派生查询

### 类型推断流程

1. **AST → HIR 降低**：函数体 AST 降低为位置无关的中间表示（每个表达式分配唯一 ID）
2. **类型推断**：`infer_query` 分析 Body，产生 `InferenceResult`
3. **trait 求解**：通过 chalk 查询 trait 实现
4. **结果缓存**：salsa 自动缓存推断结果

### chalk 集成

chalk 将 Rust trait 系统编码为逻辑谓词（类似 Prolog），通过逻辑求解器回答 trait 实现问题：

```
查询: Vec<u8> 是否实现 Debug?
降低: Vec<u8>: Debug :- Debug<Vec<T>> where T: Debug, u8: Debug.
求解: 递归展开，找到 impl Debug for Vec<T> where T: Debug
```

**chalk 已被 sunset**，Rust 编译器已迁移到下一代 trait 求解器。

### 分两遍收集-解析

rust-analyzer 使用分两遍的策略：

**第一遍：收集声明**
- 遍历所有函数、结构体、枚举的声明
- 收集类型签名、字段类型、方法签名
- 写入 `DefMap`

**第二遍：解析引用**
- 对每个表达式，查找其引用的定义
- 对每个类型表达式，解析为具体类型
- 使用 `DefMap` 查询

这种分离允许前向引用和递归类型。

## 对 CCE 的借鉴价值

### 可采用的设计模式

1. **分两遍收集-解析**：
   - 第一遍：收集所有声明（类型、函数签名）
   - 第二遍：解析引用（类型引用、函数调用）
   - CCE 的 `TypeInferenceEngine` 可借鉴此模式

2. **保守降级策略**：
   - 推断失败时回退到 `unknown` 类型，不报错
   - CCE 已采用此策略（`TypeConfidence::Low`）

3. **per-function 粒度推断**：
   - rust-analyzer 按函数粒度推断，CCE 可按文件粒度

4. **增量计算思想**：
   - 虽然 CCE 不需要完整的 salsa 框架，但可以借鉴"仅重新计算受影响部分"的思想

### 不适合直接采用的

1. **salsa 框架**：rust-analyzer 的 salsa 是重量级增量计算框架，CCE 的 `TypeInferenceContext` 无需如此复杂的增量机制
2. **chalk trait 求解**：CCE 不需要 trait bound 求解
3. **完整 HIR**：CCE 的 Entity 模型已经足够

### 实现建议

CCE 可采用的简化模型：
```
Phase 1: 收集声明
  - 遍历所有 entity，收集类型标注信息
  - 写入 ScopedTypeContext

Phase 2: 解析引用
  - 对每个调用表达式，查找 callee 的返回类型
  - 对每个 receiver，查找推断的类型
  - 使用 ScopedTypeContext 查询
```
