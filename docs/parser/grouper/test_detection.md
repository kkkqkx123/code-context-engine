# 测试识别（TestInfo）逻辑总结

> 本文档总结 grouper 阶段的测试代码标记机制。规划与覆盖矩阵见 `docs/plan/language_test_coverage.md`，标记语义与合并规则见 `docs/plan/test_marker_refactoring.md`。

## 1. 总体设计

测试标记与分组、NL 转换正交：不改变 `GroupType`/`EntityKind`，只给 chunk/group/entity 打上 `TestInfo` 标记，供 `exclude_test` 查询过滤、e2e 判定与统计使用。

- `TestInfo { status, source, granularity }`（`crates/cce_core/src/types/test_info.rs`）
  - `status ∈ { Test, Unknown }`；`Unknown` 永不默认升级为 `Test`
  - `source ∈ { Ast > Path > None }`（合并时决定优先级）
  - `granularity ∈ { File, Entity, Group }`（仅信息性，不影响逻辑）
- 合并规则：任一组员为 `Test` 则整组为 `Test`；`Ast` 覆盖 `Path`；合并结果恒为 `Group` 粒度。

## 2. 三层检测体系

入口 `TestSuiteDetector::detect_entity`（`test_suite/detector.rs`）按优先级依次判定：

1. **AST 实体直判**：`entity.kind.is_test_entity()`（提取器已提升为 `TestCase`/`TestSuite`，如 Rust `#[test]`、C# `[Test]`）→ `test_ast()`。
2. **注解缓冲**：实体携带 `test_annotations` 元数据（提取器在 `language_has_annotation_semantics` 语言上缓冲前一实体的注解，`entity_extractor.rs`）→ 交 `languages::detect_from_annotations`。
3. **注解邻接**：`AnnotationIndex` 按字节位置建立注解索引，`adjacent_annotation_names` 收集与实体之间仅隔空白/注释的注解（结构边界 `{}`/`;` 阻断）→ 交注解检测。
4. **命名约定**：`languages::detect_conventional`，语言特定且大多要求文件级约束（必须先命中路径规则）。
5. 均未命中 → `Unknown`；文件级路径规则在 grouper 中与实体级标记合并（`annotate_groups_test_info`，pipeline.rs）。

路径规则 `TestInfo::from_path` 独立于 AST 检测：测试目录/测试文件名 → `test_path()`（File 粒度）。

## 3. 模块结构

```
grouper/recognizers/test_suite/
├── detector.rs        # TestSuiteDetector、AnnotationIndex、三级判定入口
├── languages.rs       # dispatch：detect_from_annotations / detect_conventional 按语言分发
└── languages/
    ├── rust.rs        # #[test]、#[cfg(test)]
    ├── python.rs      # @pytest.mark.* 源码块重建；test_*（文件约束）
    ├── go.rs          # TestXxx/BenchmarkXxx（*_test.go 约束）
    ├── java.rs        # Java+Kotlin：@Test/@ParameterizedTest；*Test/*Tests 类；Kotlin 另含 *Spec
    ├── javascript.rs  # describe/it/test（测试文件约束），JS/TS/JSX/TSX 共用
    ├── csharp.rs      # [Test]/[TestCase]/[Fact]/[Theory]；*Test/*Tests 类
    ├── scala.rs       # @Test；*Test/*Tests/*Spec 类
    ├── php.rs         # #[Test] 属性 + @test docblock；*Test 类
    ├── cpp.rs         # TEST()/TEST_F() 宏参数形态校验（两裸标识符）
    └── lua.rs         # luaunit test_* / test+数字（文件约束）
```

路径规则集中在 `cce_core/src/types/test_info.rs::from_path`（核心类型避免向 parser 引入依赖）。

## 4. 各语言覆盖现状（批次 1-4 完成）

| 语言 | 注解检测 | 命名约定 | 路径规则 |
|------|----------|----------|----------|
| Rust | `#[test]`/`#[cfg(test)]` | 无（刻意拒绝命名） | `*_test.rs`、`tests/` |
| Python | `@pytest.mark.*`、`@pytest.fixture` | `test_*`（仅测试文件内） | `test_*.py`、`conftest.py`、`tests/` |
| Go | — | `TestXxx`/`BenchmarkXxx`（`*_test.go`） | `*_test.go` |
| Java | `@Test`/`@ParameterizedTest` | `*Test`/`*Tests` 类 | `*Test.java`/`*Tests.java`、`src/test/` |
| Kotlin | `@Test`/`@ParameterizedTest` | `*Test`/`*Tests`/`*Spec` 类 | `*Test.kt`/`*Tests.kt`/`*Spec.kt`、`src/test/` |
| JS/TS/JSX/TSX | — | `describe`/`it`/`test`（测试文件约束） | `.spec.*`/`.test.*`（8 种扩展名）、`__tests__/` |
| C# | `[Test]`/`[TestCase]`/`[Fact]`/`[Theory]` | `*Test`/`*Tests` 类 | `*Test.cs`/`*Tests.cs`、`tests/` |
| Scala | `@Test` | `*Test`/`*Tests`/`*Spec` 类 | `src/test/`、`tests/` |
| PHP | `#[Test]` + `@test` docblock | `*Test` 类 | `*Test.php`、`tests/` |
| C++ | `TEST()`/`TEST_F()` 宏（参数形态） | — | `*_test.cpp`/`*_test.cc`、`tests/` |
| C | — | —（降级） | `*_test.c`、`tests/` |
| Dart | — | —（降级：test()/group() 无实体） | `test_*.dart`、`test/`、`tests/` |
| Ruby | — | —（降级：describe/it 无实体） | `spec/`、`tests/` |
| Lua | — | luaunit `test_*`/`test`+数字（文件约束） | `test_*.lua`/`*_test.lua`/`*_spec.lua`、`test/`/`tests/`/`spec/` |
| Bash | — | —（降级：`@test` 为 command 无实体） | `*.bats`、`tests/` |
| Html/Css/Scss/Less/Vue/Svelte | — | — | 仅通用 `tests/`（明确不实施） |

## 5. 设计原则（反误报优先）

- **精确 token 匹配**：宏名/注解名完全相等或形态匹配，杜绝 `contains("test")` 子串。
- **命名约定必须文件约束**：如 Lua `test_*` 仅测试文件内生效、Go `TestXxx` 仅 `*_test.go`、JS `describe` 仅测试文件。
- **camelCase 边界**：Lua 刻意拒绝 `testXxx` camelCase（`testMode` 形态与生产命名同构），仅接受 `test_` 与 `test`+数字；`testify`、`contest`、`latest` 等反例有单测。
- **无实体则降级**：信号语法（call/command/宏调用）不被实体查询捕获时，只实现路径规则，不做降级猜测（Dart/Ruby/C/busted/bats 先例）。
- **AST 形态先验证后实现**：新语言接入前用 `tools/parse_test_detection.rs` 验证节点形态（如 bats `@test` 为 `command`、Kotlin 注解含 `user_type` 包装、Lua 类方法为 `method_index_expression`）。

## 6. 批次 4 关键经验（Kotlin/Lua/Bash）

1. **Kotlin `@Test` 缓冲失效是存量缺陷**：`language_has_annotation_semantics` 已含 Kotlin，但 kotlin scheme 无 annotation 捕获，注解从未进入缓冲通道。修复：`(annotation (user_type (identifier))) @entity.annotation`（bare 与带参注解的 name 均在 `user_type` 内）。修复后 `@Test` 方法被提升为 `TestCase`，Java/Kotlin 共用的注解检测立即生效。
2. **`*Spec` 约定仅限 Kotlin**：Java 无对应生态，保持严格 `*Test`/`*Tests`；`*Spec` 判定经 `language` 参数传入 `detect_conventional`。
3. **`.bats` 映射 `FileType::Source` 而非 `Test`**：`FileType::Test` 在 `file_processor` 路由与 `is_text()` 中未处理，映射为 Test 会导致文件被拒绝/判为二进制；测试分类交给 `TestInfo::from_path` 的 `*.bats` 规则。
4. **grouper 合并与测试边界**：test_info 在 small-fragment 合并**之前**标注（pipeline.rs），合并器按 `is_test()` 边界阻断测试片段与生产片段合并；测试文件内相邻小片段合并为同一 Test 组是正确行为，集成测试断言需按组粒度而非实体粒度编写。
5. **测试文件内的反例实体**（如 `tests/` 下的 `testMode` 函数）会因文件级路径规则被标记为 Test——这是设计内行为（文件本身即测试代码）；反误报的价值体现在**生产文件**中同名实体不被标记（有独立用例覆盖）。

## 7. 验证体系

- 单测：每语言正反例（三层各自）+ `TestInfo::from_path` 一致性（`test_info.rs` 含 14 语言路径规则用例）。
- 全链路集成：`crates/cce_parser/tests/test_info_integration.rs`（真实 parse → extract → group → test_info，覆盖 Rust/Java/C#/Scala/PHP/C++/Lua/Bash/Kotlin）。
- 离线回归：`crates/cce_e2e_tests/tests/regression/test_marker_language_coverage.rs`（NoTest 过滤变体与 `TestDiagnostics` 统计）。
- 质量门：`cargo clippy --all-targets --all-features` + `cargo fmt`。
