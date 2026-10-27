# 字符切分与 UTF-8 边界处理规范

本文档说明项目中对 `&str` 做字节切分时必须遵守的 UTF-8 字符边界规则，以及统一处理方式的约定。分析详情见 `docs/plan/utf8-boundary-splitting-analysis.md`。

## 背景问题

Rust 的 `str` 切片（`&s[..n]`、`&s[a..b]`、`String::truncate(n)`）在索引落在多字节字符（`¡`、CJK、emoji）内部时直接 panic。项目处理大量中文文本（注释、摘要、文档），任何字节算术——`len * ratio`、`pos - window`、`max - 3`、固定上限如 `[..256]`——都可能产生这样的索引。

## 统一规则

1. **边界取整**：任何非字面量的字节索引在切片前必须经过 `cce_utils::text::floor_char_boundary`（向下取整，越界 clamp 到 `len`）或 `ceil_char_boundary`（向上取整，专用于 split 循环中防止 floored 切点等于起点导致死循环）。
2. **直接截断**：仅需按字节预算截断前缀时，优先使用 `truncate_to_byte_len` / `truncate_string_to_byte_len`（内部已走 floor）。
3. **逐字符迭代**：`.chars()` / `.char_indices()` / 收集 `Vec<char>` 后按字符数操作的写法天然安全，能覆盖需求时应优先采用。
4. **两种预算口径并存**：按字节预算（floor 系，面向 LLM token 成本近似）与按字符数预算（chars 系，面向展示宽度）均合理，但函数 doc comment 须注明预算单位，避免调用方混用。
5. **大小写不敏感查找返回原文偏移**：不得对 `to_lowercase()` 后的文本做 `find` 再把字节偏移用于原文。Unicode 小写映射可能改变字节长度（如 `İ`），偏移和长度都会错位；必须使用 `cce_utils::text::find_case_insensitive`，由它返回原文中的起止字节。
6. **候选切点不得越过预算前缀**：先按度量得到“保证不超限”的前缀，再在前缀内部寻找自然边界；任何候选抬高到前缀末尾之外都必须拒绝。允许单字符仍超限时返回错误，不允许为了自然边界而写出超限文本。
7. **发布与调试同一套校验**：所有覆盖、连续性与字符边界检查均为返回错误的发布代码，不依赖断言；调试断言只可作为额外断言存在。

## 测试口径

新增切分逻辑的测试统一包含两条断言：

- 结果的首、尾字节均满足 `is_char_boundary`；
- 使用多字节文本（CJK + emoji）做属性测试，对 `0..=len` 的每个索引验证不 panic 且结果为合法前缀。

现有范本：`chunker/strategies/tokens.rs`、`ast_to_nl/embedding/text_cleaner.rs`、`cce-utils/src/token_estimation.rs`。

## 依赖倒置例外

`infra/llm-suite` 系列 crate 不依赖 core（`cce_utils`），因此在本地复制相同的 floor 语义（见 `llm-suite-codec/src/codecs/helpers.rs` 与 `llm-suite-token/src/estimation.rs`），并以注释锚定与 `cce_utils` 的一致性。此为例外而非新惯例；其余 crate 一律使用 `cce_utils::text`。

## 已修复的历史隐患

以下位置的裸切片已改为经过边界处理（2026-10 修复）：

- `cce-orchestrator/.../ast_diagnosis/strategies/common.rs` — 诊断消息 `&text[..50]`；
- `llm-suite-token/src/estimation.rs` — split 点仅 `debug_assert` 防护，升级为运行时 floor；
- `cce-e2e-tests/src/pipeline_debug.rs` — 两处调试输出截断；
- `cce-e2e-tests/examples/rust/dump_retrieval.rs` — 预览截断。

## 不适用场景

以下形式相似的代码**不是** `&str` 切片，无需处理：二进制字节流切片（simvec/tantivy 的 `&bytes[..N]`）、`Vec<String>` 切片、保证 ASCII 的输入（数字字面量后缀剥离、hex 字符串）、以及索引来自 `char_indices()` 的切片（天然为边界）。



## 直接切片点概况与复现命令

直接切片点的完整清单不内嵌于本文档，避免文档与代码漂移。以下命令可随时复现当前状态，自行判断安全性。

### 复现命令

1. 全部第一方直接切片点（排除 fixtures/tests/tantivy）：

```shell
grep -rnE '&[a-zA-Z_][a-zA-Z0-9_]*\[' --include="*.rs" crates   | grep -v '/fixtures/' | grep -v '/tests/' | grep -v '/tantivy/'
```

2. 手动字节循环且直接切片的高风险文件：

```shell
grep -rln "while .* < .*\.len()" --include="*.rs" crates   | grep -v fixtures | grep -v tests | grep -v tantivy   | while read f; do
      if grep -qE '&(text|source|line|str|cleaned|content|header|rest|name)\[' "$f"; then echo "$f"; fi
    done
```

3. 统计切片点数量与分布：

```shell
grep -rnE '&[a-zA-Z_][a-zA-Z0-9_]*\[' --include="*.rs" crates   | grep -v '/fixtures/' | grep -v '/tests/' | grep -v '/tantivy/'   | cut -d: -f1 | sed 's#/src/.*##' | sort | uniq -c | sort -rn
```

### 当前概况（基于最近一次复现）

- 全部第一方直接切片点数量：660 条。
- 主要分布：`cce-parser`（162 文件级命中）、`cce-codegraph`（99）、`llm-suite-codec`（51）、`simvec`（37）、`cce-orchestrator`（33）、`cce-document`（32）。
- 高风险手动字节循环文件：`index_enrichment.rs`、`control_flow.rs`、`python.rs`、`scala.rs`、`variable_patterns.rs`、`macro_body_call_extractor.rs`、`call_extractor.rs`、`metadata.rs`、`capture/parser.rs`、`common/utils.rs`、`budget.rs`、`segment.rs`、`openapi.rs`、`injection.rs`、`license_detector.rs`、`docker_chunker.rs`、`chunker.rs`、`encoding-guess/stages.rs`、`text.rs`。
- 安全判断依据：`char_indices()` / `.find()` / `.get()` / tree-sitter span / ASCII 扫描得到的下标通常安全；手动字节循环中的任意下标切片需要额外守卫或统一 helper。

### 已迁移状态

- `cce-utils::text`：新增 `prev_char_boundary`、`next_char_boundary`、`char_at`、`slice_clamped`、`slice_exact`。
- `index_enrichment::branch_discriminants`：仅在字符边界推进，切片经 `slice_exact`。
- `control_flow::split_top_level_conjuncts` / `split_top_level_word_conjuncts`：循环加入字符边界守卫。
- `python::truncate_at_header_colon`：加入字符边界守卫。
- `macro_body_call_extractor`、`call_extractor`、`metadata::extract_binary_plus_operands`：加入字符边界守卫。
- `metadata.rs` tree-sitter span 切片：改用 `slice_clamped`。

### 待收敛

- 其余直接切片点按三段式模板逐步迁移：字节数组扫描 ASCII 分隔符 → 边界下标 → 统一 helper 切片。
- tree-sitter 偏移进入字符串处理前统一 `is_char_boundary` 校验。
- 新增切分逻辑必须包含多字节属性测试；CI 对新增直接切片模式做 grep 守卫。


## 代码复用改进分析

### 现状问题

1. **边界逻辑分散**：`floor_char_boundary` / `ceil_char_boundary` 已集中于 `cce-utils::text`，但调用点仍大量直接写 `&s[..n]`、`&s[a..b]`、`truncate`，导致每次新增逻辑都要重新判断边界。
2. **扫描模板不统一**：手动字节循环中，ASCII 分隔符扫描与 `&str` 切片混在一起；部分模块用 `char_indices()`，部分用 `bytes[i]`，风格不一致，容易在复制代码时引入 unsafe 切片。
3. **外部偏移处理不一致**：tree-sitter span 有的通过 `source.get(...)`，有的通过 `.min(len)` 直接切片；后者在异常偏移下仍可能 panic。
4. **llm-suite 例外复制**：`infra/llm-suite` 因依赖倒置复制了 floor 语义，容易与 `cce-utils` 漂移。

### 改进建议

1. **唯一入口**：所有 `&str` 切片必须经过 `cce_utils::text` 的 `floor_char_boundary` / `ceil_char_boundary` / `slice_clamped` / `slice_exact` / `truncate_to_byte_len`；直接 `&s[..n]` 仅允许字面量常量（如 `&bytes[..4]`）。
2. **统一扫描模板**：手动字节扫描应遵循“字节数组扫描 ASCII 分隔符 → 边界下标 → 统一 helper 切片”的三段式，禁止在循环中直接 `text[i..]`。
3. **外部偏移统一规范化**：tree-sitter/capture span 在进入字符串处理前统一做 `is_char_boundary` 校验与修正，优先使用 `source.get(start..end)`。
4. **llm-suite 一致性**：保留复制实现，但以测试锚定与 `cce_utils` 的等价性，避免语义漂移。
5. **防回归**：新增切分逻辑必须包含多字节属性测试；CI 中对新增直接切片模式做 grep 守卫。
