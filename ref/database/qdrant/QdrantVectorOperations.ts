import { injectable, inject } from 'inversify';
import { LoggerService } from '../../utils/LoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { TYPES } from '../../types';
import { IQdrantConnectionManager } from './QdrantConnectionManager';
import { IQdrantCollectionManager } from './QdrantCollectionManager';
import { DatabaseLoggerService } from '../common/DatabaseLoggerService';
import { PerformanceMonitor } from '../../infrastructure/monitoring/PerformanceMonitor';
import { DatabaseEventType } from '../common/DatabaseEventTypes';
import {
  VectorPoint,
  SearchOptions,
  SearchResult
} from './IVectorStore';
import {
  VectorUpsertOptions,
  VectorSearchOptions,
  BatchResult,
  DEFAULT_VECTOR_UPSERT_OPTIONS,
  DEFAULT_VECTOR_SEARCH_OPTIONS,
  ERROR_MESSAGES,
  QdrantEventType,
  QdrantEvent
} from './QdrantTypes';
import { HashUtils } from '../../utils/cache/HashUtils';

/**
 * Qdrant 向量操作接口
 */
export interface IQdrantVectorOperations {
  upsertVectors(collectionName: string, vectors: VectorPoint[]): Promise<boolean>;
  upsertVectorsWithOptions(collectionName: string, vectors: VectorPoint[], options?: VectorUpsertOptions): Promise<BatchResult>;
  searchVectors(collectionName: string, query: number[], options?: SearchOptions): Promise<SearchResult[]>;
  searchVectorsWithOptions(collectionName: string, query: number[], options?: VectorSearchOptions): Promise<SearchResult[]>;
  deletePoints(collectionName: string, pointIds: string[]): Promise<boolean>;
  clearCollection(collectionName: string): Promise<boolean>;
  getPointCount(collectionName: string): Promise<number>;
  subscribe(type: QdrantEventType, listener: (event: QdrantEvent) => void): { id: string; eventType: string; handler: any; unsubscribe: () => void };
}

/**
 * Qdrant 向量操作实现
 * 
 * 负责向量的插入、更新、删除、搜索等操作
 */
@injectable()
export class QdrantVectorOperations implements IQdrantVectorOperations {
  private logger: LoggerService;
  private errorHandler: ErrorHandlerService;
  private databaseLogger: DatabaseLoggerService;
  private performanceMonitor: PerformanceMonitor;
  private connectionManager: IQdrantConnectionManager;
  private collectionManager: IQdrantCollectionManager;
  private eventListeners: Map<QdrantEventType, ((event: QdrantEvent) => void)[]> = new Map();

  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.IQdrantConnectionManager) connectionManager: IQdrantConnectionManager,
    @inject(TYPES.IQdrantCollectionManager) collectionManager: IQdrantCollectionManager,
    @inject(TYPES.DatabaseLoggerService) databaseLogger: DatabaseLoggerService,
    @inject(TYPES.PerformanceMonitor) performanceMonitor: PerformanceMonitor
  ) {
    this.logger = logger;
    this.errorHandler = errorHandler;
    this.connectionManager = connectionManager;
    this.collectionManager = collectionManager;
    this.databaseLogger = databaseLogger;
    this.performanceMonitor = performanceMonitor;
  }

  /**
   * 插入或更新向量
   */
  async upsertVectors(collectionName: string, vectors: VectorPoint[]): Promise<boolean> {
    const result = await this.upsertVectorsWithOptions(collectionName, vectors);
    return result.success;
  }

  /**
   * 使用选项插入或更新向量
   */
  async upsertVectorsWithOptions(collectionName: string, vectors: VectorPoint[], options?: VectorUpsertOptions): Promise<BatchResult> {
    const finalOptions = { ...DEFAULT_VECTOR_UPSERT_OPTIONS, ...options };
    const batchResult: BatchResult = {
      success: true,
      processedCount: 0,
      failedCount: 0,
      errors: []
    };

    try {
      const client = this.connectionManager.getClient();
      if (!client) {
        throw new Error(ERROR_MESSAGES.CONNECTION_FAILED);
      }

      if (vectors.length === 0) {
        await this.databaseLogger.logDatabaseEvent({
          type: DatabaseEventType.SERVICE_INITIALIZED,
          timestamp: new Date(),
          source: 'qdrant',
          data: { operation: 'upsert', collectionName, message: `No vectors to upsert for collection ${collectionName}` }
        });
        return batchResult;
      }

      // 验证向量数据
      if (finalOptions.validateDimensions) {
        await this.validateVectors(collectionName, vectors);
      }

      // 处理向量数据
      const processedVectors = this.processVectors(vectors, collectionName);

      // 批量处理
      const batchSize = finalOptions.batchSize || 100;
      const totalBatches = Math.ceil(processedVectors.length / batchSize);

      for (let i = 0; i < processedVectors.length; i += batchSize) {
        const batch = processedVectors.slice(i, i + batchSize);
        const batchNumber = Math.floor(i / batchSize) + 1;

        await this.databaseLogger.logDatabaseEvent({
          type: DatabaseEventType.SERVICE_INITIALIZED,
          timestamp: new Date(),
          source: 'qdrant',
          data: {
            operation: 'upsert_batch',
            collectionName,
            batchNumber,
            totalBatches,
            batchSize: batch.length,
            startIdx: i,
            endIdx: i + batch.length
          }
        });

        try {
          const processedPoints = batch.map(point => this.processPoint(point));

          // 验证数据格式
          if (finalOptions.validateDimensions) {
            this.validatePoints(processedPoints);
          }

          // 添加调试信息查看实际传递给Qdrant的数据
          // 现在只支持数组格式的向量
          const vectorForDebug = processedPoints[0]?.vector;

          await this.databaseLogger.logDatabaseEvent({
            type: DatabaseEventType.SERVICE_INITIALIZED,
            timestamp: new Date(),
            source: 'qdrant',
            data: {
              operation: 'upsert_points',
              collectionName,
              pointCount: processedPoints.length,
              samplePoint: {
                id: processedPoints[0]?.id,
                vectorLength: vectorForDebug?.length,
                vectorSample: vectorForDebug ? vectorForDebug.slice(0, 3) : undefined,
                payloadSample: Object.keys(processedPoints[0]?.payload || {}),
                vectorFormat: typeof processedPoints[0]?.vector
              }
            }
          });

          // processedPoints已经由processPoint方法处理成正确的格式，直接使用
          await client.upsert(collectionName, {
            points: processedPoints
          });

          batchResult.processedCount += batch.length;

          await this.databaseLogger.logDatabaseEvent({
            type: DatabaseEventType.SERVICE_INITIALIZED,
            timestamp: new Date(),
            source: 'qdrant',
            data: {
              operation: 'upsert_batch_success',
              collectionName,
              batchNumber,
              totalBatches,
              upsertedCount: batch.length,
              totalProcessed: batchResult.processedCount
            }
          });
        } catch (batchError) {
          batchResult.failedCount += batch.length;
          batchResult.success = false;

          const error = batchError instanceof Error ? batchError : new Error(String(batchError));
          batchResult.errors!.push(error);

          await this.databaseLogger.logDatabaseEvent({
            type: DatabaseEventType.ERROR_OCCURRED,
            timestamp: new Date(),
            source: 'qdrant',
            data: {
              operation: 'upsert_batch_failed',
              collectionName,
              batchNumber,
              totalBatches,
              batchSize: batch.length,
              error: error.message,
              sampleIds: batch.slice(0, 3).map(p => p.id)
            },
            error: error
          });

          if (!finalOptions.skipInvalidPoints) {
            throw error;
          }
        }
      }

      await this.databaseLogger.logBatchOperation('upsert', batchResult.processedCount, {
        collectionName,
        totalVectors: vectors.length,
        processedCount: batchResult.processedCount,
        failedCount: batchResult.failedCount,
        duration: this.performanceMonitor.getOperationStats('upsert_vectors')?.averageDuration
      });

      this.emitEvent(QdrantEventType.VECTORS_UPSERTED, {
        collectionName,
        processedCount: batchResult.processedCount,
        failedCount: batchResult.failedCount,
        totalVectors: vectors.length
      });

      return batchResult;
    } catch (error) {
      batchResult.success = false;
      const errorMessage = error instanceof Error ? error.message : String(error);

      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.ERROR_OCCURRED,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          operation: 'upsert_failed',
          collectionName,
          vectorCount: vectors.length,
          error: errorMessage,
          stack: error instanceof Error ? error.stack : undefined
        },
        error: error instanceof Error ? error : new Error(errorMessage)
      });

      this.errorHandler.handleError(
        new Error(`${ERROR_MESSAGES.UPSERT_FAILED}: ${errorMessage}`),
        { component: 'QdrantVectorOperations', operation: 'upsertVectors' }
      );

      this.emitEvent(QdrantEventType.ERROR, {
        error: error instanceof Error ? error : new Error(errorMessage),
        operation: 'upsertVectors',
        collectionName
      });

      return batchResult;
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
    const searchOptions: VectorSearchOptions = { ...options };
    return this.searchVectorsWithOptions(collectionName, query, searchOptions);
  }

  /**
   * 使用选项搜索向量
   */
  async searchVectorsWithOptions(
    collectionName: string,
    query: number[],
    options: VectorSearchOptions = {}
  ): Promise<SearchResult[]> {
    const finalOptions = { ...DEFAULT_VECTOR_SEARCH_OPTIONS, ...options };

    try {
      const client = this.connectionManager.getClient();
      if (!client) {
        throw new Error(ERROR_MESSAGES.CONNECTION_FAILED);
      }

      const searchParams: any = {
        limit: finalOptions.limit || 10,
        with_payload: finalOptions.withPayload !== false,
        with_vector: finalOptions.withVector || false,
      };

      if (finalOptions.scoreThreshold !== undefined) {
        searchParams.score_threshold = finalOptions.scoreThreshold;
      }

      if (finalOptions.filter) {
        searchParams.filter = finalOptions.filter;
      }

      // 添加搜索策略选项
      if (finalOptions.searchStrategy && finalOptions.searchStrategy !== 'default') {
        searchParams.search_params = {
          hnsw_ef: finalOptions.searchStrategy === 'hnsw' ? 128 : undefined,
          exact: finalOptions.exactSearch
        };
      }

      const results = await client.search(collectionName, {
        vector: query,
        ...searchParams,
      });

      const processedResults = results.map(result => ({
        id: result.id as string,
        score: result.score,
        payload: {
          ...(result.payload as any),
          timestamp:
            result.payload?.timestamp && typeof result.payload.timestamp === 'string'
              ? new Date(result.payload.timestamp)
              : new Date(),
        },
      }));

      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.SERVICE_INITIALIZED,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          operation: 'search_completed',
          collectionName,
          queryLength: query.length,
          resultsCount: processedResults.length,
          searchOptions: finalOptions
        }
      });

      this.emitEvent(QdrantEventType.VECTORS_SEARCHED, {
        collectionName,
        queryLength: query.length,
        resultsCount: processedResults.length,
        searchOptions: finalOptions
      });

      return processedResults;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(
          `${ERROR_MESSAGES.SEARCH_FAILED} in ${collectionName}: ${error instanceof Error ? error.message : String(error)}`
        ),
        { component: 'QdrantVectorOperations', operation: 'searchVectors' }
      );

      this.emitEvent(QdrantEventType.ERROR, {
        error: error instanceof Error ? error : new Error(String(error)),
        operation: 'searchVectors',
        collectionName
      });

      return [];
    }
  }

  /**
   * 删除点
   */
  async deletePoints(collectionName: string, pointIds: string[]): Promise<boolean> {
    try {
      const client = this.connectionManager.getClient();
      if (!client) {
        throw new Error(ERROR_MESSAGES.CONNECTION_FAILED);
      }

      await client.delete(collectionName, {
        filter: {
          must: [
            {
              key: 'id',
              match: {
                any: pointIds,
              },
            },
          ],
        },
      });

      await this.databaseLogger.logBatchOperation('delete', pointIds.length, {
        collectionName,
        operation: 'delete_points'
      });

      this.emitEvent(QdrantEventType.POINTS_DELETED, {
        collectionName,
        deletedCount: pointIds.length,
        pointIds
      });

      return true;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(
          `${ERROR_MESSAGES.DELETE_FAILED} from ${collectionName}: ${error instanceof Error ? error.message : String(error)}`
        ),
        { component: 'QdrantVectorOperations', operation: 'deletePoints' }
      );

      this.emitEvent(QdrantEventType.ERROR, {
        error: error instanceof Error ? error : new Error(String(error)),
        operation: 'deletePoints',
        collectionName
      });

      return false;
    }
  }

  /**
   * 清空集合
   */
  async clearCollection(collectionName: string): Promise<boolean> {
    try {
      const client = this.connectionManager.getClient();
      if (!client) {
        throw new Error(ERROR_MESSAGES.CONNECTION_FAILED);
      }

      await client.delete(collectionName, {
        filter: {
          must: [
            {
              key: 'id',
              match: {
                any: true,
              },
            },
          ],
        },
      });

      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.SERVICE_INITIALIZED,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          operation: 'clear_collection',
          collectionName,
          message: `Cleared collection ${collectionName}`
        }
      });
      return true;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(
          `Failed to clear collection ${collectionName}: ${error instanceof Error ? error.message : String(error)}`
        ),
        { component: 'QdrantVectorOperations', operation: 'clearCollection' }
      );

      this.emitEvent(QdrantEventType.ERROR, {
        error: error instanceof Error ? error : new Error(String(error)),
        operation: 'clearCollection',
        collectionName
      });

      return false;
    }
  }

  /**
   * 获取点数量
   */
  async getPointCount(collectionName: string): Promise<number> {
    try {
      const client = this.connectionManager.getClient();
      if (!client) {
        return 0;
      }

      const info = await client.getCollection(collectionName);
      return info.points_count || 0;
    } catch (error) {
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.SERVICE_ERROR,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          operation: 'get_point_count_failed',
          collectionName,
          error: error instanceof Error ? error.message : String(error)
        }
      });
      return 0;
    }
  }

  /**
   * 验证向量数据
   */
  private async validateVectors(collectionName: string, vectors: VectorPoint[]): Promise<void> {
    if (vectors.length === 0) {
      return;
    }

    const firstVector = vectors[0];
    const vectorSize = firstVector.vector.length;

    await this.databaseLogger.logDatabaseEvent({
      type: DatabaseEventType.SERVICE_INITIALIZED,
      timestamp: new Date(),
      source: 'qdrant',
      data: {
        operation: 'validate_vectors',
        collectionName,
        vectorCount: vectors.length,
        vectorSize,
        sampleIds: vectors.slice(0, 3).map(v => v.id)
      }
    });

    // 检查所有向量的维度是否一致
    const inconsistentVectors = vectors.filter(v => v.vector.length !== vectorSize);
    if (inconsistentVectors.length > 0) {
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.ERROR_OCCURRED,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          operation: 'vector_validation_failed',
          collectionName,
          expectedSize: vectorSize,
          inconsistentCount: inconsistentVectors.length,
          sampleInconsistent: inconsistentVectors.slice(0, 3).map(v => ({
            id: v.id,
            size: v.vector.length
          }))
        },
        error: new Error(`${ERROR_MESSAGES.INVALID_VECTOR_DIMENSIONS}: expected ${vectorSize}, found varying sizes`)
      });
      throw new Error(`${ERROR_MESSAGES.INVALID_VECTOR_DIMENSIONS}: expected ${vectorSize}, found varying sizes`);
    }

    // 检查集合的向量维度是否匹配
    try {
      const collectionInfo = await this.collectionManager.getCollectionInfo(collectionName);
      if (collectionInfo && collectionInfo.vectors.size !== vectorSize) {
        await this.databaseLogger.logDatabaseEvent({
          type: DatabaseEventType.ERROR_OCCURRED,
          timestamp: new Date(),
          source: 'qdrant',
          data: {
            operation: 'vector_dimension_mismatch',
            collectionName,
            collectionSize: collectionInfo.vectors.size,
            vectorSize
          },
          error: new Error(`${ERROR_MESSAGES.VECTOR_DIMENSION_MISMATCH}: collection expects ${collectionInfo.vectors.size}, got ${vectorSize}`)
        });
        throw new Error(`${ERROR_MESSAGES.VECTOR_DIMENSION_MISMATCH}: collection expects ${collectionInfo.vectors.size}, got ${vectorSize}`);
      }
    } catch (collectionError) {
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.SERVICE_ERROR,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          operation: 'collection_validation_failed',
          collectionName,
          error: collectionError instanceof Error ? collectionError.message : String(collectionError)
        }
      });
    }
  }

  /**
   * 处理向量数据
   */
  private processVectors(vectors: VectorPoint[], collectionName: string): VectorPoint[] {
    return vectors.map(vector => {
      // 提取项目ID从集合名称（如果不在payload中）
      if (!vector.payload.projectId) {
        const projectId = collectionName.startsWith('project-') ? collectionName.substring(8) : null;
        if (projectId) {
          return {
            ...vector,
            payload: {
              ...vector.payload,
              projectId
            }
          };
        }
      }
      return vector;
    });
  }

  /**
   * 处理单个点
   */
  private processPoint(point: VectorPoint): any {
    // 验证向量维度
    if (!Array.isArray(point.vector)) {
      throw new Error(`${ERROR_MESSAGES.INVALID_VECTOR_DATA}: must be an array, got ${typeof point.vector}`);
    }

    // 验证ID格式 - Qdrant只接受无符号整数或UUID格式
    let processedId = point.id;
    if (typeof point.id === 'string') {
      // 检查是否为UUID格式
      const uuidRegex = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
      if (uuidRegex.test(point.id)) {
        // UUID格式，直接使用
        processedId = point.id;
      } else {
        // 非UUID格式的字符串ID，需要转换为数字ID以满足Qdrant要求
        // 使用字符串的哈希值转换为数字
        const hash = HashUtils.simpleHash(point.id);
        processedId = Math.abs(parseInt(hash, 36)); // 确保是正数
      }
    } else if (typeof point.id === 'number') {
      // 确保是无符号整数
      processedId = Math.abs(Math.floor(point.id));
    } else {
      throw new Error(`${ERROR_MESSAGES.INVALID_POINT_ID}: must be a string or number, got ${typeof point.id}: ${point.id}`);
    }

    // 创建payload副本并处理特殊字段
    const processedPayload: any = {};
    for (const [key, value] of Object.entries(point.payload)) {
      if (value instanceof Date) {
        processedPayload[key] = value.toISOString();
      } else if (value === null) {
        processedPayload[key] = value;
      } else if (value === undefined) {
        continue;
      } else if (typeof value === 'object' && value !== null) {
        processedPayload[key] = this.processNestedObject(value);
      } else {
        processedPayload[key] = value;
      }
    }

    // 使用直接数组格式，因为集合是用简单向量配置创建的
    // 当使用 {vectors: {size: ..., distance: ...}} 格式创建集合时，向量应该直接是数组格式
    return {
      id: processedId,
      vector: point.vector,  // 直接使用数组格式
      payload: processedPayload,
    };
  }

  /**
   * 验证点数据
   */
  private validatePoints(points: any[]): void {
    if (points.length === 0) {
      return;
    }

    // 检查第一个点的向量格式 - 现在只支持数组格式
    const firstPoint = points[0];

    if (!Array.isArray(firstPoint.vector)) {
      throw new Error(`${ERROR_MESSAGES.INVALID_VECTOR_DATA}: expected array, got ${typeof firstPoint.vector}`);
    }

    const firstPointVector = firstPoint.vector;
    const firstPointVectorSize = firstPoint.vector.length;

    for (const point of points) {
      if (!point.id) {
        throw new Error(`${ERROR_MESSAGES.INVALID_POINT_ID}: ${point.id}`);
      }
      if (!point.payload) {
        throw new Error(`${ERROR_MESSAGES.INVALID_PAYLOAD_DATA}: ${point.payload}`);
      }

      // 验证向量数据 - 现在只支持数组格式
      if (!Array.isArray(point.vector)) {
        throw new Error(`${ERROR_MESSAGES.INVALID_VECTOR_DATA}: expected array, got ${typeof point.vector} for point ${point.id}`);
      }

      const vectorToValidate = point.vector;

      // 验证向量维度是否正确
      if (vectorToValidate.length !== firstPointVectorSize) {
        throw new Error(`${ERROR_MESSAGES.VECTOR_DIMENSION_MISMATCH} for point ${point.id}: expected ${firstPointVectorSize}, got ${vectorToValidate.length}`);
      }

      // 验证向量元素是否为数字
      for (let i = 0; i < vectorToValidate.length; i++) {
        const value = vectorToValidate[i];
        if (typeof value !== 'number' || isNaN(value) || !isFinite(value)) {
          throw new Error(`Invalid vector value at index ${i} for point ${point.id}: ${value}`);
        }
      }

      // 确保payload中不包含undefined值
      for (const [key, value] of Object.entries(point.payload)) {
        if (value === undefined) {
          delete point.payload[key];
        } else if (value instanceof Date) {
          point.payload[key] = value.toISOString();
        } else if (typeof value === 'object' && value !== null) {
          point.payload[key] = this.processNestedObject(value);
        }
      }
    }
  }

  /**
   * 处理嵌套对象
   */
  private processNestedObject(obj: any): any {
    if (obj === null || obj === undefined) {
      return obj;
    }

    if (obj instanceof Date) {
      return obj.toISOString();
    }

    if (Array.isArray(obj)) {
      return obj.map(item => this.processNestedObject(item));
    }

    if (typeof obj === 'object') {
      const processed: any = {};
      for (const [key, value] of Object.entries(obj)) {
        if (value instanceof Date) {
          processed[key] = value.toISOString();
        } else if (value === null || value === undefined) {
          processed[key] = value;
        } else if (typeof value === 'object' && value !== null) {
          processed[key] = this.processNestedObject(value);
        } else {
          processed[key] = value;
        }
      }
      return processed;
    }

    return obj;
  }

  /**
   * 订阅事件
   */
  subscribe(type: QdrantEventType, listener: (event: QdrantEvent) => void) {
    if (!this.eventListeners.has(type)) {
      this.eventListeners.set(type, []);
    }
    this.eventListeners.get(type)!.push(listener);

    // 返回订阅对象，允许取消订阅
    const subscription = {
      id: `${type}_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
      eventType: type,
      handler: listener,
      unsubscribe: () => {
        const listeners = this.eventListeners.get(type);
        if (listeners) {
          const index = listeners.indexOf(listener);
          if (index > -1) {
            listeners.splice(index, 1);
          }
        }
      }
    };

    return subscription;
  }

  /**
   * 发射事件
   */
  private emitEvent(type: QdrantEventType, data?: any, error?: Error): void {
    const event: QdrantEvent = {
      type,
      timestamp: new Date(),
      data,
      error
    };

    const listeners = this.eventListeners.get(type);
    if (listeners) {
      listeners.forEach(async listener => {
        try {
          listener(event);
        } catch (err) {
          await this.databaseLogger.logDatabaseEvent({
            type: DatabaseEventType.ERROR_OCCURRED,
            timestamp: new Date(),
            source: 'qdrant',
            data: {
              operation: 'event_listener_error',
              eventType: type,
              error: err instanceof Error ? err.message : String(err)
            },
            error: err instanceof Error ? err : new Error(String(err))
          });
        }
      });
    }
  }
}