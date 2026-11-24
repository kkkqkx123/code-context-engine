# cce-e2e-tests

`cce-e2e-tests` 是代码上下文引擎的集成测试支持包。共享测试能力放在库 crate 中，测试入口按目的分层放在 `tests/` 下。

## 目录职责

- `src/`
  - 共享测试工具，包括 fixture、输出写入、断言和轻量级测试封装。
- `examples/`
  - 展示性输出生成器。运行示例可生成 `outputs/` 下的文件供人工审查。
  - **不参与断言**，与 `tests/` 分离以避免测试与输出去耦。
  - 按语言划分子目录：`rust/`、`java/`、`multi_language/`（跨语言场景）。
  - 子目录中的示例需在 `Cargo.toml` 中声明 `[[example]]` 条目。
- `tests/`
  - 集成测试入口和场景模块。
  - `regression.rs` 是回归测试入口（仅做断言，不写输出文件）。
  - `workflow.rs` 是工作流测试入口，场景和支撑模块放在 `tests/e2e/`。
  - `benchmark/` 仅保留索引效果对比说明，不放性能测试实现。
- `crate root fixtures/`
  - 唯一的夹具树，供 `src/`、`examples/`、`tests/` 共用（集成测试复用同一套库加载器，不再保留单独副本）。
- `outputs/`
  - 由 `examples/` 生成，供人工审查。

## 模块位置规范

- 回归测试放在 `tests/regression/`，不要再散落在根目录。
- 工作流场景放在 `tests/e2e/`，这里是内部支持树，不要把回归测试塞回这里。
- `tests/benchmark/` 只放对比说明，不放性能测试代码。
- 只有 crate 根目录 `fixtures/` 一套夹具树；`tests/` 经由 `src/fixture.rs` 的库加载器加载，不保留单独副本。

## 入口约定

- `tests/workflow.rs` 负责加载整个工作流模块树。
- `tests/regression.rs` 负责加载回归测试模块树。
- 回归测试仅做断言验证，不写输出文件。
- 输出文件由 `examples/` 下的示例生成，供人工审查产出变化。

## 运行方式

```bash
# 回归断言（无输出写入）
cargo test -p cce-e2e-tests --test regression -- --nocapture

# 工作流测试
cargo test -p cce-e2e-tests --test workflow -- --nocapture

# 生成展示输出（运行示例）
cargo run --example rust_basic
cargo run --example rust_export
cargo run --example rust_relation
cargo run --example java_spring_boot_export
cargo run --example multi_language_basic
```

## 说明

- 代码文件和注释使用英文。
- 项目相关文档使用简体中文。
- 输出文件不参与自动断言，修改核心逻辑后请运行对应示例以观察输出变化。
