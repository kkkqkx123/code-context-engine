# examples

展示性输出生成器，运行后可生成 `outputs/` 下的文件供人工审查。

## 原则

- 示例**不参与自动断言**，与 `tests/` 严格分离。
- 修改核心逻辑后，运行对应示例观察 `outputs/` 的输出变化。
- 示例不需要符合测试规范（不需要 `#[tokio::test]`、断言等）。

## 目录结构

按语言/场景划分子目录：

- `rust/` — Rust 语言相关的示例（含 `export_rs` 审查导出）
- `python/` — Python 语言相关的示例
- `java/` — Java 语言相关的示例
- `javascript/` — JavaScript 语言相关的示例
- `typescript/` — TypeScript 语言相关的示例
- `go/` — Go 语言相关的示例
- `csharp/` — C# 语言相关的示例
- `kotlin/` — Kotlin 语言相关的示例
- `php/` — PHP 语言相关的示例
- `ruby/` — Ruby 语言相关的示例
- `lua/` — Lua 语言相关的示例
- `dart/` — Dart 语言相关的示例
- `scala/` — Scala 语言相关的示例
- `c/` — C 语言相关的示例
- `cpp/` — C++ 语言相关的示例
- `bash/` — Bash 语言相关的示例
- `multi_language/` — 跨语言场景的示例
- `tools/` — 无状态工具展示示例
- `type_inference/` — 类型推断专项可视化

审查类 `export_*` 示例只覆盖 `fixtures/<lang>/review/` 夹具，产出 NL summary、
emb/bm25 chunk 切分与 structured 符号/关系报告，供人工对照转换结果。
类型推断专项用例走 `export_type_inference`，不重复混入语言 export。

## 命名规范

- 文件名应反映 fixture 名称或场景用途（如 `basic.rs`、`export_rs.rs`、`relation.rs`）。
- 示例名与文件名对应：`rust/basic.rs` 注册为 `rust_basic` 示例。
- 注册在 `Cargo.toml` 中通过 `[[example]]` 条目声明。

## 运行

```bash
# 列出所有可用示例
cargo run --example rust_basic
cargo run --example export_rs
cargo run --example export_py
cargo run --example export_js
cargo run --example export_ts
cargo run --example export_java
cargo run --example export_go
cargo run --example export_cs
cargo run --example export_kt
cargo run --example export_php
cargo run --example export_rb
cargo run --example export_lua
cargo run --example export_dart
cargo run --example export_scala
cargo run --example export_c
cargo run --example export_cpp
cargo run --example export_sh
cargo run --example export_type_inference
cargo run --example java_spring_boot_export
cargo run --example multi_language_basic
```

## 添加新示例步骤

1. 在对应语言子目录下创建 `.rs` 文件
2. 在 `Cargo.toml` 中添加 `[[example]]` 条目
3. 文件顶部用 `//!` 注释说明输出内容
4. 审查导出优先复用 `cce_e2e_tests::review_export::{ReviewExportJob, export_jobs}`，
   避免在各语言示例中复制扫描 / 切分 / structured 输出流水线
