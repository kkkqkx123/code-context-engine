# 测试脚本快速参考卡

## 基本命令

```bash
# 运行所有C语言测试
node scripts/process-test-cases.js c

# 运行特定类别
node scripts/process-test-cases.js c:lifecycle
node scripts/process-test-cases.js c:control
node scripts/process-test-cases.js c:structs

# 运行特定测试用例
node scripts/process-test-cases.js c:lifecycle:001
node scripts/process-test-cases.js c:lifecycle:025

# 运行多个测试
node scripts/process-test-cases.js c:lifecycle:001,003,005
node scripts/process-test-cases.js c:lifecycle:001,002,003,004,005

# 运行多个类别
node scripts/process-test-cases.js c:lifecycle c:structs
node scripts/process-test-cases.js c:lifecycle c:control-flow c:structs

# 混合指定
node scripts/process-test-cases.js c:lifecycle:001 c:structs:001,002

# 查看帮助
node scripts/process-test-cases.js --help
```

## 参数格式

| 格式 | 示例 | 说明 |
|------|------|------|
| 语言 | `c` | 该语言的所有类别 |
| 语言:类别 | `c:lifecycle` | 该类别的所有测试 |
| 语言:类别:序号 | `c:lifecycle:001` | 特定的测试 |
| 前缀匹配 | `c:life` | 匹配 lifecycle-relationships |
| 多个序号 | `c:lifecycle:001,003` | 逗号分隔 |
| 多个类别 | `c:lifecycle c:structs` | 空格或逗号分隔 |

## 支持的类别

**C语言** (`c`):
- `lifecycle-relationships` (或 `life`, `lifecycle`)
- `control-flow` (或 `control`, `ctrl`)
- `control-flow-relationships` (或 `control-rel`)
- `data-flow` (或 `data`)
- `functions` (或 `func`)
- `structs` (或 `struct`)
- `concurrency` (或 `concurrent`)

## 常见任务

```bash
# 修改了 test-025，快速测试
node scripts/process-test-cases.js c:lifecycle:025

# 修复了某个问题，验证整个类别
node scripts/process-test-cases.js c:lifecycle

# 修改影响多个类别，全部验证
node scripts/process-test-cases.js c:lifecycle c:control-flow c:structs

# 发现多个失败，批量测试
node scripts/process-test-cases.js c:lifecycle:001,010,015,020,025

# 验证没有破坏其他测试，运行全部
node scripts/process-test-cases.js c
```

## 检查结果

```bash
# 查看单个测试的详细结果
cat src/service/parser/__tests__/c/lifecycle-relationships/results/result-001.json

# 查看代码和查询
cat src/service/parser/__tests__/c/lifecycle-relationships/tests/test-001/code.c
cat src/service/parser/__tests__/c/lifecycle-relationships/tests/test-001/query.txt

# 查看元数据
cat src/service/parser/__tests__/c/lifecycle-relationships/tests/test-001/metadata.json

# 统计通过率
grep -c '"success": true' src/service/parser/__tests__/c/lifecycle-relationships/results/*.json
```

## 文件位置

```
代码文件:     src/service/parser/__tests__/c/{category}/tests/test-XXX/code.c
查询文件:     src/service/parser/__tests__/c/{category}/tests/test-XXX/query.txt
元数据文件:   src/service/parser/__tests__/c/{category}/tests/test-XXX/metadata.json
索引文件:     src/service/parser/__tests__/c/{category}/{category}.json
结果文件:     src/service/parser/__tests__/c/{category}/results/result-XXX.json
```

## 编辑工作流

```bash
# 1. 修改查询文件
vim src/service/parser/__tests__/c/lifecycle-relationships/tests/test-001/query.txt

# 2. 快速测试
node scripts/process-test-cases.js c:lifecycle:001

# 3. 检查结果
cat src/service/parser/__tests__/c/lifecycle-relationships/results/result-001.json

# 4. 通过后验证整个类别
node scripts/process-test-cases.js c:lifecycle
```

## 返回码

- `0` - 执行成功（可能有测试失败）
- `1` - 执行异常（参数错误、API不可达等）

## 控制台输出说明

```
✓ 表示该测试通过
✗ 表示该测试失败

⚠️  空匹配 - 查询成功但无匹配结果（可能需要修改查询或代码）
REQUEST_ERROR - API请求失败（检查服务是否运行）
PARSING_ERROR - 查询执行失败（检查查询语法）
```

## 故障排除

| 问题 | 解决 |
|------|------|
| 索引文件不存在 | 运行迁移脚本: `node scripts/migrate-test-cases.js` |
| 空匹配结果 | 检查查询模式和代码是否匹配 |
| 参数不被识别 | 检查类别名拼写，使用 `--help` 查看支持的类别 |

## 文档链接

- [完整指南](./TESTING_GUIDE.md)
- [使用文档](./scripts/USAGE.md)
- [使用示例](./scripts/EXAMPLES.md)
- [脚本说明](./scripts/README.md)
- [架构说明](./TEST_ARCHITECTURE.md)

---

**提示**: 大多数任务只需要 3-5 个最常见的命令，其他需要时查看完整文档。
