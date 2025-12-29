/**
 * 向量存储接口
 * 定义了向量数据库的基本操作
 */
export interface VectorPoint {
  id: string | number;
  vector: number[];
  payload: {
    content: string;
    filePath: string;
    language: string;
    chunkType: string[];
    startLine: number;
    endLine: number;
    functionName?: string;
    className?: string;
    snippetMetadata?: any;
    metadata: Record<string, any>;
    timestamp: Date;
    projectId?: string;
    
    // 保留上游模块提供的丰富信息
    complexity?: number;
    complexityAnalysis?: any;
    nestingLevel?: number;
    strategy?: string;
    isSignatureOnly?: boolean;
    originalStructure?: string;
    
    // AST和语义信息
    astNodes?: any;
    semanticBoundary?: any;
    
    // 其他有价值的元数据
    size?: number;
    lineCount?: number;
    hash?: string;
    overlapInfo?: any;
    contextLines?: string[];
  };
}

export interface CollectionInfo {
  name: string;
  vectors: {
    size: number;
    distance: 'Cosine' | 'Euclid' | 'Dot' | 'Manhattan';
  };
  pointsCount: number;
  status: 'green' | 'yellow' | 'red' | 'grey';
}

export interface SearchOptions {
  limit?: number;
  scoreThreshold?: number;
  filter?: {
    language?: string[];
    chunkType?: string[];
    filePath?: string[];
    projectId?: string;
    snippetType?: string[];
  };
  withPayload?: boolean;
  withVector?: boolean;
}

export interface SearchResult {
  id: string;
  score: number;
  payload: VectorPoint['payload'];
}

export interface IVectorStore {
  /**
   * 初始化向量存储连接
   */
  initialize(): Promise<boolean>;

  /**
   * 创建集合
   * @param name 集合名称
   * @param vectorSize 向量维度
   * @param distance 距离计算方式
   * @param recreateIfExists 如果存在是否重新创建
   */
  createCollection(
    name: string,
    vectorSize: number,
    distance?: 'Cosine' | 'Euclid' | 'Dot' | 'Manhattan',
    recreateIfExists?: boolean
  ): Promise<boolean>;

  /**
   * 检查集合是否存在
   * @param name 集合名称
   */
  collectionExists(name: string): Promise<boolean>;

  /**
   * 删除集合
   * @param name 集合名称
   */
  deleteCollection(name: string): Promise<boolean>;

  /**
   * 获取集合信息
   * @param collectionName 集合名称
   */
  getCollectionInfo(collectionName: string): Promise<CollectionInfo | null>;

  /**
   * 插入或更新向量点
   * @param collectionName 集合名称
   * @param vectors 向量点数组
   */
  upsertVectors(collectionName: string, vectors: VectorPoint[]): Promise<boolean>;

  /**
   * 搜索向量
   * @param collectionName 集合名称
   * @param query 查询向量
   * @param options 搜索选项
   */
  searchVectors(collectionName: string, query: number[], options?: SearchOptions): Promise<SearchResult[]>;

  /**
   * 删除向量点
   * @param collectionName 集合名称
   * @param pointIds 向量点ID数组
   */
  deletePoints(collectionName: string, pointIds: string[]): Promise<boolean>;

  /**
   * 清空集合
   * @param collectionName 集合名称
   */
  clearCollection(collectionName: string): Promise<boolean>;

  /**
   * 获取集合中的向量点数量
   * @param collectionName 集合名称
   */
  getPointCount(collectionName: string): Promise<number>;

  /**
   * 创建payload索引
   * @param collectionName 集合名称
   * @param field 字段名
   */
  createPayloadIndex(collectionName: string, field: string): Promise<boolean>;

  /**
   * 检查是否已连接到数据库
   */
  isConnected(): boolean;

  /**
   * 关闭连接
   */
  close(): Promise<void>;

  /**
   * 根据文件路径获取代码块ID
   * @param collectionName 集合名称
   * @param filePaths 文件路径数组
   */
  getChunkIdsByFiles(collectionName: string, filePaths: string[]): Promise<string[]>;

  /**
   * 获取已存在的代码块ID
   * @param collectionName 集合名称
   * @param chunkIds 代码块ID数组
   */
  getExistingChunkIds(collectionName: string, chunkIds: string[]): Promise<string[]>;
}