**任务：** 创建和验证TreeSitter查询测试用例

注意：所有命令从根目录出发

开始工作前查看src\service\parser\__tests__\TESTING_GUIDE.md，其他文档后续按需读取。

## 1. 创建测试用例

为特定文件的特定类别创建测试用例，例如src\service\parser\constants\queries\c\data-flow.ts创建测试用例，格式参考src\service\parser\__tests__\c\concurrency\c-concurrency-queries-test-cases.md。
之后使用命令把md文档转为结构化的文件夹：
```powershell
node src\service\parser\__tests__\scripts\convert-markdown-to-structure.js <原md文件绝对路径> <目标绝对路径>
```
目标路径使用src\service\parser\__tests__\c\concurrency的形式。具体说明见src\service\parser\__tests__\scripts\md-convert-README.md(仅在需要时查看)

使用Markdown文件+命令的形式是为了方便你快速写入所有测试用例。

生成的文件结构为：

**文件结构**
```
src/service/parser/__tests__/{language}/{category}/
├── {category}.json              # 索引文件（引用式）
├── tests/
│   ├── test-001/
│   │   ├── code.c               # 源代码文件
│   │   ├── query.txt            # TreeSitter查询语句
│   │   └── metadata.json        # 元数据
│   ├── test-002/
│   │   ├── code.c
│   │   ├── query.txt
│   │   └── metadata.json
│   └── ...
└── results/                      # API响应结果
    ├── result-001.json
    ├── result-002.json
    └── ...
```

### 代码文件
正常的代码文件格式。文件后缀名视语言而定。

### 索引文件格式 ({category}.json)
```json
{
  "category": "lifecycle-relationships",
  "totalTests": 30,
  "requests": [
    {
      "id": "lifecycle-relationships-001",
      "language": "c",
      "codeFile": "tests/test-001/code.c",
      "queryFile": "tests/test-001/query.txt",
      "metadataFile": "tests/test-001/metadata.json",
      "description": "malloc/calloc/realloc测试"
    }
  ]
}
```

### 元数据文件格式 (metadata.json)
```json
{
  "id": "lifecycle-relationships-001",
  "language": "c",
  "description": "malloc/calloc/realloc测试",
  "category": "lifecycle-relationships",
  "expectedMatches": 4
}
```

## 2. 验证查询
执行统一测试脚本处理所有测试类别。建议一次测试特定语言的特定类别，参考：
```powershell
node src\service\parser\__tests__\scripts\process-test-cases.js c:lifecycle
```

注意：该脚本使用的是外部api，你可以默认该api已经开始提供服务。服务位于4001端口，提供api/health用于健康检查，api/parse用于解析代码块，api/parse/batch用于批量解析。如果执行失败且api端点测活失败，结束任务并要求我添加。

此脚本会：
- 自动扫描所有测试类别的索引文件
- 从tests/test-XXX/目录加载code和query文件
- 向API发送请求并保存结果到results/目录
- 生成测试执行总结报告

## 3. 根据失败情况修复问题

### 问题诊断
如果出现 "Query executed successfully but found no matches"：
1. **验证代码语法** - 使用API parse端点检查AST：
   ```bash
   POST /api/parse
   {
     "language": "c",
     "code": "..."  // 检查从code.c加载的代码
   }
   ```

2. **检查查询语法** - 使用验证脚本：
   ```bash
   node src/service/parser/__tests__/scripts/validate-queries.js \
     src/service/parser/__tests__/c/{category}/tests/test-XXX/query.txt
   ```

3. **对比查询模式定义** - 检查src\service\parser\constants\queries\c\*.ts中的定义

### 临时测试脚本
创建用于调试特定问题的脚本：
```
src/service/parser/__tests__/scripts/c/temp/
├── test-single-query.js        # 测试单个查询
├── test-language-detection.js  # 测试语言检测
└── ...
```
已有脚本包括src\service\parser\__tests__\scripts\validate-queries.js，src\service\parser\__tests__\scripts\debug-ast-structure.js

需要修改时修改测试代码与查询模式，并同步修改查询常量定义文件。

**顽固错误预计需要使用专门的脚本了解具体解析结构，参考src\service\parser\__tests__\scripts\debug-ast-structure.js(使用外部api的api/parser端点，api文档见src\service\parser\__tests__\api.md)**

批量修改测试用例时建议使用脚本


## 验收标准

### 第一步：测试通过
✅ node src\service\parser\__tests__\scripts\process-test-cases.js 测试特定内容后：
- 所有supported类别的results/目录都包含result-XXX.json文件
- 每个result文件的response.success为true
- response.data数组非空（有匹配结果）
- 没有REQUEST_ERROR或PARSING_ERROR

通常出现"Query executed successfully but found no matches"就代表部分查询不成功。

### 第二步：查询优化（可选）
- 检查哪些查询可以改造为交替查询，并修改src\service\parser\constants\queries中相应的常量定义文件。参考下方说明。
- 修改后需要重新运行完整流程，确保所有校验仍然通过

### 第三步：查询一致性校验（必须执行）
✅ 必须运行一致性校验脚本，确保测试用例与查询常量完全一致：
```powershell
node src\service\parser\__tests__\scripts\validate-queries-consistency.js <language> <category>
```

示例：
```powershell
node src\service\parser\__tests__\scripts\validate-queries-consistency.js c lifecycle
```

校验脚本会检查：
- 测试用例中的所有query.txt是否在对应的常量定义文件中存在
- 常量定义文件中的所有查询是否都被测试用例使用
- 任何不匹配都会详细报告，包括相似度分析

近一步诊断可以使用`src\service\parser\__tests__\scripts\diagnose-query-mismatches.js`

**此步骤失败则整个验收失败**


**交替查询改造工作流**
分析当前测试的查询常量定义，例如 src/service/parser/constants/queries/c/structs.ts ，分析哪些查询模式语法高度相近的查询可以合并为一个交替查询，以提高查询效率，并作出修改。
然后参考src\service\parser\__tests__\scripts\c\temp\test_alternation_queries.js使用一个测试用例来验证修改后的交替查询(不需要运行完整测试，只有保证每处改造没有破坏语法即可。理论上除了语法问题不存在其他问题)。
