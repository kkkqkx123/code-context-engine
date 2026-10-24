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
