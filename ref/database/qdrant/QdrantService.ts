import { injectable, inject } from 'inversify';
import { LoggerService } from '../../utils/LoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { ConfigService } from '../../config/ConfigService';
import { TYPES } from '../../types';
import { ProjectIdManager } from '../ProjectIdManager';
import { IVectorStore, VectorPoint, CollectionInfo, SearchOptions, SearchResult } from './IVectorStore';
import { IQdrantConnectionManager } from './QdrantConnectionManager';
import { IQdrantCollectionManager } from './QdrantCollectionManager';
import { IQdrantVectorOperations } from './QdrantVectorOperations';
import { IQdrantQueryUtils } from './QdrantQueryUtils';
import { IQdrantProjectManager } from './QdrantProjectManager';
import {
  QdrantConfig,
  VectorDistance,
  CollectionCreateOptions,
  VectorUpsertOptions,
  VectorSearchOptions,
  QueryFilter,
  BatchResult,
  ProjectInfo,
  QdrantEventType,
  QdrantEvent
} from './QdrantTypes';
import { BaseDatabaseService } from '../common/BaseDatabaseService';
import { IDatabaseService, IConnectionManager, IProjectManager } from '../common/IDatabaseService';
import { DatabaseEventType, QdrantEventType as UnifiedQdrantEventType } from '../common/DatabaseEventTypes';
import { DatabaseLoggerService } from '../common/DatabaseLoggerService';
import { PerformanceMonitor } from '../../infrastructure/monitoring/PerformanceMonitor';
import { DatabaseError, DatabaseErrorType } from '../common/DatabaseError';
import { Subscription } from '../common/DatabaseEventTypes';

/**
 * Qdrant 服务类
 *
 * 作为外观模式，协调各个模块，提供统一的API接口
 * 保持向后兼容性，内部实现委托给各个专门的模块
 * 实现统一的数据库服务接口
 */
@injectable()
export class QdrantService extends BaseDatabaseService implements IVectorStore, IDatabaseService {
  private logger: LoggerService;
  private errorHandler: ErrorHandlerService;
  private projectIdManager: ProjectIdManager;
  protected connectionManager: IQdrantConnectionManager;
  private collectionManager: IQdrantCollectionManager;
  private vectorOperations: IQdrantVectorOperations;
  private queryUtils: IQdrantQueryUtils;
  protected projectManager: IQdrantProjectManager;

  constructor(
    @inject(TYPES.ConfigService) configService: ConfigService,
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.ProjectIdManager) projectIdManager: ProjectIdManager,
    @inject(TYPES.IQdrantConnectionManager) connectionManager: IQdrantConnectionManager,
    @inject(TYPES.IQdrantCollectionManager) collectionManager: IQdrantCollectionManager,
    @inject(TYPES.IQdrantVectorOperations) vectorOperations: IQdrantVectorOperations,
    @inject(TYPES.IQdrantQueryUtils) queryUtils: IQdrantQueryUtils,
    @inject(TYPES.IQdrantProjectManager) projectManager: IQdrantProjectManager,
    @inject(TYPES.DatabaseLoggerService) private databaseLogger: DatabaseLoggerService,
    @inject(TYPES.PerformanceMonitor) private performanceMonitor: PerformanceMonitor
  ) {
    // 调用父类构造函数，提供必要的依赖
    super(
      connectionManager as unknown as IConnectionManager,
      projectManager as unknown as IProjectManager
    );

    this.logger = logger;
    this.errorHandler = errorHandler;
    this.projectIdManager = projectIdManager;
    this.connectionManager = connectionManager;
    this.collectionManager = collectionManager;
    this.vectorOperations = vectorOperations;
    this.queryUtils = queryUtils;
    this.projectManager = projectManager;
  }

  /**
   * 初始化 Qdrant 服务
   */
  async initialize(): Promise<boolean> {
    try {
      // 初始化基础服务
      const baseInitialized = await super.initialize();
      if (!baseInitialized) {
        return false;
      }

      // 初始化连接管理器
      const connectionInitialized = await this.connectionManager.initialize();
      if (!connectionInitialized) {
        this.emitEvent('error', new Error('Failed to initialize Qdrant connection manager'));
        return false;
      }

      this.emitEvent('initialized', { timestamp: new Date() });

      return true;
    } catch (error) {
      this.emitEvent('error', error instanceof Error ? error : new Error(String(error)));
      return false;
    }
  }

  /**
   * 创建集合
   */
  async createCollection(
    name: string,
    vectorSize: number,
    distance: VectorDistance = 'Cosine',
    recreateIfExists: boolean = false
  ): Promise<boolean> {
    return this.collectionManager.createCollection(name, vectorSize, distance, recreateIfExists);
  }

  /**
   * 使用选项创建集合
   */
  async createCollectionWithOptions(name: string, options: CollectionCreateOptions): Promise<boolean> {
    return this.collectionManager.createCollectionWithOptions(name, options);
  }

  /**
   * 检查集合是否存在
   */
  async collectionExists(name: string): Promise<boolean> {
    return this.collectionManager.collectionExists(name);
  }

  /**
   * 删除集合
   */
  async deleteCollection(name: string): Promise<boolean> {
    return this.collectionManager.deleteCollection(name);
  }

  /**
   * 获取集合信息
   */
  async getCollectionInfo(collectionName: string): Promise<CollectionInfo | null> {
    return this.collectionManager.getCollectionInfo(collectionName);
  }

  /**
   * 获取集合统计信息
   */
  async getCollectionStats(collectionName: string): Promise<any> {
    return this.collectionManager.getCollectionStats(collectionName);
  }

  /**
   * 创建有效载荷索引
   */
  async createPayloadIndex(collectionName: string, field: string, fieldType?: string): Promise<boolean> {
    return this.collectionManager.createPayloadIndex(collectionName, field, fieldType);
  }

  /**
   * 批量创建有效载荷索引
   */
  async createPayloadIndexes(collectionName: string, fields: string[]): Promise<boolean> {
    return this.collectionManager.createPayloadIndexes(collectionName, fields);
  }

  /**
   * 列出所有集合
   */
  async listCollections(): Promise<string[]> {
    return this.collectionManager.listCollections();
  }

  /**
   * 插入或更新向量
   */
  async upsertVectors(collectionName: string, vectors: VectorPoint[]): Promise<boolean> {
    return this.vectorOperations.upsertVectors(collectionName, vectors);
  }

  /**
   * 使用选项插入或更新向量
   */
  async upsertVectorsWithOptions(collectionName: string, vectors: VectorPoint[], options?: VectorUpsertOptions): Promise<BatchResult> {
    const startTime = Date.now();
    try {
      const result = await this.vectorOperations.upsertVectorsWithOptions(collectionName, vectors, options);
      const duration = Date.now() - startTime;

      this.performanceMonitor.recordOperation('upsert_vectors', duration);

      return result;
    } catch (error) {
      const duration = Date.now() - startTime;
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_ERROR,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          operation: 'upsert_vectors',
          collectionName,
          vectorCount: vectors.length,
          duration,
          error: error instanceof Error ? error.message : String(error)
        }
      });
      throw error;
    }
  }

  /**
   * 搜索向量
   */
  async searchVectors(
    collectionName: string,
    query: number[],
    options: SearchOptions = {}
  ): Promise<SearchResult[]> {
    return this.vectorOperations.searchVectors(collectionName, query, options);
  }

  /**
   * 使用选项搜索向量
   */
  async searchVectorsWithOptions(collectionName: string, query: number[], options?: VectorSearchOptions): Promise<SearchResult[]> {
    const startTime = Date.now();
    try {
      const results = await this.vectorOperations.searchVectorsWithOptions(collectionName, query, options);
      const duration = Date.now() - startTime;

      this.performanceMonitor.recordOperation('search_vectors', duration);

      await this.databaseLogger.logQueryPerformance(`search in ${collectionName}`, duration, results.length);

      return results;
    } catch (error) {
      const duration = Date.now() - startTime;
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_ERROR,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          operation: 'search_vectors',
          collectionName,
          queryLength: query.length,
          duration,
          error: error instanceof Error ? error.message : String(error)
        }
      });
      throw error;
    }
  }

  /**
   * 删除点
   */
  async deletePoints(collectionName: string, pointIds: string[]): Promise<boolean> {
    return this.vectorOperations.deletePoints(collectionName, pointIds);
  }

  /**
   * 清空集合
   */
  async clearCollection(collectionName: string): Promise<boolean> {
    return this.vectorOperations.clearCollection(collectionName);
  }

  /**
   * 获取点数量
   */
  async getPointCount(collectionName: string): Promise<number> {
    return this.vectorOperations.getPointCount(collectionName);
  }

  /**
   * 根据文件路径获取块ID
   */
  async getChunkIdsByFiles(collectionName: string, filePaths: string[]): Promise<string[]> {
    return this.queryUtils.getChunkIdsByFiles(collectionName, filePaths);
  }

  /**
   * 获取已存在的块ID
   */
  async getExistingChunkIds(collectionName: string, chunkIds: string[]): Promise<string[]> {
    return this.queryUtils.getExistingChunkIds(collectionName, chunkIds);
  }

  /**
   * 滚动浏览点
   */
  async scrollPoints(collectionName: string, filter?: any, limit?: number, offset?: any): Promise<any[]> {
    return this.queryUtils.scrollPoints(collectionName, filter, limit, offset);
  }

  /**
   * 计算点数量
   */
  async countPoints(collectionName: string, filter?: any): Promise<number> {
    return this.queryUtils.countPoints(collectionName, filter);
  }

  /**
   * 构建查询过滤器
   */
  buildFilter(filter: SearchOptions['filter']): any {
    return this.queryUtils.buildFilter(filter);
  }

  /**
   * 构建高级查询过滤器
   */
  buildAdvancedFilter(filter: QueryFilter): any {
    return this.queryUtils.buildAdvancedFilter(filter);
  }

  /**
   * 为特定项目创建集合
   */
  async createCollectionForProject(projectPath: string, vectorSize: number, distance?: VectorDistance): Promise<boolean> {
    try {
      const result = await this.projectManager.createCollectionForProject(projectPath, vectorSize, distance);

      if (result) {
        this.emitEvent('project_space_created', { projectPath, vectorSize, distance });
      }

      return result;
    } catch (error) {
      this.emitEvent('error', error instanceof Error ? error : new Error(String(error)));
      throw error;
    }
  }

  /**
   * 为特定项目插入或更新向量
   */
  async upsertVectorsForProject(projectPath: string, vectors: VectorPoint[]): Promise<boolean> {
    try {
      const startTime = Date.now();
      const result = await this.projectManager.upsertVectorsForProject(projectPath, vectors);
      const duration = Date.now() - startTime;

      if (result) {
        this.emitEvent('data_inserted', { projectPath, vectorCount: vectors.length, duration });
      }

      return result;
    } catch (error) {
      this.emitEvent('error', error instanceof Error ? error : new Error(String(error)));
      throw error;
    }
  }

  /**
   * 在特定项目的集合中搜索向量
   */
  async searchVectorsForProject(projectPath: string, query: number[], options?: SearchOptions): Promise<SearchResult[]> {
    try {
      const startTime = Date.now();
      const results = await this.projectManager.searchVectorsForProject(projectPath, query, options);
      const duration = Date.now() - startTime;

      this.emitEvent('data_queried', { projectPath, queryLength: query.length, options, duration, resultCount: results.length });

      return results;
    } catch (error) {
      this.emitEvent('error', error instanceof Error ? error : new Error(String(error)));
      throw error;
    }
  }

  /**
   * 获取特定项目的集合信息
   */
  async getCollectionInfoForProject(projectPath: string): Promise<CollectionInfo | null> {
    return this.projectManager.getCollectionInfoForProject(projectPath);
  }

  /**
   * 删除特定项目的集合
   */
  async deleteCollectionForProject(projectPath: string): Promise<boolean> {
    try {
      const result = await this.projectManager.deleteCollectionForProject(projectPath);

      if (result) {
        this.emitEvent('project_space_deleted', { projectPath });
      }

      return result;
    } catch (error) {
      this.emitEvent('error', error instanceof Error ? error : new Error(String(error)));
      throw error;
    }
  }

  /**
   * 获取项目信息
   */
  async getProjectInfo(projectPath: string): Promise<ProjectInfo | null> {
    return this.projectManager.getProjectInfo(projectPath);
  }

  /**
   * 列出所有项目
   */
  async listProjects(): Promise<ProjectInfo[]> {
    return this.projectManager.listProjects();
  }

  /**
   * 根据项目ID获取项目路径
   */
  getProjectPath(projectId: string): string | undefined {
    return this.projectIdManager.getProjectPath(projectId);
  }

  /**
   * 删除项目中的特定向量
   */
  async deleteVectorsForProject(projectPath: string, vectorIds: string[]): Promise<boolean> {
    return this.projectManager.deleteVectorsForProject(projectPath, vectorIds);
  }

  /**
   * 清空项目
   */
  async clearProject(projectPath: string): Promise<boolean> {
    return this.projectManager.clearProject(projectPath);
  }

  /**
   * 检查是否已连接
   */
  isConnected(): boolean {
    return this.connectionManager.isConnected();
  }

  /**
   * 获取连接状态
   */
  getConnectionStatus(): any {
    return this.connectionManager.getConnectionStatus();
  }

  /**
   * 获取配置信息
   */
  getConfig(): QdrantConfig {
    return this.connectionManager.getConfig();
  }

  /**
   * 更新配置信息
   */
  updateConfig(config: Partial<QdrantConfig>): void {
    this.connectionManager.updateConfig(config);
  }

  /**
   * 关闭连接
   */
  async close(): Promise<void> {
    try {
      await this.connectionManager.close();
      await super.close();

      this.emitEvent('closed', { timestamp: new Date() });
    } catch (error) {
      this.emitEvent('error', error instanceof Error ? error : new Error(String(error)));
      throw error;
    }
  }

  /**
  * 订阅事件（推荐的新API）
  */
  subscribe(type: QdrantEventType | string, listener: (event: any) => void): Subscription {
    // 订阅所有底层管理器的同一事件
    const connectionSub = this.connectionManager.subscribe(type as QdrantEventType, listener);
    const collectionSub = this.collectionManager.subscribe(type as QdrantEventType, listener);
    const vectorSub = this.vectorOperations.subscribe(type as QdrantEventType, listener);
    const querySub = this.queryUtils.subscribe(type as QdrantEventType, listener);
    const projectSub = this.projectManager.subscribe(type as QdrantEventType, listener);

    // 返回一个组合订阅对象，取消时会取消所有订阅
    return {
      id: `qdrant-service-${Date.now()}`,
      eventType: type,
      handler: listener,
      unsubscribe: () => {
        connectionSub.unsubscribe();
        collectionSub.unsubscribe();
        vectorSub.unsubscribe();
        querySub.unsubscribe();
        projectSub.unsubscribe();
      }
    };
  }

  /**
   * 健康检查
   */
  /**
   * 健康检查
   */
  async healthCheck(): Promise<{
    status: 'healthy' | 'unhealthy';
    details?: any;
    error?: string;
  }> {
    try {
      const baseHealth = await super.healthCheck();

      if (baseHealth.status === 'unhealthy') {
        return baseHealth;
      }

      // 检查Qdrant特定组件
      const connectionStatus = this.isConnected();
      const collections = await this.listCollections();

      return {
        status: connectionStatus ? 'healthy' : 'unhealthy',
        details: {
          ...baseHealth.details,
          collectionsCount: collections.length,
          qdrantStatus: 'operational'
        }
      };
    } catch (error) {
      return {
        status: 'unhealthy',
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }
}