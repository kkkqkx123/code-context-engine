# BM25 基线对比分析：direct_chunking 与 full_pipeline（jackson_core、ripgrep、express、flask）

## 1. 数据纠偏与总体结论

以 `aggregate_metrics_top5/10/20/30/50.md` 的 `all + BM25` 为口径，四个项目的实际关系如下：

| 项目 | 小 k（5/10） | 大 k（20/30/50） | 精度与召回结构 |
| --- | --- | --- | --- |
| jackson_core | 基本持平（`R_s 0.53/0.57` 对 `0.51/0.57`） | `full` 召回略超、精度略低（`top30 R 0.68` 对 `0.73`，`P 0.25` 对 `0.24`） | 互有胜负 |
| ripgrep | `full` 占优（`top5 R 0.34` 对 `0.41`，`top10 0.38` 对 `0.43`） | `direct` 反超（`top30 R 0.70` 对 `0.61`） | 小 k 精度胜、大 k 召回负 |
| express | `full` 大胜（`top5 R 0.42` 对 `0.60`，`F1 0.14` 对 `0.21`；`top30 R 0.75` 对 `0.88`） | `full` 全程大胜 | 精度召回双胜 |
| flask | `R_s` 略降（`top5 0.77` 对 `0.71`）但精度大胜（`P 0.31` 对 `0.46`，`F1 0.41` 对 `0.54`），各 k 一致 | 同左 | 以精度换召回，综合占优 |

因此不存在“`full` 在 ripgrep 全面不如 `direct`”或“`full` 在 jackson 全面下滑”的说法。准确表述是统一机制下的两面：`full` 的混合文本提升头部精度、损失深层召回，净效果取决于语料特征。`express/flask` 精度增益压倒召回损失，`ripgrep/jackson` 在大 k 召回损失压倒精度增益。

补充证据：`full_pipeline_raw_source`（同切分、换原文）在各项目 BM25 上约等于或略优于 `full`，说明 BM25 通道的差异主要来自文本内容而非切分边界；而 `emb` 通道上 `raw` 远差于 `full`，说明向量通道的差异主要来自语义改写。两者正交。

## 2. 统一机制：精度上升、深层召回下降

`direct` 的 BM25 文档是实体级源码切片，文档短、词频集中、标题即方法名。`full` 的 BM25 文档是组级混合文本，仅保留组头签名、文档注释与基类信息，成员名、导入类成员、方法体词汇被有意排除，再按词数上限合并多个实体。结合生产 BM25 计分（标题与关键词加权高于正文、长文档归一化惩罚、切分词降权），结果是：

- 头部查询（精确名、同义小改写）得分更集中，`top5` 精度与一级命中上升。
- 体词汇查询（功能描述落在方法体、调用目标、分支注释中）彻底失去词项，深排名也召回不回来；大类合并进一步稀释词频，长文档归一化放大惩罚，`top30` 召回下降。

逐查询证据：`jackson` 的 `FZ-G1Q7`（合并缓冲字符段）在 `full` 各 k 全零、同边界 `raw` 在 `top10` 命中；`G1Q8 findName` 在 `full top5` 为零、`top10` 恢复一半；`ripgrep` 的 `FZ-G1Q1 findAt` 与 `G2Q3/G2Q8/G2Q19/G2Q20` 在 `full top30` 全零、`direct top30` 各命中一条；`express` 仅 `G2Q8` 在 `top5` 单点落后、`top30` 追平；`flask` 仅个别同义词与单条语义在 `top5` 落后、`top30` 仅剩一条语义落后。

## 3. 分项目对比：为何走向不同

### 3.1 jackson_core（Java，大类重载型）

结构化输出为 `405 文件、16857 实体`，`method 3719`，`test` 占 `267 文件、9947 实体`。`JsonParser` 单类上百方法、`ReaderBasedJsonParser` 近三百实体、`ByteQuadsCanonicalizer` 二百实体且 `findName/addName` 各四重载，`NumberInput` 数十个 `parse*` 重载。组级混合文本把重载家族压成一条组头，精确查询失去签名区分度；方法体多为缓冲位运算与通用局变量，体词汇本就噪声大。结果是头部打平、深层互有胜负，测试污染（`top5` 近百次测试命中）进一步抹平差异，`impl_only` 过滤后全面回升。

### 3.2 ripgrep（Rust，特性分散型）

结构化输出为 `88 文件、5228 实体`，`function 2304` 占近半，另有大量 `trait_impl/inherent_impl/struct/enum/macro`。查询一半是语义描述（内存映射、行缓冲、二进制检测、并行遍历），信号恰好在函数体与分支注释中，混合文本丢弃后深层召回塌缩（`top30 semantic 0.37` 对 `0.22`）。精确名查询（`find_at/WalkBuilder/is_match`）反而因签名规整而在小 k 获益。`new/build` 等高频通用名在多构造器间歧义大，组头信息不足以区分，这也是 `qualified` 在大 k 被追平的原因之一。

### 3.3 express（JavaScript，小函数歧义型）

结构化输出为 `141 文件、4640 实体`，`function 2574 + variable 1925`，`method` 仅 `141`。`app.use/handle/send/json/get/render/listen` 等短名高度歧义，源码切片多为一两行动词调用，`direct` 的短文档缺乏上下文，`IDF` 失效。混合文本补足签名、文档与文件路径分组，`qualified 0.5` 对 `0.8`、`fuzzy 0.4` 对 `0.6`、`semantic 0.4` 对 `0.5` 全面上升。测试污染极重（`emb 597/789` 为测试），分组去重附带受益，`impl_only` 下 `full` 优势更大。

### 3.4 flask（Python，文档装饰器型）

结构化输出仅 `35 文件、1637 实体`，`method 288 + function 133`，装饰器与文档字符串丰富（`add_url_rule/register_blueprint/dispatch_request`）。混合文本保留签名与清洗后文档字符串，与语义查询措辞天然对齐，精度跃升（`fuzzy P 0.37` 对 `0.53`，`qualified P 0.5` 对 `0.7`），召回小幅让步（`semantic R 0.55` 对 `0.5`）但综合占优。同边界 `raw` 与 `full` 精度几乎相同，证实增益来自富上下文而非单纯切分。

## 4. 可优化问题与建议

按收益与风险排序，只列可落地的改动，不涉及不可兼得的权衡复述。

1. 组头丢成员名与体关键词（高优）。混合模板为降噪完全排除成员名与方法体，导致 `findName` 重载、`TextBuffer` 体动词、`ripgrep` 语义体词汇丢失。建议在 BM25 内容字段追加有界成员名表与体关键词溢出（按词频截断、低权重），标题与关键词字段保持现状以保住头部精度。
2. 大组自适应拆分（高优）。`max_bm25_words` 固定合并使大类文档过长。对成员数或词数超限的组按成员拆块，对小函数保持合并。`jackson/ripgrep` 的深层召回应直接回升，`express` 的小函数不受影响。
3. 命名变体归一化（中优）。`findAt` 查 `find_at` 在 `full top30` 全零，说明大小写与分隔符变体的切分权重不足。建议在关键词字段补充分隔形式的全量精确项（仅关键词字段，不污染正文词频），或取消标题切分词的降权。
4. 调用与类型信号回填（中优）。关键词抽取的设计意图是调用目标留在正文，但组头模板下正文并无方法体，调用边实际丢失。建议把组内调用目标与参数类型以低权重回填正文，语义类查询可部分恢复，且不冲击精确名排序。
5. 测试块降权与默认口径（中优）。`express` 测试块占四分之三、`jackson` 占近六成，`top5` 大量命中来自测试。建议生产对 `test_case` 单独降权或分索引，基准默认以 `impl_only/no_test` 为主口径、`all` 仅作对照。
6. 语言差异化清洗（低优）。`flask` 靠文档字符串获益，`ripgrep` 的文档注释可能被过度归一。建议按语言分别调文档保留比与冗余短语表，而非全局同一清洗强度。

不建议的方向：单纯调大 `max_bm25_words` 会同时加剧稀释；单纯往混合文本贴回完整源码会退化为 `raw`，丢掉 `express/flask` 的精度增益；对标题字段做同义扩展会污染精确名排序，同义应只进向量通道。

## 5. 复现方式

```shell
cargo test --lib -- --nocapture
cargo test <test_name>
```

基准数据路径：

- `crates/app/cce-e2e-tests/outputs/benchmark/jackson_core/`
- `crates/app/cce-e2e-tests/outputs/benchmark/ripgrep/`
- `crates/app/cce-e2e-tests/outputs/benchmark/express/`
- `crates/app/cce-e2e-tests/outputs/benchmark/flask/`

结构化输出路径：

- `crates/app/cce-e2e-tests/outputs/scenarios/java/structured/jackson-core/SUMMARY.md`
- `crates/app/cce-e2e-tests/outputs/scenarios/rust/structured/ripgrep/SUMMARY.md`
- `crates/app/cce-e2e-tests/outputs/scenarios/javascript/structured/express/SUMMARY.md`
- `crates/app/cce-e2e-tests/outputs/scenarios/python/structured/flask/SUMMARY.md`
