# 编码检测方案调研

## 背景

code-context-engine 在索引非 UTF-8 源文件（如 GBK 中文注释的 C/C++ 文件）时，现有手写启发式检测器存在两个问题：

1. **误拒**：正确识别出 GBK 但因置信度公式缺陷（density 惩罚）被 hard-error 拒绝，导致文件被丢弃。
2. **误判**：对稀疏 CJK 内容（11 个含 GBK 标点的文件）全部判为 WINDOWS-1252，产生静默乱码。

调研目标：确定编码检测的技术路线，为 `encoding-guess` crate 的设计提供依据。

## Rust 生态候选方案

| 库 | 许可 | 最近更新 | 核心问题 |
|---|---|---|---|
| `chardetng` 1.0.0 | Apache-2.0 OR MIT | 2026-03 | 维护频率低（1.0.0 后未发新版）；稀疏 CJK 实测准确率 7/11；`guess()` 无置信度输出 |
| `chardet` 0.2.4 | LGPL-3.0 | 2017-09 | 许可有毒（copyleft），与 Apache-2.0 不兼容；9 年未更新 |
| `uchardet` 2.0.4 | Unlicense | 2017-12 | 9 年仅 90 行 FFI，绑定外部 C++ 库；未跟进上游改进 |
| `detone` 1.0.1 | Apache-2.0 OR MIT | 2024-07 | 不是编码检测器（越南语声调分解库） |

**结论**：Rust 生态中没有可直接采用的现成编码检测库。要么维护停滞、要么许可有毒、要么准确率不足。

## 权威参考算法

### chardet (Python) — 12 阶段流水线

目前最成熟的编码检测算法（宣称 98.2% 准确率）。其核心思路：

| 阶段 | 说明 | 对代码索引器的适用性 |
|---|---|---|
| 1. BOM 检测 | 精确匹配 UTF-8/16/32 BOM | 适用 |
| 2. UTF-16/32 模式 | 空字节交错检测 | 适用 |
| 3. 转义序列 | ISO-2022, HZ-GB-2312, UTF-7 | 适用 |
| 4. Magic Number | 二进制文件格式检测 | 适用 |
| 5. 二进制检测 | 控制字符 > 1% 判定为二进制 | 适用 |
| 6. Markup Charset | HTML/XML/Python 编码声明 | 适用 |
| 7. ASCII 检查 | 全可打印 ASCII | 适用 |
| 8. UTF-8 验证 | 多字节序列结构 | 适用 |
| 9. 字节有效性过滤 | 逐个候选解码排除 | **缺失 — 应添加** |
| 10. CJK Gating | 4 门控过滤假 CJK | **缺失 — 关键改进** |
| 11. 结构探测 | lead/trail byte 模式 | **已有但粗糙 — 需加强** |
| 12. 统计评分 | bigram 频率模型 | **缺失 — 可选** |

### CJK Gating 的 4 个门控（关键设计参考）

1. **structural pair ratio >= 0.05** — valid_pairs / lead_bytes
2. **non-ascii count >= 2** — 至少 2 个高字节
3. **byte coverage >= 0.35** — 多字节序列中的非 ASCII 字节 / 总非 ASCII 字节
4. **lead diversity >= 4**（当 non-ascii >= 16 时）

### charset_normalizer (Python) — Mess/Coherence 两步法

- **Mess（混乱度）**：用候选编码解码后观察到的"混乱"程度
- **Coherence（连贯性）**：每种语言的字母频率排名

### uchardet (C++) — Mozilla Universal Charset Detector

Mozilla 出品，基于统计和字符分布，广泛用于邮件客户端等场景。但算法细节未公开文档化，且 Rust 绑定已停滞。

## 设计决策

### 技术路线

参考 chardet 的 12 阶段流水线，在 Rust 中实现精简版：

- **保留**：BOM、UTF-16/32 模式、ASCII、UTF-8 验证
- **新增**：字节有效性过滤、CJK Gating（4 门控）、结构探测（每种编码独立分析器）
- **简化**：统计评分使用轻量级字符频率表，不引入 1.5MB 模型文件
- **省略**：Markup Charset（代码文件无需 HTML 解析）、Magic Number（由调用方负责二进制过滤）

### 支持的编码集

核心集（代码文件常见）：
- UTF-8, UTF-16LE, UTF-16BE
- ASCII
- GBK, GB18030, Big5, Big5-HKSCS
- Shift_JIS, EUC-JP, EUC-KR
- Windows-1252, ISO-8859-1

### 输出

- `GuessResult { encoding: &'static str, confidence: f64 }`
- 置信度仅用于排序，不做 hard-error 拒绝（由调用方决定阈值）

### 远程仓库

https://github.com/kkkqkx123/encoding-guess
