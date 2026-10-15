# C/C++ 头文件解析报错问题分析

问题编号：ISS-2026-10-15-001
状态：已修复（含两个改动，见 §5）
影响范围：所有 C/C++ 项目索引；`cargo run --example gen_bench_media_server` 等 bench 生成链路

## 1. 问题现象

执行 `cargo run --example gen_bench_media_server` 时，Phase 1 对每个文件都输出

```
WARN Parsed with syntax errors; extracted entities may be incomplete
     file=libmov/include/mp4-writer.h language=C
```

且远不止这几个文件。以 `fixtures/cpp/review/media-server` 为例，527 个 C/C++ 文件里有 138 个被打上 degraded 标记，日志被同类告警淹没，无法从中分辨真实的解析失败。

## 2. 根因一：`extern "C"` 互操作守卫让条件嵌套整体错位

真实世界的 C/C++ 公开头文件几乎都写成这样：

```c
#ifdef __cplusplus
extern "C" {
#endif

/* 声明 */

#ifdef __cplusplus
}
#endif
#endif /* !_foo_h_ */
```

当前固定的 tree-sitter grammar（`tree-sitter-c 0.24.2` / `tree-sitter-cpp 0.23.4`，均已是最新发布版）无法解析该形状，失败路径如下：

1. `extern "C" {` 被识别为 `linkage_specification`，其 body 是**按花括号配对的 `declaration_list`**；
2. 该列表内部遇到的下一个 `#endif` 被通用的 `preproc_call` 规则（`preproc_directive` 匹配任意 `#word`）吞掉，而不是终止外层条件块；
3. 从这一点起整个条件嵌套错位一格：tree-sitter 补出一个零宽度的 `MISSING #endif` 节点，且 `#ifdef __cplusplus` 节点一路吞到文件末尾。

后果有两层。表层是 `has_error()` 为真、告警刷屏；深层是所有预处理条件节点的 span 失真（`#ifdef` 节点横跨整个文件后半部分），下游依赖 `preproc_ifdef`/`preproc_if` 的宏依赖识别全部拿到错误区间。

实测影响面：media-server fixture 中 138 个报错文件里有 78 个的唯一成因就是这一条，其中包含用户报告的 4 个文件。

有一个反直觉的结论需要记录：该缺陷**不影响实体抽取**。用守卫存在与守卫剔除后的两份源码分别跑完整管线，实体名与实体类型集合完全一致（libmov/include 213 个实体逐一相同）。损坏集中在预处理结构上。

## 3. 根因二：`.h` 被无条件判为 C

`builtin_language_for_extension` 把 `.h` 映射到 `Language::C`。`.h` 到底是 C 头还是 C++ 头，仅凭扩展名无法判断，真实项目里两者都大量存在。用 C grammar 解析 C++ 头会得到错误结构，例如 media-server 的 154 个 `.h` 文件里 19 个因此报错。

典型误判见 `librtsp/test/rtp-udp-transport.h`：按 C 解析产出 `Function/Init`、`Variable/m_rtp`（成员函数识别为自由函数、成员变量识别为全局变量）；按 C++ 解析产出 `Class/RTPUdpTransport`、`Constructor/~`、`Method/Send`、`Field/m_rtp`。逐文件核对，没有任何符号在这 19 个文件上因改用 C++ 而丢失。

支持改用 C++ 的两点结构性依据：

- `tree_sitter_query/scheme/cpp.rs` 的 `entity_query()` 第一行就是 `query.push_str(c::entity_query())`，`dependency_query()` 同理；C++ scheme 是 C scheme 的超集，不会漏 C 的 pattern；
- C++ grammar 覆盖了 C 的共享节点类型，纯 C 头文件用 C++ 解析结果一致（154 个 `.h` 中有 135 个实体结果与按 C 解析完全相同）。

反向验证 `.c` 不能动：291 个 `.c` 文件按 C 解析 15 个报错，按 C++ 解析 18 个报错，C++ 反而略差。因此只改 `.h`，`.c` 维持 C。

## 4. 剩余报错文件清单与归因（32 个）

在应用 §5 两处修复后，遍历 `crates/app/cce-e2e-tests/fixtures` 下全部 540 个 C/C++ 文件，仍有 32 个 degraded，全部集中在 `cpp/review/media-server`，共 7 类根因。

| 根因 | 机制 | 文件 |
|---|---|---|
| 类型作为宏实参 | `list_entry(p, struct X_t, link)` 的第二个实参是类型而非表达式，tree-sitter 无法把类型作为函数实参 | `libdash/src/dash-mpd.c`、`libhls/source/hls-m3u8.c`、`librtmp/aio/aio-rtmp-transport.c`、`libsip/src/uac/sip-uac.c`、`libsip/src/uas/sip-uas.c` |
| `offsetof` 成员链 | grammar 有 `offsetof_expression` 规则，但第二实参为 `a.b.c` 成员链时仍失败 | `libmkv/src/mkv-reader.c` |
| 预处理边界切开结构 | tree-sitter 不求值条件，`#if`/`#else`/`#endif` 边界内外的语句、表达式、花括号无法配对 | `libmkv/src/mkv-reader.c`、`librtsp/source/client/rtsp-client-play.c`、`rtsp-client-record.c`、`librtp/payload/rtp-av1-unpack.c`、`rtp-payload-helper.c`、`libmpeg/source/mpeg-packet.c`、`librtsp/test/rtsp-client-input-test.cpp`、`rtsp-client-push-test.cpp`、`rtsp-client-test.c` |
| 调用约定修饰符 | 两个 grammar 都不识别 `STDCALL` | `libhls/demo/hls-server.cpp`、`librtmp/test/` 下 4 个、`librtp/test/rtp-receiver-test.c`、`librtsp/test/media/vod-file-source.h`、`libsip/test/` 下 4 个 |
| 文件作用域宏调用 | `DEF_FUN_*(name, const char* f)` / `SDP_UTIL_*(attr, NONE)` 在文件作用域出现"调用表达式 + 类型实参或无分号"，不是合法声明 | `test/test.cpp`、`librtsp/source/sdp-options.c` |
| 字符串与宏标识符相邻拼接 | tree-sitter-cpp 缺少 C 的 `concatenated_string` 规则，`printf("…" PRId64 "…")` 解析失败 | `libmpeg/test/mpeg-ps-dec-test.cpp`、`librtsp/test/media/ps-file-reader.cpp`、`librtp/test/rtp-dump-test.cpp`、`librtsp/test/rtsp-demuxer-test.cpp`、`libsip/test/sip-uac-test2.cpp`、`sip-uas-test2.cpp` |
| CRLF 续行字符串 | grammar 的 string token 不认 `\`+`\r\n`，CRLF 文件中的反斜杠续行字符串字面量解析失败 | `librtsp/test/sdp-test.cpp` |

分类说明：`offsetof` 与 `concatenated_string` 两类是纯 grammar 缺陷，需要上游修复，本仓库无法解决；类型作实参、预处理边界、文件作用域宏调用三类源于"非展开宏 + 无预处理器"在 CFG grammar 下的固有歧义；`STDCALL` 与 CRLF 续行属于编译器专有扩展。这 32 个告警是真实信号，应保留。

## 5. 修复内容

### 5.1 新增 `cpp_interop_guard` shadow 源码

新增 `crates/parser/cce-parser/src/parser/cpp_interop_guard.rs`，在 C/C++ AST 解析前生成一份**字节长度不变**的源码副本，把成对守卫中的两行（`extern "C" {` 与配对的 `}`）替换为等长空格。

设计要点：

- 字节偏移、行号、列号完全不变，shadow 树上的 span 依然精确索引到原始源码，实体与 chunk 文本保持原字节，`extern "C" {` 原文不会从索引内容中消失；
- 只处理 `#ifdef __cplusplus` 与 `#if defined(__cplusplus)` / `#if defined __cplusplus` 三种写法，容忍行首缩进与 CRLF；
- **全有或全无**：必须同时匹配到开、闭两半才改写。未识别或不平衡的文件原样交给 grammar，避免把"降级树"升级成"硬错误"；
- 所有条件指令保留原位，预处理结构及其宏依赖不受影响。

接入点为 `pipeline::ast_parsing`：用 `shadow_for_parse` 的返回值替代 `context.source()` 喂给 parser，`context.source()` 本身不变。

### 5.2 `.h` 统一按 C++ 解析

`crates/core/cce-types/src/types/language.rs` 的 `"h"` 映射由 `Language::C` 改为 `Language::Cpp`，`FileType::Header` 不变。同步修正 `cce-parser/src/parser/language_detector.rs` 的 `test_detect_c_files` 断言。

`.c` 维持 `Language::C`（反向数据见 §3）。

## 6. 效果

| 阶段 | 报错文件数 |
|---|---|
| 初始 | 138（仅 media-server） |
| 应用 5.1 | 50 |
| 应用 5.2 | 32 |

剩余 32 个均为 §4 所列的真实语法缺口。`test_cpp_basic_resolves_header_source_calls` 的 `calculator.h` 不再产生告警。

## 7. 验证记录

- `cce-parser` 单测 1236 通过（含 `cpp_interop_guard` 12 个单测与 coordinator 端到端回归测试）、集成测试通过；
- `cce-codegraph` 1029 个测试通过；`cce-types` 201 个单测通过；
- `cargo clippy`（cce-types / cce-parser / cce-codegraph / cce-orchestrator）无警告，`cargo fmt --check` 通过；
- 逐文件比对确认：应用修复后**没有新增任何 degraded 文件**；
- `cce-e2e-tests` 的 46 个回归失败集合与修复前完全一致，均为缺少 embedder / 关系索引等基础设施依赖导致，非本次引入。

## 8. 后续可选项（未实施）

1. upstream grammar 修复 `offsetof` 成员链与 `concatenated_string` 后，可移除对应测试基线；
2. 若上游发布修复 `extern "C"` 守卫的版本，`cpp_interop_guard` 可退场（其存在与其他逻辑无冲突，先留着无害）；
3. `STDCALL`、CRLF 续行字符串若成为高频噪声，可考虑在 shadow 阶段一并中和，但需要先确认改写不会引入硬错误；
4. `.h` 改判 C++ 后，跨文件类型推断链路建议补一个"C 头 + C++ 源"的专项回归。
