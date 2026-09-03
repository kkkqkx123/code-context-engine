# Medium Project Fixture

## 说明

这是一个用于性能基准测试的中型项目 fixture,目标规模约 1000 个源文件。

## 当前状态

- [ ] 已填充真实代码
- [ ] 文件数量: 0 (目标: ~1000)
- [ ] 总代码行数: 0
- [ ] 包含语言: Rust, Python, JavaScript, TypeScript, Java

## 填充指南

请从以下开源项目中复制代码:

### 推荐项目

1. **Tokio** (Rust - 部分模块)
   - GitHub: https://github.com/tokio-rs/tokio
   - License: MIT
   - 需要复制的目录: `tokio/src/`, `tokio-util/src/` (部分)
   - 文件数: ~300

2. **Serde** (Rust - 核心部分)
   - GitHub: https://github.com/serde-rs/serde
   - License: MIT/Apache-2.0
   - 需要复制的目录: `serde/src/`, `serde_derive/src/`
   - 文件数: ~200

3. **Flask** (Python - 核心模块)
   - GitHub: https://github.com/pallets/flask
   - License: BSD-3-Clause
   - 需要复制的目录: `src/flask/`
   - 文件数: ~150

4. **React Utilities** (JavaScript/TypeScript)
   - React 生态的工具库
   - 或 Next.js 的部分模块
   - 文件数: ~200

5. **Java Spring Utils** (Java)
   - Spring Framework 的工具类
   - 文件数: ~150

### 备选项目

- Axum (Rust web framework)
- FastAPI (Python web framework)
- Express middleware (Node.js)
- Vue.js core modules

## 目录结构建议

```
medium/
├── src/
│   ├── rust_projects/      # Rust 项目 (~500 files)
│   │   ├── tokio_subset/
│   │   └── serde_subset/
│   ├── python_projects/    # Python 项目 (~200 files)
│   │   └── flask_subset/
│   ├── js_ts_projects/     # JS/TS 项目 (~200 files)
│   │   └── react_utils/
│   └── java_projects/      # Java 项目 (~100 files)
│       └── spring_utils/
├── tests/                  # 测试文件 (~100 files)
└── docs/                   # 文档文件 (可选)
```

## 验证清单

填充完成后,请验证:

- [ ] 所有文件可正常解析 (无语法错误)
- [ ] 包含多种实体类型:
  - [ ] Functions (普通函数、方法、异步函数)
  - [ ] Classes/Structs/Traits
  - [ ] Modules/Packages
  - [ ] Interfaces/Enums
  - [ ] Macros (Rust)
  - [ ] Decorators (Python)
- [ ] 包含不同复杂度级别的代码:
  - [ ] 简单工具函数
  - [ ] 中等业务逻辑
  - [ ] 复杂算法和架构
- [ ] 包含设计模式示例:
  - [ ] Builder pattern
  - [ ] Factory pattern
  - [ ] Singleton pattern
  - [ ] Observer pattern
- [ ] 包含测试文件
- [ ] 包含跨文件依赖关系
- [ ] 总文件数接近 1000

## 预期性能指标

填充完成后,预期达到:

- **索引时间**: 2-5 分钟
- **向量数量**: 5000-10000
- **内存使用**: 1-2 GB
- **查询延迟**: < 200ms (P95)

## 多语言特性

确保包含以下语言特性:

### Rust
- Async/await
- Traits and impl blocks
- Macros
- Error handling (Result/Option)
- Pattern matching

### Python
- Classes and inheritance
- Decorators
- Generators
- Type hints
- Async functions

### JavaScript/TypeScript
- ES6+ features
- Async/await
- Classes
- TypeScript interfaces/types
- React components (if included)

### Java (可选)
- Classes and interfaces
- Generics
- Annotations
- Streams API

## 维护说明

- 每半年检查是否需要更新
- 跟踪原始项目的重大版本更新
- 记录任何自定义修改或裁剪

## 许可证

请确保所有复制的代码都使用允许的许可证,并在此处列出:

- Tokio: MIT
- Serde: MIT/Apache-2.0
- Flask: BSD-3-Clause
- [添加其他项目的许可证]

注意: 仅复制源代码用于性能测试,不包含在分发版本中。
