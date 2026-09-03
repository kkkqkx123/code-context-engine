# Small Project Fixture

## 说明

这是一个用于性能基准测试的小型项目 fixture,目标规模约 100 个源文件。

## 当前状态

- [ ] 已填充真实代码
- [ ] 文件数量: 0 (目标: ~100)
- [ ] 总代码行数: 0
- [ ] 包含语言: Rust, Python, JavaScript, TypeScript

## 填充指南

请从以下开源项目中复制代码:

### 推荐项目

1. **once_cell** (Rust)
   - GitHub: https://github.com/matklad/once_cell
   - License: MIT/Apache-2.0
   - 需要复制的目录: `src/`, `tests/`
   - 文件数: ~15

2. **fastrand** (Rust)
   - GitHub: https://github.com/smol-rs/fastrand
   - License: MIT/Apache-2.0
   - 需要复制的目录: `src/`, `tests/`
   - 文件数: ~10

3. **small Python utilities**
   - 选择几个小型 Python 工具包
   - 每个包 10-20 个文件
   - 确保许可证允许使用

4. **TypeScript utility functions**
   - Lodash 的部分模块
   - 或类似的工具函数库
   - 选择 20-30 个文件

### 备选项目

如果上述项目不合适,可以考虑:

- 其他小型 Rust crates (< 500 stars)
- Python 标准库的部分模块
- 前端工具函数集合

## 目录结构建议

```
small/
├── src/
│   ├── rust_code/          # Rust 源文件 (~30 files)
│   ├── python_code/        # Python 源文件 (~30 files)
│   ├── js_code/            # JavaScript 源文件 (~20 files)
│   └── ts_code/            # TypeScript 源文件 (~20 files)
├── tests/                  # 测试文件 (~10 files)
└── docs/                   # 文档文件 (可选)
```

## 验证清单

填充完成后,请验证:

- [ ] 所有文件可正常解析 (无语法错误)
- [ ] 包含多种实体类型:
  - [ ] Functions (普通函数、方法)
  - [ ] Classes/Structs
  - [ ] Modules/Packages
  - [ ] Interfaces/Traits
  - [ ] Enums
- [ ] 包含不同复杂度级别的代码:
  - [ ] 简单函数 (< 10 行)
  - [ ] 中等复杂度 (10-50 行)
  - [ ] 复杂逻辑 (> 50 行)
- [ ] 包含测试文件
- [ ] 包含文档字符串/注释
- [ ] 总文件数接近 100

## 预期性能指标

填充完成后,预期达到:

- **索引时间**: < 30 秒
- **向量数量**: < 500
- **内存使用**: < 500 MB
- **查询延迟**: < 100ms (P95)

## 维护说明

- 每季度检查是否需要更新
- 如果原始项目有重大更新,考虑同步
- 记录任何自定义修改

## 许可证

请确保所有复制的代码都使用允许的许可证 (MIT, Apache-2.0, BSD 等),并在此处列出:

- once_cell: MIT/Apache-2.0
- fastrand: MIT/Apache-2.0
- [添加其他项目的许可证]
