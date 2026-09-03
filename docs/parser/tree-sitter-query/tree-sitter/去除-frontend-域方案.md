# 去除 Frontend 域修改方案

## 背景

当前系统中存在 `@frontend` 域用于捕获前端特定的概念（组件、绑定、事件等）。但从代码关系的本质来看，前端和后端的概念是语义等价的，没有必要维护独立的域。

## 设计原则

1. **语义等价统一**：前端概念映射到后端等价概念
2. **向后兼容**：过渡期支持别名，逐步迁移
3. **最小改动**：优先复用现有捕获名称
4. **清晰可扩展**：保留框架特定语义的表达能力

---

## 映射方案总览

### Frontend → Entity 域映射

| 原 Frontend 捕获 | 新捕获名称 | 说明 |
|-----------------|-----------|------|
| `@frontend.component.usage` | `@call.constructor.component.name` | 组件实例化 = 构造函数调用 |
| `@frontend.component.dependency` | `@dependency.import.component.name` | 组件导入 = 导入依赖 |
| `@frontend.prop.binding` | `@entity.variable.parameter.prop.name` | Props = 参数 |
| `@frontend.event.binding` | `@call.callback.event.handler` | 事件绑定 = 回调函数 |
| `@frontend.class.binding` | `@entity.variable.parameter.class.name` | 类绑定 = 参数 |
| `@frontend.style.binding` | `@entity.variable.parameter.style.name` | 样式绑定 = 参数 |
| `@frontend.template.reference` | `@reference.field_access.template` | 模板引用 = 字段访问 |
| `@frontend.element.contains` | `@structural.contains.element` | 元素包含 = 结构包含 |
| `@frontend.slot.usage` | `@entity.variable.parameter.slot.name` | Slot = 参数传递 |
| `@frontend.style.scope` | `@entity.style_rule.scope` | 样式作用域 = 样式规则属性 |

### Frontend → 扩展属性映射

对于框架特有的语义，使用扩展属性（attribute）表达：

| 概念 | 基础捕获 | 扩展属性 |
|-----|---------|---------|
| Vue v-model 双向绑定 | `@entity.variable.parameter.name` | `.reactive` |
| Vue 计算属性 | `@entity.property.computed.name` | `.cached` |
| Vue 侦听器 | `@call.callback.watcher.name` | `.immediate` / `.deep` |
| React Hook | `@call.direct.hook.name` | `.stateful` |
| Svelte 响应式声明 | `@entity.variable.reactive.name` | `.auto_subscribe` |

---

## 具体修改内容

### 1. EntityKind 映射（types/entity.rs）

```rust
// 移除独立的前端实体类型，合并到现有类型中

// Component → 映射到 Class
EntityKind::Component → EntityKind::Class
// 添加 metadata: {"_type": "component", "framework": "vue"}

// Element → 映射到新的 Element 子类型
EntityKind::Element → 保留，作为 Class 的轻量级变体

// Directive → 映射到 Attribute
EntityKind::Directive → 移除，使用 @entity.attribute 表达

// StyleRule/StyleSelector/StyleProperty → 保留在 Entity 域
// 作为 @entity.style_rule 等

// Hook/Computed/Watcher → 映射到 Function/Property
EntityKind::Hook → EntityKind::Function (with .hook attribute)
EntityKind::Computed → EntityKind::Property (with .computed attribute)
EntityKind::Watcher → EntityKind::Function (with .watcher attribute)
```

### 2. RelationType 映射（types/relation.rs）

```rust
// 移除 Frontend Domain，映射到现有关系类型

// ComponentImport → ImportNamed (添加 .component 属性)
RelationType::ComponentImport → RelationType::ImportNamed

// ComponentUsage → ConstructorCall (添加 .component 属性)
RelationType::ComponentUsage → RelationType::ConstructorCall

// PropBinding → Parameter (添加 .prop 属性)
RelationType::PropBinding → RelationType::Parameter

// EventBinding → CallbackCall (添加 .event 属性)
RelationType::EventBinding → RelationType::CallbackCall

// ElementContains → Contains (添加 .element 属性)
RelationType::ElementContains → RelationType::Contains

// 移除以下类型（完全被现有类型覆盖）：
// - SlotUsage
// - TemplateReference
// - StyleScope
// - ClassBinding
```

### 3. 查询捕获修改（query/scheme/*.rs）

#### vue.rs 修改示例

```rust
// 修改前
(self_closing_tag
  (tag_name) @frontend.component.usage.name
) @frontend.component.usage

// 修改后
(self_closing_tag
  (tag_name) @call.constructor.component.name
) @call.constructor.component
```

```rust
// 修改前
(directive_attribute
  (directive_name) @entity.directive.bind.shorthand
  (directive_argument) @entity.directive.bind.arg
) @entity.directive.bind

// 修改后
(directive_attribute
  (directive_name) @entity.attribute.bind.name
  (directive_argument) @entity.variable.parameter.bind.arg
) @entity.attribute.bind
```

#### svelte.rs 修改示例

```rust
// 修改前
(attribute
  (attribute_name) @entity.binding.name
  (#match? @entity.binding.name "^bind:")
) @entity.binding

// 修改后
(attribute
  (attribute_name) @entity.attribute.binding.name
  (#match? @entity.attribute.binding.name "^bind:")
) @entity.attribute.binding
```

#### tsx.rs 修改示例

```rust
// 修改前
(jsx_self_closing_element
  name: (identifier) @frontend.component.usage.self_closing.name
) @frontend.component.usage.self_closing

// 修改后
(jsx_self_closing_element
  name: (identifier) @call.constructor.component.self_closing.name
) @call.constructor.component.self_closing
```

### 4. Domain Enum 修改（types/parser.rs）

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Domain {
    #[serde(rename = "entity")]
    #[default]
    Entity,
    #[serde(rename = "call")]
    Call,
    #[serde(rename = "dependency")]
    Dependency,
    #[serde(rename = "comment")]
    Comment,
    // 移除 Frontend Domain
    // #[serde(rename = "frontend")]
    // Frontend,
}
```

### 5. CaptureName 解析修改

```rust
// 移除 Frontend 域的解析支持
impl std::str::FromStr for Domain {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "entity" => Ok(Domain::Entity),
            "call" => Ok(Domain::Call),
            "dependency" => Ok(Domain::Dependency),
            "comment" => Ok(Domain::Comment),
            // 移除："frontend" => Ok(Domain::Frontend),
            _ => Err(format!("Unknown domain: {}", s)),
        }
    }
}
```

---

## 迁移计划

### Phase 1: 添加扩展属性支持（1 周）

1. 修改 `CaptureName` 结构，支持扩展属性解析
2. 添加扩展属性验证逻辑
3. 更新 `is_entity_capture` 等函数支持扩展属性

### Phase 2: 并行捕获（2 周）

1. 在查询文件中同时生成新旧捕获（保持向后兼容）
2. 修改 `vue.rs`, `svelte.rs`, `tsx.rs` 等文件
3. 添加废弃警告（deprecated）

```rust
// 示例：同时生成新旧捕获
(self_closing_tag
  (tag_name) @call.constructor.component.name
             @frontend.component.usage.name  // 废弃，但保留
) @call.constructor.component
```

### Phase 3: 更新类型定义（1 周）

1. 修改 `EntityKind` 移除前端特有类型
2. 修改 `RelationType` 移除前端特有类型
3. 更新类型检查函数（`is_frontend_entity` 等）

### Phase 4: 移除 Frontend 域（1 周）

1. 从 `Domain` enum 中移除 `Frontend`
2. 从查询文件中移除废弃的 `@frontend.*` 捕获
3. 更新所有测试用例

### Phase 5: 文档更新（1 周）

1. 更新 `spec.md` 去除 Frontend 域描述
2. 更新各语言的查询文档
3. 编写迁移指南

---

## 收益分析

### 维护成本降低

| 项目 | 修改前 | 修改后 | 节省 |
|-----|-------|-------|-----|
| Domain 数量 | 5 | 4 | 20% |
| 关系类型数量 | 35+ | 25+ | 28% |
| 验证逻辑复杂度 | 高 | 中 | 显著降低 |

### 查询一致性提升

- 统一使用 `@entity.*`, `@call.*`, `@dependency.*` 表达概念
- 减少开发人员理解成本
- 便于跨语言分析（如同时分析 Vue 组件和 TypeScript 类）

### 扩展性增强

- 新增框架支持时，复用现有捕获名称
- 通过扩展属性表达框架特性，不影响核心模型

---

## 风险评估

| 风险 | 影响 | 缓解措施 |
|-----|-----|---------|
| 破坏性变更 | 高 | Phase 2 并行期提供充分的迁移时间 |
| 查询文件重写 | 中 | 提供自动化迁移脚本 |
| 测试用例失效 | 中 | 同步更新测试，确保覆盖率 |
| 外部工具依赖 | 低 | 预留 1 个版本的废弃期 |

---

## 附录：完整映射表

### Vue 捕获映射

| 原捕获 | 新捕获 |
|-------|-------|
| `@entity.component.root` | `@entity.type.component.root` |
| `@entity.component.self_closing.name` | `@call.constructor.component.self_closing.name` |
| `@entity.directive.bind.arg` | `@entity.variable.parameter.bind.arg` |
| `@entity.directive.on.arg` | `@call.callback.event.arg` |
| `@entity.directive.for.value` | `@entity.control.flow.for.value` |
| `@entity.interpolation.content` | `@entity.expression.interpolation.content` |
| `@entity.slot.name.value` | `@entity.variable.parameter.slot.name` |
| `@frontend.component.usage.name` | `@call.constructor.component.name` |
| `@frontend.prop.binding.prop` | `@entity.variable.parameter.prop.name` |
| `@frontend.event.binding.event` | `@call.callback.event.name` |

### Svelte 捕获映射

| 原捕获 | 新捕获 |
|-------|-------|
| `@entity.component.name` | `@entity.type.component.name` |
| `@entity.if.start` | `@entity.control.flow.if.start` |
| `@entity.each.start` | `@entity.control.flow.each.start` |
| `@entity.await.start` | `@entity.control.flow.await.start` |
| `@entity.event.handler.name` | `@call.callback.event.name` |
| `@entity.binding.name` | `@entity.attribute.binding.name` |
| `@entity.transition.name` | `@entity.attribute.transition.name` |
| `@frontend.component.usage.name` | `@call.constructor.component.name` |
| `@frontend.binding.property.name` | `@entity.variable.parameter.bind.name` |

### TSX/JSX 捕获映射

| 原捕获 | 新捕获 |
|-------|-------|
| `@entity.jsx.element.name` | `@entity.type.element.name` |
| `@entity.jsx.component.opening.name` | `@call.constructor.component.opening.name` |
| `@entity.jsx.attribute.name` | `@entity.variable.parameter.jsx.name` |
| `@entity.jsx.event.name` | `@call.callback.event.jsx.name` |
| `@frontend.component.usage.name` | `@call.constructor.component.name` |
| `@frontend.prop.binding.name` | `@entity.variable.parameter.prop.name` |

### HTML 捕获映射

| 原捕获 | 新捕获 |
|-------|-------|
| `@frontend.element.contains` | `@structural.contains.element` |
| `@frontend.template.reference` | `@reference.field_access.template` |

### CSS 捕获映射

| 原捕获 | 新捕获 |
|-------|-------|
| `@frontend.at.media.contains` | `@structural.contains.media` |
| `@frontend.style_scope.contains` | `@structural.contains.style` |

---

## 实施检查清单

- [ ] Phase 1: 扩展属性支持实现
- [ ] Phase 2: 并行捕获实现，添加废弃警告
- [ ] Phase 3: EntityKind/RelationType 更新
- [ ] Phase 4: Domain 枚举更新，Frontend 域移除
- [ ] Phase 5: 文档更新
- [ ] 自动化迁移脚本
- [ ] 回归测试通过
- [ ] 性能基准测试无回归
