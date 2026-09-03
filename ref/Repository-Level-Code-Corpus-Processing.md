### 3.3 Repository-Level Code Corpus Processing Framework

In this section, we describe in detail the corpus preprocessing pipeline, AST-based semantic segmentation, and the construction process of the code knowledge graph. Corpus preprocessing aims to improve the quality of training corpora by reducing noise and irrelevant information; AST-based semantic segmentation ensures the integrity of semantic units and prevents semantic disruption; while the code knowledge graph focuses on further constructing and enhancing the relationships between semantic units, thereby improving the model’s global and cross-library semantic understanding capabilities.

To ensure that the generated knowledge graph corpora are of high quality, we have established a systematic preprocessing workflow, including key steps such as data filtering, data cleaning, and deduplication. Specific filtering rules, cleaning methods, and deduplication processes are detailed in Appx. A.

#### 3.3.1 Syntax-Aware Semantic Unit Extraction via AST

To enhance the structural awareness of pretraining corpora, we propose an AST-based semantic segmentation method as an alternative to traditional random or sliding-window masking strategies based on tokens. This method uses semantically closed subtrees in the AST as segmentation units, ensuring the structural integrity and contextual continuity of the masked units. Specifically, the method includes the following four steps:

1. Tools like Tree-sitter are used to parse the source code and extract semantically complete AST subtrees, such as function bodies and conditional branches.
2. A subtree is randomly sampled as the masking target, replaced with a placeholder, and concatenated with its preceding and succeeding contexts to form the training input.
3. Structural integrity checks are performed to ensure that the masking operation does not disrupt the syntactic parsability of the remaining code.
4. A granularity control parameter θ is introduced to adjust the size of the subtrees, supporting multi-scale structural modeling.

This method can be completed in linear complexity and is suitable for large-scale code corpus construction. For the formal modeling, algorithm, and comparative analysis with greedy segmentation, please refer to Appx. B.

#### 3.3.2 Structure-Preserving and Semantically-Reordered Code Graph

After AST-based semantic unit extraction, segmentation, and completeness verification, we build a **Structure-Preserving and Semantically-Reordered Code Graph (SPSR-Graph)** to generate training corpora that maintain global call consistency. The construction proceeds in two stages:

1. **Semantic-unit extraction**: Parse the vertical-domain codebase to obtain self-contained units such as functions, structs, and classes.
2. **Semantic-relationship graphing**: Connect these units with directed edges that encode calls, references, and inclusions.

Traversing this graph along call paths allows us to reorder source code into contextually aligned sequences, enriching the structural depth of the corpus and enabling repository-level, cross-library context modeling.

To further extend the structural depth of training corpora and enhance cross-library context modeling, we propose organizing semantic units into structured graphs, constructing a semantic dependency graph named **SPSR-Graph**. The graph construction process is divided into two stages:

**Stage 1: Element Extraction**  
We use an AST parser to parse the entire codebase and extract all top-level semantic units \( \nu_i \in \boldsymbol{\nu} \), such as function bodies, structs, and class definitions. Each \( \nu_i \) is semantically complete and stored in a structured database for subsequent calls.

**Stage 2: Relationship Extraction and Graph Construction**  
We represent the code graph as \( \Gamma = (\mathcal{V}, \epsilon) \), where:
- \( \mathcal{V} \) is the set of nodes, i.e., the extracted semantic units;
- \( \epsilon \subseteq \mathcal{V} \times \mathcal{V} \) is the set of directed edges.

If there is a call relationship \( \nu_i \rightarrow \nu_j \), we define the edge \( (\nu_i, \nu_j) \in \epsilon \). Edge types can include, but are not limited to:
- Direct Call
- Member Reference
- Type Usage
- Macro Expansion
- Include Dependency

To preserve contextual integrity, graph construction supports node attribute enhancement (e.g., definition location, module affiliation, syntax type labels) and edge type annotations, further enhancing the graph’s semantic capacity.

On the directed graph \( \Gamma \), we use directed BFS to search for all paths \( \mathcal{P} \) with depth \( d \leq D \):

\[
\mathcal{P} = \{ p_k = (\nu_{k1}, \nu_{k2}, \dots, \nu_{km}) \mid \nu_{ki} \in \mathcal{V}, m \leq D \}
\]

Path selection supports multiple strategies: Forward Call Expansion, Field Access Expansion, and Header Inclusion Prioritization. Each path \( p_k \) is mapped to the following training sample:

\[
\text{Sample}(p_k) = \nu_{k1} \oplus \nu_{k2} \oplus \dots \oplus \nu_{km}
\]

where \( \oplus \) denotes structure-aware concatenation.

To enhance the model’s cross-file structural modeling capability, we insert file path information and structural comments during concatenation:

\[
\nu_{ki} \mapsto \text{/* file: path/to/file */} \oplus \text{code}
\]

The following algorithm explicitly shows the training sample construction process for SPSR-Graph:

1. Construct the graph structure using extracted AST semantic units (nodes = code units, edges = call/reference relationships).
2. Use breadth-first traversal (BFS) to enumerate all semantic paths with depth not exceeding \( D \).
3. For each valid path, sequentially load source code fragments corresponding to each node and embed structural annotation information at cross-file boundaries.
4. Concatenate structured fragments in dependency order to form a training sample with global semantic consistency, stored for subsequent language model pretraining.

Let the total number of nodes be \( n \), the average outdegree be \( d \), and the maximum path depth be \( D \). Then the complexity is:

\[
\mathcal{O}(n + n d + n \cdot d^D \cdot m)
\]

This process preserves both syntactic structural integrity and contextual consistency while achieving corpus reordering along call paths, enabling the model to explicitly encounter and model cross-function and cross-module structural dependencies during training.

---

**Algorithm 1: SPSR-Graph Generator**

**Input:** AST-unit DB `ASTDB`; max depth \( D \)  
**Output:** structure-aware sample set \( \mathcal{S} \)

```
1  Γ ← InitGraph()
   for each u ∈ ASTDB:
       Γ.addNode(u)
   for each (u, v) ∈ CallPairs(ASTDB):
       Γ.addEdge(u → v)
   S ← ∅

2  for each p ∈ BFSPaths(Γ, D):
       s ← Concat( Tag(u) | u ∈ p )

3  S ← S ∪ { s }

4  return S

5  Function BFSPaths(Γ, D):
       return all directed paths of Γ with length ≤ D (BFS)

6  Function Tag(u):
       if CrossFile(u) then
           return /* file: u.file */ ∥ code(u)
       else
           return code(u)
```

--- 

Let me know if you need a LaTeX version or further formatting adjustments.

======

根据您提供的文本，以下是对 **SPSR-Graph（结构保持与语义重排序代码图）算法** 的具体总结与介绍：

---

## 一、算法目标

SPSR-Graph 算法旨在构建高质量的训练语料，用于提升代码预训练模型对**全局调用一致性**和**跨文件/跨库语义依赖**的理解能力。其核心思想是将代码库中的语义单元（函数、类、结构体等）组织成有向图，并按调用路径重排代码顺序，生成结构感知的训练样本。

---

## 二、算法流程（两个阶段）

### 阶段1：语义单元提取
- 使用 AST 解析器（如 Tree-sitter）解析整个代码库。
- 提取所有**顶层语义单元**（如函数体、类定义、结构体），每个单元语义完整。
- 将这些单元存储在结构化数据库中。

### 阶段2：关系抽取与图构建
- 构建有向图 \( \Gamma = (\mathcal{V}, \epsilon) \)：
  - **节点 \( \mathcal{V} \)**：提取出的语义单元。
  - **边 \( \epsilon \)**：表示单元之间的调用、引用或包含关系（如函数调用、成员引用、类型依赖、宏展开、文件包含等）。
- 支持节点属性增强（位置、模块、语法类型）和边类型标注。

---

## 三、训练样本生成

1. **路径搜索**  
   在图上使用**有向广度优先搜索（BFS）**，找出所有深度不超过 \( D \) 的路径 \( \mathcal{P} \)：
   \[
   \mathcal{P} = \{ p_k = (\nu_{k1}, \nu_{k2}, \dots, \nu_{km}) \mid m \leq D \}
   \]

2. **路径选择策略**（可配置）：
   - 前向调用扩展
   - 字段访问链扩展
   - 头文件包含优先

3. **样本构造**  
   每条路径 \( p_k \) 映射为一个训练样本：
   \[
   \text{Sample}(p_k) = \nu_{k1} \oplus \nu_{k2} \oplus \dots \oplus \nu_{km}
   \]
   其中 \( \oplus \) 表示**结构感知的拼接**。

4. **跨文件标注**  
   为增强跨文件建模能力，在拼接时插入文件路径注释：
   \[
   \nu_{ki} \mapsto \text{/* file: path/to/file */} \oplus \text{code}
   \]

---

## 四、算法伪代码（Algorithm 1）

```
输入：AST单元数据库 ASTDB，最大深度 D
输出：结构感知样本集 S

1. 初始化图 Γ
   for each u ∈ ASTDB:
       Γ.addNode(u)
   for each (u, v) ∈ CallPairs(ASTDB):
       Γ.addEdge(u → v)
   S ← ∅

2. for each p ∈ BFSPaths(Γ, D):
       s ← Concat(Tag(u) for u in p)

3. S ← S ∪ {s}

4. return S

函数 BFSPaths(Γ, D):
    返回 Γ 中所有长度 ≤ D 的有向路径（BFS）

函数 Tag(u):
    if 跨文件(u) then
        return "/* file: u.file */" + code(u)
    else
        return code(u)
```

---

## 五、复杂度分析

设：
- \( n \)：节点总数
- \( d \)：平均出度
- \( D \)：最大路径深度
- \( m \)：路径平均长度

时间复杂度为：
\[
\mathcal{O}(n + n d + n \cdot d^D \cdot m)
\]

---

## 六、算法特点总结

| 特点 | 说明 |
|------|------|
| **结构保持** | 保留 AST 的语法结构完整性 |
| **语义重排序** | 按调用路径重排代码，增强上下文连贯性 |
| **跨文件建模** | 插入文件路径注释，显式区分文件边界 |
| **可配置路径策略** | 支持多种调用/依赖扩展方式 |
| **线性可扩展** | 适用于大规模代码语料构建 |

---

## 七、应用价值

该算法生成的训练样本能够使预训练模型在训练过程中**显式地学习跨函数、跨模块的结构依赖**，从而提升模型在代码理解、代码生成、库调用推荐等任务上的表现。

======

下面通过一个**具体的代码示例**，逐步演示 SPSR-Graph 算法的计算原理，包括 AST 解析、图构建、路径搜索和样本生成。

---

## 示例代码库

假设我们有一个小型代码库，包含两个文件：

### `math_utils.h`
```c
// file: math_utils.h
int add(int a, int b);
int multiply(int a, int b);
```

### `math_utils.c`
```c
// file: math_utils.c
#include "math_utils.h"

int add(int a, int b) {
    return a + b;
}

int multiply(int a, int b) {
    return add(a, b) + add(a, b);  // 调用 add
}
```

### `main.c`
```c
// file: main.c
#include "math_utils.h"

int main() {
    int x = 3, y = 4;
    int sum = add(x, y);          // 调用 add
    int product = multiply(x, y); // 调用 multiply
    return 0;
}
```

---

## 步骤1：AST 解析与语义单元提取

使用 AST 解析器提取**顶层语义单元**（函数定义、函数声明）：

| 节点ID | 类型 | 名称 | 文件 | 代码片段 |
|--------|------|------|------|----------|
| v₁ | 函数声明 | `add` | math_utils.h | `int add(int a, int b);` |
| v₂ | 函数声明 | `multiply` | math_utils.h | `int multiply(int a, int b);` |
| v₃ | 函数定义 | `add` | math_utils.c | `int add(int a, int b) { return a + b; }` |
| v₄ | 函数定义 | `multiply` | math_utils.c | `int multiply(int a, int b) { return add(a, b) + add(a, b); }` |
| v₅ | 函数定义 | `main` | main.c | `int main() { int x = 3, y = 4; int sum = add(x, y); int product = multiply(x, y); return 0; }` |

---

## 步骤2：构建有向图

分析调用关系，添加有向边（`caller → callee`）：

| 边 | 来源节点 | 目标节点 | 边类型 |
|----|----------|----------|--------|
| e₁ | v₃ (add定义) | — | 无调用（叶子节点） |
| e₂ | v₄ (multiply定义) | v₃ (add定义) | Direct Call |
| e₃ | v₅ (main) | v₃ (add定义) | Direct Call |
| e₄ | v₅ (main) | v₄ (multiply定义) | Direct Call |
| e₅ | v₅ (main) | v₁ (add声明) | Include Dependency（间接） |
| e₆ | v₅ (main) | v₂ (multiply声明) | Include Dependency（间接） |

最终得到图结构：

```
v₁(声明:add)    v₂(声明:multiply)
      ↖               ↖
        \               \
         \               \
    v₃(add定义) ←───── v₄(multiply定义) ←───── v₅(main)
```

---

## 步骤3：路径搜索（BFS，设 D=3）

从每个节点出发，执行有向 BFS，收集所有长度 ≤ 3 的路径：

### 从 v₅ (main) 出发
- 长度1：`[v₅]`
- 长度2：`[v₅ → v₃]`，`[v₅ → v₄]`
- 长度3：`[v₅ → v₄ → v₃]`

### 从 v₄ (multiply定义) 出发
- 长度1：`[v₄]`
- 长度2：`[v₄ → v₃]`

### 从 v₃ 出发
- 长度1：`[v₃]`

### 从 v₁, v₂ 出发
- 仅长度1：`[v₁]`，`[v₂]`

---

## 步骤4：样本生成（带跨文件标注）

对每条路径，按顺序拼接代码片段，并在跨文件时插入 `/* file: ... */` 注释。

### 示例1：路径 `[v₅ → v₄ → v₃]`

```
/* file: main.c */
int main() {
    int x = 3, y = 4;
    int sum = add(x, y);
    int product = multiply(x, y);
    return 0;
}

/* file: math_utils.c */
int multiply(int a, int b) {
    return add(a, b) + add(a, b);
}

/* file: math_utils.c */
int add(int a, int b) {
    return a + b;
}
```

### 示例2：路径 `[v₅ → v₃]`

```
/* file: main.c */
int main() {
    int x = 3, y = 4;
    int sum = add(x, y);
    int product = multiply(x, y);
    return 0;
}

/* file: math_utils.c */
int add(int a, int b) {
    return a + b;
}
```

### 示例3：路径 `[v₄ → v₃]`

```
/* file: math_utils.c */
int multiply(int a, int b) {
    return add(a, b) + add(a, b);
}

/* file: math_utils.c */
int add(int a, int b) {
    return a + b;
}
```

### 示例4：单节点路径（如 `[v₅]`）

```
/* file: main.c */
int main() {
    int x = 3, y = 4;
    int sum = add(x, y);
    int product = multiply(x, y);
    return 0;
}
```

---

## 步骤5：复杂度计算

设：
- \( n = 5 \) 个节点
- \( d = 0.8 \)（平均出度：总边数4条/5节点 ≈ 0.8）
- \( D = 3 \)
- \( m \approx 2 \)（平均路径长度）

理论复杂度：
\[
\mathcal{O}(5 + 5×0.8 + 5×(0.8)^3×2) \approx \mathcal{O}(5 + 4 + 5×0.512×2) = \mathcal{O}(5 + 4 + 5.12) = \mathcal{O}(14.12)
\]

实际生成的路径数：
- 长度1：5条
- 长度2：3条（v₅→v₃，v₅→v₄，v₄→v₃）
- 长度3：1条（v₅→v₄→v₃）

共 **9 条路径**，每条路径生成一个训练样本。

---

## 算法计算原理图示

```
┌─────────────────────────────────────────────────────────────────┐
│  输入代码库                                                      │
│  math_utils.h, math_utils.c, main.c                             │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  步骤1: AST解析 + 语义单元提取                                   │
│  v₁(声明add), v₂(声明multiply), v₃(定义add),                     │
│  v₄(定义multiply), v₅(定义main)                                 │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  步骤2: 构建有向图 Γ                                             │
│                                                                  │
│     v₁         v₂                                                │
│      ╲         ╱                                                │
│       ╲       ╱                                                 │
│        v₃ ←── v₄                                                │
│               ↑                                                 │
│               │                                                 │
│               v₅                                                │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  步骤3: BFS路径枚举 (D=3)                                        │
│                                                                  │
│  长度1: [v₁],[v₂],[v₃],[v₄],[v₅]                                │
│  长度2: [v₅→v₃],[v₅→v₄],[v₄→v₃]                                 │
│  长度3: [v₅→v₄→v₃]                                              │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  步骤4: 拼接 + 跨文件标注                                        │
│                                                                  │
│  样本1: /* file: main.c */ code(main)                           │
│         /* file: math_utils.c */ code(multiply)                 │
│         /* file: math_utils.c */ code(add)                      │
│                                                                  │
│  样本2: ...                                                      │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  输出: 训练样本集 S                                              │
│  共9个样本，用于预训练                                            │
└─────────────────────────────────────────────────────────────────┘
```

---

## 核心计算原理总结

| 步骤 | 操作 | 示例中的结果 |
|------|------|--------------|
| 1 | AST解析 | 提取5个语义单元 |
| 2 | 构建有向边 | 识别4条调用/依赖关系 |
| 3 | BFS路径枚举 | 生成9条路径（长度1~3） |
| 4 | 带标注拼接 | 每条路径→一个训练样本 |

**关键洞察**：通过路径 `[v₅ → v₄ → v₃]`，模型能够一次性看到 `main` → `multiply` → `add` 的**完整调用链**，从而理解跨函数、跨文件的语义依赖，而不仅仅是孤立的函数片段。