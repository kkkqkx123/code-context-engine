# 内容分类与路由架构

本文档说明代码库索引过程中的内容分类体系与处理路由机制，是
`ContentRoute` 统一化重构及分类体系收尾（新增 `Other`、Schema 进入路由层、
判定入口单源化）后的权威描述。

## 三层分类体系

项目维护**三层**单向派生的分类类型：

| 层级 | 类型 | 位置 | 职责 |
| ---- | ---- | ---- | ---- |
| 路由层 | `FileType` / `ContentRoute` | `cce_core::types::language` | 决定文件走 AST 管线还是文档管线 |
| 业务层 | `FileCategory` | `cce_core::types::ast_to_nl::file_category` | 摘要生成、重要性评分、查询过滤的主键 |
| 块级负载 | `ChunkContentType` | `cce_core::types::ast_to_nl::chunked` | 描述单个 chunk 的负载形态（语言 / 格式） |

### 单一数据源约定

所有派生方向自上而下、单向收敛，**唯一判定入口是
`LanguageInfo::detect_from_path()`** 及其两个派生方法：

1. `LanguageInfo::detect_from_path()` 是唯一的路径 → 路由信息判定入口
   （内部经 `builtin_language_for_extension` 静态表）；
2. `LanguageInfo::file_category()`（即 `FileCategory::from_file_type`）是唯一的
   `FileType` → `FileCategory` 映射：
   - `Source|Header => Code`
   - `Config => Config`
   - `Documentation => Documentation`
   - `Schema => Schema`（路由层原生变体，无任何覆盖补丁）
   - `Text => Other`（日志、`.txt`、未知扩展名等通用文本不再冒充 Code）
3. `LanguageInfo::chunk_content_type(_for_path)()` 由同一 `FileType`
   派生块级负载；schema 文件与 `for_schema` 一致地取 `Document` 负载；
   `Config { format }` 的 format 由路径计算且永不为空串
   （有扩展名取小写扩展名；`Makefile`/`Dockerfile` 固定 `make`/`docker`；
   其余为 `other`）。

配套入口 `FileCategory::determine_from_path(path)` 是对上述链条的纯委托，
仅作为"只持路径时的复原入口"；任何新增消费方都应直接使用
`LanguageInfo::detect_from_path` 派生链，不得重新从路径推导。

## ContentRoute 五变体

`ContentRoute` 在解析时一次性确定并随解析结果向下传递：

| 变体 | 触发条件 (`FileType`) | 处理管线 |
| ---- | --------------------- | -------- |
| `Ast` | `Source` / `Header` | tree-sitter AST 解析 → 实体抽取 → 关系解析 |
| `Documentation` | `Documentation`（`.md`、`.rst` 等） | 文档管线 |
| `Config` | `Config`（`.toml`、`.json`、`.xml`、`Makefile` 等） | 文档管线（按 language 细分格式子管线） |
| `Schema` | `Schema`（`.proto`、`.graphql`、`.thrift`、`.avsc`） | 文档管线（纯文本分段分块），类别保持 `Schema` |
| `PlainText` | `Text`（`.txt`、`.log` 及未知文本） | 文档管线（纯文本 pipeline），类别 `Other` |

解析期的管线闸门是 `LanguageInfo::is_document_like()`：命中即进入文档
管线，并在解析结果上一次性写入显式的 `ContentRoute`。此后所有下游阶段
（存储、BM25/Embedding/摘要热更新处理器等）只读取随结果携带的
`ContentRoute`（`is_document()`），不得重新从路径推导；恢复与扫描边界
只持有路径时，用 `ContentRoute::detect_from_path()` 按同一谓词复原路由。

## 文档管线内的显式类别传递

文档管线入口（`TextPipeline::process`）对每个文件调用一次
`DocumentClassification::detect(file_path)`，得到 `(payload, category)`
配对并沿 `chunk()` → 各格式 chunker → `TwoTierParams` 显式下传。
所有下游 chunker（markdown/json/xml/toml/yaml/plain/plugin）都复用该配对
构造 `ChunkMetadata::with_classification`，**不再自行从路径推导**：

- plain chunker 不再有 `determine_from_path` 反推与 `Schema` 事后覆写；
- JSON/YAML/TOML/XML 子管线由检测到的 `Language` 选择
  （`PipelineRouter::select_config_pipeline`），不再二次匹配扩展名；
- `PipelineRouter::get_doc_type` 本身是对 `LanguageInfo::detect_from_path`
  的纯委托，无独立扩展名表。行为要点：结构化配置语言
  （JSON/YAML/TOML/XML）报 `DocType::Config`；markdown 规则命中
  （`.md`/`.markdown` 扩展名或无扩展名文档名单）报 `Markdown`；
  rst/adoc 维持 `PlainText` 管线以保留 RST 专用分块。

## 块级双标签治理

`ChunkMetadata` 同时携带 `content_type`（块级负载）与
`file_category`（业务类别）。二者关系约束如下：

- 二者由入口的 `DocumentClassification` 配对赋值
  （`ChunkMetadata::with_classification` / `from_parts` 内建一致性断言）；
- 合法组合包括 `(Code, Code)`、`(Document, Documentation)`、
  `(Document, Schema)`、`(Config, Config)`、`(PlainText, Other)`；
- 存储与查询只以 `FileCategory` 为过滤主键，块级负载仅用于展示与分词权重。

## 索引格式版本门禁

分类编码（`FileCategory`/`FileType`/`ContentRoute` 变体集）与 rkyv 缓存
布局由 `cce_core::types::INDEX_FORMAT_VERSION` 统一版本化：

- **SQLite**：`migration.rs::LATEST_SCHEMA_VERSION` 与之联动（当前为 2），
  `user_version` 不符即拒绝打开，提示执行全量重建（`force_reindex=true`）
  或删除数据目录；
- **BM25**：物理索引位于 `<root>/i{INDEX_FORMAT_VERSION}` 版本子目录，
  旧版本目录一律视为不存在，直接新建；
- **Qdrant**：collection 名追加 `-i{INDEX_FORMAT_VERSION}` 后缀，
  新旧 collection 物理隔离；
- **rkyv 缓存信封**（`serialize_for_cache`/`deserialize_from_cache`）：头部
  记录 magic + 版本号 + 插件语言指纹，不匹配的条目整体作废重解析；
- **checkpoint 信封**：`ParsedCheckpointEnvelope` /
  `SummaryCheckpointPayload` 记录 `index_format_version` 与可选的
  `plugin_language_fingerprint`（仅当文件引用 `Language::Custom` 时记录，
  指纹漂移即失效；不含自定义语言的条目不受插件注册变化影响）。

任何影响持久化表示的分类变更必须递增 `INDEX_FORMAT_VERSION` 并触发一次
全量重建；旧版本残留（旧 collection、旧索引目录）不做在线读取，由维护
任务或手动清理。

## 展示标签启发式（非存储类别）

`FileCategory::looks_like_config` / `looks_like_documentation` 是**仅供
展示标签**的目录/文件名启发式（如 `config/` 目录段、`settings.*`、
`docs/` 目录段）。它们不参与存储类别或管线选择——那由上面的统一派生链
决定。watcher 的配置文件识别则继续复用
`cce_core::utils::path::BUILD_CONFIG_FILE_NAMES` 单一名单。

## 构建文件的关系解析

构建配置文件的识别以 `cce_core::utils::path::BUILD_CONFIG_FILE_NAMES`
为单一事实来源，watcher、热更新关系处理器与构建系统探测器共享该列表。

### 支持矩阵

| 构建系统 | 配置文件 | 解析器 | 受影响扩展名 |
| -------- | -------- | ------ | ------------ |
| Cargo | `Cargo.toml` | `parsers/cargo.rs` | `rs` |
| NPM/PNPM/Yarn | `package.json` 等 | `parsers/javascript.rs` | `js/ts/jsx/tsx` |
| PyPI | `requirements.txt` 等 | `parsers/python.rs` | `py` |
| Go Modules | `go.mod` | `parsers/go.rs` | `go` |
| Maven/Gradle | `pom.xml`、`build.gradle*` | `parsers/java.rs` | `java/kt` |
| CMake | `CMakeLists.txt` | `parsers/cpp/cmake.rs` | `c/cpp/h/hpp...` |
| Composer | `composer.json` | `parsers/php.rs` | `php` |
| .NET | `*.csproj` 等 | `parsers/dotnet.rs` | `cs/fs/vb` |
| Bundler | `Gemfile` | `parsers/ruby.rs` | `rb` |
| Make | `Makefile`/`GNUmakefile` | `parsers/makefile.rs` | `c/cpp/h/hpp...` |
| Docker | `Dockerfile` | `parsers/dockerfile.rs` | 无（仅触发重载） |

### Makefile 解析

提取四类信息用于导入分类：`-l<name>` 链接标志与 `pkg-config` 包名归入
外部依赖；`include/-include/sinclude` 片段及同文件内"目标 ↔ 前置条件"
互相引用的目标名归入内部依赖。依赖同时挂到 C 与 C++ 两个语言桶。

### Dockerfile 解析

`FROM <image>` 提取基础镜像（剥离 tag/digest，跳过 `scratch`），
`COPY --from=<stage>` 若指向本文件内 `AS` 定义的阶段则记为内部依赖，
否则记为外部镜像引用。Docker 无关联源码语言，依赖存于 `Unknown`
语言桶；变更仅触发热更新的配置重载流程，不做按扩展名的受影响闭包。

### CMake 增强

除原有 `find_package()` 外，现还解析：`find_library()`（外部库）、
`add_subdirectory()`（内部子工程）、`include_directories()`（内部头文件
搜索路径，过滤 `${VAR}` 表达式、绝对路径与泛化段名）、
`target_link_libraries()`（链接项若命中 `add_library`/`add_executable`
定义的本地目标则记为内部依赖，否则记为外部）。结果同样写入 C 与
C++ 两个语言桶。

### 热更新联动

`watcher/coordinator.rs` 的配置识别基于同一份 `BUILD_CONFIG_FILE_NAMES`
判定；Makefile/Dockerfile 变更会进入配置重载流程：先全量重扫构建配置，
再由 `BuildConfigParser::get_affected_extensions()` 确定受影响的源码扩展
名闭包并重建其关系。

## 扩展指引

- **新增文档/配置类型**：在 `builtin_language_for_extension` 登记
  扩展名 → `FileType` 映射即可获得正确路由；如需专用分块逻辑再实现新的
  `TextPipeline` 并接入 `PipelineRouter`。
- **新增 schema 类语言**：登记为 `FileType::Schema` 即可同时获得正确的
  路由（文档管线）与业务类别（`Schema`），无需额外覆盖逻辑。
- **修改分类编码**：递增 `INDEX_FORMAT_VERSION`，三个存储后端与缓存/
  checkpoint 信封随之整体失效。
- **新增构建系统**：
  1. 将配置文件名加入 `BUILD_CONFIG_FILE_NAMES`；
  2. 在 `config_parser/parsers/` 下新增解析模块并在
     `scan_project_at` 注册；
  3. 在 `get_supported_build_systems()` 补充元数据（配置文件 ↔ 语言 ↔
     受影响扩展名），使热更新能计算受影响闭包。
