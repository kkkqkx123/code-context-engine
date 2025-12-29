# TreeSitter查询测试完整指南

## 📖 文档导航

本套文档分为以下几个部分，根据你的需要选择阅读：

### 快速参考
- **当前文件** - 整体指南和快速查询
- **[scripts/USAGE.md](./scripts/USAGE.md)** - 脚本参数详细说明
- **[scripts/VALIDATION.md](./scripts/VALIDATION.md)** - 查询一致性校验指南

### 详细教程
- **[scripts/README.md](./scripts/README.md)** - 脚本功能和工作流程
- **[scripts/EXAMPLES.md](./scripts/EXAMPLES.md)** - 真实使用场景和示例
- **[prompt.md](./prompt.md)** - 提示词和自动化调试指南

### 参考资料
- **[api.md](./api.md)** - 测试脚本使用的外部API的完整说明文档

## 🎯 常见任务速查表

| 任务 | 命令 |
|------|------|
| 运行所有C语言测试 | `node src/service/parser/__tests__/scripts/process-test-cases.js  c` |
| 运行某个类别 | `node src/service/parser/__tests__/scripts/process-test-cases.js  c:lifecycle` |
| 运行特定测试 | `node src/service/parser/__tests__/scripts/process-test-cases.js  c:lifecycle:001` |
| 运行多个测试 | `node src/service/parser/__tests__/scripts/process-test-cases.js  c:lifecycle:001,003,005` |
| 验证查询一致性 | `node src/service/parser/__tests__/scripts/validate-queries-consistency.js c lifecycle` |
| 诊断不匹配查询 | `node src/service/parser/__tests__/scripts/diagnose-query-mismatches.js c lifecycle` |
| 查看帮助 | `node src/service/parser/__tests__/scripts/process-test-cases.js  --help` |

## 快速开始

### 步骤1：验证单个查询修改

你修改了 `lifecycle-relationships/tests/test-025/query.txt`，想快速验证：

```bash
node src/service/parser/__tests__/scripts/process-test-cases.js  c:lifecycle:025
```

### 步骤2：查看详细结果

```bash
cat src/service/parser/__tests__/c/lifecycle-relationships/results/result-025.json
```

### 步骤3：验证整个类别

```bash
node src/service/parser/__tests__/scripts/process-test-cases.js  c:lifecycle
```

### 步骤4：验证查询一致性（重要！）

```bash
node src/service/parser/__tests__/scripts/validate-queries-consistency.js c:lifecycle
```

如果失败，诊断具体差异：

```bash
node src/service/parser/__tests__/scripts/diagnose-query-mismatches.js c lifecycle
```

## 📁 项目结构速览

```
src/service/parser/__tests__/
├── scripts/                          # 测试脚本
│   ├── process-test-cases.js        # 主脚本（推荐使用）
│   ├── USAGE.md                     # 参数详细说明
│   ├── README.md                    # 脚本概览
│   ├── EXAMPLES.md                  # 使用示例
│   └── c/
│       ├── process-c-test-cases.js  # (已弃用)
│       └── temp/                    # 临时调试脚本
│
├── c/                               # C语言测试
│   ├── lifecycle-relationships/
│   │   ├── lifecycle-relationships.json      # 索引文件
│   │   ├── tests/
│   │   │   ├── test-001/
│   │   │   │   ├── code.c           # 源代码
│   │   │   │   ├── query.txt        # 查询语句
│   │   │   │   └── metadata.json    # 元数据
│   │   │   └── ...
│   │   └── results/                 # API响应结果
│   │       ├── result-001.json
│   │       └── ...
│   ├── control-flow/
│   ├── control-flow-relationships/
│   ├── data-flow/
│   ├── functions/
│   ├── structs/
│   └── concurrency/
│
├── TESTING_GUIDE.md                 # 本文档
├── prompt.md                        # 提示词和调试指南
└── api.md                           # API文档
```

## 🔑 关键概念

### 参数格式

参数采用三层结构，用冒号分隔：

```
[语言]:[类别]:[序号]
```

#### 示例

```bash
c                      # 所有C语言，所有类别
c:lifecycle            # C语言，lifecycle-relationships类别，所有测试
c:lifecycle:001        # C语言，lifecycle类别，第1个测试
c:lifecycle:001,003,005 # C语言，lifecycle类别，第1、3、5个测试
c:lifecycle c:structs  # 两个类别混合指定
```

### 支持的指定方式

| 方式 | 示例 | 说明 |
|------|------|------|
| 无参数 | `c` | 该语言的所有类别 |
| 全名 | `c:lifecycle-relationships` | 完整的类别名 |
| 前缀 | `c:life` | 类别前缀匹配 |
| 特殊 | `c:all` | 该语言的所有类别 |
| 单个 | `c:lifecycle:001` | 特定的测试用例 |
| 多个 | `c:lifecycle:001,003,005` | 多个测试用例 |

## 🔍 调试工作流

### 问题1：查询无结果

**症状**：`⚠️  空匹配: 1 (查询无结果)`

**诊断步骤**：

```bash
# 1. 查看结果文件
cat src/service/parser/__tests__/c/lifecycle-relationships/results/result-001.json

# 输出如下：
# {
#   "response": {
#     "success": true,
#     "data": [],  # <-- 这里是空的
#     "message": "Query executed successfully but found no matches"
#   }
# }

# 2. 检查代码文件
cat src/service/parser/__tests__/c/lifecycle-relationships/tests/test-001/code.c

# 3. 检查查询文件
cat src/service/parser/__tests__/c/lifecycle-relationships/tests/test-001/query.txt

# 4. 对比查询定义
cat src/service/parser/constants/queries/c/lifecycle-relationships.ts

# 5. 检查AST结构（可选）
# 通过 /api/parse 端点查看代码的AST，与查询模式对比
```

**常见原因**：
- 查询模式与AST结构不匹配
- 捕获组名拼写错误
- 述语条件过于严格
- 代码文件与查询不对应
- 查询模式存在符号闭合问题(尤其是在多层嵌套结构中，很容易缺下括号)

### 问题2：API请求失败

**症状**：`✗ 错误处理测试用例 XXX: Error: connect ECONNREFUSED`

**解决**：

```bash
# 确保后端服务运行在 localhost:4001
npm run dev

# 在另一个终端运行测试
node src/service/parser/__tests__/scripts/process-test-cases.js  c:lifecycle:001
```

### 问题3：索引文件不存在

**症状**：`⏭️  跳过 c:lifecycle (索引文件不存在)`

**解决**：

```bash
# 确认该类别已迁移到新架构
ls src/service/parser/__tests__/c/lifecycle-relationships/

# 应该包含：
# - lifecycle-relationships.json (索引文件)
# - tests/ (测试目录)
# - results/ (结果目录)

# 如果不存在，运行迁移脚本
node scripts/migrate-test-cases.js lifecycle-relationships \
  "src/service/parser/__tests__/c/lifecycle-relationships/c-lifecycle-relationships.json"
```

## 💡 最佳实践

### 1. 修改前验证原状态

```bash
# 修改前运行测试，记录当前通过率
node src/service/parser/__tests__/scripts/process-test-cases.js  c:lifecycle > before.txt

# 修改后
node src/service/parser/__tests__/scripts/process-test-cases.js  c:lifecycle > after.txt

# 对比
diff before.txt after.txt
```

### 2. 逐步调试而非全量测试

```bash
# ❌ 不要这样做 - 修改后立即运行全量测试
node src/service/parser/__tests__/scripts/process-test-cases.js  c

# ✅ 这样做 - 先测试单个相关用例
node src/service/parser/__tests__/scripts/process-test-cases.js  c:lifecycle:001

# ✅ 验证通过后，再测试整个类别
node src/service/parser/__tests__/scripts/process-test-cases.js  c:lifecycle

# ✅ 最后验证整个语言，避免之前有所遗漏
node src/service/parser/__tests__/scripts/process-test-cases.js  c
```

## 🎓 进阶话题

### 扩展支持新语言

需要添加 Python 或 JavaScript 支持？

1. **修改脚本**

```javascript
// process-test-cases.js
const SUPPORTED_LANGUAGES = ['c', 'python', 'javascript'];
const TEST_CATEGORIES = {
  c: [...],
  python: ['comprehensions', 'decorators', 'context-managers'],
  javascript: ['async-await', 'promises', 'closures']
};
```

2. **创建测试结构**
参考prompt.md中创建测试用例的工作流

### 编写性能优化查询

对于大型代码库，优化查询性能很重要：

```
✅ 好的做法
(call_expression
  function: (identifier) @func
  (#match? @func "^(malloc|free)$"))  # 先过滤

❌ 不好的做法
(call_expression
  function: (identifier) @func)
  arguments: (...)
  (#match? @func "^(malloc|free)$")  # 后过滤，检查了所有call_expression

✅ 好的做法
; 字面量查询 - 使用交替模式
[
  (number_literal) @definition.number_literal
  (string_literal) @definition.string_literal
  (char_literal) @definition.char_literal
  (true) @definition.boolean_literal
  (false) @definition.boolean_literal
  (null) @definition.null_literal
] @definition.literal
通过交替查询一次性查询相似模式，提高效率。
复杂查询谨慎使用交替模式，且必须严格使用测试用例验证(而非仅验证符号闭合)，且相似查询少于4个时收益不大，不建议使用交替查询。
```


## 🔗 快速链接

- [脚本USAGE文档](./scripts/USAGE.md) - 参数详细说明
- [脚本README文档](./scripts/README.md) - 脚本功能总览
- [查询一致性校验](./scripts/VALIDATION.md) - 校验脚本使用指南
- [使用示例文档](./scripts/EXAMPLES.md) - 真实场景示例
- [测试架构说明](./TEST_ARCHITECTURE.md) - 新架构详解
- [API文档](./api.md) - API端点说明
- [提示词指南](./prompt.md) - 自动化调试指南

## ❓ 常见问题

**Q: 如何运行所有测试？**

```bash
node src/service/parser/__tests__/scripts/process-test-cases.js  c
```

**Q: 如何只运行失败的测试？**

目前需要手动指定。先运行所有测试找出失败的，然后：

```bash
node src/service/parser/__tests__/src/service/parser/__tests__/scripts/process-test-cases.js  c:lifecycle:003,010,015,020,025
```

**Q: 结果文件保存在哪里？**

```
src/service/parser/__tests__/c/{category}/results/result-{序号}.json
```

**Q: 如何添加新的测试用例？**
建议直接创建md文件，再使用src\service\parser\__tests__\scripts\convert-markdown-to-structure.js直接转为目标结构，使用说明参考src\service\parser\__tests__\scripts\md-convert-README.md

需要精细操作时：
1. 创建目录 `tests/test-XXX/`
2. 添加 `code.c`, `query.txt`, `metadata.json`
3. 更新 `{category}.json` 索引文件
