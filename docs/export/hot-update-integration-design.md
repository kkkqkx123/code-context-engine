# 热更新集成方案

## 1. 功能概述

### 1.1 目标

将自然语言文档导出功能集成到热更新流程中，实现：
- 文件修改时自动更新对应的导出文档
- 文件新增时自动创建导出文档
- 文件删除时自动删除对应的导出文档

### 1.2 核心价值

- **实时同步**：导出文档与源代码保持实时同步
- **自动化**：无需手动触发导出命令
- **一致性**：确保导出文档始终反映最新代码状态

## 2. 现有热更新架构

### 2.1 热更新流程

```
┌─────────────────────────────────────────────────────────────────┐
│                      现有热更新流程                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  FileEvent (Created/Modified/Deleted)                           │
│      │                                                          │
│      ▼                                                          │
│  HotUpdateCoordinator                                           │
│      │                                                          │
│      ▼                                                          │
│  BatchChangeResult                                              │
│      │                                                          │
│      ▼                                                          │
│  ┌─────────────────────────────────────────┐                    │
│  │         UpdateProcessors                │                    │
│  │                                         │                    │
│  │  - EmbeddingUpdateProcessor             │                    │
│  │  - Bm25UpdateProcessor                  │                    │
│  │  - RelationUpdateProcessor              │                    │
│  │  - SummaryUpdateProcessor               │                    │
│  └─────────────────────────────────────────┘                    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 UpdateProcessor Trait

```rust
#[async_trait]
pub trait UpdateProcessor: Send + Sync {
    /// 获取处理器名称
    fn name(&self) -> &'static str;
    
    /// 检查是否启用
    fn is_enabled(&self) -> bool;
    
    /// 处理批量变更
    async fn process(&self, batch_result: &BatchChangeResult) -> Result<()>;
    
    /// 带状态追踪的处理
    async fn process_tracked(
        &self,
        batch_result: &BatchChangeResult,
        state_tracker: &UpdateStateTracker,
    ) -> Result<()>;
}
```

### 2.3 BatchChangeResult 结构

```rust
pub struct BatchChangeResult {
    /// 文件变更列表
    pub file_changes: Vec<FileChange>,
    /// 解析结果列表
    pub parse_results: Vec<ParseResultWithChanges>,
    /// 失败列表
    pub failed: Vec<(PathBuf, String)>,
}

pub struct FileChange {
    pub path: PathBuf,
    pub change_type: FileChangeType,
    pub content: String,
    pub size: u64,
    pub timestamp: DateTime<Utc>,
}

pub enum FileChangeType {
    Added,
    Modified,
    Deleted,
}
```

## 3. 设计方案

### 3.1 新增 NlDocumentUpdateProcessor

```rust
// src/export/update_processor.rs

/// 自然语言文档更新处理器
/// 
/// 实现 UpdateProcessor trait，集成到热更新流程
pub struct NlDocumentUpdateProcessor {
    /// 导出器
    exporter: Arc<NlDocumentExporter>,
    /// 文件处理器（用于生成 chunks）
    file_processor: FileProcessor,
    /// 摘要生成器
    summary_generator: RuleBasedGenerator,
    /// 是否启用
    enabled: bool,
}

impl NlDocumentUpdateProcessor {
    /// 创建新处理器
    pub fn new(exporter: Arc<NlDocumentExporter>) -> Self {
        Self {
            exporter,
            file_processor: FileProcessor::new(),
            summary_generator: RuleBasedGenerator::new(),
            enabled: true,
        }
    }
    
    /// 设置启用状态
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}
```

### 3.2 实现 UpdateProcessor Trait

```rust
#[async_trait]
impl UpdateProcessor for NlDocumentUpdateProcessor {
    fn name(&self) -> &'static str {
        "nl_document"
    }
    
    fn is_enabled(&self) -> bool {
        self.enabled
    }
    
    async fn process(&self, batch_result: &BatchChangeResult) -> Result<()> {
        // 处理删除的文件
        for file_change in &batch_result.file_changes {
            if file_change.change_type == FileChangeType::Deleted {
                self.handle_deleted_file(&file_change.path).await?;
            }
        }
        
        // 处理新增/修改的文件
        for parse_result in &batch_result.parse_results {
            self.handle_file_update(parse_result).await?;
        }
        
        Ok(())
    }
    
    async fn process_tracked(
        &self,
        batch_result: &BatchChangeResult,
        state_tracker: &UpdateStateTracker,
    ) -> Result<()> {
        // 处理删除的文件
        for file_change in &batch_result.file_changes {
            if file_change.change_type == FileChangeType::Deleted {
                self.handle_deleted_file(&file_change.path).await?;
                
                // 更新状态
                state_tracker
                    .mark_success(&file_change.path, ModuleType::Export)
                    .await;
            }
        }
        
        // 处理新增/修改的文件
        for parse_result in &batch_result.parse_results {
            match self.handle_file_update(parse_result).await {
                Ok(_) => {
                    state_tracker
                        .mark_success(&parse_result.file_path, ModuleType::Export)
                        .await;
                }
                Err(e) => {
                    state_tracker
                        .mark_failed(&parse_result.file_path, ModuleType::Export, e.to_string())
                        .await;
                }
            }
        }
        
        Ok(())
    }
}
```

### 3.3 文件处理逻辑

```rust
impl NlDocumentUpdateProcessor {
    /// 处理文件更新（新增/修改）
    async fn handle_file_update(
        &self,
        parse_result: &ParseResultWithChanges,
    ) -> Result<()> {
        let file_path = &parse_result.file_path;
        
        // 1. 生成 chunks
        let chunks = self.file_processor
            .process_parsed_file(&parse_result.parsed_file)?;
        
        if chunks.is_empty() {
            tracing::debug!(
                path = %file_path.display(),
                "No chunks generated, skipping export"
            );
            return Ok(());
        }
        
        // 2. 生成摘要（如果启用）
        let summary = if self.exporter.config().include_summary {
            Some(self.summary_generator.generate(&parse_result.parsed_file).await)
        } else {
            None
        };
        
        // 3. 导出文档
        self.exporter
            .export_file(&chunks, summary.as_ref())
            .await?;
        
        tracing::info!(
            path = %file_path.display(),
            change_type = ?parse_result.file_change_type,
            "Exported NL document"
        );
        
        Ok(())
    }
    
    /// 处理文件删除
    async fn handle_deleted_file(&self, path: &Path) -> Result<()> {
        self.exporter.remove_file(path).await?;
        
        tracing::info!(
            path = %path.display(),
            "Removed NL document"
        );
        
        Ok(())
    }
}
```

## 4. 集成流程

### 4.1 完整热更新流程

```
┌─────────────────────────────────────────────────────────────────┐
│                      增强后的热更新流程                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  FileEvent (Created/Modified/Deleted)                           │
│      │                                                          │
│      ▼                                                          │
│  HotUpdateCoordinator                                           │
│      │                                                          │
│      ▼                                                          │
│  BatchChangeResult                                              │
│      │                                                          │
│      ▼                                                          │
│  ┌─────────────────────────────────────────┐                    │
│  │         UpdateProcessors                │                    │
│  │                                         │                    │
│  │  - EmbeddingUpdateProcessor             │                    │
│  │  - Bm25UpdateProcessor                  │                    │
│  │  - RelationUpdateProcessor              │                    │
│  │  - SummaryUpdateProcessor               │                    │
│  │  - NlDocumentUpdateProcessor ◄── 新增   │                    │
│  └─────────────────────────────────────────┘                    │
│      │                                                          │
│      ▼                                                          │
│  .cce/nl_docs/ (自动更新)                                        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 4.2 初始化集成

```rust
// 在应用初始化时创建处理器

async fn setup_hot_update(config: &AppConfig) -> HotUpdateCoordinator {
    // 创建导出器
    let export_config = ExportConfig {
        project_root: config.project_root.clone(),
        include_summary: config.export.include_summary,
        enable_relation_enhancement: config.export.enable_relation_enhancement,
    };
    let exporter = Arc::new(NlDocumentExporter::new(export_config));
    
    // 创建处理器列表
    let processors: Vec<BoxedUpdateProcessor> = vec![
        Box::new(EmbeddingUpdateProcessor::new(...)),
        Box::new(Bm25UpdateProcessor::new(...)),
        Box::new(RelationUpdateProcessor::new(...)),
        Box::new(SummaryUpdateProcessor::new(...)),
        Box::new(NlDocumentUpdateProcessor::new(exporter)), // 新增
    ];
    
    // 创建热更新协调器
    let coordinator = HotUpdateCoordinator::new(config.hot_update.clone());
    
    coordinator
}
```

### 4.3 运行时集成

```rust
// 在热更新循环中执行

async fn run_hot_update_loop(
    mut coordinator: HotUpdateCoordinator,
    processors: Vec<BoxedUpdateProcessor>,
) {
    loop {
        // 检查是否需要更新
        if coordinator.check_should_update(false).await {
            // 执行更新（包含所有处理器）
            match coordinator.update_with_processors(&processors).await {
                Ok(result) => {
                    tracing::info!(
                        processed = result.processed_count(),
                        failed = result.failed_count(),
                        "Hot update completed"
                    );
                }
                Err(e) => {
                    tracing::error!(error = %e, "Hot update failed");
                }
            }
        }
        
        // 等待下一次检查
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
```

## 5. 文件事件处理

### 5.1 文件创建

```
FileEvent(Created)
    │
    ▼
ParseCoordinator.parse()
    │
    ▼
FileProcessor.process_parsed_file()
    │
    ▼
NlDocumentExporter.export_file()
    │
    ▼
.cce/nl_docs/<path>.md (创建)
```

### 5.2 文件修改

```
FileEvent(Modified)
    │
    ▼
ParseCoordinator.parse()
    │
    ▼
FileProcessor.process_parsed_file()
    │
    ▼
NlDocumentExporter.export_file()
    │
    ▼
.cce/nl_docs/<path>.md (覆盖)
```

### 5.3 文件删除

```
FileEvent(Deleted)
    │
    ▼
NlDocumentExporter.remove_file()
    │
    ▼
.cce/nl_docs/<path>.md (删除)
```

## 6. 状态追踪

### 6.1 ModuleType 扩展

```rust
// 在 index_state.rs 中添加新的模块类型

pub enum ModuleType {
    Embedding,
    Bm25,
    Relation,
    Summary,
    Export,  // 新增
}
```

### 6.2 状态报告

```rust
// 导出模块的状态会包含在整体状态报告中

pub struct IndexStateReport {
    pub modules: HashMap<ModuleType, ModuleUpdateState>,
    // ...
}

// 示例输出
{
    "modules": {
        "embedding": "completed",
        "bm25": "completed",
        "relation": "completed",
        "summary": "completed",
        "export": "completed"  // 新增
    }
}
```

## 7. 错误处理

### 7.1 错误隔离

```rust
impl NlDocumentUpdateProcessor {
    async fn process(&self, batch_result: &BatchChangeResult) -> Result<()> {
        // 错误隔离：单个文件失败不影响其他文件
        for parse_result in &batch_result.parse_results {
            match self.handle_file_update(parse_result).await {
                Ok(_) => {}
                Err(e) => {
                    // 记录错误但继续处理其他文件
                    tracing::error!(
                        path = %parse_result.file_path.display(),
                        error = %e,
                        "Failed to export NL document"
                    );
                }
            }
        }
        
        Ok(())
    }
}
```

### 7.2 重试机制

```rust
// 利用现有的状态追踪重试机制

impl NlDocumentUpdateProcessor {
    async fn process_tracked(
        &self,
        batch_result: &BatchChangeResult,
        state_tracker: &UpdateStateTracker,
    ) -> Result<()> {
        for parse_result in &batch_result.parse_results {
            // 检查是否需要重试
            let state = state_tracker.get_state(&parse_result.file_path).await;
            if state.map(|s| s.should_retry()).unwrap_or(false) {
                match self.handle_file_update(parse_result).await {
                    Ok(_) => {
                        state_tracker
                            .mark_success(&parse_result.file_path, ModuleType::Export)
                            .await;
                    }
                    Err(e) => {
                        state_tracker
                            .mark_failed(&parse_result.file_path, ModuleType::Export, e.to_string())
                            .await;
                    }
                }
            }
        }
        
        Ok(())
    }
}
```

## 8. 配置

### 8.1 热更新配置扩展

```toml
[hot_update]
# 是否启用自然语言文档导出
enable_nl_export = true

[export]
# 是否包含文件摘要
include_summary = true

# 是否启用关系增强
enable_relation_enhancement = false
```

### 8.2 环境变量

```bash
# 是否启用自然语言文档导出
CCE_HOT_UPDATE_ENABLE_NL_EXPORT=true
```

## 9. 性能考虑

### 9.1 异步处理

- 所有 I/O 操作使用 `tokio::fs`
- 不阻塞其他处理器的执行

### 9.2 批量处理

- 利用现有的批量处理机制
- 减少单文件处理开销

### 9.3 增量更新

- 仅处理变更的文件
- 不重新导出未变更的文件

## 10. 测试策略

### 10.1 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_handle_file_created() {
        // 测试文件创建时的导出
    }
    
    #[tokio::test]
    async fn test_handle_file_modified() {
        // 测试文件修改时的更新
    }
    
    #[tokio::test]
    async fn test_handle_file_deleted() {
        // 测试文件删除时的清理
    }
}
```

### 10.2 集成测试

```rust
#[tokio::test]
async fn test_hot_update_with_nl_export() {
    // 1. 创建测试文件
    // 2. 触发热更新
    // 3. 验证 .cce/nl_docs/ 中的文档已创建
    // 4. 修改测试文件
    // 5. 触发热更新
    // 6. 验证文档已更新
    // 7. 删除测试文件
    // 8. 触发热更新
    // 9. 验证文档已删除
}
```

## 11. 总结

热更新集成方案通过实现 `UpdateProcessor` trait，将自然语言文档导出功能无缝集成到现有热更新流程：

1. **自动同步**：文件变更自动触发导出文档更新
2. **完整生命周期**：支持创建、修改、删除三种事件
3. **错误隔离**：单个文件失败不影响其他文件
4. **状态追踪**：集成到现有状态追踪系统
5. **性能优化**：异步处理、增量更新

---

**文档版本**：1.0
**创建日期**：2026-04-30
**维护者**：架构团队
