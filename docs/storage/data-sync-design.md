# 数据同步方案设计

## 问题分析

### 当前架构的数据一致性问题

在当前的存储架构中，BM25、Redb 和 Qdrant 三个存储引擎独立工作，存在以下数据一致性问题：

1. **实体标识符不统一**
   - BM25 使用 `document_id` (格式: `kind:name`)
   - Redb 使用结构化 ID (如 `function:path:name`)
   - Qdrant 使用点 ID (格式: `kind:name`)

2. **查询结果无法关联**
   - BM25 搜索结果无法直接获取 Redb 中的元数据
   - 向量搜索结果无法显示调用关系
   - 混合查询时无法统一排序和去重

3. **增量更新困难**
   - 文件更新时需要同步更新三个存储
   - 删除操作需要保证原子性
   - 部分失败导致数据不一致

### 同步需求评估

经过分析，我们得出以下结论：

1. **不需要强一致性**
   - 代码索引是批处理操作，通常一次性完成
   - 数据更新频率较低（代码库不会频繁变化）
   - 短暂的数据不一致可以接受

2. **需要轻量级关联机制**
   - 查询时需要关联不同存储的数据
   - 需要实体映射表建立关联关系
   - 需要最终一致性保证

## 设计方案

### 方案一：实体映射表（推荐）

在 Redb 中添加实体映射表，建立不同存储之间的关联关系。

#### 映射表设计

```rust
// 在 Redb 中添加以下表定义
pub const ENTITY_MAPPING: TableDefinition<&str, &[u8]> = TableDefinition::new("entity_mapping");
pub const BM25_TO_ENTITY: TableDefinition<&str, &str> = TableDefinition::new("bm25_to_entity");
pub const QDRANT_TO_ENTITY: TableDefinition<&str, &str> = TableDefinition::new("qdrant_to_entity");

/// 实体映射记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityMapping {
    /// 实体 ID (Redb 中的主键)
    pub entity_id: String,
    /// 实体类型 (function, class, module, etc.)
    pub entity_type: String,
    /// 文件路径
    pub file_path: String,
    /// 实体名称
    pub entity_name: String,
    /// BM25 文档 ID
    pub bm25_doc_id: Option<String>,
    /// Qdrant 点 ID
    pub qdrant_point_id: Option<String>,
    /// 创建时间戳
    pub created_at: i64,
    /// 更新时间戳
    pub updated_at: i64,
}
```

#### 映射关系建立

在索引过程中建立映射关系：

```rust
impl IndexOrchestrator {
    async fn store_with_mapping(
        &self,
        conversion: &ConversionResult,
        metadata: &EntityMetadata,
    ) -> Result<(), OrchestratorError> {
        // 1. 生成统一实体 ID
        let entity_id = Self::generate_entity_id(metadata);
        
        // 2. 存储到 Redb
        let file_record = FileRecord::from(metadata);
        self.metadata_store.insert_file(&file_record)?;
        
        // 3. 索引到 BM25
        let bm25_doc = Bm25Document::from(conversion);
        let bm25_doc_id = bm25_doc.document_id.clone();
        self.bm25_client.lock().await.index_document("default", &bm25_doc).await?;
        
        // 4. 存储到 Qdrant
        let vector = self.generate_vector(conversion).await?;
        let qdrant_point_id = format!("{}:{}", conversion.kind, conversion.name);
        let point = VectorPoint::new(qdrant_point_id.clone(), vector, payload);
        self.qdrant_client.upsert_points(&[point]).await?;
        
        // 5. 保存映射关系
        let mapping = EntityMapping {
            entity_id: entity_id.clone(),
            entity_type: conversion.kind.to_string(),
            file_path: metadata.file_path.clone(),
            entity_name: conversion.name.clone(),
            bm25_doc_id: Some(bm25_doc_id),
            qdrant_point_id: Some(qdrant_point_id),
            created_at: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64,
            updated_at: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64,
        };
        
        self.save_entity_mapping(&mapping).await?;
        
        Ok(())
    }
    
    async fn save_entity_mapping(&self, mapping: &EntityMapping) -> Result<(), OrchestratorError> {
        // 保存到 Redb 的 ENTITY_MAPPING 表
        self.metadata_store.with_write_transaction(|tx| {
            let data = serialize(mapping)?;
            
            let mut table = tx.open_table(ENTITY_MAPPING)?;
            table.insert(&mapping.entity_id, data.as_slice())?;
            
            // 保存反向映射
            if let Some(ref bm25_id) = mapping.bm25_doc_id {
                let mut bm25_table = tx.open_table(BM25_TO_ENTITY)?;
                bm25_table.insert(bm25_id, &mapping.entity_id)?;
            }
            
            if let Some(ref qdrant_id) = mapping.qdrant_point_id {
                let mut qdrant_table = tx.open_table(QDRANT_TO_ENTITY)?;
                qdrant_table.insert(qdrant_id, &mapping.entity_id)?;
            }
            
            Ok(())
        })
    }
}
```

#### 查询时关联

```rust
impl QueryOrchestrator {
    async fn enriched_search(
        &self,
        query: &str,
        options: QueryOptions,
    ) -> Result<Vec<EnrichedSearchResult>, OrchestratorError> {
        // 1. 执行 BM25 搜索
        let bm25_results = self.execute_bm25_search(query, options.limit).await?;
        
        // 2. 执行向量搜索
        let vector_results = self.execute_vector_search(query, options.limit).await?;
        
        // 3. 合并结果
        let mut all_results = Vec::new();
        
        // 处理 BM25 结果
        for result in bm25_results {
            if let Some(entity_id) = self.get_entity_id_for_bm25(&result.document_id).await? {
                if let Some(metadata) = self.get_entity_metadata(&entity_id).await? {
                    all_results.push(EnrichedSearchResult {
                        search_result: result,
                        metadata: Some(metadata),
                        source: "bm25".to_string(),
                        // 其他丰富信息
                    });
                }
            }
        }
        
        // 处理向量结果
        for result in vector_results {
            if let Some(entity_id) = self.get_entity_id_for_qdrant(&result.id).await? {
                if let Some(metadata) = self.get_entity_metadata(&entity_id).await? {
                    all_results.push(EnrichedSearchResult {
                        search_result: result,
                        metadata: Some(metadata),
                        source: "vector".to_string(),
                        // 其他丰富信息
                    });
                }
            }
        }
        
        // 4. 排序和去重
        all_results.sort_by(|a, b| b.search_result.score.partial_cmp(&a.search_result.score).unwrap());
        
        // 按实体 ID 去重
        let mut seen = HashSet::new();
        all_results.retain(|result| {
            if let Some(metadata) = &result.metadata {
                seen.insert(metadata.entity_id.clone())
            } else {
                true
            }
        });
        
        Ok(all_results)
    }
    
    async fn get_entity_id_for_bm25(&self, bm25_doc_id: &str) -> Result<Option<String>, OrchestratorError> {
        self.metadata_store.with_read_transaction(|tx| {
            let table = tx.open_table(BM25_TO_ENTITY)?;
            Ok(table.get(bm25_doc_id)?.map(|guard| guard.value().to_string()))
        })
    }
    
    async fn get_entity_id_for_qdrant(&self, qdrant_point_id: &str) -> Result<Option<String>, OrchestratorError> {
        self.metadata_store.with_read_transaction(|tx| {
            let table = tx.open_table(QDRANT_TO_ENTITY)?;
            Ok(table.get(qdrant_point_id)?.map(|guard| guard.value().to_string()))
        })
    }
    
    async fn get_entity_metadata(&self, entity_id: &str) -> Result<Option<EntityMetadata>, OrchestratorError> {
        // 从 Redb 获取完整的实体元数据
        self.metadata_store.with_read_transaction(|tx| {
            let table = tx.open_table(ENTITY_MAPPING)?;
            let data = table.get(entity_id)?;
            
            if let Some(guard) = data {
                let mapping: EntityMapping = deserialize(guard.value())?;
                
                // 获取文件信息
                let files_table = tx.open_table(FILES)?;
                let file_data = files_table.get(&mapping.file_path)?;
                
                // 获取函数信息（如果是函数）
                let functions_table = tx.open_table(FUNCTIONS)?;
                let function_data = functions_table.get(entity_id)?;
                
                // 构建完整的元数据
                Ok(Some(EntityMetadata {
                    entity_id: mapping.entity_id,
                    entity_type: mapping.entity_type,
                    file_path: mapping.file_path,
                    entity_name: mapping.entity_name,
                    file_info: file_data.map(|guard| deserialize(guard.value())).transpose()?,
                    function_info: function_data.map(|guard| deserialize(guard.value())).transpose()?,
                    // 其他元数据...
                }))
            } else {
                Ok(None)
            }
        })
    }
}
```

### 方案二：统一查询层

在协调层添加统一查询接口，隐藏存储细节。

#### 统一查询接口设计

```rust
/// 统一查询选项
#[derive(Debug, Clone)]
pub struct UnifiedQueryOptions {
    /// 查询文本
    pub query: String,
    /// 查询类型
    pub query_type: UnifiedQueryType,
    /// 结果数量限制
    pub limit: usize,
    /// 最小分数阈值
    pub min_score: Option<f32>,
    /// 是否包含关系信息
    pub include_relations: bool,
    /// 是否包含代码片段
    pub include_code_snippets: bool,
    /// 字段权重配置
    pub field_weights: HashMap<String, f32>,
}

/// 统一查询类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnifiedQueryType {
    /// 关键词搜索
    Keyword,
    /// 语义搜索
    Semantic,
    /// 混合搜索
    Hybrid,
    /// 关系查询
    Relation,
    /// 综合查询（所有类型）
    Comprehensive,
}

/// 丰富的结果项
#[derive(Debug, Clone)]
pub struct EnrichedResultItem {
    /// 实体 ID
    pub entity_id: String,
    /// 实体类型
    pub entity_type: String,
    /// 实体名称
    pub entity_name: String,
    /// 文件路径
    pub file_path: String,
    /// 搜索分数
    pub score: f32,
    /// 数据来源
    pub sources: Vec<String>,
    /// 代码片段
    pub code_snippet: Option<String>,
    /// 关系信息
    pub relations: Option<EntityRelations>,
    /// 元数据
    pub metadata: HashMap<String, String>,
}

/// 实体关系
#[derive(Debug, Clone)]
pub struct EntityRelations {
    /// 调用者
    pub callers: Vec<CallRelation>,
    /// 被调用者
    pub callees: Vec<CallRelation>,
    /// 导入
    pub imports: Vec<ImportRelation>,
    /// 导出
    pub exports: Vec<ExportRelation>,
}

/// 统一查询器
pub struct UnifiedQuerier {
    /// BM25 客户端
    bm25_client: Arc<tokio::sync::Mutex<Bm25Client>>,
    /// Qdrant 客户端
    qdrant_client: Option<Arc<QdrantClient>>,
    /// Redb 元数据存储
    metadata_store: Arc<RedbMetadataStore>,
    /// 嵌入器
    embedder: Option<Arc<Embedder>>,
    /// 关系查询器
    relation_querier: Option<CallChainQuery>,
}

impl UnifiedQuerier {
    /// 执行统一查询
    pub async fn query(
        &self,
        options: UnifiedQueryOptions,
    ) -> Result<Vec<EnrichedResultItem>, OrchestratorError> {
        let mut all_results = Vec::new();
        let mut sources_used = Vec::new();
        
        // 根据查询类型执行不同的搜索
        match options.query_type {
            UnifiedQueryType::Keyword => {
                let results = self.keyword_search(&options).await?;
                all_results.extend(results);
                sources_used.push("bm25".to_string());
            }
            UnifiedQueryType::Semantic => {
                let results = self.semantic_search(&options).await?;
                all_results.extend(results);
                sources_used.push("vector".to_string());
            }
            UnifiedQueryType::Hybrid => {
                let keyword_results = self.keyword_search(&options).await?;
                let semantic_results = self.semantic_search(&options).await?;
                
                all_results.extend(keyword_results);
                all_results.extend(semantic_results);
                
                sources_used.push("bm25".to_string());
                sources_used.push("vector".to_string());
            }
            UnifiedQueryType::Relation => {
                let results = self.relation_search(&options).await?;
                all_results.extend(results);
                sources_used.push("relation".to_string());
            }
            UnifiedQueryType::Comprehensive => {
                // 并行执行所有搜索
                let (keyword_results, semantic_results, relation_results) = tokio::join!(
                    self.keyword_search(&options),
                    self.semantic_search(&options),
                    self.relation_search(&options),
                );
                
                all_results.extend(keyword_results?);
                all_results.extend(semantic_results?);
                all_results.extend(relation_results?);
                
                sources_used.extend(vec![
                    "bm25".to_string(),
                    "vector".to_string(),
                    "relation".to_string(),
                ]);
            }
        }
        
        // 丰富结果
        let enriched_results = self.enrich_results(all_results, &options).await?;
        
        // 排序和去重
        let sorted_results = self.sort_and_dedup(enriched_results, &options);
        
        Ok(sorted_results)
    }
    
    /// 关键词搜索
    async fn keyword_search(
        &self,
        options: &UnifiedQueryOptions,
    ) -> Result<Vec<EnrichedResultItem>, OrchestratorError> {
        let mut client = self.bm25_client.lock().await;
        let bm25_results = client
            .search("default", &options.query, options.limit as i32)
            .await?;
        
        let mut results = Vec::new();
        for bm25_result in bm25_results {
            if let Some(entity_id) = self.get_entity_id_for_bm25(&bm25_result.document_id).await? {
                let metadata = self.get_basic_metadata(&entity_id).await?;
                
                results.push(EnrichedResultItem {
                    entity_id,
                    entity_type: metadata.entity_type,
                    entity_name: metadata.entity_name,
                    file_path: metadata.file_path,
                    score: bm25_result.score,
                    sources: vec!["bm25".to_string()],
                    code_snippet: bm25_result.fields.get("content").cloned(),
                    relations: None,
                    metadata: bm25_result.fields,
                });
            }
        }
        
        Ok(results)
    }
    
    /// 语义搜索
    async fn semantic_search(
        &self,
        options: &UnifiedQueryOptions,
    ) -> Result<Vec<EnrichedResultItem>, OrchestratorError> {
        let qdrant = self.qdrant_client.as_ref()
            .ok_or_else(|| OrchestratorError::Config("Qdrant client not configured".to_string()))?;
        
        let embedder = self.embedder.as_ref()
            .ok_or_else(|| OrchestratorError::Config("Embedder not configured".to_string()))?;
        
        // 生成查询向量
        let query_vector = embedder.embed_one(&options.query).await?;
        
        // 执行向量搜索
        let search_query = SearchQuery::new(query_vector, options.limit);
        let vector_results = qdrant.search(search_query).await?;
        
        let mut results = Vec::new();
        for vector_result in vector_results {
            if let Some(entity_id) = self.get_entity_id_for_qdrant(&vector_result.id).await? {
                let metadata = self.get_basic_metadata(&entity_id).await?;
                
                results.push(EnrichedResultItem {
                    entity_id,
                    entity_type: metadata.entity_type,
                    entity_name: metadata.entity_name,
                    file_path: metadata.file_path,
                    score: vector_result.score,
                    sources: vec!["vector".to_string()],
                    code_snippet: Some(vector_result.payload.code_chunk),
                    relations: None,
                    metadata: HashMap::new(),
                });
            }
        }
        
        Ok(results)
    }
    
    /// 关系搜索
    async fn relation_search(
        &self,
        options: &UnifiedQueryOptions,
    ) -> Result<Vec<EnrichedResultItem>, OrchestratorError> {
        // 从查询中提取实体信息
        let entity_id = self.extract_entity_id_from_query(&options.query).await?;
        
        let relation_querier = self.relation_querier.as_ref()
            .ok_or_else(|| OrchestratorError::Config("Relation querier not configured".to_string()))?;
        
        // 查询调用链
        let call_chain = relation_querier
            .query_forward_by_entity(entity_id, 3)
            .map_err(OrchestratorError::Query)?;
        
        let mut results = Vec::new();
        for node in call_chain {
            let metadata = self.get_basic_metadata(&node.function_id).await?;
            
            results.push(EnrichedResultItem {
                entity_id: node.function_id,
                entity_type: "function".to_string(),
                entity_name: node.function_name,
                file_path: node.file_path,
                score: 1.0 / (node.depth as f32 + 1.0),
                sources: vec!["relation".to_string()],
                code_snippet: None,
                relations: Some(EntityRelations {
                    callers: Vec::new(),
                    callees: Vec::new(),
                    imports: Vec::new(),
                    exports: Vec::new(),
                }),
                metadata: HashMap::new(),
            });
        }
        
        Ok(results)
    }
    
    /// 丰富结果
    async fn enrich_results(
        &self,
        results: Vec<EnrichedResultItem>,
        options: &UnifiedQueryOptions,
    ) -> Result<Vec<EnrichedResultItem>, OrchestratorError> {
        let mut enriched = Vec::new();
        
        for mut result in results {
            // 获取关系信息
            if options.include_relations {
                result.relations = self.get_entity_relations(&result.entity_id).await?;
            }
            
            // 获取更多元数据
            if options.include_code_snippets && result.code_snippet.is_none() {
                result.code_snippet = self.get_code_snippet(&result.entity_id).await?;
            }
            
            enriched.push(result);
        }
        
        Ok(enriched)
    }
    
    /// 排序和去重
    fn sort_and_dedup(
        &self,
        mut results: Vec<EnrichedResultItem>,
        options: &UnifiedQueryOptions,
    ) -> Vec<EnrichedResultItem> {
        // 去重（按实体 ID）
        let mut seen = HashSet::new();
        results.retain(|result| seen.insert(result.entity_id.clone()));
        
        // 排序
        results.sort_by(|a, b| {
            b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        // 应用限制
        if results.len() > options.limit {
            results.truncate(options.limit);
        }
        
        // 应用分数阈值
        if let Some(min_score) = options.min_score {
            results.retain(|result| result.score >= min_score);
        }
        
        results
    }
}
```

### 方案三：同步管理器

添加专门的同步管理器，处理存储间的数据同步。

#### 同步管理器设计

```rust
/// 同步管理器
pub struct SyncManager {
    /// BM25 客户端
    bm25_client: Arc<tokio::sync::Mutex<Bm25Client>>,
    /// Redb 元数据存储
    metadata_store: Arc<RedbMetadataStore>,
    /// Qdrant 客户端
    qdrant_client: Option<Arc<QdrantClient>>,
    /// 同步队列
    sync_queue: Arc<Mutex<Vec<SyncTask>>>,
    /// 同步工作器
    sync_worker: Option<JoinHandle<()>>,
    /// 同步状态
    sync_status: Arc<AtomicBool>,
}

/// 同步任务
#[derive(Debug, Clone)]
pub enum SyncTask {
    /// 索引实体
    IndexEntity {
        entity_id: String,
        bm25_doc: Bm25Document,
        metadata: EntityMetadata,
        vector: Option<Vec<f32>>,
    },
    /// 更新实体
    UpdateEntity {
        entity_id: String,
        bm25_doc: Option<Bm25Document>,
        metadata: Option<EntityMetadata>,
        vector: Option<Vec<f32>>,
    },
    /// 删除实体
    DeleteEntity {
        entity_id: String,
        bm25_doc_id: Option<String>,
        qdrant_point_id: Option<String>,
    },
    /// 批量同步
    BatchSync {
        tasks: Vec<SyncTask>,
    },
}

impl SyncManager {
    /// 创建同步管理器
    pub fn new(
        bm25_client: Arc<tokio::sync::Mutex<Bm25Client>>,
        metadata_store: Arc<RedbMetadataStore>,
        qdrant_client: Option<Arc<QdrantClient>>,
    ) -> Self {
        Self {
            bm25_client,
            metadata_store,
            qdrant_client,
            sync_queue: Arc::new(Mutex::new(Vec::new())),
            sync_worker: None,
            sync_status: Arc::new(AtomicBool::new(false)),
        }
    }
    
    /// 启动同步工作器
    pub fn start(&mut self) {
        if self.sync_worker.is_some() {
            return;
        }
        
        let queue = self.sync_queue.clone();
        let bm25_client = self.bm25_client.clone();
        let metadata_store = self.metadata_store.clone();
        let qdrant_client = self.qdrant_client.clone();
        let status = self.sync_status.clone();
        
        status.store(true, Ordering::SeqCst);
        
        let handle = tokio::spawn(async move {
            while status.load(Ordering::SeqCst) {
                // 处理同步任务
                let task = {
                    let mut queue = queue.lock().await;
                    queue.pop()
                };
                
                if let Some(task) = task {
                    match task {
                        SyncTask::IndexEntity { entity_id, bm25_doc, metadata, vector } => {
                            Self::process_index_task(
                                &bm25_client,
                                &metadata_store,
                                &qdrant_client,
                                entity_id,
                                bm25_doc,
                                metadata,
                                vector,
                            ).await;
                        }
                        SyncTask::UpdateEntity { entity_id, bm25_doc, metadata, vector } => {
                            Self::process_update_task(
                                &bm25_client,
                                &metadata_store,
                                &qdrant_client,
                                entity_id,
                                bm25_doc,
                                metadata,
                                vector,
                            ).await;
                        }
                        SyncTask::DeleteEntity { entity_id, bm25_doc_id, qdrant_point_id } => {
                            Self::process_delete_task(
                                &bm25_client,
                                &metadata_store,
                                &qdrant_client,
                                entity_id,
                                bm25_doc_id,
                                qdrant_point_id,
                            ).await;
                        }
                        SyncTask::BatchSync { tasks } => {
                            for task in tasks {
                                let queue = queue.clone();
                                let mut queue_lock = queue.lock().await;
                                queue_lock.push(task);
                            }
                        }
                    }
                } else {
                    // 队列为空，等待新任务
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        });
        
        self.sync_worker = Some(handle);
    }
    
    /// 停止同步工作器
    pub async fn stop(&mut self) {
        self.sync_status.store(false, Ordering::SeqCst);
        
        if let Some(worker) = self.sync_worker.take() {
            let _ = worker.await;
        }
    }
    
    /// 添加同步任务
    pub async fn add_task(&self, task: SyncTask) {
        let mut queue = self.sync_queue.lock().await;
        queue.push(task);
    }
    
    /// 批量添加任务
    pub async fn add_tasks(&self, tasks: Vec<SyncTask>) {
        let mut queue = self.sync_queue.lock().await;
        queue.extend(tasks);
    }
    
    /// 处理索引任务
    async fn process_index_task(
        bm25_client: &Arc<tokio::sync::Mutex<Bm25Client>>,
        metadata_store: &Arc<RedbMetadataStore>,
        qdrant_client: &Option<Arc<QdrantClient>>,
        entity_id: String,
        bm25_doc: Bm25Document,
        metadata: EntityMetadata,
        vector: Option<Vec<f32>>,
    ) {
        // 1. 存储到 Redb
        if let Err(e) = metadata_store.insert_entity(&metadata).await {
            tracing::error!("Failed to insert entity to Redb: {}", e);
            return;
        }
        
        // 2. 索引到 BM25
        let mut client = bm25_client.lock().await;
        if let Err(e) = client.index_document("default", &bm25_doc).await {
            tracing::error!("Failed to index document to BM25: {}", e);
            // 回滚 Redb 操作
            let _ = metadata_store.delete_entity(&entity_id).await;
            return;
        }
        
        // 3. 存储到 Qdrant
        if let (Some(qdrant), Some(vector)) = (qdrant_client, vector) {
            let point = VectorPoint::new(
                entity_id.clone(),
                vector,
                Payload {
                    file_path: metadata.file_path.clone(),
                    code_chunk: bm25_doc.fields.get("content").cloned().unwrap_or_default(),
                    start_line: 0,
                    end_line: 0,
                    entity_type: Some(metadata.entity_type.clone()),
                    path_segments: HashMap::new(),
                    extra: HashMap::new(),
                },
            );
            
            if let Err(e) = qdrant.upsert_points(&[point]).await {
                tracing::error!("Failed to upsert point to Qdrant: {}", e);
                // 回滚 BM25 和 Redb 操作
                let _ = client.delete("default", &bm25_doc.document_id).await;
                let _ = metadata_store.delete_entity(&entity_id).await;
                return;
            }
        }
        
        // 4. 保存映射关系
        let mapping = EntityMapping {
            entity_id: entity_id.clone(),
            entity_type: metadata.entity_type,
            file_path: metadata.file_path,
            entity_name: metadata.entity_name,
            bm25_doc_id: Some(bm25_doc.document_id),
            qdrant_point_id: qdrant_client.as_ref().map(|_| entity_id),
            created_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64,
            updated_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64,
        };
        
        if let Err(e) = metadata_store.save_entity_mapping(&mapping).await {
            tracing::error!("Failed to save entity mapping: {}", e);
            // 回滚所有操作
            let _ = client.delete("default", &bm25_doc.document_id).await;
            let _ = metadata_store.delete_entity(&entity_id).await;
            if let Some(qdrant) = qdrant_client {
                let _ = qdrant.delete_by_id(&entity_id).await;
            }
        }
    }
    
    /// 处理更新任务
    async fn process_update_task(
        bm25_client: &Arc<tokio::sync::Mutex<Bm25Client>>,
        metadata_store: &Arc<RedbMetadataStore>,
        qdrant_client: &Option<Arc<QdrantClient>>,
        entity_id: String,
        bm25_doc: Option<Bm25Document>,
        metadata: Option<EntityMetadata>,
        vector: Option<Vec<f32>>,
    ) {
        // 1. 更新 Redb
        if let Some(metadata) = metadata {
            if let Err(e) = metadata_store.update_entity(&entity_id, &metadata).await {
                tracing::error!("Failed to update entity in Redb: {}", e);
            }
        }
        
        // 2. 更新 BM25
        if let Some(bm25_doc) = bm25_doc {
            let mut client = bm25_client.lock().await;
            // 先删除旧文档，再插入新文档
            let _ = client.delete("default", &bm25_doc.document_id).await;
            if let Err(e) = client.index_document("default", &bm25_doc).await {
                tracing::error!("Failed to update document in BM25: {}", e);
            }
        }
        
        // 3. 更新 Qdrant
        if let (Some(qdrant), Some(vector)) = (qdrant_client, vector) {
            let point = VectorPoint::new(
                entity_id.clone(),
                vector,
                Payload::default(), // 需要实际 payload
            );
            if let Err(e) = qdrant.upsert_points(&[point]).await {
                tracing::error!("Failed to update point in Qdrant: {}", e);
            }
        }
        
        // 4. 更新映射关系
        if let Err(e) = metadata_store.update_entity_mapping_timestamp(&entity_id).await {
            tracing::error!("Failed to update entity mapping timestamp: {}", e);
        }
    }
    
    /// 处理删除任务
    async fn process_delete_task(
        bm25_client: &Arc<tokio::sync::Mutex<Bm25Client>>,
        metadata_store: &Arc<RedbMetadataStore>,
        qdrant_client: &Option<Arc<QdrantClient>>,
        entity_id: String,
        bm25_doc_id: Option<String>,
        qdrant_point_id: Option<String>,
    ) {
        // 1. 从 Redb 删除
        if let Err(e) = metadata_store.delete_entity(&entity_id).await {
            tracing::error!("Failed to delete entity from Redb: {}", e);
        }
        
        // 2. 从 BM25 删除
        if let Some(bm25_doc_id) = bm25_doc_id {
            let mut client = bm25_client.lock().await;
            if let Err(e) = client.delete("default", &bm25_doc_id).await {
                tracing::error!("Failed to delete document from BM25: {}", e);
            }
        }
        
        // 3. 从 Qdrant 删除
        if let (Some(qdrant), Some(point_id)) = (qdrant_client, qdrant_point_id) {
            if let Err(e) = qdrant.delete_by_id(&point_id).await {
                tracing::error!("Failed to delete point from Qdrant: {}", e);
            }
        }
        
        // 4. 删除映射关系
        if let Err(e) = metadata_store.delete_entity_mapping(&entity_id).await {
            tracing::error!("Failed to delete entity mapping: {}", e);
        }
    }
}
```

## 实施建议

### 阶段一：实体映射表（立即实施）
1. 在 Redb 中添加实体映射表
2. 修改 IndexOrchestrator 在索引时建立映射
3. 扩展 QueryOrchestrator 支持结果关联

### 阶段二：统一查询层（短期目标）
1. 实现 UnifiedQuerier
2. 提供丰富的查询接口
3. 支持混合结果排序和去重

### 阶段三：同步管理器（长期目标）
1. 实现 SyncManager
2. 添加任务队列和重试机制
3. 提供监控和管理接口

## 监控和运维

### 监控指标
- **同步延迟**: 任务从入队到完成的时间
- **同步成功率**: 成功同步的任务比例
- **队列长度**: 等待处理的任务数量
- **存储一致性**: 映射表的完整性检查

### 运维工具
- **一致性检查工具**: 检查三个存储的数据一致性
- **修复工具**: 修复不一致的数据
- **同步状态查看**: 查看同步任务状态
- **手动同步工具**: 手动触发同步操作

## 总结

本数据同步方案采用渐进式实施策略：

1. **实体映射表**提供最基本的数据关联能力
2. **统一查询层**提供丰富的查询接口
3. **同步管理器**提供完整的同步保障

这种设计平衡了实现复杂度和功能需求，可以逐步实施，逐步完善。最终实现三个存储引擎的协同工作，提供一致、高效的数据访问体验。