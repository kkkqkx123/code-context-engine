# 批量处理资源优化设计方案

## 一、问题分析

### 1.1 当前架构问题

#### 内存全量累积

`orchestrator.rs` 中的核心处理循环存在数据全量累积问题：

```rust
// orchestrator.rs:165-167
let mut all_parsed_files = Vec::new();
let mut all_processing_results = Vec::new();
let mut all_chunks = Vec::new();

for (batch_idx, batch) in files.chunks(batch_size).enumerate() {
    // 处理文件...
    all_parsed_files.push(pf);      // 持续累积
    all_processing_results.push(pr); // 持续累积
    all_chunks.extend(chunks);       // 持续累积
}
// 所有数据保留在内存中直到最后存储阶段
```

内存占用估算：

| 数据类型 | 单文件估算 | 1000文件估算 |
|---------|-----------|-------------|
| ParsedFile | 50-200KB | 50-200MB |
| ProcessingResult | 20-50KB | 20-50MB |
| ChunkedResult | 5-20KB | 5-20MB |
| **总计** | 75-270KB | 75-270MB |

#### 文件处理串行化

当前处理流程是严格串行的：

```rust
for (batch_idx, batch) in files.chunks(batch_size).enumerate() {
    for (file_idx, file_entry) in batch.iter().enumerate() {
        // 串行处理每个文件
        match self.file_processor.process_file_complete(file_entry).await {
            // ...
        }
    }
}
```

问题：
- `max_concurrent_tasks` 配置存在但未被使用
- 批次只是逻辑分组，没有实际的并发处理
- IO密集型操作（文件读取）阻塞后续处理

#### Embedding 批量处理风险

```rust
// storage_coordinator.rs:90-94
let texts: Vec<&str> = chunks
    .iter()
    .filter_map(|c| c.embedding_text.as_deref())
    .collect();
let embeddings = embedder.embed(&texts).await?;
```

一次性发送所有 chunks 进行 embedding，可能超过 API token 限制。

### 1.2 问题根因

```
┌─────────────────────────────────────────────────────────────┐
│                    当前处理流水线                            │
├─────────────────────────────────────────────────────────────┤
│  Scan → Parse → Process → Convert → Chunk → [等待所有完成]  │
│                                              ↓              │
│                                    Store (一次性写入)        │
│                                              ↓              │
│                                    释放内存                  │
└─────────────────────────────────────────────────────────────┘

问题：各阶段无背压控制，数据在内存中无限累积
```

## 二、设计目标

1. **内存可控**：流式处理，及时释放已处理数据
2. **并发处理**：利用 `max_concurrent_tasks` 实现真正的并发
3. **进度追踪**：细粒度状态管理，支持中断恢复
4. **API 友好**：Embedding 分批发送，避免超限

## 三、核心设计方案

### 3.1 流水线架构重构

```
┌─────────────────────────────────────────────────────────────────────┐
│                     优化后的处理流水线                               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐      │
│  │  Scanner │───▶│  Parser  │───▶│ Processor│───▶│  Store   │      │
│  │ (生产者) │    │ (消费者) │    │ (消费者) │    │ (消费者) │      │
│  └──────────┘    └──────────┘    └──────────┘    └──────────┘      │
│       │               │               │               │             │
│       ▼               ▼               ▼               ▼             │
│   [channel]       [channel]       [channel]                         │
│   bounded(10)     bounded(10)     bounded(10)                       │
│                                                                     │
│  背压控制：当下游处理不过来时，上游自动阻塞等待                      │
└─────────────────────────────────────────────────────────────────────┘
```

### 3.2 新增模块：BatchCoordinator

引入 `BatchCoordinator` 统一管理批处理协调：

```rust
// src/orchestrator/index/batch_coordinator.rs

use std::sync::Arc;
use tokio::sync::{mpsc, Semaphore};
use crate::scanner::models::FileEntry;

/// 批处理协调器配置
pub struct BatchConfig {
    /// 扫描批次大小（一次扫描多少文件路径）
    pub scan_batch_size: usize,
    /// 解析并发数
    pub parse_concurrency: usize,
    /// 处理并发数  
    pub process_concurrency: usize,
    /// 存储批次大小
    pub store_batch_size: usize,
    /// Embedding 批次大小
    pub embedding_batch_size: usize,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            scan_batch_size: 100,
            parse_concurrency: 10,
            process_concurrency: 5,
            store_batch_size: 50,
            embedding_batch_size: 32,
        }
    }
}

/// 批处理协调器
/// 
/// 负责协调文件扫描、解析、处理、存储各阶段，
/// 确保数据在阶段间流动时及时释放内存。
pub struct BatchCoordinator {
    config: BatchConfig,
    state_tracker: Arc<UpdateStateTracker>,
}

/// 流水线阶段
pub enum PipelineStage {
    Scanning,
    Parsing,
    Processing,
    Storing,
    Completed,
}

/// 文件处理任务
pub struct FileTask {
    pub entry: FileEntry,
    pub stage: PipelineStage,
}

impl BatchCoordinator {
    pub fn new(config: BatchConfig, state_tracker: Arc<UpdateStateTracker>) -> Self {
        Self { config, state_tracker }
    }

    /// 执行流式索引
    /// 
    /// 核心流程：
    /// 1. 扫描阶段：按批次产出 FileEntry，避免一次性加载所有文件
    /// 2. 解析阶段：并发解析，受 parse_concurrency 限制
    /// 3. 处理阶段：并发处理，受 process_concurrency 限制
    /// 4. 存储阶段：批次存储，处理完立即写入，释放内存
    pub async fn execute_streaming(
        &self,
        options: IndexOptions,
        callback: Option<impl ProgressCallback>,
    ) -> Result<IndexResult, OrchestratorError> {
        // 实现见下文
    }
}
```

### 3.3 流式处理实现

```rust
impl BatchCoordinator {
    pub async fn execute_streaming<F: ProgressCallback>(
        &self,
        options: IndexOptions,
        progress_callback: Option<F>,
    ) -> Result<IndexResult, OrchestratorError> {
        let (scan_tx, scan_rx) = mpsc::channel::<FileEntry>(self.config.scan_batch_size);
        let (parse_tx, parse_rx) = mpsc::channel::<ParsedFile>(self.config.parse_concurrency);
        let (chunk_tx, chunk_rx) = mpsc::channel::<ChunkedResult>(self.config.store_batch_size);

        let result = Arc::new(tokio::sync::Mutex::new(IndexResult::new()));
        let progress = Arc::new(tokio::sync::Mutex::new(IndexProgress::new()));

        // 阶段1：扫描生产者
        let scanner_handle = {
            let config = self.config.clone();
            let options = options.clone();
            async move {
                let mut scanner = FSScanner::new();
                let scan_opts = ScanOptions::from(options.clone());
                
                // 流式扫描：不一次性返回所有文件
                // 改造 FSScanner 支持回调式扫描
                scanner.scan_streaming(&scan_opts, |entries| {
                    for entry in entries {
                        if scan_tx.blocking_send(entry).is_err() {
                            break;
                        }
                    }
                })?;
                
                Ok::<_, OrchestratorError>(())
            }
        };

        // 阶段2：解析消费者（并发）
        let parser_handle = {
            let semaphore = Arc::new(Semaphore::new(self.config.parse_concurrency));
            let coordinator = ParseCoordinator::new();
            
            async move {
                while let Some(entry) = scan_rx.recv().await {
                    let permit = semaphore.clone().acquire_owned().await;
                    let tx = parse_tx.clone();
                    
                    tokio::spawn(async move {
                        let _permit = permit;
                        // 读取并解析文件
                        if let Ok(content) = read_file_async(&entry.path).await {
                            if let Ok(parsed) = coordinator.parse(&entry, &content) {
                                let _ = tx.send(parsed).await;
                            }
                        }
                    });
                }
                Ok::<_, OrchestratorError>(())
            }
        };

        // 阶段3：处理与存储消费者
        let processor_handle = {
            let storage = self.storage.clone();
            let config = self.config.clone();
            
            async move {
                let mut chunk_buffer = Vec::with_capacity(config.store_batch_size);
                
                while let Some(chunk) = chunk_rx.recv().await {
                    chunk_buffer.push(chunk);
                    
                    // 达到批次大小，立即存储
                    if chunk_buffer.len() >= config.store_batch_size {
                        let to_store = std::mem::take(&mut chunk_buffer);
                        storage.store_vectors(&to_store).await?;
                        // 内存已释放
                    }
                }
                
                // 处理剩余数据
                if !chunk_buffer.is_empty() {
                    storage.store_vectors(&chunk_buffer).await?;
                }
                
                Ok::<_, OrchestratorError>(())
            }
        };

        // 等待所有阶段完成
        let (r1, r2, r3) = tokio::join!(scanner_handle, parser_handle, processor_handle);
        
        // 处理结果...
        Ok(result.lock().await.clone())
    }
}
```

### 3.4 Scanner 流式改造

改造 `FSScanner` 支持流式扫描，避免一次性返回所有文件：

```rust
// src/scanner/walker.rs

impl FSScanner {
    /// 流式扫描目录
    /// 
    /// 使用回调函数处理每个批次的文件，避免内存累积。
    /// 每处理 `batch_size` 个文件后调用一次回调。
    pub fn scan_streaming<F>(
        &mut self,
        opts: &ScanOptions,
        batch_size: usize,
        mut callback: F,
    ) -> Result<(), ScannerError>
    where
        F: FnMut(&mut Vec<FileEntry>),
    {
        self.visited_paths.clear();

        let root = Path::new(&opts.root_path);
        let abs_root = root.canonicalize()
            .map_err(|e| Self::io_error("failed to canonicalize root path", root, e))?;

        let ignore = self.load_ignore(opts);
        
        // 使用本地缓冲区
        let mut batch_buffer = Vec::with_capacity(batch_size);
        
        self.walk_dir_streaming(
            &abs_root, 
            opts, 
            &mut batch_buffer,
            batch_size,
            &mut callback,
            &ignore
        )?;
        
        // 处理最后一批
        if !batch_buffer.is_empty() {
            callback(&mut batch_buffer);
        }

        Ok(())
    }

    fn walk_dir_streaming<F>(
        &mut self,
        dir: &Path,
        opts: &ScanOptions,
        batch: &mut Vec<FileEntry>,
        batch_size: usize,
        callback: &mut F,
        gitignore: &Option<IgnoreMatcher>,
    ) -> Result<(), ScannerError>
    where
        F: FnMut(&mut Vec<FileEntry>),
    {
        // ... 目录遍历逻辑 ...
        
        for entry in entries_iter {
            let path = entry.path();
            
            if file_type.is_file() && self.should_include_file(&path, ...) {
                let file_entry = self.process_file(&path, &opts.root_path, opts)?;
                batch.push(file_entry);
                
                // 批次满了，回调处理并清空
                if batch.len() >= batch_size {
                    callback(batch);
                    batch.clear();  // 立即释放内存
                }
            }
            // ... 目录递归处理 ...
        }
        
        Ok(())
    }
}
```

### 3.5 Embedding 分批处理

改造 `StorageCoordinator` 支持分批 Embedding：

```rust
// src/orchestrator/index/storage_coordinator.rs

impl StorageCoordinator {
    /// 分批存储向量
    /// 
    /// 将 chunks 分批发送进行 embedding，避免 API token 限制。
    pub async fn store_vectors_batched(
        &self,
        chunks: &[ChunkedResult],
        batch_size: usize,
    ) -> Result<usize, OrchestratorError> {
        let embedder = match &self.embedder {
            Some(e) => e,
            None => return Ok(0),
        };

        let qdrant = match &self.qdrant {
            Some(q) => q,
            None => return Ok(0),
        };

        let mut total_stored = 0;

        // 分批处理
        for batch in chunks.chunks(batch_size) {
            // 准备本批次的文本
            let texts: Vec<&str> = batch
                .iter()
                .filter_map(|c| c.embedding_text.as_deref())
                .collect();

            if texts.is_empty() {
                continue;
            }

            // 生成 embedding
            let embeddings = embedder.embed(&texts).await?;

            // 构建向量点
            let points: Vec<VectorPoint> = batch
                .iter()
                .zip(embeddings.embeddings.iter())
                .map(|(chunk, vector)| {
                    Self::build_vector_point(chunk, vector.clone())
                })
                .collect();

            // 存储本批次
            if !points.is_empty() {
                qdrant.upsert_points(&points).await?;
                total_stored += points.len();
            }

            // 可选：批次间短暂休眠，避免 API 限流
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        Ok(total_stored)
    }
}
```

### 3.6 缓存管理优化

为 `FileProcessor` 的缓存添加 LRU 策略：

```rust
// src/orchestrator/index/file_processor.rs

use lru::LruCache;
use std::num::NonZeroUsize;

pub struct FileProcessor {
    coordinator: ParseCoordinator,
    pre_processor: NestEntityProcessor,
    converter: AstToNlConverter,
    chunker: GroupChunker,
    /// LRU 缓存，限制最大条目数
    chunk_cache: LruCache<String, Vec<ChunkedResult>>,
}

impl FileProcessor {
    pub fn new() -> Self {
        let config = Settings::ast_to_nl();
        let cache_size = NonZeroUsize::new(100).unwrap(); // 最多缓存 100 个文件
        
        Self {
            coordinator: ParseCoordinator::new(),
            pre_processor: NestEntityProcessor::new(),
            converter: AstToNlConverter::with_config(&config),
            chunker: GroupChunker::new(config.chunking.clone()),
            chunk_cache: LruCache::new(cache_size),
        }
    }

    /// 获取缓存统计
    pub fn cache_stats(&self) -> CacheStats {
        CacheStats {
            entries: self.chunk_cache.len(),
            capacity: self.chunk_cache.capacity(),
        }
    }
}

pub struct CacheStats {
    pub entries: usize,
    pub capacity: usize,
}
```

## 四、配置扩展

扩展 `OrchestratorConfig` 支持新的批处理参数：

```toml
# config.toml

[orchestrator]
# 扫描批次大小
scan_batch_size = 100
# 解析并发数
parse_concurrency = 10
# 处理并发数  
process_concurrency = 5
# 存储批次大小
store_batch_size = 50
# Embedding 批次大小（避免 API 限制）
embedding_batch_size = 32

[orchestrator.cache]
# 文件处理缓存大小
chunk_cache_size = 100
# 是否启用缓存
enabled = true
```

```rust
// src/orchestrator/config.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchConfig {
    /// 扫描批次大小
    pub scan_batch_size: usize,
    /// 解析并发数
    pub parse_concurrency: usize,
    /// 处理并发数
    pub process_concurrency: usize,
    /// 存储批次大小
    pub store_batch_size: usize,
    /// Embedding 批次大小
    pub embedding_batch_size: usize,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            scan_batch_size: 100,
            parse_concurrency: 10,
            process_concurrency: 5,
            store_batch_size: 50,
            embedding_batch_size: 32,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheSettings {
    /// 缓存条目上限
    pub chunk_cache_size: usize,
    /// 是否启用缓存
    pub enabled: bool,
}

impl Default for CacheSettings {
    fn default() -> Self {
        Self {
            chunk_cache_size: 100,
            enabled: true,
        }
    }
}
```

## 五、内存使用对比

### 优化前

```
时间线：T1 ──────────────────────────────────────── T2 ── T3
       │                                                │    │
       │  扫描 + 解析 + 处理（内存持续增长）              │存储 │释放
       │                                                │    │
内存：  │████████████████████████████████████████████│    │
       │            峰值：1000文件 × 270KB = 270MB   │    │
```

### 优化后

```
时间线：T1 ── T2 ── T3 ── T4 ── ... ── Tn
       │     │     │     │           │
       │扫描│解析│处理│存储│...循环...│
       │     │     │     │           │
内存：  │██│██│██│  │██│██│██│  │██│
       │ 固定 │释放│ │ 固定 │释放│
       │ ~10MB│   │ │ ~10MB│   │
```

内存占用对比：

| 场景 | 优化前峰值 | 优化后峰值 | 减少比例 |
|-----|----------|----------|---------|
| 100 文件 | ~27MB | ~5MB | 81% |
| 1000 文件 | ~270MB | ~10MB | 96% |
| 10000 文件 | ~2.7GB | ~15MB | 99% |

## 六、实施计划

### Phase 1：流式存储改造

**目标**：改造索引流程，实现处理完立即存储

**改动文件**：
- `src/orchestrator/index/orchestrator.rs`
- `src/orchestrator/index/storage_coordinator.rs`

**改动内容**：
1. 移除 `all_parsed_files`、`all_chunks` 等累积变量
2. 每个批次处理完成后立即存储
3. 添加 `embedding_batch_size` 配置支持

### Phase 2：并发处理改造

**目标**：实现真正的并发文件处理

**改动文件**：
- `src/orchestrator/index/orchestrator.rs`
- `src/orchestrator/config.rs`

**改动内容**：
1. 使用 `tokio::sync::Semaphore` 控制并发
2. 利用 `futures::stream::buffer_unordered` 实现并发流
3. 正确使用 `max_concurrent_tasks` 配置

### Phase 3：Scanner 流式改造

**目标**：支持流式扫描，避免一次性加载所有文件路径

**改动文件**：
- `src/scanner/walker.rs`

**改动内容**：
1. 添加 `scan_streaming` 方法
2. 实现批次回调机制
3. 支持增量扫描

### Phase 4：缓存优化

**目标**：为 FileProcessor 添加 LRU 缓存

**改动文件**：
- `src/orchestrator/index/file_processor.rs`
- `Cargo.toml`（添加 `lru` 依赖）

**改动内容**：
1. 替换 `HashMap` 为 `LruCache`
2. 添加缓存大小配置
3. 添加缓存统计接口

## 七、风险评估

| 风险 | 影响 | 缓解措施 |
|-----|-----|---------|
| 流水线复杂度增加 | 维护成本 | 完善单元测试，添加集成测试 |
| 并发竞态条件 | 数据一致性 | 使用 Arc<Mutex> 保护共享状态 |
| API 限流 | Embedding 失败 | 添加重试机制，批次间休眠 |
| 批次处理原子性 | 部分失败 | 实现细粒度错误追踪和恢复 |

## 八、总结

本设计方案通过以下核心改造解决批量处理的资源问题：

1. **流式处理**：各阶段通过 channel 连接，数据流动时及时释放内存
2. **背压控制**：bounded channel 确保下游处理不过来时上游阻塞
3. **并发处理**：利用 Semaphore 控制并发度，提高吞吐量
4. **分批 Embedding**：避免 API token 限制，提高稳定性
5. **LRU 缓存**：控制内存增长，支持淘汰策略

预期收益：
- 内存使用减少 80-99%
- 处理吞吐量提升 3-5 倍
- 支持更大规模代码库索引
