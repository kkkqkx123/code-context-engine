# regression

回归测试，仅做**断言验证**，不写输出文件。

## 适用内容

- 固定 fixture 的结构化结果校验（entity 数量、relation 正确性、call chain 可达性等）。
- 展示输出内容校验（control-flow/behavior sidecar、BM25 关键词标记等）。
- 适合不依赖真实外部服务的场景。

## 已覆盖的 fixture

- `rust/review/index_sidecar` — sidecar 展示内容校验
- `rust/review/relation_demo` — 关系索引断言
- `rust/review/relation_diamond` — 钻石依赖关系断言

另外 `test_marker_language_coverage` 以内联代码覆盖 C#/Dart/Scala/PHP/C++ 的测试标记回归（见 `docs/plan/language_test_coverage.md`）。

`tree_sitter_queries` 按语言覆盖查询回归 fixture（`fixtures/<lang>/query_regression`）：`go`、`csharp`、`rust`、`ruby`、`php`、`c`、`python`、`scala`、`cpp`、`java`、`javascript`、`typescript`、`kotlin`、`lua`、`dart`、`bash`，断言文件级依赖目标、已消除噪声的否定断言与个别解析归属（见 `docs/plan/tests/query-regression-coverage-supplement-2026-11-11.md`）。

## 输出文件生成

对应 `outputs/` 下的展示文件由 `examples/` 下的示例生成：

```bash
cargo run --example rust_basic
cargo run --example rust_export
cargo run --example rust_relation
cargo run --example java_spring_boot_export
cargo run --example multi_language_basic
```

## 约束

- 这里的测试应该尽量稳定、可重复。
- 不要引入需要人工观察才能判断结果是否正确的长流程。
- 不要在这里写性能测试。
