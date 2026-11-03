# 词项概览卡改动前后基准对照

日期：2026-11-04
改动：概览卡只在向量路径生成，词项路径跳过（分块器概览后处理一处按路径过滤）。
对照：`ref/backup` 为改动前，`outputs/benchmark` 为改动后重跑（含导出、数据生成、离线评测）。

## 块形态对照

### ripgrep（三基线块数）

| 基线 | 向量块数（前→后） | 词项块数（前→后） |
|------|------------------|------------------|
| full_pipeline | 2190 → 2190 | 2231 → 2067（约减 164，概览卡清零） |
| raw_source | 2190 → 2190 | 2231 → 2067 |
| direct_chunking | 2109 → 2109 | 1503 → 1412（直接基线同样走概览后处理，约减 91） |

### ripgrep 词项重复文本

| 基线 | 重复文本数（前→后） | 重复占比（前→后） |
|------|-------------------|------------------|
| full | 340 → 12 | 15.2% → 0.6% |
| raw | 49 → 40 | 2.2% → 1.9% |
| direct | 192 → 10 | 12.8% → 0.7% |

空区间改动前后均为零；共享区间 full 34 → 25。

### flask（三基线块数）

| 基线 | 向量块数（前→后） | 词项块数（前→后） |
|------|------------------|------------------|
| full_pipeline | 774 → 774 | 627 → 590（概览卡清零约 37） |
| raw_source | 774 → 774 | 627 → 590 |
| direct_chunking | 576 → 576 | 450 → 423 |

flask 词项重复文本 full 89 → 15（14.2% → 2.5%），direct 71 → 17；空区间均为零。

## 聚合指标对照（R_s，新减旧）

### ripgrep

top10：
| variant | retriever | 基线 | 旧 | 新 | 差值 |
|---------|-----------|------|----|----|------|
| all | BM25 | direct | 0.5222 | 0.5778 | +0.0556 |
| all | BM25_group | direct | 0.6111 | 0.6333 | +0.0222 |
| all | BM25_group | raw | 0.6333 | 0.6111 | -0.0222 |
| impl_only | BM25 | full | 0.5444 | 0.5444 | 持平 |

top30 值得注意：
| variant | retriever | 基线 | 差值 |
|---------|-----------|------|------|
| impl_only | BM25 | direct | +0.0445 |
| impl_only | BM25 | raw | -0.0445 |
| impl_only/all/no_test | BM25_group | full | +0.0111 |

top50 值得注意：
| variant | retriever | 基线 | 差值 |
|---------|-----------|------|------|
| all | BM25 | full | +0.0333 |
| all | BM25_group | full | +0.0333 |
| impl_only | BM25_group | full | +0.0334 |
| impl_only | BM25_group | raw | +0.0222 |
| impl_only/no_test | BM25 | 全基线 | -0.0111 |

top5 仅直接基线向量小幅波动（-0.0222），词项 full 持平。

### flask

top10：
| variant | retriever | 基线 | 旧 | 新 | 差值 |
|---------|-----------|------|----|----|------|
| impl_only | BM25 | direct | 0.8889 | 0.8444 | -0.0445 |
| impl_only | BM25 | full | 0.8889 | 0.8444 | -0.0445 |
| impl_only | BM25 | raw | 0.8667 | 0.8444 | -0.0223 |
| all | BM25 | direct/full | 0.8889 | 0.8222 | -0.0667 |
| all | BM25 | raw | 0.8667 | 0.8444 | -0.0223 |
| all | BM25_group | full/raw | 0.9556 | 0.9778 | +0.0222 |
| all | emb_group | direct | 0.9556 | 0.9778 | +0.0222 |

top5 回退幅度更大（full BM25 impl_only -0.0889，raw -0.1000；分组 full/raw +0.0222）。
top30/top50 收敛：impl_only BM25 direct +0.0445，all BM25 raw +0.0223，all BM25 full（top50）+0.0222；其余持平。向量两路各切口基本持平。

## 逐查询翻转（top10 R_s）

### ripgrep（仅 5 处）

| variant | retriever | 查询 | 基线 | 旧→新 |
|---------|-----------|------|------|-------|
| all | BM25 | FZ-G1Q1-naming_case | direct | 0.0 → 1.0（恢复） |
| all | BM25 | FZ-G1Q9-abbrev_expand | direct | 0.0 → 1.0（恢复） |
| all | BM25 | G2Q13 | direct | 0.0 → 0.5（部分恢复） |
| all | BM25_group | FZ-G1Q1-naming_case | direct | 0.0 → 1.0（恢复） |
| all | BM25_group | FZ-G1Q1-naming_case | raw | 1.0 → 0.0（回退） |

目标 5 查询（impl_only BM25）改动前后完全一致，均未恢复：
G1Q9、FZ-G1Q9、G2Q3、G2Q20 为 direct=1.0、full=0.0、raw=0.0；G1Q1 为 direct=0.0、full=0.0、raw=1.0。

### flask（top10）

回退（未分组词项，切散成员单命中覆盖随卡移除）：
G2Q3（direct/full，impl_only 与 no_test、all 一致掉 1.0 → 0.0）、G2Q17（direct/full/raw 三基线一致掉 1.0 → 0.0）、G2Q11（仅 all direct/full 掉 1.0 → 0.0）。

恢复：
G2Q6（all BM25_group full/raw 0.0 → 1.0）、G2Q19（all emb_group direct 0.0 → 1.0）。

其余为命中数 s_m 上升但 R_s 不变的精度侧变化（如大量查询 s_m 0 → 2/5/6），属片段命中增多、整段覆盖未变。

## 审查结论

* 组级口径（生产相关）无回退：ripgrep 分组词项持平或微升，flask 分组词项 +0.022；向量两路双项目持平。
* 未分组词项小切口存在置换：ripgrep 直接基线恢复 2~3 个查询，flask 切散成员掉 2~3 个查询，大切口收敛，未新增系统性失败。
* 目标 5 查询未动：前 4 个系切分竞争与同切分排名问题，删卡幅度不足以将其推入前十；G1Q1 系纯文本稀释，留作模板治理后续。
