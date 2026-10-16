# C/C++ 语法错误 degraded 处理：grammar vs host-side 决策记录

> 日期：2026-10-16
> 关联：`docs/issue/cpp-header-parse-errors.md`、`docs/plan/cpp-tree-sitter-grammar-fix-plan.md`

## 一、结论

1. 可 grammar 化的缺陷（concatenated_string 起链、offsetof 成员链、CRLF 续行字符串、STDCALL）在 vendored `tree-sitter-c/cpp` grammar 层修正。
2. `extern "C"` 守卫 neutralization 保留在 pre-parse `cpp_interop_guard::shadow_for_parse` 阶段，不迁移到 after-parse。
3. 类型作为宏实参、预处理边界切开结构、文件作用域宏调用三类问题继续保留为 real degraded signals，不做 host-side simple evaluation、blanking 或 after-parse auto-fix。

## 二、分层边界

- pre-parse：只处理语义上“braces 无声明”且可 byte-size-preserving neutralize 的 guard shape（extern C）。
- parser/grammar：修正真正的 CFG grammar 缺陷。
- after-parse：仅做 observation/counting（`has_syntax_errors -> degraded_files` + warning），不修正 AST 或 span。

## 三、为什么不后移 / 不做 host-side 简单求值

- extern C 的 degraded 根因是 preprocessor conditions 下 braces 错配；后移只能基于已失真的错误 tree 修补，最终仍要 re-parse。
- 剩余 degraded 的 correction 需要预处理器或宏求值，blanking 会 mask real signals，并破坏 preproc node 与宏依赖 span。
- after-parse local correction 属于 duplicated fallback logic，与工程 misuse-resistance 与 “real signal” 定位冲突。

## 四、后续约定

- 不在 after-parse degraded 路径中 blank source 或调整 entity counts。
- 若要做更细 triage，只能增加只读 tag/统计，不改变 span 与实体集。
- 任何 future host-side preprocessor-like change 必须单独论证 span 与 entity-set 保持性。

## 五、当前 ERROR 节点处理方式（补充）

### 5.1 解析期（只读诊断，不改变 degraded 定义）
- `pipeline::ast_parsing` 解析后设置 `context.has_syntax_errors`，并写入 `context.syntax_diagnostics`（ERROR / MISSING 的 `start_byte` / `end_byte` / `is_missing`）与 `context.syntax_diagnostic_tag`（C/C++ coarse triage tag）。
- `ParseCoordinator` 保留原有 `parse_detailed` / `parse_with_language_info_detailed` 返回 `bool` 的 API；新增 `*_with_diagnostics` 变体返回 `ParseDiagnostics`。原有调用方行为不变。
- 诊断信息不进入 entity extraction、grouper、chunker，也不改变 `has_syntax_errors` 的判定与 degraded 计数。

### 5.2 编排与报告
- `CompleteFileProcessResult` 增加 `syntax_diagnostics: Vec<SyntaxDiagnostic>` 与 `syntax_diagnostic_tag: Option<String>`，仅作为 in-memory observability。
- batch 层在 `has_syntax_errors` 分支继续 `degraded_files += 1`，同时在 warn 日志中带 `syntax_tag`，便于按 coarse tag 聚合。
- `ast_diagnosis` 的 `ErrorCollector` 继续独立收集 ERROR / MISSING spans 供交互式诊断使用，与索引链路解耦。

### 5.3 回归基线
- `cce-parser/src/parser/diagnostics.rs` 单测覆盖：
  - 畸形 C 源码仍能收集 ERROR 节点；
  - `extern "C"` 守卫经 shadow 后无 syntax error；
  - `list_entry(p, struct X_t, link)` 与 `DEF_FUN_(name, const char* f)` 保持 degraded，证明这三类 real signal 不会被 after-parse 处理吞掉。