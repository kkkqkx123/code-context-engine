# mod.rs 重构工具使用指南

## 概述

本工具用于将 Rust 项目中的传统 `mod.rs` 文件结构转换为现代 Rust 约定：

```
旧式 (传统):
  src/foo/mod.rs           ← 模块入口
  src/foo/bar.rs           ← 子模块文件

新式 (现代):
  src/foo.rs               ← 模块入口 (原 src/foo/mod.rs)
  src/foo/bar.rs           ← 子模块文件 (不变)
```

## 为什么需要重构？

1. **现代 Rust 约定**: Rust 2018+ 推荐使用 `foo.rs` 替代 `foo/mod.rs`
2. **IDE 支持**: 多数 IDE 对 `foo.rs` 的导航更好
3. **减少认知负担**: `mod.rs` 在文件浏览器中难以与子文件区分

## 工具特性

- **智能分析**: 自动检测冲突的内联模块模式 (`foo/foo.rs`)
- **安全执行**: 默认使用 `--dry-run` 预览变更
- **CSV 输出**: 可输出文件映射供后续批量导入路径更新
- **跨 crate 支持**: 可通过包名或目录路径指定目标

## 使用方法

### 基础用法

```bash
# 预览某个 crate 的变更 (不执行)
./tools/mod-refactor.sh -p cce-cli --dry-run

# 执行重构
./tools/mod-refactor.sh -p cce-cli --execute

# 使用目录路径
./tools/mod-refactor.sh crates/cce_core --execute
```

### 带 CSV 输出

```bash
# 执行重构并输出 CSV 文件映射
./tools/mod-refactor.sh -p cce-cli --execute --csv

# 保存映射到文件
./tools/mod-refactor.sh -p cce-cli --execute --csv > file_mapping.csv
```

### 批量处理

```bash
# 遍历所有 crate
for crate in cce-cli crates/cce_core crates/cce_server crates/cce_parser; do
    echo "Refactoring $crate..."
    ./tools/mod-refactor.sh "$crate" --execute
done
```

## 输出示例

### 预览模式

```
==============================================
  mod.rs Refactoring Tool
==============================================
  Target:   cce-cli
  Mode:     DRY RUN

  Found 3 mod.rs files

  ┌─────────────────────────────────────────────────────────────────┐
  │  Changes                                                       │
  └─────────────────────────────────────────────────────────────────┘
  ✓  src/commands/mod.rs
     → src/commands.rs
  ⚠  CONFLICT  src/chunker/chunker/mod.rs
     → already exists: src/chunker/chunker.rs
  ✓  src/config/mod.rs
     → src/config.rs
```

### 执行模式

```
==============================================
  Applying Changes
==============================================
  ✓  src/commands.rs
  ✓  src/config.rs
  ✓  src/utils.rs
```

## 冲突处理

### 内联模块冲突

当出现 `foo/foo.rs` 与 `foo/mod.rs` 共存时，工具会标记为冲突：

```
⚠  CONFLICT  src/chunker/chunker/mod.rs
   → already exists: src/chunker/chunker.rs
```

**解决方案**:
1. **合并**: 将 `mod.rs` 内容合并到 `foo.rs` 中，然后删除 `mod.rs`
2. **重命名**: 重命名现有的 `foo.rs` 为 `foo_core.rs` 等
3. **保留**: 某些第三方库可能需要保持原有结构

### 手动处理示例

```bash
# 1. 查看冲突文件
./tools/mod-refactor.sh -p cce_parser --dry-run

# 2. 手动处理某个冲突
cat src/chunker/chunker/mod.rs >> src/chunker/chunker.rs
rm src/chunker/chunker/mod.rs
```

## 验证步骤

重构完成后，应执行以下验证：

```bash
# 1. 编译检查
cargo check -p cce-cli

# 2. 测试运行
cargo test -p cce-cli --lib

# 3. Clippy 检查
cargo clippy -p cce-cli --all-targets

# 4. 模块完整性检查
./tools/validate-refactor.sh cce-cli
```

## 导入路径更新

重构后，所有 `use` 语句**无需修改**。Rust 会按以下顺序解析模块：

1. `src/foo.rs` (新式)
2. `src/foo/mod.rs` (旧式, 已重命名)

但是，如果使用绝对路径（如 `crate::foo::bar`），也无需更改。

## 常见问题

### Q: 重构后编译失败怎么办？
A: 检查父级 `lib.rs` 或 `main.rs` 中的模块声明。虽然语法不变，但要确保文件存在。

### Q: `git mv` 失败怎么办？
A: 使用 `--execute` 参数时会先尝试 `git mv`，失败则使用 `mv`。

### Q: 需要更新 Cargo.toml 吗？
A: 不需要。模块路径重构不影响包依赖。

### Q: 集成测试文件也需要重构吗？
A: 是的。工具会处理 `tests/` 目录下的 `mod.rs`。

## 技术细节

### 转换规则
- `src/foo/mod.rs` → `src/foo.rs`
- `src/foo/bar/mod.rs` → `src/foo/bar.rs`
- 不修改父级 `mod foo;` 声明（Rust 自动处理）

### 排除路径
工具自动跳过：
- `*/target/*` (构建产物)
- `*/benches/*` (基准测试)
- `*/fixtures/*` (测试数据)
- `*/.git/*` (Git 目录)

### 安全性
- 默认 `--dry-run` 模式只分析不执行
- 冲突文件不会被自动重命名
- 支持 `git mv` 以保持版本历史

## 项目集成

### CI/CD 检查

可在 CI 中添加检查，防止项目退回到 `mod.rs` 模式：

```yaml
# .github/workflows/check-mod-rs.yml
name: Check mod.rs files
on: [push, pull_request]
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Check for mod.rs files
        run: |
          MOD_RS_COUNT=$(find . -name "mod.rs" -not -path "*/target/*" -not -path "*/.git/*" | wc -l)
          if [ "$MOD_RS_COUNT" -gt 0 ]; then
            echo "Found $MOD_RS_COUNT mod.rs files. Use modern foo.rs structure instead."
            exit 1
          fi
```

### IDE 配置

重构后，建议更新 IDE 设置以利用新结构：

```json
// VS Code rust-analyzer
"rust-analyzer.cargo.features": "all",
"rust-analyzer.checkOnSave": true,
"rust-analyzer.imports.granularity.enabled": true
```

## 进度跟踪

### 已完成
- ✅ cce-cli: 1/1 files refactored
- ✅ cce-e2e-tests: 8/8 files refactored
- ✅ 工具脚本完善

### 待完成
- cce_server: 7 mod.rs files
- cce_core: 11 mod.rs files
- cce_infrastructure: 22 mod.rs files
- cce_orchestrator: 27 mod.rs files
- cce_parser: 49 mod.rs files (有冲突需特殊处理)

### 总计
213 个 `mod.rs` 文件，已完成 9 个（~4.2%）