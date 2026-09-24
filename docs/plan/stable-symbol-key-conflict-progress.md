# 稳定符号键冲突修复 —— 进度记录

> 本文件记录阶段性进度,配合完整方案 `stable-symbol-key-conflict-resolution-plan.md` 阅读。
> 更新时间点:方案已落地大部分,尚余验证与回归测试,且最后一次编辑后未重新编译。

## 测量基线

- 修复前:`export_rs` 共 **897** 条 `stable symbol key already registered` 警告
  (once_cell 5 / ripgrep 892;index_sidecar、re_export、relation_demo、relation_diamond 已为 0)。
- 中途测量(在"关联类型重挂"编辑**之前**):**897 → 13**。
  剩余 13 条:`TypeAlias`(color.rs `Err` ×3、sink.rs `sinks::Error` ×2 与 `Error`、stats.rs `Output`、
  matcher/tests/util.rs `Error`)与 `Function`(defs.rs `mkctx` ×2、`select`,tests `m`)。
- 最后一次编辑(缩小 `allow_type_parent` 范围)之后 **未重新编译、未重新测量**。

## 已完成改动(代码已写,状态见下注)

1. **TraitImpl 方法作用域名(A 类)** — `cce-types/src/types/entity/file.rs`
   `resolve_scoped_name_from_map` 遍历 parent 链时,`TraitImpl` 段渲染为 `<impl_for_type as Trait>`,
   缺失回退 trait 名。已编译通过(首轮)。

2. **cfg 谓词折入 discriminator(cfg 根本修复)**
   - `cce-types/.../entity/meta_keys.rs`:新增 `CFG_PREDICATE`。
   - `cce-parser/.../post_processing/attribute_extractor.rs`:收集 cfg/cfg_attr 谓词,
     排序去重后写入实体自身 `metadata[CFG_PREDICATE]`。
   - `cce-types/.../relation/canonical.rs`:新增 `StableSymbolKey::for_entity`,
     discriminator 折入 签名 + kind + cfg + (空签名或 variable-like 时的 span);
     空 cfg 不改变哈希(零回归)。
   - `cce-relation/.../index/core.rs`:`register_symbol_key` 改用 `SymbolKey::for_entity`。
   已编译通过(首轮)。

3. **variable-like span 折叠(D 类)** — 已并入上面 `for_entity`(Field/Property/Variable/Constant/EnumVariant
   折 span)。首轮已验证消除 `err`/`lineterm`/`TypeChange::name` 等。

4. **跳过不可寻址实体的稳定键注册(E、F 类)** —
   `cce-relation/.../index/builder/file_processor.rs` 注册循环:
   `is_import_like() || is_impl_block()` 直接 `continue`。首轮已验证消除 Import/裸 impl 块冲突。

5. **函数体内嵌套类型 / 关联类型重挂(C 类 + 关联类型)** —
   `cce-parser/.../extractor/parent_child_resolver.rs` `establish_function_scope_relationships`:
   - 候选 kind 扩展:函数样 + Variable/Constant + Struct/Enum/Union/Trait/TypeAlias
     (仅重挂到函数容器,解决 `race.rs::Void`)。
   - `allow_type_parent`:函数样 **或 TypeAlias** 可挂到类型/impl 容器
     (解决 impl/trait 体内的关联类型 `type Err`)。
   - **此项含本阶段最后两次编辑,尚未编译/测量。**

## 待办

- [ ] 重新编译受影响 crate(`cce-types`/`cce-parser`/`cce-relation`)。
- [ ] 重跑 `export_rs`,确认剩余警告进一步下降;记录最终数与样例。
- [ ] 分析剩余 `Function` 冲突(`mkctx`/`select`/`m`):判定是"同 cfg 同名合法重载(first-wins 可接受)"
      还是仍属可修复的父链/作用域遗漏。
- [ ] 回归其余 `export_*`(至少 export_py)确认无新增 panic、分组不变。
- [ ] 单元测试(每个修复各一例):TraitImpl 作用域名;`for_entity` 往返一致性
      (注册后从 `CanonicalEntity` 还原再算 key 相等);variable-like span 折叠;
      关联类型 / 局部类型重挂。
- [ ] `cargo test --workspace --lib`、`cargo clippy --all-targets --all-features`、`cargo fmt`。

## 风险与注意

- 关联类型/局部类型重挂改变了实体 parent/children,可能影响 grouper 分组与 NL 渲染
  (尤其 Struct/Enum 现在会作为候选进入本 pass)。**务必以全量分组/导出测试兜底**。
- stable_id 因 discriminator 规则变化整体重排:开发期无向后兼容要求,需重建既有索引/缓存。
- 若最终仍有少量 `Function` 同 cfg 同名同签名冲突,属 Rust 语义下真实的显式重复声明,
  first-wins 合理,可保留 debug 级记录而非视为缺陷。
