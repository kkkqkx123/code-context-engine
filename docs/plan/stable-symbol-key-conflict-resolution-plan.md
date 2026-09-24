# 稳定符号键冲突修复方案

## 背景与现象

执行 `export_rs` 时,关系索引在注册稳定符号键(`register_symbol_key`)时输出大量
`stable symbol key already registered to a different entity; keeping the existing mapping`
警告。经统计共 897 条,全部来自 `once_cell`(5 条)与 `ripgrep`(892 条);
`index_sidecar`、`re_export`、`relation_demo`、`relation_diamond` 四个 fixture 均已归零
(上一轮 `process_values::outcome` 冲突已随函数作用域重排改动消失)。

稳定符号键的结构为 `(file_path, scoped_name, kind, overload_discriminator)`,
其中 `overload_discriminator = SHA256(规范化 signature ‖ kind)`;
signature 为空时退化为 `SHA256(span ‖ kind)`。first-wins:同键不同实体时保留旧映射并告警。

## 根因分类

| 类别 | 数量 | 典型样例 | 根因 |
| ---- | ---- | -------- | ---- |
| A. Trait impl 方法作用域名 | ~807 | `Flag::is_switch`、`sync::Debug::fmt` | `impl Trait for Type` 的 TraitImpl 实体 name 被设为 trait 简单名(见 `impl_metadata.rs`),而 scoped_name 仅按 parent 链拼 name。于是 trait 声明方法与**每一个** `impl Flag for *` 方法都得到同一 `Flag::方法` 作用域名;signature 只含函数头(不含函数体),不同 impl 的头部完全相同 → discriminator 相同。ripgrep `defs.rs` 约 104 个 `impl Flag for *` × 6 方法即贡献约 590 条。 |
| B. cfg 互斥的重复声明 | ~15 | `imp` Module ×3、`error` TestCase(unix/not(unix))、`Error`/`Err`/`Output` TypeAlias、`Macro debug` | 解析器同时索引所有互斥 `#[cfg(...)]` 分支,cfg 这一维度未进入符号身份;分支内声明文本相同 → signature/kind/scoped_name 全同。cfg 谓词其实已被 `attribute_extractor` 写入实体自身 metadata 的 `annotations`。 |
| C. 函数体内嵌套类型声明 | ~3 | `race.rs::Void`(在 `get_or_init`/`get_and_update` 体内 `enum Void {}`) | `establish_function_scope_relationships` 仅把函数样与 Variable/Constant 重挂到所在函数,函数体内声明的 Enum/Struct 等类型仍是"孤儿",作用域名不含所在函数 → 多个函数里的同名局部类型相撞。 |
| D. 同作用域同名局部绑定 / 变体字段 | ~32 | `SortMode::supported::err`(三处 `let Err(err)`)、`is_fixed_strings::lineterm`(两处)、`TypeChange::name`、`Error::err` | 同名同 signature 的多条局部 `let` 或不同枚举变体的同名字段,statement 文本一致 → discriminator 一致。Variable-like kind 本质是位置作用域、不参与重载,却用了签名式 discriminator。 |
| E. Import 实体 | 41 | `self::StandardStreamKind::*`、`std::io::IsTerminal` | `use` 语句作为 Import kind 实体被注册为可寻址符号;cfg 分支内重复的 glob/具名导入路径全同 → 冲突。Import 是文件级事实,不是被调用/被引用的定义。 |
| F. 结构体 impl 块本身 | 4 | `Standard`、`Summary`、`UserColorSpec`、`Searcher` InherentImpl | 同一类型可有多个 inherent impl 块;块实体本身被注册为符号但无寻址价值。 |

## 设计原则与关键约束

**约束(决定了 cfg 方案走向):`overload_discriminator` 只在注册时计算一次,之后作为不透明列持久化并原样读回**——
`snapshot_reader.rs` 直接从 SQLite 列还原 key,`snapshot_loader.rs` 重新注册时从 `CanonicalEntity` 还原实体
(signature、span、metadata 均随行持久化),delta 重放携带的也是已算好的 key。

推论:任何折进 discriminator 的新输入,**必须能从单个实体自身被持久化的字段复现**
(signature / span / kind / 自身 metadata)。因此:

- ✅ 采用**实体自身的 cfg 谓词**(存于自身 metadata `annotations`,随 `CanonicalEntity.metadata` 往返)。
- ❌ 不采用"祖先链 cfg 路径":注册时 `register_symbol_key` 只拿到单实体、拿不到父链;若在别处(作用域解析阶段)算好祖先 cfg 再传入,重载路径无法以相同方式复现,会造成 base-cache 重载后 key 漂移。函数体内嵌套类型改由 C 类的作用域重排修复,而非祖先 cfg。

其余修复(scoped_name 与 kind 白名单)不触碰 discriminator 的可复现性。

## 修复项(按实施顺序)

### 修复 1 —— TraitImpl 方法作用域名(A 类,收益最大)

`cce-types/src/types/entity/file.rs` 的 `resolve_scoped_name_from_map`:遍历 parent 链时,
若某段实体 kind 为 `TraitImpl`,用 `<{impl_for_type} as {trait_name}>` 作为该段,
`impl_for_type` 取自该实体 metadata(trait name 即其 `name`);缺失时回退原 name。
`InherentImpl` 段保持原样(其 name 已是类型名,单类型多块的问题交给修复 5 的过滤)。

效果:`impl Flag for ColorFlag` 的 `name_long` → `<ColorFlag as Flag>::name_long`,
与 trait 声明的 `Flag::name_long` 及彼此都区分。约消除 810/897 条。显示名 `entity.name` 不变,
不影响分组与 NL 渲染。

### 修复 2 —— cfg 谓词折入 discriminator(B 类根本修复,详见下节)

### 修复 3 —— 位置作用域 kind 折入 span(D 类)

`cce-relation/src/index/core.rs` 的 `register_symbol_key`:当 `kind.is_variable_like()`
(Field/Property/Variable/Constant/EnumVariant)时,无论 signature 是否为空,
都把 `span` 一并折入 discriminator。这些 kind 不参与重载解析,身份本质由位置界定;
span 随行持久化,重载可复现。消除 `err`/`lineterm`/`TypeChange::name` 等冲突。

### 修复 4 —— 函数体内嵌套类型重挂到所在函数(C 类)

`parent_child_resolver.rs` 的 `establish_function_scope_relationships`:
把"重挂到最近函数样祖先"的适用 kind 从"函数样 + Variable/Constant"扩展到"函数样 + variable-like + 类型声明类
(Enum/Struct/Union/TypeAlias/Trait)"。仅重挂**位于函数体内**者(span 包含 + parent 为空),
不影响模块/命名空间层的正常类型。`race.rs` 的 `Void` → `...get_or_init::Void`,彼此区分。

### 修复 5 —— 跳过不可寻址实体的稳定键注册(E、F 类)

`cce-relation/src/index/builder/file_processor.rs` 注册循环中,
对 `Import` 与 impl 块(`InherentImpl`/`TraitImpl`)kind 的实体直接 `continue`,不注册稳定键:
`use` 与裸 impl 块不是被引用/被调用的目标定义,其"符号身份"无寻址价值。
跨文件解析、import/export diff 均走各自的导入表 / 导出符号路径,不依赖这些 kind 的稳定键。

## 针对 cfg 问题的根本解决

**问题的根本**:Rust 的 `#[cfg(...)]` 表达的是"按编译配置互斥选择的同一逻辑符号的多个候选定义"。
解析器把所有分支一次性铺平成同一符号集,而稳定身份里没有任何维度承载"这个定义在什么配置下成立",
于是互斥分支塌缩成同一 key,被误判为冲突。

**根本解法(修复 2)**:把 cfg 从"被忽略的维度"提升为"符号身份的组成部分"——
在 `register_symbol_key` 构造 key 前,从实体自身 metadata `annotations` 中提取 `cfg(...)` /
`cfg_attr(...)` 谓词并规范化,作为附加输入折入 `overload_discriminator`
(实现上扩展 `StableSymbolKey::new`/`new_with_span`,或在注册处计算折入;保持既有 `new`/`new_with_span`
签名不变以免波及测试调用点)。仅当实体自身带 cfg 时生效;非 Rust 或无 cfg 实体谓词为空串 →
discriminator 与今日完全一致,零回归。

这样:
- `#[cfg(feature="std")] mod imp` 与 `#[cfg(not(feature="std"))] mod imp` 因谓词不同 → 各自独立稳定符号,
  每个分支的成员都获得自己的 key 与 stable_id(顺带修复 first-wins 下"落选变体实体被孤儿化、无法稳定寻址"的隐患)。
- `#[cfg(test)]` 与被测代码的同名符号天然分离。
- 谓词取自实体自身 metadata,跨会话/重载/delta 全部可复现(满足上文关键约束)。

**为何这是"根本"而非"降噪"**:仅把 warn 降 debug 或继续 first-wins,会掩盖"互斥分支本应各自可寻址"这一真实缺陷,
并让部分变体的下游引用悬空;把 cfg 纳入身份则从模型上承认了 cfg 这一正交维度,冲突不再发生,
且与关系索引的跨会话稳定 ID 目标一致。若后续需要"按 feature 视图裁剪",也可在身份里已具备的 cfg 谓词上叠加,
无需再改动 key 结构。

## 验证计划

- `cargo run --example export_rs` / `export_py` / 其余 `export_*`:统计
  `stable symbol key already registered` 归零(或仅剩真实重名且同 cfg 的合法重载)。
- `cargo test --workspace --lib`:确保解析、分组、关系、delta、快照往返测试通过。
- `cargo clippy --all-targets --all-features`、`cargo fmt`。
- 单测:为 TraitImpl 作用域名、cfg discriminator 复现(注册后从 `CanonicalEntity` 还原再算 key 相等)、
  variable-like span 折叠、函数体内嵌套类型重挂各补一例。

## 影响与风险

- stable_id 全部因 discriminator 变化而重排:项目处于开发期、无向后兼容要求(见 AGENTS.md),可接受;
  需重建既有索引/缓存。
- TraitImpl 段改为 `<T as Trait>`:确认无消费方按旧 `Trait::method` 字符串解析 Rust 作用域名(关系解析按
  名称片段与符号表匹配,非整串相等;NL 渲染用的是 entity.name 非 scoped_name)。以全量测试兜底。
- 跳过 Import/impl-block 注册:以 `import_export_diff`、跨文件 import 解析相关测试验证不依赖这些 kind 的稳定键。
