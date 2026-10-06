# 关系图查询功能分析报告

> 范围：对 CCE 现有关系图查询功能进行全面盘点，评估设计合理性与功能完整性。

---

## 1. 关系类型体系

共定义 **38 种关系类型**，归属 5 大领域：

| 领域 | 数量 | 代表类型 |
|------|------|----------|
| 调用 | 13 | DirectCall, ConstructorCall, AsyncCall, MacroCall, GoroutineCall... |
| 依赖 | 10 | ImportNamed, Use, ModuleDependency, MacroDependency... |
| 结构 | 9 | Inheritance, Implementation, Contains, Embedding, Mixin... |
| 引用 | 2 | TypeReference, FieldAccess |
| 模板/标记 | 4 | ElementContains, TemplateReference, ParameterBinding, EventCallback |

另有 5 种外部调用分类：StandardLibrary, ExternalLibrary, DevDependency, LocalDependency, Unknown。

---

## 2. 查询操作清单

### 2.1 图级查询（14 个端点）

| 端点 | 方法 | 说明 |
|------|------|------|
| `/graph/ego` | GET | BFS  ego 邻域，可配深度/方向 |
| `/graph/path` | GET | 两实体间最短路径 |
| `/graph/subgraph` | GET | 显式实体集的诱导子图 |
| `/graph/components` | GET | 连通分量（Union-Find） |
| `/graph/export` | GET | 全项目图导出（node-link JSON） |
| `/graph/impact` | GET | 文件变更影响分析 |
| `/graph/entity-impact` | GET | 实体变更影响分析 |
| `/graph/cycles` | GET | 依赖环检测（实体级/文件级） |
| `/graph/structural` | GET | 单实体结构/前端关系 |
| `/graph/module` | GET | 单文件模块级关系 |

### 2.2 实体关系查询（6 个端点）

| 端点 | 说明 |
|------|------|
| `/function/{id}/calls` | 获取被调用者 |
| `/function/{id}/callers` | 获取调用者 |
| `/call-chain/{id}` | 正向/反向调用链遍历 |
| `/call-path` | 两函数间调用路径 |
| `/class/{id}/inheritance` | 类继承层次（传递闭包） |
| `/class/{id}/implementations` | 接口实现查询 |

### 2.3 CLI 命令

`cce-cli graph` 子命令覆盖：ego, path, subgraph, components, export, impact, entity-impact, cycles, structural, module。

---

## 3. 图遍历算法

| 算法 | 实现位置 | 用途 |
|------|----------|------|
| BFS | `CallChainTraverser::traverse_from()` | 调用链遍历、ego 邻域 |
| 双向 BFS | `CallChainTraverser::find_path()` | 最短路径 |
| Yen 算法 | `CallChainQuery::find_k_shortest_paths()` | K 最短路径 |
| Union-Find | `GraphService::connected_components_with_options()` | 连通分量 |
| 迭代 DFS | `CallChainQuery::find_entity_cycles()` | 依赖环检测 |
| Kahn 拓扑排序 | `FileDependencyGraph::topological_sort()` | 文件处理顺序 |
| 传递 BFS | `collect_transitive_dependents/dependencies()` | 影响传播 |
| 层次闭包 BFS | `CallChainQuery::hierarchy_closure()` | 继承层次 |

---

## 4. 数据流

```
源码 → Tree-sitter 解析 → 符号解析（两阶段：本地 + 跨文件）
    → 索引构建（RelationIndex, DashMap 内存结构）
    → 快照发布（LayeredSnapshotIndex, CoW + Arc 零拷贝）
    → 查询层（RelationSearcher + GraphService + CallChainQuery）
    → API / CLI 输出
```

### 关键文件

| 阶段 | 文件 |
|------|------|
| 索引构建 | `crates/parser/cce-relation/src/index/builder.rs` |
| 符号解析 | `crates/parser/cce-relation/src/index/resolver.rs` |
| 快照发布 | `crates/parser/cce-relation/src/index/snapshot_index.rs` |
| 查询门面 | `crates/app/cce-orchestrator/src/query/relation_searcher.rs` |
| 图服务 | `crates/app/cce-orchestrator/src/query/graph/service.rs` |
| 遍历核心 | `crates/parser/cce-relation/src/query.rs` |
| 依赖图 | `crates/parser/cce-relation/src/dependency_graph.rs` |

---

## 5. 存储设计

### 5.1 内存索引（RelationIndex）

| 索引 | 类型 | 用途 |
|------|------|------|
| `function_index` | `DashMap<EntityId, Entity>` | 实体主存储 |
| `name_index` | `DashMap<String, SmallVec<[EntityId; 2]>>` | 名称倒排 |
| `resolved_relation_index` | `DashMap<EntityId, RelationEdgeSet>` | 正向关系（caller → edges） |
| `reverse_callee_index` | `DashMap<EntityId, Vec<EntityId>>` | 反向关系（callee → callers） |
| `file_relation_index` | `DashMap<String, RelationEdgeSet>` | 文件级关系 |
| `file_callers_by_callee` | `DashMap<EntityId, HashSet<String>>` | 文件级反向索引 |
| `dependency_graph` | `FileDependencyGraph` | 文件依赖图 |
| `symbol_key_to_entity` | `RwLock<HashMap<SymbolKey, EntityId>>` | 稳定 ID 映射 |

### 5.2 持久化

- 格式：`CanonicalRelationSnapshot`（rkyv 序列化）
- 校验：SHA-256 指纹
- 存储：SQLite（`cce-storage-sqlite`）
- 增量：base + delta 分层快照

### 5.3 设计决策

1. **双索引模式**：正向 + 反向，O(1) 双向查找
2. **CoW 快照**：`Arc` 零拷贝共享，无锁并发读
3. **分层快照**：base + delta 增量更新
4. **稳定 ID**：`(file_path, scoped_name, kind)` 三元组
5. **文件级分离**：文件关系独立存储，避免污染实体查询

---

## 6. 设计合理性评估

### 优点

- 双索引模式保证双向查询效率
- CoW 快照支持高并发无锁读
- 文件级与实体级关系分离，查询粒度清晰
- 算法覆盖全面（BFS/DFS/Union-Find/Yen/Kahn）
- 稳定 ID 设计支持跨会话一致性

### 架构层面的合理设计

- 查询层与存储层解耦（`RelationIndexView` trait）
- 快照不可变，读写分离
- CLI 与 API 共享同一查询服务层
