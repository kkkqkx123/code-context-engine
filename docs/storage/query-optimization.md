# 查询优化建议

## 当前查询架构分析

### 现有查询流程
```
用户查询 → QueryOrchestrator → 并行查询 → ResultMerger → 返回结果
                │           │           │
                ├─→ BM25 ───┘           │
                ├─→ Qdrant ─────────────┘
                └─→ Redb (关系查询) ──────┘
```

### 性能瓶颈识别
1. **查询延迟**: 并行查询等待最慢的存储响应
2. **结果合并**: 大量结果合并和排序开销
3. **数据关联**: 查询后需要从 Redb 获取元数据
4. **内存使用**: 大量中间结果占用内存

## 优化方案

### 方案一：查询流水线优化

#### 1.1 异步并行查询
```rust
impl QueryOrchestrator {
    /// 优化后的混合查询
    pub async fn optimized_hybrid_search(
        &self,
        query: &HybridQuery,
    ) -> Result<HybridQueryResult, OrchestratorError> {
        let start = std::time::Instant::now();
        
        // 并行执行所有查询
        let (bm25_future, vector_future, relation_future) = tokio::join!(
            self.execute_bm25_search_async(query),
            self.execute_vector_search_async(query),
            self.execute_relation_search_async(query),
        );
        
        // 收集结果
        let mut all_results = Vec::new();
        let mut sources_used = Vec::new();
        
        match bm25_future {
            Ok(results) => {
                all_results.extend(results);
                sources_used.push("bm25".to_string());
            }
            Err(e) => tracing::warn!("BM25 search failed: {}", e),
        }
        
        match vector_future {
            Ok(results) => {
                all_results.extend(results);
                sources_used.push("vector".to_string());
            }
            Err(e) => tracing::warn!("Vector search failed: {}", e),
        }
        
        match relation_future {
            Ok(results) => {
                all_results.extend(results);
                sources_used.push("relation".to_string());
            }
            Err(e) => tracing::warn!("Relation search failed: {}", e),
        }
        
        // 流式合并和排序
        let merged = self.streaming_merge(all_results, query.options.limit);
        
        Ok(HybridQueryResult {
            items: merged,
            total: all_results.len(),
            elapsed_ms: start.elapsed().as_millis() as u64,
            sources_used,
        })
    }
    
    /// 异步 BM25 搜索
    async fn execute_bm25_search_async(
        &self,
        query: &HybridQuery,
    ) -> Result<Vec<QueryResultItem>, OrchestratorError> {
        // 使用单独的运行时或线程池
        let bm25 = self.bm25.clone();
        let query = query.clone();
        
        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let mut client = bm25.lock().await;
                client.search("default", &query.query, query.options.limit as i32).await
            })
        })
        .await
        .map_err(|e| OrchestratorError::Query(QueryError::internal(e.to_string())))?
        .map(|results| {
            results.into_iter().map(|r| QueryResultItem {
                id: r.document_id,
                score: r.score,
                file_path: r.fields.get("file_path").cloned().unwrap_or_default(),
                code_chunk: r.fields.get("content").cloned().unwrap_or_default(),
                start_line: r.fields.get("start_line").and_then(|s| s.parse::<u32>().ok()).unwrap_or(1),
                end_line: r.fields.get("end_line").and_then(|s| s.parse::<u32>().ok()).unwrap_or(1),
                entity_type: None,
                source: "bm25".to_string(),
                call_chain: None,
            })
            .collect()
        })
    }
}
```

#### 1.2 流式结果合并
```rust
impl ResultMerger {
    /// 流式合并结果（减少内存使用）
    pub fn streaming_merge(
        &self,
        results: Vec<QueryResultItem>,
        limit: usize,
    ) -> Vec<QueryResultItem> {
        use std::collections::BinaryHeap;
        use std::cmp::Reverse;
        
        // 使用最小堆维护 top-k 结果
        let mut heap = BinaryHeap::new();
        
        for item in results {
            // 使用 Reverse 实现最大堆（按分数降序）
            heap.push(Reverse(ScoredItem {
                score: item.score,
                item,
            }));
            
            // 保持堆大小不超过 limit
            if heap.len() > limit {
                heap.pop();
            }
        }
        
        // 从堆中提取结果（已按分数排序）
        heap.into_sorted_vec()
            .into_iter()
            .map(|Reverse(scored)| scored.item)
            .collect()
    }
}

/// 带分数的项（用于堆排序）
#[derive(Debug, Clone)]
struct ScoredItem {
    score: f32,
    item: QueryResultItem,
}

impl PartialEq for ScoredItem {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score
    }
}

impl Eq for ScoredItem {}

impl PartialOrd for ScoredItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.score.partial_cmp(&other.score)
    }
}

impl Ord for ScoredItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap_or(std::cmp::Ordering::Equal)
    }
}
```

### 方案二：查询缓存优化

#### 2.1 多级查询缓存
```rust
/// 查询缓存管理器
pub struct QueryCacheManager {
    /// L1 缓存：内存缓存（高频查询）
    l1_cache: Arc<Mutex<LruCache<String, CachedQueryResult>>>,
    /// L2 缓存：磁盘缓存（中频查询）
    l2_cache: Arc<RedbMetadataStore>,
    /// 缓存统计
    stats: Arc<AtomicU64>,
}

impl QueryCacheManager {
    /// 获取缓存结果
    pub async fn get_cached(
        &self,
        query_key: &str,
        ttl: Duration,
    ) -> Option<CachedQueryResult> {
        // 1. 检查 L1 缓存
        if let Some(result) = self.l1_cache.lock().await.get(query_key) {
            if result.timestamp.elapsed() < ttl {
                self.stats.fetch_add(1, Ordering::Relaxed);
                return Some(result.clone());
            }
        }
        
        // 2. 检查 L2 缓存
        if let Ok(Some(cached)) = self.l2_cache.get_query_cache(query_key).await {
            if cached.timestamp.elapsed() < ttl {
                // 提升到 L1 缓存
                self.l1_cache.lock().await.put(query_key.to_string(), cached.clone());
                return Some(cached);
            }
        }
        
        None
    }
    
    /// 设置缓存结果
    pub async fn set_cached(
        &self,
        query_key: String,
        result: CachedQueryResult,
        ttl: Duration,
    ) {
        // 1. 设置 L1 缓存
        self.l1_cache.lock().await.put(query_key.clone(), result.clone());
        
        // 2. 异步设置 L2 缓存
        let l2_cache = self.l2_cache.clone();
        let query_key_clone = query_key.clone();
        tokio::spawn(async move {
            if let Err(e) = l2_cache.set_query_cache(&query_key_clone, &result, ttl).await {
                tracing::warn!("Failed to set L2 cache: {}", e);
            }
        });
    }
    
    /// 生成查询缓存键
    pub fn generate_cache_key(
        &self,
        query: &str,
        query_type: &QueryType,
        options: &QueryOptions,
    ) -> String {
        use sha2::{Sha256, Digest};
        
        let mut hasher = Sha256::new();
        hasher.update(query.as_bytes());
        hasher.update(query_type.to_string().as_bytes());
        hasher.update(options.limit.to_string().as_bytes());
        
        if let Some(min_score) = options.min_score {
            hasher.update(min_score.to_string().as_bytes());
        }
        
        if let Some(ref prefix) = options.directory_prefix {
            hasher.update(prefix.as_bytes());
        }
        
        format!("query:{}", hex::encode(hasher.finalize()))
    }
}
```

#### 2.2 缓存预热策略
```rust
impl QueryCacheManager {
    /// 预热缓存
    pub async fn warmup_cache(&self, warmup_queries: Vec<&str>) {
        let mut tasks = Vec::new();
        
        for query in warmup_queries {
            let cache = self.clone();
            let query = query.to_string();
            
            tasks.push(tokio::spawn(async move {
                // 执行查询并缓存结果
                let cache_key = cache.generate_cache_key(
                    &query,
                    &QueryType::Hybrid,
                    &QueryOptions::default(),
                );
                
                // 检查是否已有缓存
                if cache.get_cached(&cache_key, Duration::from_secs(3600)).await.is_none() {
                    // 执行查询（这里需要实际的查询执行器）
                    // let result = execute_query(&query).await;
                    // cache.set_cached(cache_key, result, Duration::from_secs(3600)).await;
                }
            }));
        }
        
        // 并行执行预热任务
        futures::future::join_all(tasks).await;
    }
    
    /// 智能缓存淘汰
    pub async fn smart_eviction(&self) {
        let mut l1_cache = self.l1_cache.lock().await;
        let now = Instant::now();
        
        // 淘汰过期缓存
        l1_cache.retain(|_, cached| {
            now.duration_since(cached.timestamp) < Duration::from_secs(3600)
        });
        
        // 如果缓存仍然太大，淘汰最不常用的
        if l1_cache.len() > 1000 {
            while l1_cache.len() > 800 {
                l1_cache.pop_lru();
            }
        }
    }
}
```

### 方案三：查询下推优化

#### 3.1 BM25 查询优化
```rust
impl Bm25Client {
    /// 优化后的搜索（支持更多查询选项）
    pub async fn optimized_search(
        &mut self,
        index_name: &str,
        query: &str,
        options: &Bm25SearchOptions,
    ) -> Result<Vec<Bm25SearchResult>, Bm25Error> {
        let manager = self.index_manager.as_ref().ok_or(Bm25Error::Disabled)?;
        
        let manager_guard = manager.read().await;
        let schema = manager_guard.schema();
        
        // 构建优化后的查询选项
        let search_options = SearchOptions {
            limit: options.limit,
            offset: options.offset,
            field_weights: options.field_weights.clone(),
            highlight: options.highlight,
            boost_title: options.boost_title,
            filter: options.filter.clone(),
            sort_by: options.sort_by.clone(),
        };
        
        // 使用更高效的搜索算法
        let (results, max_score) = if options.use_fuzzy {
            self.fuzzy_search(&manager_guard, schema, query, &search_options)?
        } else if options.use_phrase {
            self.phrase_search(&manager_guard, schema, query, &search_options)?
        } else {
            search(&manager_guard, schema, query, &search_options)?
        };
        
        // 应用后处理
        let processed_results = if options.group_by_file {
            self.group_results_by_file(results)
        } else {
            results.into_iter().map(|r| r.into()).collect()
        };
        
        tracing::debug!(
            "Optimized BM25 search for '{}' returned {} results (max_score: {})",
            query,
            processed_results.len(),
            max_score
        );
        
        Ok(processed_results)
    }
    
    /// 模糊搜索
    fn fuzzy_search(
        &self,
        manager: &IndexManager,
        schema: &IndexSchema,
        query: &str,
        options: &SearchOptions,
    ) -> Result<(Vec<SearchResult>, f32), Bm25Error> {
        // 实现模糊搜索逻辑
        // ...
    }
    
    /// 短语搜索
    fn phrase_search(
        &self,
        manager: &IndexManager,
        schema: &IndexSchema,
        query: &str,
        options: &SearchOptions,
    ) -> Result<(Vec<SearchResult>, f32), Bm25Error> {
        // 实现短语搜索逻辑
        // ...
    }
    
    /// 按文件分组结果
    fn group_results_by_file(&self, results: Vec<SearchResult>) -> Vec<Bm25SearchResult> {
        use std::collections::HashMap;
        
        let mut groups: HashMap<String, Vec<SearchResult>> = HashMap::new();
        
        for result in results {
            if let Some(file_path) = result.fields.get("file_path") {
                groups.entry(file_path.clone()).or_default().push(result);
            }
        }
        
        // 对每个文件的结果进行合并和排序
        let mut grouped_results = Vec::new();
        
        for (file_path, file_results) in groups {
            // 取每个文件的前 N 个结果
            let mut sorted = file_results;
            sorted.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
            
            for result in sorted.into_iter().take(3) {
                grouped_results.push(Bm25SearchResult {
                    document_id: result.document_id,
                    score: result.score,
                    fields: result.fields,
                    highlights: result.highlights,
                });
            }
        }
        
        grouped_results
    }
}

/// BM25 搜索选项
#[derive(Debug, Clone)]
pub struct Bm25SearchOptions {
    /// 结果数量限制
    pub limit: usize,
    /// 偏移量
    pub offset: usize,
    /// 字段权重
    pub field_weights: HashMap<String, f32>,
    /// 是否高亮
    pub highlight: bool,
    /// 标题字段权重提升
    pub boost_title: bool,
    /// 过滤器
    pub filter: Option<SearchFilter>,
    /// 排序方式
    pub sort_by: Option<SortOption>,
    /// 使用模糊搜索
    pub use_fuzzy: bool,
    /// 使用短语搜索
    pub use_phrase: bool,
    /// 按文件分组
    pub group_by_file: bool,
}

/// 搜索过滤器
#[derive(Debug, Clone)]
pub struct SearchFilter {
    /// 文件路径前缀
    pub file_path_prefix: Option<String>,
    /// 实体类型过滤
    pub entity_types: Vec<String>,
    /// 最小分数
    pub min_score: Option<f32>,
    /// 最大分数
    pub max_score: Option<f32>,
}

/// 排序选项
#[derive(Debug, Clone)]
pub enum SortOption {
    /// 按分数降序
    ScoreDesc,
    /// 按分数升序
    ScoreAsc,
    /// 按文件路径
    FilePath,
    /// 按实体名称
    EntityName,
}
```

#### 3.2 向量查询优化
```rust
impl QdrantClient {
    /// 优化后的向量搜索
    pub async fn optimized_search(
        &self,
        query: SearchQuery,
        options: &VectorSearchOptions,
    ) -> Result<Vec<VectorSearchResult>, QdrantError> {
        // 构建优化后的搜索请求
        let mut request = SearchPoints {
            collection_name: self.collection_name.clone(),
            vector: query.vector,
            limit: query.limit as u64,
            with_payload: Some(true.into()),
            with_vector: Some(false.into()),
            filter: options.filter.clone(),
            params: Some(SearchParams {
                hnsw_ef: options.hnsw_ef,
                exact: options.exact,
                quantization: options.quantization.clone(),
                ..Default::default()
            }),
            score_threshold: options.min_score,
            ..Default::default()
        };
        
        // 添加预过滤（如果可用）
        if let Some(ref pre_filter) = options.pre_filter {
            if let Some(ref mut filter) = request.filter {
                filter.must.extend(pre_filter.must.clone());
                filter.should.extend(pre_filter.should.clone());
                filter.must_not.extend(pre_filter.must_not.clone());
            } else {
                request.filter = Some(pre_filter.clone());
            }
        }
        
        // 执行搜索
        let response = self.client
            .search_points(&request)
            .await
            .map_err(|e| QdrantError::Search(e.to_string()))?;
        
        // 处理结果
        let results = response.result.into_iter().map(|point| {
            VectorSearchResult {
                id: point.id.unwrap().str_id.unwrap_or_default(),
                score: point.score.unwrap_or(0.0),
                payload: point.payload.into_iter()
                    .map(|(k, v)| (k, v.into_string().unwrap_or_default()))
                    .collect(),
                vector: point.vector.map(|v| v.data),
            }
        }).collect();
        
        Ok(results)
    }
    
    /// 批量搜索（减少网络开销）
    pub async fn batch_search(
        &self,
        queries: Vec<SearchQuery>,
        options: &VectorSearchOptions,
    ) -> Result<Vec<Vec<VectorSearchResult>>, QdrantError> {
        let requests = queries.into_iter().map(|query| {
            SearchPoints {
                collection_name: self.collection_name.clone(),
                vector: query.vector,
                limit: query.limit as u64,
                with_payload: Some(true.into()),
                with_vector: Some(false.into()),
                filter: options.filter.clone(),
                params: Some(SearchParams {
                    hnsw_ef: options.hnsw_ef,
                    exact: options.exact,
                    quantization: options.quantization.clone(),
                    ..Default::default()
                }),
                score_threshold: options.min_score,
                ..Default::default()
            }
        }).collect();
        
        let request = SearchBatchPoints {
            collection_name: self.collection_name.clone(),
            search_points: requests,
            ..Default::default()
        };
        
        let response = self.client
            .search_batch_points(&request)
            .await
            .map_err(|e| QdrantError::Search(e.to_string()))?;
        
        let results = response.result.into_iter().map(|points| {
            points.result.into_iter().map(|point| {
                VectorSearchResult {
                    id: point.id.unwrap().str_id.unwrap_or_default(),
                    score: point.score.unwrap_or(0.0),
                    payload: point.payload.into_iter()
                        .map(|(k, v)| (k, v.into_string().unwrap_or_default()))
                        .collect(),
                    vector: point.vector.map(|v| v.data),
                }
            }).collect()
        }).collect();
        
        Ok(results)
    }
}

/// 向量搜索选项
#[derive(Debug, Clone)]
pub struct VectorSearchOptions {
    /// HNSW ef 参数
    pub hnsw_ef: Option<u32>,
    /// 是否使用精确搜索
    pub exact: bool,
    /// 量化配置
    pub quantization: Option<QuantizationConfig>,
    /// 最小分数阈值
    pub min_score: Option<f32>,
    /// 预过滤器
    pub pre_filter: Option<Filter>,
    /// 后过滤器
    pub post_filter: Option<Filter>,
}
```

### 方案四：智能查询路由

#### 4.1 查询分析器
```rust
/// 查询分析器
pub struct QueryAnalyzer {
    /// 关键词提取器
    keyword_extractor: KeywordExtractor,
    /// 语义分析器
    semantic_analyzer: SemanticAnalyzer,
    /// 查询分类器
    query_classifier: QueryClassifier,
}

impl QueryAnalyzer {
    /// 分析查询并推荐查询策略
    pub async fn analyze_query(&self, query: &str) -> QueryAnalysis {
        // 提取关键词
        let keywords = self.keyword_extractor.extract(query);
        
        // 分析语义
        let semantic = self.semantic_analyzer.analyze(query).await;
        
        // 分类查询类型
        let query_type = self.query_classifier.classify(query, &keywords, &semantic);
        
        // 推荐查询策略
        let strategy = self.recommend_strategy(&query_type, &keywords, &semantic);
        
        QueryAnalysis {
            original_query: query.to_string(),
            keywords,
            semantic,
            query_type,
            recommended_strategy: strategy,
        }
    }
    
    /// 推荐查询策略
    fn recommend_strategy(
        &self,
        query_type: &QueryType,
        keywords: &[String],
        semantic: &SemanticAnalysis,
    ) -> QueryStrategy {
        match query_type {
            QueryType::Keyword => {
                if keywords.len() == 1 && keywords[0].len() > 10 {
                    // 长关键词，使用 BM25 短语搜索
                    QueryStrategy::Bm25Phrase
                } else if keywords.len() > 3 {
                    // 多个关键词，使用 BM25 布尔搜索
                    QueryStrategy::Bm25Boolean
                } else {
                    // 普通关键词搜索
                    QueryStrategy::Bm25Standard
                }
            }
            QueryType::Semantic => {
                if semantic.has_technical_terms {
                    // 包含技术术语，使用向量搜索
                    QueryStrategy::VectorSemantic
                } else {
                    // 普通语义搜索
                    QueryStrategy::VectorStandard
                }
            }
            QueryType::Hybrid => {
                if keywords.is_empty() {
                    // 没有关键词，主要使用向量搜索
                    QueryStrategy::HybridVectorWeighted
                } else if semantic.confidence > 0.7 {
                    // 高置信度语义，平衡搜索
                    QueryStrategy::HybridBalanced
                } else {
                    // 低置信度语义，主要使用关键词搜索
                    QueryStrategy::HybridKeywordWeighted
                }
            }
            QueryType::Relation => {
                QueryStrategy::RelationOnly
            }
            QueryType::Comprehensive => {
                QueryStrategy::AllSources
            }
        }
    }
}

/// 查询分析结果
#[derive(Debug, Clone)]
pub struct QueryAnalysis {
    /// 原始查询
    pub original_query: String,
    /// 提取的关键词
    pub keywords: Vec<String>,
    /// 语义分析结果
    pub semantic: SemanticAnalysis,
    /// 查询类型
    pub query_type: QueryType,
    /// 推荐的查询策略
    pub recommended_strategy: QueryStrategy,
}

/// 查询策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryStrategy {
    /// 标准 BM25 搜索
    Bm25Standard,
    /// BM25 短语搜索
    Bm25Phrase,
    /// BM25 布尔搜索
    Bm25Boolean,
    /// 标准向量搜索
    VectorStandard,
    /// 语义向量搜索
    VectorSemantic,
    /// 混合搜索（平衡）
    HybridBalanced,
    /// 混合搜索（侧重关键词）
    HybridKeywordWeighted,
    /// 混合搜索（侧重向量）
    HybridVectorWeighted,
    /// 仅关系查询
    RelationOnly,
    /// 所有数据源
    AllSources,
}

/// 语义分析结果
#[derive(Debug, Clone)]
pub struct SemanticAnalysis {
    /// 语义向量
    pub embedding: Option<Vec<f32>>,
    /// 置信度分数
    pub confidence: f32,
    /// 是否包含技术术语
    pub has_technical_terms: bool,
    /// 查询意图
    pub intent: QueryIntent,
}

/// 查询意图
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryIntent {
    /// 查找函数定义
    FindFunction,
    /// 查找类定义
    FindClass,
    /// 查找调用关系
    FindCalls,
    /// 查找依赖
    FindDependencies,
    /// 查找示例
    FindExamples,
    /// 查找错误
    FindErrors,
    /// 未知意图
    Unknown,
}
```

#### 4.2 智能查询路由器
```rust
/// 智能查询路由器
pub struct SmartQueryRouter {
    /// 查询分析器
    analyzer: QueryAnalyzer,
    /// BM25 客户端
    bm25_client: Arc<tokio::sync::Mutex<Bm25Client>>,
    /// Qdrant 客户端
    qdrant_client: Option<Arc<QdrantClient>>,
    /// Redb 元数据存储
    metadata_store: Arc<RedbMetadataStore>,
    /// 查询缓存
    cache: QueryCacheManager,
    /// 性能监控
    monitor: QueryMonitor,
}

impl SmartQueryRouter {
    /// 智能路由查询
    pub async fn route_query(
        &self,
        query: &str,
        options: QueryOptions,
    ) -> Result<Vec<EnrichedResultItem>, OrchestratorError> {
        let start = Instant::now();
        
        // 1. 分析查询
        let analysis = self.analyzer.analyze_query(query).await;
        
        // 2. 检查缓存
        let cache_key = self.cache.generate_cache_key(
            query,
            &analysis.query_type,
            &options,
        );
        
        if let Some(cached) = self.cache.get_cached(&cache_key, Duration::from_secs(300)).await {
            self.monitor.record_cache_hit();
            return Ok(cached.results);
        }
        
        // 3. 根据策略执行查询
        let results = match analysis.recommended_strategy {
            QueryStrategy::Bm25Standard => {
                self.execute_bm25_standard(query, &analysis, &options).await?
            }
            QueryStrategy::Bm25Phrase => {
                self.execute_bm25_phrase(query, &analysis, &options).await?
            }
            QueryStrategy::Bm25Boolean => {
                self.execute_bm25_boolean(query, &analysis, &options).await?
            }
            QueryStrategy::VectorStandard => {
                self.execute_vector_standard(query, &analysis, &options).await?
            }
            QueryStrategy::VectorSemantic => {
                self.execute_vector_semantic(query, &analysis, &options).await?
            }
            QueryStrategy::HybridBalanced => {
                self.execute_hybrid_balanced(query, &analysis, &options).await?
            }
            QueryStrategy::HybridKeywordWeighted => {
                self.execute_hybrid_keyword_weighted(query, &analysis, &options).await?
            }
            QueryStrategy::HybridVectorWeighted => {
                self.execute_hybrid_vector_weighted(query, &analysis, &options).await?
            }
            QueryStrategy::RelationOnly => {
                self.execute_relation_only(query, &analysis, &options).await?
            }
            QueryStrategy::AllSources => {
                self.execute_all_sources(query, &analysis, &options).await?
            }
        };
        
        // 4. 丰富结果
        let enriched_results = self.enrich_results(results, &analysis, &options).await?;
        
        // 5. 缓存结果
        let cached_result = CachedQueryResult {
            results: enriched_results.clone(),
            timestamp: Instant::now(),
            strategy: analysis.recommended_strategy,
            analysis: analysis.clone(),
        };
        
        self.cache.set_cached(cache_key, cached_result, Duration::from_secs(300)).await;
        
        // 6. 记录性能指标
        let duration = start.elapsed();
        self.monitor.record_query(
            query,
            &analysis.query_type,
            &analysis.recommended_strategy,
            duration,
            enriched_results.len(),
        );
        
        Ok(enriched_results)
    }
    
    /// 执行 BM25 标准搜索
    async fn execute_bm25_standard(
        &self,
        query: &str,
        analysis: &QueryAnalysis,
        options: &QueryOptions,
    ) -> Result<Vec<QueryResultItem>, OrchestratorError> {
        let mut client = self.bm25_client.lock().await;
        
        let bm25_options = Bm25SearchOptions {
            limit: options.limit,
            offset: 0,
            field_weights: options.bm25_field_weights.clone(),
            highlight: true,
            boost_title: true,
            filter: None,
            sort_by: Some(SortOption::ScoreDesc),
            use_fuzzy: false,
            use_phrase: false,
            group_by_file: false,
        };
        
        client.optimized_search("default", query, &bm25_options)
            .await
            .map(|results| {
                results.into_iter().map(|r| QueryResultItem {
                    id: r.document_id,
                    score: r.score,
                    file_path: r.fields.get("file_path").cloned().unwrap_or_default(),
                    code_chunk: r.fields.get("content").cloned().unwrap_or_default(),
                    start_line: r.fields.get("start_line").and_then(|s| s.parse::<u32>().ok()).unwrap_or(1),
                    end_line: r.fields.get("end_line").and_then(|s| s.parse::<u32>().ok()).unwrap_or(1),
                    entity_type: None,
                    source: "bm25".to_string(),
                    call_chain: None,
                })
                .collect()
            })
            .map_err(OrchestratorError::from)
    }
    
    /// 执行混合平衡搜索
    async fn execute_hybrid_balanced(
        &self,
        query: &str,
        analysis: &QueryAnalysis,
        options: &QueryOptions,
    ) -> Result<Vec<QueryResultItem>, OrchestratorError> {
        // 并行执行 BM25 和向量搜索
        let (bm25_results, vector_results) = tokio::join!(
            self.execute_bm25_standard(query, analysis, options),
            self.execute_vector_standard(query, analysis, options),
        );
        
        let bm25_results = bm25_results.unwrap_or_default();
        let vector_results = vector_results.unwrap_or_default();
        
        // 平衡合并（各取一半）
        let mut all_results = Vec::new();
        
        let bm25_limit = options.limit / 2;
        let vector_limit = options.limit - bm25_limit;
        
        all_results.extend(bm25_results.into_iter().take(bm25_limit));
        all_results.extend(vector_results.into_iter().take(vector_limit));
        
        Ok(all_results)
    }
    
    /// 丰富结果
    async fn enrich_results(
        &self,
        results: Vec<QueryResultItem>,
        analysis: &QueryAnalysis,
        options: &QueryOptions,
    ) -> Result<Vec<EnrichedResultItem>, OrchestratorError> {
        if !options.include_relations && !options.include_call_chain {
            // 不需要丰富，直接返回
            return Ok(results.into_iter().map(|r| EnrichedResultItem {
                entity_id: r.id,
                entity_type: r.entity_type.unwrap_or_default(),
                entity_name: String::new(),
                file_path: r.file_path,
                score: r.score,
                sources: vec![r.source],
                code_snippet: Some(r.code_chunk),
                relations: None,
                metadata: HashMap::new(),
            }).collect());
        }
        
        // 并行丰富结果
        let mut tasks = Vec::new();
        
        for result in results {
            let metadata_store = self.metadata_store.clone();
            let result = result.clone();
            
            tasks.push(tokio::spawn(async move {
                // 获取实体 ID
                let entity_id = if result.source == "bm25" {
                    metadata_store.get_entity_id_for_bm25(&result.id).await.ok().flatten()
                } else if result.source == "vector" {
                    metadata_store.get_entity_id_for_qdrant(&result.id).await.ok().flatten()
                } else {
                    Some(result.id.clone())
                };
                
                if let Some(entity_id) = entity_id {
                    // 获取元数据
                    let metadata = metadata_store.get_entity_metadata(&entity_id).await.ok().flatten();
                    
                    // 获取关系信息
                    let relations = if options.include_relations {
                        metadata_store.get_entity_relations(&entity_id).await.ok()
                    } else {
                        None
                    };
                    
                    Some(EnrichedResultItem {
                        entity_id,
                        entity_type: metadata.as_ref().map(|m| m.entity_type.clone()).unwrap_or_default(),
                        entity_name: metadata.as_ref().map(|m| m.entity_name.clone()).unwrap_or_default(),
                        file_path: result.file_path,
                        score: result.score,
                        sources: vec![result.source],
                        code_snippet: Some(result.code_chunk),
                        relations,
                        metadata: HashMap::new(),
                    })
                } else {
                    None
                }
            }));
        }
        
        // 等待所有任务完成
        let task_results = futures::future::join_all(tasks).await;
        
        // 收集成功的结果
        let mut enriched_results = Vec::new();
        for task_result in task_results {
            if let Ok(Some(enriched)) = task_result {
                enriched_results.push(enriched);
            }
        }
        
        Ok(enriched_results)
    }
}
```

## 实施路线图

### 阶段一：基础优化（1-2周）
1. **实现异步并行查询**
   - 修改 QueryOrchestrator 使用 tokio::join
   - 添加超时机制
   - 实现错误处理

2. **实现流式结果合并**
   - 添加 ScoredItem 结构
   - 实现最小堆排序
   - 优化内存使用

### 阶段二：缓存优化（2-3周）
1. **实现查询缓存**
   - 添加 QueryCacheManager
   - 实现 L1/L2 缓存
   - 添加缓存预热

2. **添加智能缓存淘汰**
   - 基于时间的淘汰
   - 基于使用频率的淘汰
   - 基于大小的淘汰

### 阶段三：查询下推（3-4周）
1. **优化 BM25 查询**
   - 添加模糊搜索支持
   - 添加短语搜索支持
   - 添加分组功能

2. **优化向量查询**
   - 添加批量搜索
   - 优化搜索参数
   - 添加预过滤

### 阶段四：智能路由（4-6周）
1. **实现查询分析器**
   - 关键词提取
   - 语义分析
   - 查询分类

2. **实现智能路由器**
   - 策略选择
   - 结果丰富
   - 性能监控

## 预期收益

### 性能提升
- **查询延迟**: 减少 30-50%（通过并行和缓存）
- **吞吐量**: 提高 2-3 倍（通过优化和批处理）
- **内存使用**: 减少 50-70%（通过流式处理）

### 功能增强
- **查询准确性**: 提高 20-30%（通过智能路由）
- **结果相关性**: 提高 40-50%（通过结果丰富）
- **用户体验**: 显著改善（通过快速响应）

### 运维改进
- **可观测性**: 完整的监控指标
- **可调优性**: 灵活的配置选项
- **可扩展性**: 支持水平扩展

## 监控指标

### 关键性能指标
- **查询延迟 P95/P99**
- **缓存命中率**
- **各存储引擎响应时间**
- **结果丰富度**
- **内存使用率**

### 业务指标
- **查询成功率**
- **用户满意度**
- **功能使用率**
- **错误率**

## 总结

本查询优化方案通过四个层次的优化：
1. **基础优化**: 异步并行和流式处理
2. **缓存优化**: 多级缓存和智能淘汰
3. **查询下推**: 存储引擎级别优化
4. **智能路由**: 基于查询分析的策略选择

这些优化可以显著提升查询性能，改善用户体验，同时保持系统的可维护性和可扩展性。建议按照实施路线图逐步推进，每完成一个阶段都进行性能测试和验证。